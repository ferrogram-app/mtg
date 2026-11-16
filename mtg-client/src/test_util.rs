//! Fixtures shared by the tests of this crate: a client over a [`FakeServer`], peer references and
//! TL values that have too many fields to write out (built from their wire form).

use mtg_mtsender::SenderPool;
use mtg_session::types::{PeerAuth, PeerId, PeerRef};
use mtg_testkit::FakeServer;
use mtg_tl_types as tl;
use mtg_tl_types::{Deserializable, Serializable};

use crate::Client;

/// A client talking to `server` with the pre-shared key.
pub(crate) fn client(server: &FakeServer) -> Client {
    let pool = SenderPool::new(server.session(), 1);
    tokio::spawn(pool.runner.run());
    Client::new(pool.handle)
}

pub(crate) fn user_ref(id: i64) -> PeerRef {
    PeerRef {
        id: PeerId::user_unchecked(id),
        auth: PeerAuth::from_hash(id * 10),
    }
}

pub(crate) fn chat_ref(id: i64) -> PeerRef {
    PeerRef {
        id: PeerId::chat_unchecked(id),
        auth: PeerAuth::default(),
    }
}

pub(crate) fn channel_ref(id: i64) -> PeerRef {
    PeerRef {
        id: PeerId::channel_unchecked(id),
        auth: PeerAuth::from_hash(id * 10),
    }
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    s.to_owned().serialize(&mut |b| out.extend(b));
}

/// A plain text `message` with the given `flags` (bit 1 = out, 14 = post, ...).
pub(crate) fn message(id: i32, peer: PeerRef, text: &str, flags: u32) -> tl::enums::Message {
    let mut b = flags.to_le_bytes().to_vec();
    b.extend(0u32.to_le_bytes()); // flags2
    b.extend(id.to_le_bytes());
    // from_id is present only with flags.8: not set here
    b.extend(tl::enums::Peer::from(peer.id).to_bytes());
    b.extend(100i32.to_le_bytes()); // date
    put_string(&mut b, text);
    tl::enums::Message::Message(tl::types::Message::from_bytes(&b).unwrap())
}

pub(crate) fn user(id: i64) -> tl::enums::User {
    let mut b = (1u32).to_le_bytes().to_vec(); // access_hash present
    b.extend(0u32.to_le_bytes());
    b.extend(id.to_le_bytes());
    b.extend((id * 10).to_le_bytes());
    tl::enums::User::User(tl::types::User::from_bytes(&b).unwrap())
}

/// `updates` carrying one new message and the `updateMessageID` that maps `random_id` to it.
pub(crate) fn sent(random_id: i64, msg: tl::enums::Message) -> tl::enums::Updates {
    let id = match &msg {
        tl::enums::Message::Message(m) => m.id,
        _ => 0,
    };
    tl::enums::Updates::Updates(tl::types::Updates {
        updates: vec![
            tl::types::UpdateMessageId { id, random_id }.into(),
            tl::types::UpdateNewMessage {
                message: msg,
                pts: 1,
                pts_count: 1,
            }
            .into(),
        ],
        users: Vec::new(),
        chats: Vec::new(),
        date: 0,
        seq: 0,
    })
}
