// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt;

use mtg_session::types::{PeerAuth, PeerId, PeerInfo, PeerRef};
use mtg_tl_types as tl;

use crate::Client;

/// A group chat.
///
/// Telegram's API internally distinguishes between "small group chats" and "megagroups", also
/// known as "supergroups" in the UI of Telegram applications.
///
/// Small group chats are the default, and offer less features than megagroups, but you can
/// join more of them. Certain actions in official clients, like setting a chat's username,
/// silently upgrade the chat to a megagroup.
#[derive(Clone)]
pub struct Group {
    pub raw: tl::enums::Chat,
    pub(crate) client: Client,
}

impl fmt::Debug for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.raw.fmt(f)
    }
}

// TODO it might be desirable to manually merge all the properties of the chat to avoid endless matching

impl Group {
    pub fn from_raw(client: &Client, chat: tl::enums::Chat) -> Self {
        use tl::enums::Chat as C;

        match chat {
            C::Empty(_) | C::Chat(_) | C::Forbidden(_) => Self {
                raw: chat,
                client: client.clone(),
            },
            C::Channel(ref channel) => {
                if channel.broadcast {
                    panic!("tried to create megagroup channel from broadcast");
                } else {
                    Self {
                        raw: chat,
                        client: client.clone(),
                    }
                }
            }
            C::ChannelForbidden(ref channel) => {
                if channel.broadcast {
                    panic!("tried to create megagroup channel from broadcast");
                } else {
                    Self {
                        raw: chat,
                        client: client.clone(),
                    }
                }
            }
            C::CommunityForbidden(_) => {
                panic!("tried to create megagroup channel from community forbidden")
            }
            C::Community(_) => panic!("tried to create megagroup channel from community"),
        }
    }

    /// Return the unique identifier for this group.
    ///
    /// Note that if this group is migrated to a megagroup, both this group and the new one will
    /// exist as separate chats, with different identifiers.
    pub fn id(&self) -> PeerId {
        use tl::enums::Chat;

        match &self.raw {
            Chat::Empty(chat) => PeerId::chat_unchecked(chat.id),
            Chat::Chat(chat) => PeerId::chat_unchecked(chat.id),
            Chat::Forbidden(chat) => PeerId::chat_unchecked(chat.id),
            Chat::Channel(channel) => PeerId::channel_unchecked(channel.id),
            Chat::ChannelForbidden(channel) => PeerId::channel_unchecked(channel.id),
            Chat::CommunityForbidden(community) => PeerId::channel_unchecked(community.id),
            Chat::Community(community) => PeerId::channel_unchecked(community.id),
        }
    }

    /// Non-min auth stored in the group, if any.
    pub(crate) fn auth(&self) -> Option<PeerAuth> {
        use tl::enums::Chat;

        Some(match &self.raw {
            Chat::Empty(_) => PeerAuth::default(),
            Chat::Chat(_) => PeerAuth::default(),
            Chat::Forbidden(_) => PeerAuth::default(),
            Chat::Channel(channel) => {
                return channel
                    .access_hash
                    .filter(|_| !channel.min)
                    .map(PeerAuth::from_hash);
            }
            Chat::ChannelForbidden(channel) => PeerAuth::from_hash(channel.access_hash),
            Chat::CommunityForbidden(community) => {
                return community.access_hash.map(PeerAuth::from_hash);
            }
            Chat::Community(community) => {
                return community
                    .access_hash
                    .filter(|_| !community.min)
                    .map(PeerAuth::from_hash);
            }
        })
    }

    /// Convert the group to its reference.
    ///
    /// This is only possible if the peer would be usable on all methods or if it is in the session cache.
    pub async fn to_ref(
        &self,
    ) -> Result<Option<PeerRef>, Box<dyn std::error::Error + Send + Sync>> {
        super::to_ref(&self.client, self.id(), self.auth()).await
    }

    /// Return the title of this group.
    ///
    /// The title may be the empty string if the group is not accessible.
    pub fn title(&self) -> Option<&str> {
        use tl::enums::Chat;

        match &self.raw {
            Chat::Empty(_) => None,
            Chat::Chat(chat) => Some(chat.title.as_str()),
            Chat::Forbidden(chat) => Some(chat.title.as_str()),
            Chat::Channel(channel) => Some(channel.title.as_str()),
            Chat::ChannelForbidden(channel) => Some(channel.title.as_str()),
            Chat::CommunityForbidden(community) => Some(community.title.as_str()),
            Chat::Community(community) => Some(community.title.as_str()),
        }
    }

    /// Return the public @username of this group, if any.
    ///
    /// The returned username does not contain the "@" prefix.
    ///
    /// Outside of the application, people may link to this user with one of Telegram's URLs, such
    /// as https://t.me/username.
    pub fn username(&self) -> Option<&str> {
        use tl::enums::Chat;

        match &self.raw {
            Chat::Empty(_)
            | Chat::Chat(_)
            | Chat::Forbidden(_)
            | Chat::ChannelForbidden(_)
            | Chat::CommunityForbidden(_)
            | Chat::Community(_) => None,
            Chat::Channel(channel) => channel.username.as_deref(),
        }
    }

