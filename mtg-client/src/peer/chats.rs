// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::future::Future;
use std::marker::PhantomPinned;
use std::mem::drop;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mtg_mtsender::{InvocationError, RpcError};
use mtg_session::types::{PeerKind, PeerRef};
use mtg_tl_types as tl;
use pin_project_lite::pin_project;

use super::Role;
use crate::Client;

type BuilderRes = Result<(), InvocationError>;
type AdminFutGen<F> = fn(AdminRightsBuilderInner) -> F;

pub(crate) struct AdminRightsBuilderInner {
    client: Client,
    peer_ref: PeerRef,
    peer: tl::enums::InputPeer,
    user: tl::enums::InputUser, // TODO redundant with `peer` (but less annoying to use)
    rights: tl::types::ChatAdminRights,
    rank: String,
}

impl AdminRightsBuilderInner {
    // Perform the call.
    pub(crate) async fn invoke(self) -> Result<(), InvocationError> {
        if self.peer_ref.id.kind() == PeerKind::Channel {
            self.client
                .invoke(&tl::functions::channels::EditAdmin {
                    channel: self.peer_ref.into(),
                    user_id: self.user.clone(),
                    admin_rights: tl::enums::ChatAdminRights::Rights(self.rights.clone()),
                    rank: Some(self.rank.clone()),
                })
                .await
                .map(drop)
        } else if self.peer_ref.id.kind() == PeerKind::Chat {
            let promote = self.rights.anonymous
                || self.rights.change_info
                || self.rights.post_messages
                || self.rights.edit_messages
                || self.rights.delete_messages
                || self.rights.ban_users
                || self.rights.invite_users
                || self.rights.pin_messages
                || self.rights.add_admins
                || self.rights.manage_call;
            self.client
                .invoke(&tl::functions::messages::EditChatAdmin {
                    chat_id: self.peer_ref.into(),
                    user_id: self.user.clone(),
                    is_admin: promote,
                })
                .await
                .map(drop)
        } else {
            Err(InvocationError::Rpc(RpcError {
                code: 400,
                name: "PEER_ID_INVALID".to_string(),
                value: None,
                caused_by: None,
            }))
        }
    }
}

pin_project! {
    /// Builder for editing the administrator rights of a user in a specific chat.
    ///
    /// Use [`Client::set_admin_rights`] to retrieve an instance of this type.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct AdminRightsBuilder<F: Future<Output = BuilderRes>> {
        inner: Option<AdminRightsBuilderInner>,
        fut_gen: AdminFutGen<F>,
        #[pin]
        fut: Option<F>,
        _phantom: PhantomPinned
    }
}

impl<F: Future<Output = BuilderRes>> Future for AdminRightsBuilder<F> {
    type Output = BuilderRes;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<BuilderRes> {
        let mut s = self.project();
        if s.fut.is_none() {
            let Some(s_inner) = s.inner.take() else {
                unreachable!("s.inner is None only when s.fut is some or s.fut is resolved")
            };
            s.fut.set(Some((s.fut_gen)(s_inner)))
        }

        s.fut.as_pin_mut().unwrap().poll(cx)
    }
}

impl<F: Future<Output = BuilderRes>> AdminRightsBuilder<F> {
    pub(crate) fn new(
        client: Client,
        peer: PeerRef,
        user: PeerRef,
        fut_gen: AdminFutGen<F>,
    ) -> Self {
        Self {
            inner: Some(AdminRightsBuilderInner {
                client,
                peer_ref: peer,
                peer: user.into(),
                user: user.into(),
                rank: "".into(),
                rights: tl::types::ChatAdminRights {
                    anonymous: false,
                    change_info: false,
                    post_messages: false,
                    edit_messages: false,
                    delete_messages: false,
                    ban_users: false,
                    invite_users: false,
                    pin_messages: false,
                    add_admins: false,
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
                },
            }),
            fut_gen,
            fut: None,
            _phantom: PhantomPinned,
        }
    }

    fn inner_mut(&mut self) -> &mut AdminRightsBuilderInner {
        // Unwrap safety: AdminRightsBuilderInner should never be None unless polled after being
        // resolved
        self.inner.as_mut().unwrap()
    }

