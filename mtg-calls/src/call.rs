//! Call signalling and the key exchange of one-to-one calls.
//!
//! Outgoing: [`Calls::request`] -> `phone.requestCall`, its result goes to
//! [`Calls::on_request_result`]; the callee's `phoneCallAccepted` produces a `phone.confirmCall`
//! ([`Request::Confirm`]); the final `phoneCall` gives [`Event::Connected`].
//! Incoming: [`Event::Incoming`], then [`Calls::accept`] (or [`Calls::discard`]); the caller's
//! `phoneCall` carries `g_a` and gives [`Event::Connected`].
//!
//! Every update goes through [`Calls::handle_update`]; the caller invokes what comes back.

use std::collections::HashMap;

use mtg_crypto::sha256_parts;
use mtg_secret::Key;
use mtg_secret::dh::{self, DhParams, Exchange};
use mtg_secret::{Error, Result};
use mtg_tl_types as tl;

use crate::emoji::emoji;

/// Oldest call library layer we claim to talk (the official client's `CALL_MIN_LAYER`).
pub const MIN_LAYER: i32 = 65;
/// Most updates for a call we do not know yet that are kept (they may precede the answer of
/// `phone.requestCall`).
const MAX_EARLY: usize = 8;
/// Bytes of server randomness asked from `messages.getDhConfig`.
const RANDOM_LENGTH: i32 = 256;

/// Our `phoneCallProtocol`: UDP p2p and reflector, layers from [`MIN_LAYER`] to `max_layer`, and
/// the versions of the call library the application embeds (e.g. `"2.7.7"`).
pub fn protocol(max_layer: i32, library_versions: Vec<String>) -> tl::types::PhoneCallProtocol {
    tl::types::PhoneCallProtocol {
        udp_p2p: true,
        udp_reflector: true,
        min_layer: MIN_LAYER,
        max_layer,
        library_versions,
    }
}

/// A relay (or WebRTC server) the media goes through.
#[derive(Clone, Debug, PartialEq)]
#[allow(missing_docs)]
pub enum Endpoint {
    /// `phoneConnection`: a reflector, reached with `peer_tag`.
    Reflector {
        id: i64,
        ip: String,
        ipv6: String,
        port: i32,
        peer_tag: Vec<u8>,
        tcp: bool,
    },
    /// `phoneConnectionWebrtc`: a STUN/TURN server.
    Webrtc {
        id: i64,
        ip: String,
        ipv6: String,
        port: i32,
        username: String,
        password: String,
        turn: bool,
        stun: bool,
    },
}

impl From<&tl::enums::PhoneConnection> for Endpoint {
    fn from(c: &tl::enums::PhoneConnection) -> Self {
        match c {
            tl::enums::PhoneConnection::Connection(c) => Endpoint::Reflector {
                id: c.id,
                ip: c.ip.clone(),
                ipv6: c.ipv6.clone(),
                port: c.port,
                peer_tag: c.peer_tag.clone(),
                tcp: c.tcp,
            },
            tl::enums::PhoneConnection::Webrtc(c) => Endpoint::Webrtc {
                id: c.id,
                ip: c.ip.clone(),
                ipv6: c.ipv6.clone(),
                port: c.port,
                username: c.username.clone(),
                password: c.password.clone(),
                turn: c.turn,
                stun: c.stun,
            },
        }
    }
}

/// A call with its key, ready for the media stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Connected {
    /// Call id.
    pub call_id: i64,
    /// We placed the call (the media library needs it to pick the key's direction).
    pub outgoing: bool,
    /// A video call.
    pub video: bool,
    /// The shared key.
    pub key: Key,
    /// The four emojis to compare with the peer.
    pub emoji: [&'static str; 4],
    /// Relays and servers.
    pub endpoints: Vec<Endpoint>,
    /// Direct peer-to-peer is allowed.
    pub p2p_allowed: bool,
    /// The protocol the server settled on (`library_versions` say which library to start).
    pub protocol: tl::types::PhoneCallProtocol,
    /// When the call started (server date).
    pub start_date: i32,
    /// `custom_parameters` (a JSON for the media library).
    pub custom_parameters: Option<String>,
}

