// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt;

use mtg_session::types::{ChannelKind, PeerAuth, PeerId, PeerInfo, PeerRef};
use mtg_tl_types as tl;

use crate::Client;

/// A broadcast channel.
///
/// In a broadcast channel, only administrators can broadcast messages to all the subscribers.
/// The rest of users can only join and see messages.
///
/// Broadcast channels and megagroups both are treated as "channels" by Telegram's API, but
/// this variant will always represent a broadcast channel. The only difference between a
/// broadcast channel and a megagroup are the permissions (default, and available).
#[derive(Clone)]
pub struct Channel {
    pub raw: tl::types::Channel,
    pub(crate) client: Client,
}

impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.raw.fmt(f)
    }
}

impl Channel {
    pub fn from_raw(client: &Client, chat: tl::enums::Chat) -> Self {
        use tl::enums::Chat as C;

        match chat {
            C::Empty(_) | C::Chat(_) | C::Forbidden(_) => panic!("cannot create from group chat"),
            C::Channel(channel) => {
                if channel.broadcast {
                    Self {
                        raw: channel,
                        client: client.clone(),
                    }
                } else {
                    panic!("tried to create broadcast channel from megagroup");
                }
            }
            C::ChannelForbidden(channel) => {
                if channel.broadcast {
                    // TODO store until_date
                    Self {
                        raw: tl::types::Channel {
                            creator: false,
                            left: false,
                            broadcast: channel.broadcast,
                            verified: false,
                            megagroup: channel.megagroup,
                            restricted: false,
                            signatures: false,
                            min: false,
                            scam: false,
                            has_link: false,
                            has_geo: false,
                            slowmode_enabled: false,
                            call_active: false,
                            call_not_empty: false,
                            fake: false,
                            gigagroup: false,
                            noforwards: false,
                            join_request: false,
                            forum: false,
                            stories_hidden: false,
                            stories_hidden_min: false,
                            stories_unavailable: true,
                            signature_profiles: false,
                            autotranslation: false,
                            broadcast_messages_allowed: false,
                            monoforum: false,
                            join_to_send: false,
                            id: channel.id,
                            access_hash: Some(channel.access_hash),
                            title: channel.title,
                            username: None,
                            photo: tl::enums::ChatPhoto::Empty,
                            date: 0,
                            restriction_reason: None,
                            admin_rights: None,
                            banned_rights: None,
                            default_banned_rights: None,
                            participants_count: None,
                            usernames: None,
                            stories_max_id: None,
                            color: None,
                            profile_color: None,
                            emoji_status: None,
                            level: None,
                            subscription_until_date: None,
                            bot_verification_icon: None,
                            send_paid_messages_stars: None,
                            forum_tabs: false,
                            in_news: false,
                            forum_sidebar: false,
                            linked_monoforum_id: None,
                            linked_community_id: None,
                            pinned_msg_id: None,
                        },
                        client: client.clone(),
                    }
                } else {
                    panic!("tried to create broadcast channel from megagroup");
                }
            }
            C::CommunityForbidden(_) => panic!("cannot create from community forbidden"),
            C::Community(_) => panic!("cannot create from community"),
        }
    }

    /// Return the unique identifier for this channel.
    pub fn id(&self) -> PeerId {
        PeerId::channel_unchecked(self.raw.id)
    }

    /// Non-min auth stored in the channel, if any.
    pub(crate) fn auth(&self) -> Option<PeerAuth> {
        self.raw
            .access_hash
            .filter(|_| !self.raw.min)
            .map(PeerAuth::from_hash)
    }

    /// Convert the channel to its reference.
    ///
    /// This is only possible if the peer would be usable on all methods or if it is in the session cache.
    pub async fn to_ref(
        &self,
    ) -> Result<Option<PeerRef>, Box<dyn std::error::Error + Send + Sync>> {
        super::to_ref(&self.client, self.id(), self.auth()).await
    }