    /// Return collectible usernames of this group, if any.
    ///
    /// The returned usernames do not contain the "@" prefix.
    ///
    /// Outside of the application, people may link to this user with one of its username, such
    /// as https://t.me/username.
    pub fn usernames(&self) -> Vec<&str> {
        use tl::enums::Chat;

        match &self.raw {
            Chat::Empty(_)
            | Chat::Chat(_)
            | Chat::Forbidden(_)
            | Chat::ChannelForbidden(_)
            | Chat::CommunityForbidden(_)
            | Chat::Community(_) => Vec::new(),
            Chat::Channel(channel) => {
                channel
                    .usernames
                    .as_deref()
                    .map_or(Vec::new(), |usernames| {
                        usernames
                            .iter()
                            .map(|username| match username {
                                tl::enums::Username::Username(username) => {
                                    username.username.as_ref()
                                }
                            })
                            .collect()
                    })
            }
        }
    }

    // Return photo of this group, if any.
    pub fn photo(&self) -> Option<&tl::types::ChatPhoto> {
        match &self.raw {
            tl::enums::Chat::Empty(_)
            | tl::enums::Chat::Forbidden(_)
            | tl::enums::Chat::ChannelForbidden(_)
            | tl::enums::Chat::CommunityForbidden(_) => None,
            tl::enums::Chat::Chat(chat) => match &chat.photo {
                tl::enums::ChatPhoto::Empty => None,
                tl::enums::ChatPhoto::Photo(photo) => Some(photo),
            },
            tl::enums::Chat::Channel(channel) => match &channel.photo {
                tl::enums::ChatPhoto::Empty => None,
                tl::enums::ChatPhoto::Photo(photo) => Some(photo),
            },
            tl::enums::Chat::Community(community) => match &community.photo {
                tl::enums::ChatPhoto::Empty => None,
                tl::enums::ChatPhoto::Photo(photo) => Some(photo),
            },
        }
    }

    /// Returns true if this group is a megagroup (also known as supergroups).
    ///
    /// In case inner type of group is Channel, that means it's a megagroup.
    pub fn is_megagroup(&self) -> bool {
        use tl::enums::Chat as C;

        match &self.raw {
            C::Empty(_)
            | C::Chat(_)
            | C::Forbidden(_)
            | C::CommunityForbidden(_)
            | C::Community(_) => false,
            C::Channel(_) | C::ChannelForbidden(_) => true,
        }
    }
}

