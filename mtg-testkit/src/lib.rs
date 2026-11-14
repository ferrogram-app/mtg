//! A fake Telegram server for tests: one TCP listener that speaks MTProto 2.0 (the "full" transport) with a
//! pre-shared auth key, so the client skips the key exchange. Handlers are registered per request type;
//! every request that arrives is recorded by name.
//!
//! Not for production: it panics on malformed client input.
#![allow(missing_docs)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use mtg_crypto::AuthKey;
use mtg_mtproto::DequeBuffer;
use mtg_mtproto::transport::{Full, Transport};
use mtg_tl_types as tl;
use mtg_tl_types::{Deserializable, Identifiable, RemoteCall, Serializable};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const KEY: [u8; 256] = [7; 256];

/// What a handler answers: the serialized result, or an RPC error `(code, text)`.
pub type Reply = Result<Vec<u8>, (i32, String)>;
type Handler = Box<dyn Fn(&mut tl::Cursor) -> Reply + Send + Sync>;

#[derive(Default)]
struct State {
    handlers: HashMap<u32, Handler>,
}

#[derive(Clone)]
pub struct FakeServer {
    pub addr: SocketAddr,
    state: Arc<Mutex<State>>,
    // separate lock: handlers run while `state` is held
    seen: Arc<Mutex<Vec<&'static str>>>,
}

impl FakeServer {
    /// A session whose home DC 2 points at this server with the shared [`KEY`], so no key exchange happens.
    pub fn session(&self) -> std::sync::Arc<mtg_session::storages::MemorySession> {
        use mtg_session::SessionData;
        use mtg_session::types::DcOption;
        let SocketAddr::V4(ipv4) = self.addr else {
            unreachable!("bound to 127.0.0.1")
        };
        let mut data = SessionData {
            home_dc: 2,
            ..Default::default()
        };
        data.dc_options.insert(
            2,
            DcOption {
                id: 2,
                ipv4,
                ipv6: "[::1]:1".parse().unwrap(),
                auth_key: Some(KEY),
            },
        );
        std::sync::Arc::new(data.into())
    }

