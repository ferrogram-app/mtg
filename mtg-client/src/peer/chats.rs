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
