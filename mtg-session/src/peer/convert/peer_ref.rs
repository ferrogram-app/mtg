use std::ops::Deref as _;

use mtg_tl_types as tl;

use super::super::*;

impl From<tl::enums::InputPeer> for PeerRef {
    #[inline]
    fn from(peer: tl::enums::InputPeer) -> Self {
        <Self as From<&tl::enums::InputPeer>>::from(&peer)
    }
}
impl<'a> From<&'a tl::enums::InputPeer> for PeerRef {
    fn from(peer: &'a tl::enums::InputPeer) -> Self {
        use tl::{enums::InputPeer, types::InputPeerSelf};
        match peer {
            InputPeer::Empty => panic!("InputPeer::Empty cannot be converted to any Peer"),
            InputPeer::PeerSelf => <Self as From<&'a _>>::from(&InputPeerSelf {}),
            InputPeer::User(user) => <Self as From<&'a _>>::from(user),
            InputPeer::Chat(chat) => <Self as From<&'a _>>::from(chat),
            InputPeer::Channel(channel) => <Self as From<&'a _>>::from(channel),
            InputPeer::UserFromMessage(user) => <Self as From<&'a _>>::from(user.deref()),
            InputPeer::ChannelFromMessage(channel) => <Self as From<&'a _>>::from(channel.deref()),
        }
    }
}

impl From<tl::types::InputPeerSelf> for PeerRef {
    #[inline]
    fn from(self_user: tl::types::InputPeerSelf) -> Self {
        <Self as From<&tl::types::InputPeerSelf>>::from(&self_user)
    }
}
impl<'a> From<&'a tl::types::InputPeerSelf> for PeerRef {
    fn from(self_user: &'a tl::types::InputPeerSelf) -> Self {
        _ = self_user;
        Self {
            id: SELF_USER_ID,
            auth: PeerAuth::default(),
        }
    }
}

impl From<tl::types::InputPeerUser> for PeerRef {
    #[inline]
    fn from(user: tl::types::InputPeerUser) -> Self {
        <Self as From<&tl::types::InputPeerUser>>::from(&user)
    }
}
impl<'a> From<&'a tl::types::InputPeerUser> for PeerRef {
    fn from(user: &'a tl::types::InputPeerUser) -> Self {
        Self {
            id: PeerId::user_unchecked(user.user_id),
            auth: PeerAuth::from_hash(user.access_hash),
        }
    }
}

impl From<tl::types::InputPeerChat> for PeerRef {
    #[inline]
    fn from(chat: tl::types::InputPeerChat) -> Self {
        <Self as From<&tl::types::InputPeerChat>>::from(&chat)
    }
}
impl<'a> From<&'a tl::types::InputPeerChat> for PeerRef {
    fn from(chat: &'a tl::types::InputPeerChat) -> Self {
        Self {
            id: PeerId::chat_unchecked(chat.chat_id),
            auth: PeerAuth::default(),
        }
    }
}

impl From<tl::types::InputPeerChannel> for PeerRef {
    #[inline]
    fn from(channel: tl::types::InputPeerChannel) -> Self {
        <Self as From<&tl::types::InputPeerChannel>>::from(&channel)
    }
}
impl<'a> From<&'a tl::types::InputPeerChannel> for PeerRef {
    fn from(channel: &'a tl::types::InputPeerChannel) -> Self {
        Self {
            id: PeerId::channel_unchecked(channel.channel_id),
            auth: PeerAuth::from_hash(channel.access_hash),
        }
    }
}

impl From<tl::types::InputPeerUserFromMessage> for PeerRef {
    #[inline]
    fn from(user: tl::types::InputPeerUserFromMessage) -> Self {
        <Self as From<&tl::types::InputPeerUserFromMessage>>::from(&user)
    }
}
impl<'a> From<&'a tl::types::InputPeerUserFromMessage> for PeerRef {
    fn from(user: &'a tl::types::InputPeerUserFromMessage) -> Self {
        // Not currently willing to make PeerRef significantly larger to accomodate for this uncommon type.
        Self {
            id: PeerId::user_unchecked(user.user_id),
            auth: PeerAuth::default(),
        }
    }
}

impl From<tl::types::InputPeerChannelFromMessage> for PeerRef {
    #[inline]
    fn from(channel: tl::types::InputPeerChannelFromMessage) -> Self {
        <Self as From<&tl::types::InputPeerChannelFromMessage>>::from(&channel)
    }
}
impl<'a> From<&'a tl::types::InputPeerChannelFromMessage> for PeerRef {
    fn from(channel: &'a tl::types::InputPeerChannelFromMessage) -> Self {
        // Not currently willing to make PeerRef significantly larger to accomodate for this uncommon type.
        Self {
            id: PeerId::channel_unchecked(channel.channel_id),
            auth: PeerAuth::default(),
        }
    }
}