    /// Load the current rights of the user. This lets you trivially grant or take away specific
    /// permissions without changing any of the previous ones.
    pub async fn load_current(mut self) -> Result<Self, InvocationError> {
        let s = self.inner_mut();
        if s.peer_ref.id.kind() == PeerKind::Channel {
            let tl::enums::channels::ChannelParticipant::Participant(user) = s
                .client
                .invoke(&tl::functions::channels::GetParticipant {
                    channel: s.peer_ref.into(),
                    participant: s.peer.clone(),
                })
                .await?;
            match user.participant {
                tl::enums::ChannelParticipant::Creator(c) => {
                    s.rights = c.admin_rights.into();
                    s.rank = c.rank.unwrap_or_default();
                }
                tl::enums::ChannelParticipant::Admin(a) => {
                    s.rights = a.admin_rights.into();
                    s.rank = a.rank.unwrap_or_default();
                }
                _ => (),
            }
        } else if s.peer_ref.id.kind() == PeerKind::Chat {
            let uid = match &s.user {
                tl::enums::InputUser::User(u) => u.user_id,
                tl::enums::InputUser::FromMessage(u) => u.user_id,
                _ => {
                    return Err(InvocationError::Rpc(RpcError {
                        code: 400,
                        name: "PEER_ID_INVALID".to_string(),
                        value: None,
                        caused_by: None,
                    }));
                }
            };

            let mut participants = s.client.iter_participants(s.peer_ref);
            while let Some(participant) = participants.next().await? {
                if matches!(participant.role, Role::Creator(_) | Role::Admin(_))
                    && participant.user.id().bare_id_unchecked() == uid
                {
                    s.rights = tl::types::ChatAdminRights {
                        change_info: true,
                        post_messages: true,
                        edit_messages: false,
                        delete_messages: true,
                        ban_users: true,
                        invite_users: true,
                        pin_messages: true,
                        add_admins: true,
                        anonymous: false,
                        manage_call: true,
                        other: true,
                        manage_topics: true,
                        post_stories: true,
                        edit_stories: true,
                        delete_stories: true,
                        manage_direct_messages: true, // TODO check it
                        manage_ranks: true,
                        manage_linked_peers: true,
                        manage_welcome_messages: true,
                    };
                    break;
                }
            }
        }

        Ok(self)
    }

    /// Whether the user will remain anonymous when sending messages.
    ///
    /// The sender of the anonymous messages becomes the group itself.
    ///
    /// Note that other people in the channel may be able to identify the anonymous admin by its
    /// custom rank, so additional care is needed when using both anonymous and custom ranks.
    ///
    /// For example, if multiple anonymous admins share the same title, users won't be able to
    /// distinguish them.
    pub fn anonymous(mut self, val: bool) -> Self {
        self.inner_mut().rights.anonymous = val;
        self
    }

    /// Whether the user is able to manage calls in the group.
    pub fn manage_call(mut self, val: bool) -> Self {
        self.inner_mut().rights.manage_call = val;
        self
    }

    /// Whether the user is able to change information about the chat such as group description or
    /// not.
    pub fn change_info(mut self, val: bool) -> Self {
        self.inner_mut().rights.change_info = val;
        self
    }

    /// Whether the user will be able to post in the channel. This will only work in broadcast
    /// channels, not groups.
    pub fn post_messages(mut self, val: bool) -> Self {
        self.inner_mut().rights.post_messages = val;
        self
    }

    /// Whether the user will be able to edit messages in the channel. This will only work in
    /// broadcast channels, not groups.
    pub fn edit_messages(mut self, val: bool) -> Self {
        self.inner_mut().rights.edit_messages = val;
        self
    }

    /// Whether the user will be able to delete messages. This includes messages from others.
    pub fn delete_messages(mut self, val: bool) -> Self {
        self.inner_mut().rights.delete_messages = val;
        self
    }

    /// Whether the user will be able to edit the restrictions of other users. This effectively
    /// lets the administrator ban (or kick) people.
    pub fn ban_users(mut self, val: bool) -> Self {
        self.inner_mut().rights.ban_users = val;
        self
    }

    /// Whether the user will be able to invite other users.
    pub fn invite_users(mut self, val: bool) -> Self {
        self.inner_mut().rights.invite_users = val;
        self
    }