impl From<Group> for PeerInfo {
    #[inline]
    fn from(group: Group) -> Self {
        <Self as From<&Group>>::from(&group)
    }
}
impl<'a> From<&'a Group> for PeerInfo {
    fn from(group: &'a Group) -> Self {
        <Self as From<&'a tl::enums::Chat>>::from(&group.raw)
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{channel as raw_channel, channel_ref, chat, chat_ref, client};

    fn megagroup(min: bool) -> tl::enums::Chat {
        let tl::enums::Chat::Channel(mut raw) = raw_channel(9) else {
            unreachable!()
        };
        raw.broadcast = false;
        raw.megagroup = true;
        raw.min = min;
        raw.username = Some("talk".into());
        raw.usernames = Some(vec![
            tl::types::Username {
                editable: false,
                active: true,
                deletable: false,
                expired: false,
                username: "talk2".into(),
            }
            .into(),
        ]);
        raw.photo = photo();
        tl::enums::Chat::Channel(raw)
    }

    fn photo() -> tl::enums::ChatPhoto {
        tl::types::ChatPhoto {
            has_video: false,
            photo_id: 3,
            stripped_thumb: None,
            dc_id: 2,
        }
        .into()
    }

    fn forbidden_channel(broadcast: bool) -> tl::enums::Chat {
        tl::types::ChannelForbidden {
            broadcast,
            megagroup: !broadcast,
            monoforum: false,
            id: 9,
            access_hash: 90,
            title: "gone".into(),
            until_date: None,
        }
        .into()
    }

    fn community(min: bool) -> tl::enums::Chat {
        tl::types::Community {
            creator: false,
            left: false,
            min,
            collapsed_in_dialogs: false,
            id: 4,
            access_hash: Some(40),
            title: "comm".into(),
            photo: photo(),
            date: 0,
            admin_rights: None,
            default_banned_rights: None,
        }
        .into()
    }

    fn community_forbidden(hash: Option<i64>) -> tl::enums::Chat {
        tl::types::CommunityForbidden {
            id: 5,
            access_hash: hash,
            title: "cf".into(),
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn groups_wrap_every_kind_of_chat_but_not_broadcasts() {
        let server = FakeServer::start().await;
        let c = client(&server);
        for raw in [
            chat(3),
            tl::types::ChatEmpty { id: 1 }.into(),
            tl::types::ChatForbidden {
                id: 2,
                title: "f".into(),
            }
            .into(),
            megagroup(false),
            forbidden_channel(false),
        ] {
            let g = Group::from_raw(&c, raw);
            assert!(format!("{g:?}").len() > 2);
        }
        for raw in [
            raw_channel(9), // a broadcast
            forbidden_channel(true),
            community(false),
            community_forbidden(None),
        ] {
            assert!(catch_unwind(AssertUnwindSafe(|| Group::from_raw(&c, raw))).is_err());
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ids_and_auth_per_variant() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let g = |raw| Group {
            raw,
            client: c.clone(),
        };

        assert_eq!(g(chat(3)).id(), PeerId::chat_unchecked(3));
        assert_eq!(
            g(tl::types::ChatEmpty { id: 1 }.into()).id(),
            PeerId::chat_unchecked(1)
        );
        assert_eq!(
            g(tl::types::ChatForbidden {
                id: 2,
                title: String::new()
            }
            .into())
            .id(),
            PeerId::chat_unchecked(2)
        );
        assert_eq!(g(megagroup(false)).id(), PeerId::channel_unchecked(9));
        assert_eq!(
            g(forbidden_channel(false)).id(),
            PeerId::channel_unchecked(9)
        );
        assert_eq!(g(community(false)).id(), PeerId::channel_unchecked(4));
        assert_eq!(
            g(community_forbidden(None)).id(),
            PeerId::channel_unchecked(5)
        );

        for raw in [
            chat(3),
            tl::types::ChatEmpty { id: 1 }.into(),
            tl::types::ChatForbidden {
                id: 2,
                title: String::new(),
            }
            .into(),
            megagroup(false),
            forbidden_channel(false),
            community(false),
            community_forbidden(Some(1)),
        ] {
            assert!(g(raw).auth().is_some());
        }
        for raw in [megagroup(true), community(true), community_forbidden(None)] {
            assert!(g(raw).auth().is_none());
        }
        assert_eq!(g(chat(3)).to_ref().await.unwrap(), Some(chat_ref(3)));
        assert_eq!(
            g(megagroup(false)).to_ref().await.unwrap(),
            Some(channel_ref(9))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn titles_usernames_and_photos() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let g = |raw| Group {
            raw,
            client: c.clone(),
        };

        assert_eq!(g(tl::types::ChatEmpty { id: 1 }.into()).title(), None);
        assert_eq!(g(chat(3)).title(), Some("group"));
        assert_eq!(
            g(tl::types::ChatForbidden {
                id: 2,
                title: "f".into()
            }
            .into())
            .title(),
            Some("f")
        );
        assert_eq!(g(megagroup(false)).title(), Some("title"));
        assert_eq!(g(forbidden_channel(false)).title(), Some("gone"));
        assert_eq!(g(community(false)).title(), Some("comm"));
        assert_eq!(g(community_forbidden(None)).title(), Some("cf"));

        let mg = g(megagroup(false));
        assert_eq!(
            (mg.username(), mg.usernames()),
            (Some("talk"), vec!["talk2"])
        );
        assert!(mg.is_megagroup() && g(forbidden_channel(false)).is_megagroup());
        for raw in [
            chat(3),
            forbidden_channel(false),
            community(false),
            community_forbidden(None),
        ] {
            let group = g(raw);
            assert!(group.username().is_none() && group.usernames().is_empty());
        }
        let bare = {
            let tl::enums::Chat::Channel(mut raw) = raw_channel(9) else {
                unreachable!()
            };
            raw.broadcast = false;
            raw.megagroup = true;
            g(tl::enums::Chat::Channel(raw))
        };
        assert!(bare.usernames().is_empty());
        for raw in [
            chat(3),
            tl::types::ChatEmpty { id: 1 }.into(),
            community(false),
            community_forbidden(None),
        ] {
            assert!(!g(raw).is_megagroup());
        }

        assert_eq!(mg.photo().unwrap().photo_id, 3);
        assert_eq!(g(community(false)).photo().unwrap().photo_id, 3);
        assert!(g(chat(3)).photo().is_none());
        assert!(g(forbidden_channel(false)).photo().is_none());
        assert!(g(community_forbidden(None)).photo().is_none());
        assert!(g(tl::types::ChatEmpty { id: 1 }.into()).photo().is_none());
        assert!(bare.photo().is_none());
        assert!(
            g(tl::types::ChatForbidden {
                id: 2,
                title: String::new()
            }
            .into())
            .photo()
            .is_none()
        );
        // a basic group with a real photo
        let tl::enums::Chat::Chat(mut basic) = chat(3) else {
            unreachable!()
        };
        basic.photo = photo();
        assert_eq!(g(tl::enums::Chat::Chat(basic)).photo().unwrap().photo_id, 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn peer_info_is_derived_from_the_raw_chat() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let group = Group::from_raw(&c, chat(3));
        assert!(matches!(PeerInfo::from(&group), PeerInfo::Chat { .. }));
        assert!(matches!(PeerInfo::from(group), PeerInfo::Chat { .. }));
    }
}