impl From<tl::enums::User> for PeerRef {
    #[inline]
    fn from(user: tl::enums::User) -> Self {
        <Self as From<&tl::enums::User>>::from(&user)
    }
}
impl<'a> From<&'a tl::enums::User> for PeerRef {
    fn from(user: &'a tl::enums::User) -> Self {
        use tl::enums::User;
        match user {
            User::Empty(user) => <Self as From<&_>>::from(user),
            User::User(user) => <Self as From<&_>>::from(user),
        }
    }
}

impl From<tl::types::UserEmpty> for PeerRef {
    #[inline]
    fn from(user: tl::types::UserEmpty) -> Self {
        <Self as From<&tl::types::UserEmpty>>::from(&user)
    }
}
impl<'a> From<&'a tl::types::UserEmpty> for PeerRef {
    fn from(user: &'a tl::types::UserEmpty) -> Self {
        Self {
            id: PeerId::user_unchecked(user.id),
            auth: PeerAuth::default(),
        }
    }
}

impl From<tl::types::User> for PeerRef {
    #[inline]
    fn from(user: tl::types::User) -> Self {
        <Self as From<&tl::types::User>>::from(&user)
    }
}
impl<'a> From<&'a tl::types::User> for PeerRef {
    fn from(user: &'a tl::types::User) -> Self {
        Self {
            id: PeerId::user_unchecked(user.id),
            auth: user
                .access_hash
                .map(PeerAuth::from_hash)
                .unwrap_or_default(),
        }
    }
}

impl From<tl::enums::Chat> for PeerRef {
    #[inline]
    fn from(chat: tl::enums::Chat) -> Self {
        <Self as From<&tl::enums::Chat>>::from(&chat)
    }
}
impl<'a> From<&'a tl::enums::Chat> for PeerRef {
    fn from(chat: &'a tl::enums::Chat) -> Self {
        use tl::enums::Chat;
        match chat {
            Chat::Empty(chat) => <Self as From<&_>>::from(chat),
            Chat::Chat(chat) => <Self as From<&_>>::from(chat),
            Chat::Forbidden(chat) => <Self as From<&_>>::from(chat),
            Chat::Channel(channel) => <Self as From<&_>>::from(channel),
            Chat::ChannelForbidden(channel) => <Self as From<&_>>::from(channel),
            Chat::CommunityForbidden(community) => <Self as From<&_>>::from(community),
            Chat::Community(community) => <Self as From<&_>>::from(community),
        }
    }
}

impl From<tl::types::ChatEmpty> for PeerRef {
    #[inline]
    fn from(chat: tl::types::ChatEmpty) -> Self {
        <Self as From<&tl::types::ChatEmpty>>::from(&chat)
    }
}
impl<'a> From<&'a tl::types::ChatEmpty> for PeerRef {
    fn from(chat: &'a tl::types::ChatEmpty) -> Self {
        Self {
            id: PeerId::chat_unchecked(chat.id),
            auth: PeerAuth::default(),
        }
    }
}

impl From<tl::types::Chat> for PeerRef {
    #[inline]
    fn from(chat: tl::types::Chat) -> Self {
        <Self as From<&tl::types::Chat>>::from(&chat)
    }
}
impl<'a> From<&'a tl::types::Chat> for PeerRef {
    fn from(chat: &'a tl::types::Chat) -> Self {
        Self {
            id: PeerId::chat_unchecked(chat.id),
            auth: PeerAuth::default(),
        }
    }
}

impl From<tl::types::ChatForbidden> for PeerRef {
    #[inline]
    fn from(chat: tl::types::ChatForbidden) -> Self {
        <Self as From<&tl::types::ChatForbidden>>::from(&chat)
    }
}
impl<'a> From<&'a tl::types::ChatForbidden> for PeerRef {
    fn from(chat: &'a tl::types::ChatForbidden) -> Self {
        Self {
            id: PeerId::chat_unchecked(chat.id),
            auth: PeerAuth::default(),
        }
    }
}

impl From<tl::types::Channel> for PeerRef {
    #[inline]
    fn from(channel: tl::types::Channel) -> Self {
        <Self as From<&tl::types::Channel>>::from(&channel)
    }
}
impl<'a> From<&'a tl::types::Channel> for PeerRef {
    fn from(channel: &'a tl::types::Channel) -> Self {
        Self {
            id: PeerId::channel_unchecked(channel.id),
            auth: channel
                .access_hash
                .map(PeerAuth::from_hash)
                .unwrap_or_default(),
        }
    }
}