    /// Whether the user will be able to pin messages.
    pub fn pin_messages(mut self, val: bool) -> Self {
        self.inner_mut().rights.pin_messages = val;
        self
    }

    /// Whether the user will be able to add other administrators with the same or less
    /// permissions than the user itself.
    pub fn add_admins(mut self, val: bool) -> Self {
        self.inner_mut().rights.add_admins = val;
        self
    }

    /// The custom rank  (also known as "admin title" or "badge") to show for this administrator.
    ///
    /// This text will be shown instead of the "admin" badge.
    ///
    /// When left unspecified or empty, the default localized "admin" badge will be shown.
    pub fn rank<S: Into<String>>(mut self, val: S) -> Self {
        self.inner_mut().rank = val.into();
        self
    }
}

mod banned;

pub use banned::BannedRightsBuilder;
pub(crate) use banned::BannedRightsBuilderInner;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use mtg_testkit::FakeServer;
    use mtg_tl_types::{Deserializable, Serializable};

    use super::*;
    use crate::test_util::{channel_ref, chat_ref, client, user, user_ref};

    type Log = Arc<Mutex<Vec<String>>>;
    type AdminLog = Arc<Mutex<Vec<(tl::types::ChatAdminRights, Option<String>)>>>;
    type RankLog = Arc<Mutex<Vec<(bool, Option<String>)>>>;

    fn updates() -> tl::enums::Updates {
        tl::enums::Updates::Updates(tl::types::Updates {
            updates: Vec::new(),
            users: Vec::new(),
            chats: Vec::new(),
            date: 0,
            seq: 0,
        })
    }

    fn rights() -> tl::types::ChatAdminRights {
        tl::types::ChatAdminRights {
            change_info: true,
            post_messages: false,
            edit_messages: false,
            delete_messages: true,
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
        }
    }

    fn answer(p: tl::enums::ChannelParticipant) -> tl::enums::channels::ChannelParticipant {
        tl::types::channels::ChannelParticipant {
            participant: p,
            chats: Vec::new(),
            users: Vec::new(),
        }
        .into()
    }

    fn banned_rights() -> tl::types::ChatBannedRights {
        tl::types::ChatBannedRights {
            view_messages: false,
            send_messages: true,
            send_media: false,
            send_stickers: false,
            send_gifs: false,
            send_games: false,
            send_inline: false,
            embed_links: false,
            send_polls: false,
            change_info: false,
            invite_users: false,
            pin_messages: false,
            manage_topics: false,
            send_photos: false,
            send_videos: false,
            send_roundvideos: false,
            send_audios: false,
            send_voices: false,
            send_docs: false,
            send_plain: false,
            edit_rank: false,
            send_reactions: false,
            manage_linked_peers: false,
            until_date: 5,
        }
    }

    fn full_chat(participants: Vec<tl::enums::ChatParticipant>) -> tl::enums::messages::ChatFull {
        let mut b = 0u32.to_le_bytes().to_vec();
        b.extend(1i64.to_le_bytes());
        b.extend(String::new().to_bytes());
        b.extend(
            tl::enums::ChatParticipants::from(tl::types::ChatParticipants {
                chat_id: 1,
                participants,
                version: 1,
            })
            .to_bytes(),
        );
        b.extend(0x99622c0cu32.to_le_bytes()); // peerNotifySettings
        b.extend(0u32.to_le_bytes());
        tl::types::messages::ChatFull {
            full_chat: tl::types::ChatFull::from_bytes(&b).unwrap().into(),
            chats: Vec::new(),
            users: vec![user(5), user(6)],
        }
        .into()
    }

    fn is_peer_invalid(r: Result<(), InvocationError>) -> bool {
        matches!(r, Err(InvocationError::Rpc(e)) if e.name == "PEER_ID_INVALID")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_rights_of_a_channel_are_sent_with_every_flag() {
        let server = FakeServer::start().await;
        let seen: AdminLog = Arc::default();
        let log = seen.clone();
        server.on(move |req: tl::functions::channels::EditAdmin| {
            let tl::enums::ChatAdminRights::Rights(r) = req.admin_rights;
            log.lock().unwrap().push((r, req.rank));
            Ok(updates())
        });
        client(&server)
            .set_admin_rights(channel_ref(9), user_ref(5))
            .anonymous(true)
            .manage_call(true)
            .change_info(true)
            .post_messages(true)
            .edit_messages(true)
            .delete_messages(true)
            .ban_users(true)
            .invite_users(true)
            .pin_messages(true)
            .add_admins(true)
            .rank("boss")
            .await
            .unwrap();
        let seen = seen.lock().unwrap();
        let (r, rank) = &seen[0];
        assert_eq!(rank.as_deref(), Some("boss"));
        assert!(r.anonymous && r.manage_call && r.change_info && r.post_messages);
        assert!(r.edit_messages && r.delete_messages && r.ban_users && r.invite_users);
        assert!(r.pin_messages && r.add_admins && !r.other);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_basic_group_promotes_or_demotes_and_other_peers_are_refused() {
        let server = FakeServer::start().await;
        let seen: Log = Arc::default();
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::EditChatAdmin| {
            log.lock()
                .unwrap()
                .push(format!("{} {}", req.chat_id, req.is_admin));
            Ok(true)
        });
        let c = client(&server);
        c.set_admin_rights(chat_ref(3), user_ref(5)).await.unwrap();
        c.set_admin_rights(chat_ref(3), user_ref(5))
            .pin_messages(true)
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), ["3 false", "3 true"]);
        assert!(is_peer_invalid(
            c.set_admin_rights(user_ref(7), user_ref(5)).await
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn current_channel_rights_come_from_creator_admin_or_are_kept() {
        let server = FakeServer::start().await;
        let which = Arc::new(Mutex::new(0));
        let w = which.clone();
        let seen: RankLog = Arc::default();
        let log = seen.clone();
        server.on(move |_: tl::functions::channels::GetParticipant| {
            Ok(answer(match *w.lock().unwrap() {
                0 => tl::types::ChannelParticipantCreator {
                    user_id: 5,
                    admin_rights: rights().into(),
                    rank: Some("owner".into()),
                }
                .into(),
                1 => tl::types::ChannelParticipantAdmin {
                    can_edit: true,
                    is_self: false,
                    user_id: 5,
                    inviter_id: None,
                    promoted_by: 1,
                    date: 1,
                    admin_rights: rights().into(),
                    rank: None,
                }
                .into(),
                _ => tl::types::ChannelParticipantLeft {
                    peer: tl::types::PeerUser { user_id: 5 }.into(),
                }
                .into(),
            }))
        });
        server.on(move |req: tl::functions::channels::EditAdmin| {
            let tl::enums::ChatAdminRights::Rights(r) = req.admin_rights;
            log.lock().unwrap().push((r.change_info, req.rank));
            Ok(updates())
        });
        let c = client(&server);
        for step in 0..3 {
            *which.lock().unwrap() = step;
            c.set_admin_rights(channel_ref(9), user_ref(5))
                .load_current()
                .await
                .unwrap()
                .await
                .unwrap();
        }
        assert_eq!(
            *seen.lock().unwrap(),
            [
                (true, Some("owner".to_string())),
                (true, Some(String::new())),
                (false, Some(String::new()))
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn current_group_rights_are_found_among_the_members() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::messages::GetFullChat| {
            Ok(full_chat(vec![
                tl::types::ChatParticipant {
                    user_id: 5,
                    inviter_id: 1,
                    date: 7,
                    rank: None,
                }
                .into(),
                tl::types::ChatParticipantAdmin {
                    user_id: 6,
                    inviter_id: 1,
                    date: 7,
                    rank: None,
                }
                .into(),
            ]))
        });
        let seen: Log = Arc::default();
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::EditChatAdmin| {
            log.lock()
                .unwrap()
                .push(format!("{} {}", req.chat_id, req.is_admin));
            Ok(true)
        });
        let c = client(&server);
        // 6 is an admin: its rights are filled in, so it stays one
        c.set_admin_rights(chat_ref(3), user_ref(6))
            .load_current()
            .await
            .unwrap()
            .await
            .unwrap();
        // 5 is an ordinary member: nothing to load
        c.set_admin_rights(chat_ref(3), user_ref(5))
            .load_current()
            .await
            .unwrap()
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), ["3 true", "3 false"]);
        // not a user: the builder cannot tell who to look for
        let bad = c
            .set_admin_rights(chat_ref(3), channel_ref(9))
            .load_current()
            .await;
        assert!(matches!(bad, Err(InvocationError::Rpc(_))));
        // other peers have nothing to load
        drop(
            c.set_admin_rights(user_ref(7), user_ref(5))
                .load_current()
                .await
                .unwrap(),
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn banning_in_a_channel_sends_the_inverted_flags() {
        let server = FakeServer::start().await;
        let seen: Arc<Mutex<Vec<tl::types::ChatBannedRights>>> = Arc::default();
        let log = seen.clone();
        server.on(move |req: tl::functions::channels::EditBanned| {
            let tl::enums::ChatBannedRights::Rights(r) = req.banned_rights;
            log.lock().unwrap().push(r);
            Ok(updates())
        });
        let c = client(&server);
        c.set_banned_rights(channel_ref(9), user_ref(5))
            .view_messages(true)
            .send_messages(false)
            .send_media(false)
            .send_stickers(false)
            .send_gifs(false)
            .send_games(false)
            .send_inline(false)
            .embed_link_previews(false)
            .send_polls(false)
            .change_info(false)
            .invite_users(false)
            .pin_messages(false)
            .until(77)
            .await
            .unwrap();
        c.set_banned_rights(channel_ref(9), user_ref(5))
            .duration(Duration::from_secs(1000))
            .await
            .unwrap();
        let seen = seen.lock().unwrap();
        let r = &seen[0];
        assert!(!r.view_messages && r.send_messages && r.send_media && r.send_stickers);
        assert!(r.send_gifs && r.send_games && r.send_inline && r.embed_links && r.send_polls);
        assert!(r.change_info && r.invite_users && r.pin_messages);
        assert_eq!(r.until_date, 77);
        assert!(seen[1].until_date > 1000);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn banning_loads_current_rights_in_channels_only() {
        let server = FakeServer::start().await;
        let seen: Arc<Mutex<Vec<i32>>> = Arc::default();
        let log = seen.clone();
        let banned = Arc::new(Mutex::new(true));
        let flag = banned.clone();
        server.on(move |_: tl::functions::channels::GetParticipant| {
            Ok(answer(if *flag.lock().unwrap() {
                tl::types::ChannelParticipantBanned {
                    left: false,
                    peer: tl::types::PeerUser { user_id: 5 }.into(),
                    kicked_by: 1,
                    date: 1,
                    banned_rights: banned_rights().into(),
                    rank: None,
                }
                .into()
            } else {
                tl::types::ChannelParticipantLeft {
                    peer: tl::types::PeerUser { user_id: 5 }.into(),
                }
                .into()
            }))
        });
        server.on(move |req: tl::functions::channels::EditBanned| {
            let tl::enums::ChatBannedRights::Rights(r) = req.banned_rights;
            log.lock().unwrap().push(r.until_date);
            Ok(updates())
        });
        let c = client(&server);
        let b = c
            .set_banned_rights(channel_ref(9), user_ref(5))
            .load_current()
            .await
            .unwrap();
        b.await.unwrap();
        *banned.lock().unwrap() = false;
        let b = c
            .set_banned_rights(channel_ref(9), user_ref(5))
            .load_current()
            .await
            .unwrap();
        b.await.unwrap();
        drop(
            c.set_banned_rights(chat_ref(3), user_ref(5))
                .load_current()
                .await
                .unwrap(),
        );
        assert_eq!(*seen.lock().unwrap(), [5, 0]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn banning_in_a_basic_group_kicks_or_refuses() {
        let server = FakeServer::start().await;
        let seen: Log = Arc::default();
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::DeleteChatUser| {
            log.lock().unwrap().push(format!("{}", req.chat_id));
            Ok(updates())
        });
        let c = client(&server);
        c.set_banned_rights(chat_ref(3), user_ref(5))
            .view_messages(false)
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), ["3"]);
        let refused = c
            .set_banned_rights(chat_ref(3), user_ref(5))
            .send_messages(false)
            .await;
        assert!(matches!(refused, Err(InvocationError::Rpc(e)) if e.name == "CHAT_INVALID"));
        assert!(is_peer_invalid(
            c.set_banned_rights(user_ref(7), user_ref(5)).await
        ));
    }
}