    /// Additional information about this channel.
    #[inline]
    pub fn kind(&self) -> Option<ChannelKind> {
        <ChannelKind as TryFrom<&Channel>>::try_from(self).ok()
    }

    /// Return the title of this channel.
    pub fn title(&self) -> &str {
        self.raw.title.as_str()
    }

    /// Return the public @username of this channel, if any.
    ///
    /// The returned username does not contain the "@" prefix.
    ///
    /// Outside of the application, people may link to this user with one of Telegram's URLs, such
    /// as https://t.me/username.
    pub fn username(&self) -> Option<&str> {
        self.raw.username.as_deref()
    }

    /// Return collectible usernames of this channel, if any.
    ///
    /// The returned usernames do not contain the "@" prefix.
    ///
    /// Outside of the application, people may link to this user with one of its username, such
    /// as https://t.me/username.
    pub fn usernames(&self) -> Vec<&str> {
        self.raw
            .usernames
            .as_deref()
            .map_or(Vec::new(), |usernames| {
                usernames
                    .iter()
                    .map(|username| match username {
                        tl::enums::Username::Username(username) => username.username.as_ref(),
                    })
                    .collect()
            })
    }

    /// Return the photo of this channel, if any.
    pub fn photo(&self) -> Option<&tl::types::ChatPhoto> {
        match &self.raw.photo {
            tl::enums::ChatPhoto::Empty => None,
            tl::enums::ChatPhoto::Photo(photo) => Some(photo),
        }
    }

    /// Return the permissions of the logged-in user in this channel.
    pub fn admin_rights(&self) -> Option<&tl::types::ChatAdminRights> {
        match &self.raw.admin_rights {
            Some(tl::enums::ChatAdminRights::Rights(rights)) => Some(rights),
            None if self.raw.creator => Some(&tl::types::ChatAdminRights {
                add_admins: true,
                other: true,
                change_info: true,
                post_messages: true,
                anonymous: false,
                ban_users: true,
                delete_messages: true,
                edit_messages: true,
                invite_users: true,
                manage_call: true,
                pin_messages: true,
                manage_topics: true,
                post_stories: true,
                edit_stories: true,
                delete_stories: true,
                manage_direct_messages: true,
                manage_ranks: true,
                manage_linked_peers: true,
                manage_welcome_messages: true,
            }),
            None => None,
        }
    }
}

impl TryFrom<Channel> for ChannelKind {
    type Error = <Self as TryFrom<&'static Channel>>::Error;

    #[inline]
    fn try_from(channel: Channel) -> Result<Self, Self::Error> {
        <Self as TryFrom<&Channel>>::try_from(&channel)
    }
}
impl<'a> TryFrom<&'a Channel> for ChannelKind {
    type Error = <Self as TryFrom<&'a tl::types::Channel>>::Error;

    #[inline]
    fn try_from(channel: &'a Channel) -> Result<Self, Self::Error> {
        <Self as TryFrom<&'a tl::types::Channel>>::try_from(&channel.raw)
    }
}

