use std::fmt;

use mtg_tl_types as tl;

use super::super::*;

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.bot_api_dialog_id() {
            Some(id) => id.fmt(f),
            None => f.write_str("<self-user>"),
        }
    }
}

impl From<tl::enums::Peer> for PeerId {
    #[inline]
    fn from(peer: tl::enums::Peer) -> Self {
        <Self as From<&tl::enums::Peer>>::from(&peer)
    }
}
impl<'a> From<&'a tl::enums::Peer> for PeerId {
    fn from(peer: &'a tl::enums::Peer) -> Self {
        use tl::enums::Peer;
        match peer {
            Peer::User(user) => Self::from(user),
            Peer::Chat(chat) => Self::from(chat),
            Peer::Channel(channel) => Self::from(channel),
        }
    }
}

impl From<tl::types::PeerUser> for PeerId {
    #[inline]
    fn from(user: tl::types::PeerUser) -> Self {
        <Self as From<&tl::types::PeerUser>>::from(&user)
    }
}
impl<'a> From<&'a tl::types::PeerUser> for PeerId {
    fn from(user: &'a tl::types::PeerUser) -> Self {
        Self::user_unchecked(user.user_id)
    }
}

impl From<tl::types::PeerChat> for PeerId {
    #[inline]
    fn from(user: tl::types::PeerChat) -> Self {
        <Self as From<&tl::types::PeerChat>>::from(&user)
    }
}
impl<'a> From<&'a tl::types::PeerChat> for PeerId {
    fn from(chat: &'a tl::types::PeerChat) -> Self {
        Self::chat_unchecked(chat.chat_id)
    }
}

impl From<tl::types::PeerChannel> for PeerId {
    #[inline]
    fn from(channel: tl::types::PeerChannel) -> Self {
        <Self as From<&tl::types::PeerChannel>>::from(&channel)
    }
}
impl<'a> From<&'a tl::types::PeerChannel> for PeerId {
    fn from(channel: &'a tl::types::PeerChannel) -> Self {
        Self::channel_unchecked(channel.channel_id)
    }
}

impl From<PeerId> for tl::enums::Peer {
    #[inline]
    fn from(peer: PeerId) -> Self {
        <Self as From<&PeerId>>::from(&peer)
    }
}
impl<'a> From<&'a PeerId> for tl::enums::Peer {
    fn from(peer: &'a PeerId) -> Self {
        match peer.kind() {
            PeerKind::User => Self::User(tl::types::PeerUser {
                user_id: peer.bare_id_unchecked(), // may have bogus data on release builds
            }),
            PeerKind::Chat => Self::Chat(tl::types::PeerChat {
                chat_id: peer.bare_id_unchecked(),
            }),
            PeerKind::Channel => Self::Channel(tl::types::PeerChannel {
                channel_id: peer.bare_id_unchecked(),
            }),
        }
    }
}
