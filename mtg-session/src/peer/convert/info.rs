use mtg_tl_types as tl;

use super::super::*;

impl From<tl::enums::Chat> for PeerInfo {
    #[inline]
    fn from(chat: tl::enums::Chat) -> Self {
        <Self as From<&tl::enums::Chat>>::from(&chat)
    }
}
impl<'a> From<&'a tl::enums::Chat> for PeerInfo {
    fn from(chat: &'a tl::enums::Chat) -> Self {
        match chat {
            tl::enums::Chat::Chat(chat) => <Self as From<&tl::types::Chat>>::from(chat),
            tl::enums::Chat::Empty(chat) => <Self as From<&tl::types::ChatEmpty>>::from(chat),
            tl::enums::Chat::Forbidden(chat) => {
                <Self as From<&tl::types::ChatForbidden>>::from(chat)
            }
            tl::enums::Chat::Channel(channel) => <Self as From<&tl::types::Channel>>::from(channel),
            tl::enums::Chat::ChannelForbidden(channel) => {
                <Self as From<&tl::types::ChannelForbidden>>::from(channel)
            }
            tl::enums::Chat::CommunityForbidden(community) => {
                <Self as From<&tl::types::CommunityForbidden>>::from(community)
            }
            tl::enums::Chat::Community(community) => {
                <Self as From<&tl::types::Community>>::from(community)
            }
        }
    }
}

impl From<tl::enums::User> for PeerInfo {
    #[inline]
    fn from(user: tl::enums::User) -> Self {
        <Self as From<&tl::enums::User>>::from(&user)
    }
}
impl<'a> From<&'a tl::enums::User> for PeerInfo {
    fn from(user: &'a tl::enums::User) -> Self {
        match user {
            tl::enums::User::User(user) => <Self as From<&tl::types::User>>::from(user),
            tl::enums::User::Empty(user) => <Self as From<&tl::types::UserEmpty>>::from(user),
        }
    }
}

impl From<tl::types::User> for PeerInfo {
    #[inline]
    fn from(user: tl::types::User) -> Self {
        <Self as From<&tl::types::User>>::from(&user)
    }
}
impl<'a> From<&'a tl::types::User> for PeerInfo {
    fn from(user: &'a tl::types::User) -> Self {
        Self::User {
            id: user.id,
            auth: user.access_hash.map(PeerAuth).filter(|_| !user.min),
            bot: Some(user.bot),
            is_self: Some(user.is_self),
        }
    }
}

impl From<tl::types::UserEmpty> for PeerInfo {
    #[inline]
    fn from(user: tl::types::UserEmpty) -> Self {
        <Self as From<&tl::types::UserEmpty>>::from(&user)
    }
}
impl<'a> From<&'a tl::types::UserEmpty> for PeerInfo {
    fn from(user: &'a tl::types::UserEmpty) -> Self {
        Self::User {
            id: user.id,
            auth: None,
            bot: None,
            is_self: None,
        }
    }
}

impl From<tl::types::Chat> for PeerInfo {
    #[inline]
    fn from(chat: tl::types::Chat) -> Self {
        <Self as From<&tl::types::Chat>>::from(&chat)
    }
}
impl<'a> From<&'a tl::types::Chat> for PeerInfo {
    fn from(chat: &'a tl::types::Chat) -> Self {
        Self::Chat { id: chat.id }
    }
}

impl From<tl::types::ChatEmpty> for PeerInfo {
    #[inline]
    fn from(chat: tl::types::ChatEmpty) -> Self {
        <Self as From<&tl::types::ChatEmpty>>::from(&chat)
    }
}
impl<'a> From<&'a tl::types::ChatEmpty> for PeerInfo {
    fn from(chat: &'a tl::types::ChatEmpty) -> Self {
        Self::Chat { id: chat.id }
    }
}

impl From<tl::types::ChatForbidden> for PeerInfo {
    #[inline]
    fn from(chat: tl::types::ChatForbidden) -> Self {
        <Self as From<&tl::types::ChatForbidden>>::from(&chat)
    }
}
impl<'a> From<&'a tl::types::ChatForbidden> for PeerInfo {
    fn from(chat: &'a tl::types::ChatForbidden) -> Self {
        Self::Chat { id: chat.id }
    }
}