impl From<Channel> for PeerInfo {
    #[inline]
    fn from(channel: Channel) -> Self {
        <Self as From<&Channel>>::from(&channel)
    }
}
impl<'a> From<&'a Channel> for PeerInfo {
    fn from(channel: &'a Channel) -> Self {
        <Self as From<&'a tl::types::Channel>>::from(&channel.raw)
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{channel as raw_channel, chat, client};

    fn broadcast(c: &Client, edit: impl FnOnce(&mut tl::types::Channel)) -> Channel {
        let tl::enums::Chat::Channel(mut raw) = raw_channel(9) else {
            unreachable!()
        };
        edit(&mut raw);
        Channel::from_raw(c, tl::enums::Chat::Channel(raw))
    }

    fn forbidden(broadcast: bool) -> tl::enums::Chat {
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

    #[tokio::test(flavor = "multi_thread")]
    async fn a_channel_is_built_from_a_broadcast_or_its_forbidden_form() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let ch = broadcast(&c, |_| {});
        assert_eq!(ch.id(), PeerId::channel_unchecked(9));
        let ch = Channel::from_raw(&c, forbidden(true));
        assert_eq!(
            (ch.title(), ch.id()),
            ("gone", PeerId::channel_unchecked(9))
        );
        assert!(ch.raw.stories_unavailable && ch.raw.access_hash == Some(90));
        assert!(format!("{ch:?}").contains("gone"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn anything_but_a_broadcast_channel_is_refused() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let megagroup = {
            let tl::enums::Chat::Channel(mut raw) = raw_channel(9) else {
                unreachable!()
            };
            raw.broadcast = false;
            raw.megagroup = true;
            tl::enums::Chat::Channel(raw)
        };
        let cases = [
            chat(3),
            tl::types::ChatEmpty { id: 1 }.into(),
            tl::types::ChatForbidden {
                id: 1,
                title: String::new(),
            }
            .into(),
            megagroup,
            forbidden(false),
            tl::types::CommunityForbidden {
                id: 1,
                access_hash: None,
                title: String::new(),
            }
            .into(),
        ];
        for raw in cases {
            assert!(catch_unwind(AssertUnwindSafe(|| Channel::from_raw(&c, raw))).is_err());
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn auth_kind_and_references() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let ch = broadcast(&c, |_| {});
        assert!(ch.auth().is_some());
        assert_eq!(ch.kind(), Some(ChannelKind::Broadcast));
        assert_eq!(ChannelKind::try_from(&ch), Ok(ChannelKind::Broadcast));
        assert_eq!(
            ChannelKind::try_from(ch.clone()),
            Ok(ChannelKind::Broadcast)
        );
        assert_eq!(
            ch.to_ref().await.unwrap(),
            Some(crate::test_util::channel_ref(9))
        );
        assert!(matches!(PeerInfo::from(&ch), PeerInfo::Channel { .. }));
        assert!(matches!(PeerInfo::from(ch), PeerInfo::Channel { .. }));
        let min = broadcast(&c, |ch| ch.min = true);
        assert!(min.auth().is_none());
        let mut plain = broadcast(&c, |_| {});
        plain.raw.broadcast = false;
        assert_eq!(plain.kind(), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn names_photo_and_rights() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let ch = broadcast(&c, |ch| {
            ch.username = Some("news".into());
            ch.usernames = Some(vec![
                tl::types::Username {
                    editable: false,
                    active: true,
                    deletable: false,
                    expired: false,
                    username: "news2".into(),
                }
                .into(),
            ]);
            ch.photo = tl::types::ChatPhoto {
                has_video: false,
                photo_id: 3,
                stripped_thumb: None,
                dc_id: 2,
            }
            .into();
        });
        assert_eq!((ch.title(), ch.username()), ("title", Some("news")));
        assert_eq!(ch.usernames(), ["news2"]);
        assert_eq!(ch.photo().unwrap().photo_id, 3);
        let bare = broadcast(&c, |_| {});
        assert!(bare.username().is_none() && bare.usernames().is_empty() && bare.photo().is_none());

        // explicit rights win, a creator without any has all of them, others have none
        let rights = tl::types::ChatAdminRights {
            change_info: false,
            post_messages: true,
            edit_messages: false,
            delete_messages: false,
            ban_users: false,
            invite_users: false,
            pin_messages: false,
            add_admins: false,
            anonymous: false,
            manage_call: false,
            other: false,
            manage_topics: false,
            post_stories: false,
            edit_stories: false,
            delete_stories: false,
            manage_direct_messages: false,
            manage_ranks: false,
            manage_linked_peers: false,
            manage_welcome_messages: false,
        };
        let admin = broadcast(&c, |ch| ch.admin_rights = Some(rights.into()));
        let got = admin.admin_rights().unwrap();
        assert!(got.post_messages && !got.add_admins);
        let creator = broadcast(&c, |ch| ch.creator = true);
        assert!(creator.admin_rights().unwrap().add_admins);
        assert!(bare.admin_rights().is_none());
    }
}