/// What happened to a call.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Event {
    /// Somebody is calling: [`Calls::accept`] or [`Calls::discard`] it.
    Incoming {
        /// Call id.
        call_id: i64,
        /// The caller.
        peer_id: i64,
        /// A video call.
        video: bool,
    },
    /// The callee's device is ringing.
    Ringing {
        /// Call id.
        call_id: i64,
    },
    /// The key is agreed: start the media.
    Connected(Connected),
    /// The call is over (the peer hung up, was busy, missed it, ...).
    Ended {
        /// Call id.
        call_id: i64,
        /// Why, if the server said.
        reason: Option<tl::enums::PhoneCallDiscardReason>,
        /// Seconds the call lasted.
        duration: Option<i32>,
        /// Ask the user for a rating ([`Calls::rate`]).
        need_rating: bool,
        /// Send debug data ([`Calls::debug`]).
        need_debug: bool,
    },
    /// The peer's media library sent signalling data: hand it to the engine.
    Signaling {
        /// Call id.
        call_id: i64,
        /// Opaque bytes for the engine.
        data: Vec<u8>,
    },
    /// The call broke the protocol (bad key, wrong hash); a discard was queued.
    Failed {
        /// Call id.
        call_id: i64,
        /// Why.
        reason: String,
    },
}

/// A request to invoke.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// `phone.confirmCall`.
    Confirm(tl::functions::phone::ConfirmCall),
    /// `phone.discardCall`.
    Discard(tl::functions::phone::DiscardCall),
}

/// What [`Calls::handle_update`] produced.
#[derive(Debug, Default, PartialEq)]
pub struct Output {
    /// For the application, in order.
    pub events: Vec<Event>,
    /// Requests to invoke.
    pub send: Vec<Request>,
}

enum State {
    /// We called and wait for the callee to accept (`a` is ours, `g_a` was sent hashed).
    Outgoing {
        exchange: Exchange,
        g_a: Vec<u8>,
    },
    /// We confirmed and wait for the final `phoneCall`.
    Confirmed {
        key: Key,
    },
    /// Ringing for us.
    Incoming {
        g_a_hash: Vec<u8>,
    },
    /// We accepted and wait for the caller's `g_a`.
    Accepted {
        exchange: Exchange,
        g_a_hash: Vec<u8>,
    },
    Connected,
}

struct Call {
    access_hash: i64,
    video: bool,
    outgoing: bool,
    state: State,
}

struct Asked {
    exchange: Exchange,
    g_a: Vec<u8>,
    video: bool,
}

/// The calls of one account.
pub struct Calls {
    ours: tl::types::PhoneCallProtocol,
    params: Option<DhParams>,
    server_random: Vec<u8>,
    asked: HashMap<i32, Asked>,
    calls: HashMap<i64, Call>,
    early: Vec<tl::enums::PhoneCall>,
}

fn input(id: i64, access_hash: i64) -> tl::enums::InputPhoneCall {
    tl::enums::InputPhoneCall::Call(tl::types::InputPhoneCall { id, access_hash })
}

impl Calls {
    /// A manager that speaks `protocol` (see [`protocol`]).
    pub fn new(protocol: tl::types::PhoneCallProtocol) -> Self {
        Self {
            ours: protocol,
            params: None,
            server_random: Vec::new(),
            asked: HashMap::new(),
            calls: HashMap::new(),
            early: Vec::new(),
        }
    }

    /// `messages.getDhConfig` for the cached version (0 the first time).
    pub fn dh_config_request(&self) -> tl::functions::messages::GetDhConfig {
        tl::functions::messages::GetDhConfig {
            version: self.params.as_ref().map_or(0, DhParams::version),
            random_length: RANDOM_LENGTH,
        }
    }

    /// Takes the answer of [`Calls::dh_config_request`]; checks the prime.
    pub fn set_dh_config(&mut self, config: &tl::enums::messages::DhConfig) -> Result<()> {
        let (params, random) = DhParams::from_config(config, self.params.as_ref())?;
        self.params = Some(params);
        self.server_random = random;
        Ok(())
    }