    /// Starts listening on a free local port.
    pub async fn start() -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = FakeServer {
            addr: listener.local_addr().unwrap(),
            state: Default::default(),
            seen: Default::default(),
        };
        server.on(|_: tl::functions::help::GetConfig| Ok(config()));
        let s = server.clone();
        tokio::spawn(async move {
            while let Ok((sock, _)) = listener.accept().await {
                tokio::spawn(s.clone().serve(sock));
            }
        });
        server
    }

    /// Answers requests of type `R` with `f`; `Err((code, "TEXT"))` becomes an `rpc_error`.
    pub fn on<R>(&self, f: impl Fn(R) -> Result<R::Return, (i32, String)> + Send + Sync + 'static)
    where
        R: RemoteCall + Deserializable + Identifiable + 'static,
        R::Return: Serializable,
    {
        let name = std::any::type_name::<R>().rsplit("::").next().unwrap();
        let name: &'static str = Box::leak(name.to_string().into_boxed_str());
        let seen = Arc::clone(&self.seen);
        let handler: Handler = Box::new(move |buf| {
            seen.lock().unwrap().push(name);
            let req = R::deserialize(buf).map_err(|e| (400, format!("BAD_REQUEST {e}")))?;
            f(req).map(|r| r.to_bytes())
        });
        self.state
            .lock()
            .unwrap()
            .handlers
            .insert(R::CONSTRUCTOR_ID, handler);
    }

    /// The requests received so far, by type name.
    pub fn requests(&self) -> Vec<&'static str> {
        self.seen.lock().unwrap().clone()
    }

    async fn serve(self, mut sock: tokio::net::TcpStream) {
        let key = AuthKey::from_bytes(KEY);
        let mut transport = Full::new();
        let mut inbuf = Vec::new();
        let mut chunk = [0u8; 4096];
        let mut msg_counter = 0u64;
        loop {
            let n = match sock.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            inbuf.extend_from_slice(&chunk[..n]);
            while let Ok(off) = transport.unpack(&mut inbuf) {
                let mut frame = inbuf[off.data_range.clone()].to_vec();
                inbuf.drain(..off.next_offset);
                let Some((session, msg_id, body)) = decrypt(&key, &mut frame) else {
                    return;
                };
                for out in self.handle(&body, msg_id) {
                    msg_counter += 1;
                    let wire = encrypt(
                        &key,
                        session,
                        // current time (the client refuses ids outside its window), odd, increasing
                        (now_secs() << 32) + 4 * msg_counter + 1,
                        &out,
                    );
                    let mut buf = DequeBuffer::with_capacity(wire.len(), 32);
                    buf.extend_back(&wire);
                    transport.pack(&mut buf);
                    if sock.write_all(buf.as_ref()).await.is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// Plain bodies to send back for one decrypted message body.
    fn handle(&self, body: &[u8], msg_id: i64) -> Vec<Vec<u8>> {
        let mut buf = tl::Cursor::from_slice(body);
        let id = u32::deserialize(&mut buf).unwrap_or(0);
        match id {
            0x73f1f8dc => {
                // msg_container: count, then (msg_id, seqno, len, body)*
                let n = u32::deserialize(&mut buf).unwrap_or(0);
                (0..n)
                    .flat_map(|_| {
                        let id = i64::deserialize(&mut buf).unwrap();
                        let _seq = u32::deserialize(&mut buf).unwrap();
                        let len = u32::deserialize(&mut buf).unwrap() as usize;
                        let mut inner = vec![0u8; len];
                        buf.read_exact(&mut inner).unwrap();
                        self.handle(&inner, id)
                    })
                    .collect()
            }
            0x62d6b459 => vec![],
            // ping_delay_disconnect / ping -> pong
            0xf3427b8c | 0x7abe77ec => {
                let ping_id = i64::deserialize(&mut buf).unwrap();
                let mut out = 0x347773c5u32.to_bytes();
                out.extend(msg_id.to_le_bytes());
                out.extend(ping_id.to_le_bytes());
                vec![out]
            }
            _ => vec![self.rpc(body, msg_id)],
        }
    }

    /// Runs a request (possibly wrapped in invokeWithLayer / initConnection) and builds `rpc_result` or `rpc_error`.
    fn rpc(&self, body: &[u8], msg_id: i64) -> Vec<u8> {
        let mut buf = tl::Cursor::from_slice(body);
        let mut id = u32::deserialize(&mut buf).unwrap();
        while id == 0xda9b0d0d || id == 0xc1cd5ea9 {
            if id == 0xda9b0d0d {
                i32::deserialize(&mut buf).unwrap();
            } else {
                // initConnection: flags, api_id, six strings, optional proxy / params
                let flags = u32::deserialize(&mut buf).unwrap();
                i32::deserialize(&mut buf).unwrap();
                (0..6).for_each(|_| drop(String::deserialize(&mut buf).unwrap()));
                if flags & 1 != 0 {
                    tl::enums::InputClientProxy::deserialize(&mut buf).unwrap();
                }
                if flags & 2 != 0 {
                    tl::enums::Jsonvalue::deserialize(&mut buf).unwrap();
                }
            }
            id = u32::deserialize(&mut buf).unwrap();
        }
        let state = self.state.lock().unwrap();
        let reply = match state.handlers.get(&id) {
            Some(h) => h(&mut buf),
            None => Err((400, format!("NOT_MOCKED_{id:08x}"))),
        };
        drop(state);
        let mut out;
        match reply {
            Ok(result) => {
                out = 0xf35c6d01u32.to_bytes();
                out.extend(msg_id.to_le_bytes());
                out.extend(result);
            }
            Err((code, text)) => {
                out = 0xf35c6d01u32.to_bytes();
                out.extend(msg_id.to_le_bytes());
                out.extend(0x2144ca19u32.to_bytes());
                out.extend(code.to_le_bytes());
                out.extend(text.to_bytes());
            }
        }
        out
    }
}

/// Client-to-server direction (x = 0): `decrypt_data_v2` derives for server-to-client (x = 8), so it is given
/// the key shifted right by 8 bytes and the matching key id.
fn decrypt(key: &AuthKey, frame: &mut [u8]) -> Option<(i64, i64, Vec<u8>)> {
    let mut shifted = [0u8; 256];
    shifted[8..].copy_from_slice(&key.to_bytes()[..248]);
    let sk = AuthKey::from_bytes(shifted);
    frame.get_mut(..8)?.copy_from_slice(&sk.key_id());
    let plain = mtg_crypto::decrypt_data_v2(frame, &sk).ok()?;
    // salt(8) session(8) msg_id(8) seq_no(4) len(4) body
    let session = i64::from_le_bytes(plain[8..16].try_into().ok()?);
    let msg_id = i64::from_le_bytes(plain[16..24].try_into().ok()?);
    let len = u32::from_le_bytes(plain[28..32].try_into().ok()?) as usize;
    Some((session, msg_id, plain.get(32..32 + len)?.to_vec()))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Server-to-client direction (x = 8) with `encrypt_data_v2` (x = 0) on the key shifted left by 8 bytes.
fn encrypt(key: &AuthKey, session: i64, msg_id: u64, body: &[u8]) -> Vec<u8> {
    let mut shifted = [0u8; 256];
    shifted[..248].copy_from_slice(&key.to_bytes()[8..]);
    let mut plain = 0i64.to_le_bytes().to_vec(); // salt
    plain.extend(session.to_le_bytes());
    plain.extend(msg_id.to_le_bytes());
    plain.extend(1u32.to_le_bytes()); // seq_no: content related
    plain.extend((body.len() as u32).to_le_bytes());
    plain.extend(body);
    let (pad, n) = mtg_crypto::get_padding_data_v2(plain.len());
    plain.extend(&pad[..n]);
    let msg_key = mtg_crypto::encrypt_data_v2(&mut plain, &AuthKey::from_bytes(shifted));
    let mut out = key.key_id().to_vec();
    out.extend(msg_key);
    out.extend(plain);
    out
}

/// A minimal `help.config` pointing every DC at nothing: enough for the client's first request.
pub fn config() -> tl::enums::Config {
    tl::enums::Config::Config(tl::types::Config {
        default_p2p_contacts: false,
        preload_featured_stickers: false,
        revoke_pm_inbox: false,
        blocked_mode: false,
        force_try_ipv6: false,
        date: 1_700_000_000,
        expires: 1_800_000_000,
        test_mode: true,
        this_dc: 2,
        dc_options: vec![],
        dc_txt_domain_name: String::new(),
        chat_size_max: 200,
        megagroup_size_max: 200_000,
        forwarded_count_max: 100,
        online_update_period_ms: 120_000,
        offline_blur_timeout_ms: 5000,
        offline_idle_timeout_ms: 30_000,
        online_cloud_timeout_ms: 300_000,
        notify_cloud_delay_ms: 30_000,
        notify_default_delay_ms: 1500,
        push_chat_period_ms: 60_000,
        push_chat_limit: 2,
        edit_time_limit: 172_800,
        revoke_time_limit: 172_800,
        revoke_pm_time_limit: 172_800,
        rating_e_decay: 2419200,
        stickers_recent_limit: 200,
        channels_read_media_period: 604_800,
        tmp_sessions: None,
        call_receive_timeout_ms: 20_000,
        call_ring_timeout_ms: 90_000,
        call_connect_timeout_ms: 30_000,
        call_packet_timeout_ms: 10_000,
        me_url_prefix: "https://t.me/".into(),
        autoupdate_url_prefix: None,
        gif_search_username: None,
        venue_search_username: None,
        img_search_username: None,
        static_maps_provider: None,
        caption_length_max: 1024,
        message_length_max: 4096,
        webfile_dc_id: 4,
        suggested_lang_code: None,
        lang_pack_version: None,
        base_lang_pack_version: None,
        reactions_default: None,
        autologin_token: None,
    })
}