impl From<tl::types::Channel> for PeerInfo {
    #[inline]
    fn from(channel: tl::types::Channel) -> Self {
        <Self as From<&tl::types::Channel>>::from(&channel)
    }
}
impl<'a> From<&'a tl::types::Channel> for PeerInfo {
    fn from(channel: &'a tl::types::Channel) -> Self {
        Self::Channel {
            id: channel.id,
            auth: channel.access_hash.map(PeerAuth).filter(|_| !channel.min),
            kind: <ChannelKind as TryFrom<&'a tl::types::Channel>>::try_from(channel).ok(),
        }
    }
}

impl From<tl::types::ChannelForbidden> for PeerInfo {
    #[inline]
    fn from(channel: tl::types::ChannelForbidden) -> Self {
        <Self as From<&tl::types::ChannelForbidden>>::from(&channel)
    }
}
impl<'a> From<&'a tl::types::ChannelForbidden> for PeerInfo {
    fn from(channel: &'a tl::types::ChannelForbidden) -> Self {
        Self::Channel {
            id: channel.id,
            auth: Some(PeerAuth(channel.access_hash)),
            kind: <ChannelKind as TryFrom<&'a tl::types::ChannelForbidden>>::try_from(channel).ok(),
        }
    }
}

impl From<tl::types::CommunityForbidden> for PeerInfo {
    #[inline]
    fn from(community: tl::types::CommunityForbidden) -> Self {
        <Self as From<&tl::types::CommunityForbidden>>::from(&community)
    }
}
impl<'a> From<&'a tl::types::CommunityForbidden> for PeerInfo {
    fn from(community: &'a tl::types::CommunityForbidden) -> Self {
        Self::Channel {
            id: community.id,
            auth: community.access_hash.map(PeerAuth::from_hash),
            kind: Some(ChannelKind::Community),
        }
    }
}

impl From<tl::types::Community> for PeerInfo {
    #[inline]
    fn from(community: tl::types::Community) -> Self {
        <Self as From<&tl::types::Community>>::from(&community)
    }
}
impl<'a> From<&'a tl::types::Community> for PeerInfo {
    fn from(community: &'a tl::types::Community) -> Self {
        Self::Channel {
            id: community.id,
            auth: community
                .access_hash
                .map(PeerAuth)
                .filter(|_| !community.min),
            kind: Some(ChannelKind::Community),
        }
    }
}

impl TryFrom<tl::types::Channel> for ChannelKind {
    type Error = <ChannelKind as TryFrom<&'static tl::types::Channel>>::Error;

    #[inline]
    fn try_from(channel: tl::types::Channel) -> Result<Self, Self::Error> {
        <ChannelKind as TryFrom<&tl::types::Channel>>::try_from(&channel)
    }
}
impl<'a> TryFrom<&'a tl::types::Channel> for ChannelKind {
    type Error = ();

    fn try_from(channel: &'a tl::types::Channel) -> Result<Self, Self::Error> {
        match channel {
            channel if channel.gigagroup => Ok(Self::Gigagroup),
            channel if channel.broadcast => Ok(Self::Broadcast),
            channel if channel.megagroup => Ok(Self::Megagroup),
            _channel => Err(()),
        }
    }
}

impl TryFrom<tl::types::ChannelForbidden> for ChannelKind {
    type Error = <ChannelKind as TryFrom<&'static tl::types::ChannelForbidden>>::Error;

    #[inline]
    fn try_from(channel: tl::types::ChannelForbidden) -> Result<Self, Self::Error> {
        <ChannelKind as TryFrom<&tl::types::ChannelForbidden>>::try_from(&channel)
    }
}
impl<'a> TryFrom<&'a tl::types::ChannelForbidden> for ChannelKind {
    type Error = ();

    fn try_from(channel: &'a tl::types::ChannelForbidden) -> Result<Self, Self::Error> {
        match channel {
            // channel if channel.gigagroup => Ok(Self::Gigagroup),
            channel if channel.broadcast => Ok(Self::Broadcast),
            channel if channel.megagroup => Ok(Self::Megagroup),
            _channel => Err(()),
        }
    }
}