    /// The DH config is known: without it incoming calls are ignored (see [`Calls::set_dh_config`]).
    pub fn has_dh_config(&self) -> bool {
        self.params.is_some()
    }

    fn params(&self) -> Result<&DhParams> {
        self.params
            .as_ref()
            .ok_or(Error::WrongState("no DH config: call set_dh_config first"))
    }

    fn protocol(&self) -> tl::enums::PhoneCallProtocol {
        tl::enums::PhoneCallProtocol::Protocol(self.ours.clone())
    }

    /// Places a call: returns `phone.requestCall` to invoke.
    pub fn request(
        &mut self,
        user: tl::enums::InputUser,
        video: bool,
    ) -> Result<tl::functions::phone::RequestCall> {
        let (exchange, g_a) = dh::start(self.params()?, &self.server_random);
        let mut id = [0u8; 4];
        getrandom::fill(&mut id).expect("system randomness");
        let random_id = i32::from_le_bytes(id);
        let request = tl::functions::phone::RequestCall {
            video,
            user_id: user,
            random_id,
            g_a_hash: sha256_parts(&[&g_a]).to_vec(),
            protocol: self.protocol(),
        };
        self.asked.insert(
            random_id,
            Asked {
                exchange,
                g_a,
                video,
            },
        );
        Ok(request)
    }

    /// Takes the result of the [`Calls::request`] call made with that `random_id`. Returns the
    /// call id and what the updates that already arrived for it produce.
    pub fn on_request_result(
        &mut self,
        random_id: i32,
        result: &tl::enums::phone::PhoneCall,
    ) -> Result<(i64, Output)> {
        let tl::enums::phone::PhoneCall::Call(result) = result;
        let tl::enums::PhoneCall::Waiting(w) = &result.phone_call else {
            return Err(Error::WrongState(
                "requestCall did not return a waiting call",
            ));
        };
        let asked = self
            .asked
            .remove(&random_id)
            .ok_or(Error::WrongState("no such request"))?;
        self.calls.insert(
            w.id,
            Call {
                access_hash: w.access_hash,
                video: asked.video,
                outgoing: true,
                state: State::Outgoing {
                    exchange: asked.exchange,
                    g_a: asked.g_a,
                },
            },
        );
        let mut out = Output::default();
        for call in std::mem::take(&mut self.early) {
            if call_id(&call) == Some(w.id) {
                self.on_call(&call, &mut out);
            } else {
                self.early.push(call);
            }
        }
        Ok((w.id, out))
    }

    /// Accepts a call from [`Event::Incoming`]: returns `phone.acceptCall`.
    pub fn accept(&mut self, call_id: i64) -> Result<tl::functions::phone::AcceptCall> {
        let (exchange, g_b) = dh::start(self.params()?, &self.server_random);
        let protocol = self.protocol();
        let call = self.calls.get_mut(&call_id).ok_or(Error::NoSuchChat(0))?;
        let State::Incoming { g_a_hash } = &mut call.state else {
            return Err(Error::WrongState("the call is not ringing"));
        };
        let g_a_hash = std::mem::take(g_a_hash);
        call.state = State::Accepted { exchange, g_a_hash };
        Ok(tl::functions::phone::AcceptCall {
            peer: input(call_id, call.access_hash),
            g_b,
            protocol,
        })
    }

    /// `phone.receivedCall`: tell the caller this device is ringing (send it on `Incoming`).
    pub fn received(&self, call_id: i64) -> Result<tl::functions::phone::ReceivedCall> {
        let call = self.calls.get(&call_id).ok_or(Error::NoSuchChat(0))?;
        Ok(tl::functions::phone::ReceivedCall {
            peer: input(call_id, call.access_hash),
        })
    }

    /// Hangs up, declines or cancels a call and forgets it: returns `phone.discardCall`.
    /// `connection_id` is the id of the endpoint the media used (0 if none).
    pub fn discard(
        &mut self,
        call_id: i64,
        duration: i32,
        reason: tl::enums::PhoneCallDiscardReason,
        connection_id: i64,
    ) -> Result<tl::functions::phone::DiscardCall> {
        let call = self.calls.remove(&call_id).ok_or(Error::NoSuchChat(0))?;
        Ok(discard_request(
            call_id,
            &call,
            duration,
            reason,
            connection_id,
        ))
    }