impl From<tl::types::ChannelForbidden> for PeerRef {
    #[inline]
    fn from(channel: tl::types::ChannelForbidden) -> Self {
        <Self as From<&tl::types::ChannelForbidden>>::from(&channel)
    }
}
impl<'a> From<&'a tl::types::ChannelForbidden> for PeerRef {
    fn from(channel: &'a tl::types::ChannelForbidden) -> Self {
        Self {
            id: PeerId::channel_unchecked(channel.id),
            auth: PeerAuth::from_hash(channel.access_hash),
        }
    }
}

impl From<tl::types::CommunityForbidden> for PeerRef {
    #[inline]
    fn from(community: tl::types::CommunityForbidden) -> Self {
        <Self as From<&tl::types::CommunityForbidden>>::from(&community)
    }
}
impl<'a> From<&'a tl::types::CommunityForbidden> for PeerRef {
    fn from(community: &'a tl::types::CommunityForbidden) -> Self {
        Self {
            id: PeerId::channel_unchecked(community.id),
            auth: community
                .access_hash
                .map(PeerAuth::from_hash)
                .unwrap_or_default(),
        }
    }
}

impl From<tl::types::Community> for PeerRef {
    #[inline]
    fn from(community: tl::types::Community) -> Self {
        <Self as From<&tl::types::Community>>::from(&community)
    }
}
impl<'a> From<&'a tl::types::Community> for PeerRef {
    fn from(community: &'a tl::types::Community) -> Self {
        Self {
            id: PeerId::channel_unchecked(community.id),
            auth: community
                .access_hash
                .map(PeerAuth::from_hash)
                .unwrap_or_default(),
        }
    }
}

/// Generate owned `From<PeerRef>` impls that forward to the `From<&PeerRef>` impl.
macro_rules! forward_peer_ref_from {
    ($($ty:ty),+ $(,)?) => {$(
        impl From<PeerRef> for $ty {
            #[inline]
            fn from(peer: PeerRef) -> Self {
                <Self as From<&PeerRef>>::from(&peer)
            }
        }
    )+};
}

forward_peer_ref_from!(
    tl::enums::InputPeer,
    tl::enums::InputUser,
    i64,
    tl::enums::InputChannel,
);

impl<'a> From<&'a PeerRef> for tl::enums::InputPeer {
    fn from(peer: &'a PeerRef) -> Self {
        match peer.id.kind() {
            PeerKind::User => match peer.id.bare_id() {
                Some(user_id) => Self::User(tl::types::InputPeerUser {
                    user_id,
                    access_hash: peer.auth.hash(),
                }),
                None => Self::PeerSelf,
            },
            PeerKind::Chat => Self::Chat(tl::types::InputPeerChat {
                chat_id: peer.id.bare_id_unchecked(),
            }),
            PeerKind::Channel => Self::Channel(tl::types::InputPeerChannel {
                channel_id: peer.id.bare_id_unchecked(),
                access_hash: peer.auth.hash(),
            }),
        }
    }
}

impl<'a> From<&'a PeerRef> for tl::enums::InputUser {
    fn from(peer: &'a PeerRef) -> Self {
        match peer.id.kind() {
            PeerKind::User => match peer.id.bare_id() {
                Some(user_id) => Self::User(tl::types::InputUser {
                    user_id,
                    access_hash: peer.auth.hash(),
                }),
                None => Self::UserSelf,
            },
            PeerKind::Chat => Self::Empty,
            PeerKind::Channel => Self::Empty,
        }
    }
}

impl<'a> From<&'a PeerRef> for i64 {
    fn from(peer: &'a PeerRef) -> Self {
        match peer.id.kind() {
            PeerKind::User => EMPTY_CHAT_ID,
            PeerKind::Chat => peer.id.bare_id_unchecked(),
            PeerKind::Channel => EMPTY_CHAT_ID,
        }
    }
}

impl<'a> From<&'a PeerRef> for tl::enums::InputChannel {
    fn from(peer: &'a PeerRef) -> Self {
        match peer.id.kind() {
            PeerKind::User => Self::Empty,
            PeerKind::Chat => Self::Empty,
            PeerKind::Channel => Self::Channel(tl::types::InputChannel {
                channel_id: peer.id.bare_id_unchecked(),
                access_hash: peer.auth.hash(),
            }),
        }
    }
}