    /// `phone.sendSignalingData`: the engine's bytes for the peer's engine.
    pub fn signal(
        &self,
        call_id: i64,
        data: Vec<u8>,
    ) -> Result<tl::functions::phone::SendSignalingData> {
        let call = self.calls.get(&call_id).ok_or(Error::NoSuchChat(0))?;
        Ok(tl::functions::phone::SendSignalingData {
            peer: input(call_id, call.access_hash),
            data,
        })
    }

    /// `phone.setCallRating` (after [`Event::Ended`] with `need_rating`).
    pub fn rate(
        call_id: i64,
        access_hash: i64,
        rating: i32,
        comment: &str,
    ) -> tl::functions::phone::SetCallRating {
        tl::functions::phone::SetCallRating {
            user_initiative: false,
            peer: input(call_id, access_hash),
            rating,
            comment: comment.to_string(),
        }
    }

    /// `phone.saveCallDebug` (after [`Event::Ended`] with `need_debug`).
    pub fn debug(
        call_id: i64,
        access_hash: i64,
        json: &str,
    ) -> tl::functions::phone::SaveCallDebug {
        tl::functions::phone::SaveCallDebug {
            peer: input(call_id, access_hash),
            debug: tl::enums::DataJson::Json(tl::types::DataJson {
                data: json.to_string(),
            }),
        }
    }

    /// The access hash of a live call (for [`Calls::rate`] and [`Calls::debug`]).
    pub fn access_hash(&self, call_id: i64) -> Option<i64> {
        self.calls.get(&call_id).map(|c| c.access_hash)
    }

    /// Handles an update; updates that are not about calls give an empty output.
    pub fn handle_update(&mut self, update: &tl::enums::Update) -> Output {
        let mut out = Output::default();
        match update {
            tl::enums::Update::PhoneCall(u) => self.on_call(&u.phone_call, &mut out),
            tl::enums::Update::PhoneCallSignalingData(u)
                if self.calls.contains_key(&u.phone_call_id) =>
            {
                out.events.push(Event::Signaling {
                    call_id: u.phone_call_id,
                    data: u.data.clone(),
                });
            }
            _ => {}
        }
        out
    }

    fn fail(&mut self, id: i64, reason: &str, out: &mut Output) {
        if let Some(call) = self.calls.remove(&id) {
            out.send.push(Request::Discard(discard_request(
                id,
                &call,
                0,
                tl::enums::PhoneCallDiscardReason::Disconnect,
                0,
            )));
        }
        out.events.push(Event::Failed {
            call_id: id,
            reason: reason.to_string(),
        });
    }

    fn on_call(&mut self, update: &tl::enums::PhoneCall, out: &mut Output) {
        use tl::enums::PhoneCall as P;
        match update {
            P::Empty(_) => {}
            P::Waiting(w) => {
                if self.calls.get(&w.id).is_some_and(|c| c.outgoing) {
                    if w.receive_date.is_some() {
                        out.events.push(Event::Ringing { call_id: w.id });
                    }
                } else {
                    self.hold(update);
                }
            }
            P::Requested(r) => {
                if self.calls.contains_key(&r.id) || self.params.is_none() {
                    return;
                }
                self.calls.insert(
                    r.id,
                    Call {
                        access_hash: r.access_hash,
                        video: r.video,
                        outgoing: false,
                        state: State::Incoming {
                            g_a_hash: r.g_a_hash.clone(),
                        },
                    },
                );
                out.events.push(Event::Incoming {
                    call_id: r.id,
                    peer_id: r.admin_id,
                    video: r.video,
                });
            }
            P::Accepted(a) => self.on_accepted(update, a, out),
            P::Call(c) => self.on_phone_call(update, c, out),
            P::Discarded(d) => {
                self.early.retain(|e| call_id(e) != Some(d.id));
                if self.calls.remove(&d.id).is_some() {
                    out.events.push(Event::Ended {
                        call_id: d.id,
                        reason: d.reason.clone(),
                        duration: d.duration,
                        need_rating: d.need_rating,
                        need_debug: d.need_debug,
                    });
                }
            }
        }
    }

    /// Keeps an update for a call whose `requestCall` answer has not arrived yet.
    fn hold(&mut self, update: &tl::enums::PhoneCall) {
        if self.asked.is_empty() {
            return;
        }
        if self.early.len() >= MAX_EARLY {
            self.early.remove(0);
        }
        self.early.push(update.clone());
    }

    fn on_accepted(
        &mut self,
        update: &tl::enums::PhoneCall,
        a: &tl::types::PhoneCallAccepted,
        out: &mut Output,
    ) {
        let Some(call) = self.calls.get_mut(&a.id) else {
            self.hold(update);
            return;
        };
        let State::Outgoing { exchange, g_a } = &call.state else {
            return;
        };
        match exchange.finish(&a.g_b) {
            Ok(key) => {
                let confirm = tl::functions::phone::ConfirmCall {
                    peer: input(a.id, call.access_hash),
                    g_a: g_a.clone(),
                    key_fingerprint: key.fingerprint(),
                    protocol: tl::enums::PhoneCallProtocol::Protocol(self.ours.clone()),
                };
                call.state = State::Confirmed { key };
                out.send.push(Request::Confirm(confirm));
            }
            Err(_) => self.fail(a.id, "bad g_b", out),
        }
    }

    fn on_phone_call(
        &mut self,
        update: &tl::enums::PhoneCall,
        c: &tl::types::PhoneCall,
        out: &mut Output,
    ) {
        let Some(call) = self.calls.get_mut(&c.id) else {
            self.hold(update);
            return;
        };
        let key = match &call.state {
            State::Confirmed { key } => Ok(key.clone()),
            State::Accepted { exchange, g_a_hash } => {
                if sha256_parts(&[&c.g_a_or_b])[..] != g_a_hash[..] {
                    Err("g_a does not match g_a_hash")
                } else {
                    exchange.finish(&c.g_a_or_b).map_err(|_| "bad g_a")
                }
            }
            _ => return,
        };
        let key = match key {
            Ok(key) if key.fingerprint() == c.key_fingerprint => key,
            Ok(_) => return self.fail(c.id, "key fingerprint mismatch", out),
            Err(why) => return self.fail(c.id, why, out),
        };
        let (video, outgoing) = (call.video || c.video, call.outgoing);
        call.state = State::Connected;
        let protocol = match &c.protocol {
            tl::enums::PhoneCallProtocol::Protocol(p) => p.clone(),
        };
        out.events.push(Event::Connected(Connected {
            call_id: c.id,
            outgoing,
            video,
            emoji: emoji(&key),
            key,
            endpoints: c.connections.iter().map(Endpoint::from).collect(),
            p2p_allowed: c.p2p_allowed,
            protocol,
            start_date: c.start_date,
            custom_parameters: c.custom_parameters.as_ref().map(|j| match j {
                tl::enums::DataJson::Json(j) => j.data.clone(),
            }),
        }));
    }
}

fn call_id(call: &tl::enums::PhoneCall) -> Option<i64> {
    use tl::enums::PhoneCall as P;
    match call {
        P::Empty(c) => Some(c.id),
        P::Waiting(c) => Some(c.id),
        P::Requested(c) => Some(c.id),
        P::Accepted(c) => Some(c.id),
        P::Call(c) => Some(c.id),
        P::Discarded(c) => Some(c.id),
    }
}

fn discard_request(
    id: i64,
    call: &Call,
    duration: i32,
    reason: tl::enums::PhoneCallDiscardReason,
    connection_id: i64,
) -> tl::functions::phone::DiscardCall {
    tl::functions::phone::DiscardCall {
        video: call.video,
        peer: input(id, call.access_hash),
        duration,
        reason,
        connection_id,
    }
}

#[cfg(test)]
impl Calls {
    pub(crate) fn early_len(&self) -> usize {
        self.early.len()
    }
    pub(crate) fn our_protocol(&self) -> tl::enums::PhoneCallProtocol {
        self.protocol()
    }
}
