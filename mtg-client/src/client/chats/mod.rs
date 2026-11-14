// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Methods related to users, groups and channels.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use mtg_mtsender::{InvocationError, RpcError};
use mtg_session::types::{PeerId, PeerKind, PeerRef};
use mtg_tl_types as tl;
use tracing::warn;

use super::Client;
use crate::peer::ActionSender;
use crate::peer::AdminRightsBuilder;
use crate::peer::BannedRightsBuilder;
use crate::peer::Peer;
use crate::peer::PeerMap;
use crate::peer::User;
use crate::peer::chats::AdminRightsBuilderInner;
use crate::peer::chats::BannedRightsBuilderInner;

mod invites;
mod iter;
mod permissions;

pub use iter::{ParticipantIter, ProfilePhotoIter};
pub use permissions::ParticipantPermissions;
use permissions::ParticipantPermissionsInner;

const KICK_BAN_DURATION: i32 = 60; // in seconds, in case the second request fails

fn updates_to_chat(client: &Client, id: Option<i64>, updates: tl::enums::Updates) -> Option<Peer> {
    use tl::enums::Updates;

    let chats = match updates {
        Updates::Combined(updates) => Some(updates.chats),
        Updates::Updates(updates) => Some(updates.chats),
        _ => None,
    };

    match chats {
        Some(chats) => match id {
            Some(id) => chats.into_iter().find(|chat| chat.id() == id),
            None => chats.into_iter().next(),
        },
        None => None,
    }
    .map(|chat| Peer::from_raw(client, chat))
}

/// Method implementations related to dealing with peers.
impl Client {
    /// Resolves a username into the peer that owns it, if any.
    ///
    /// Note that this method is expensive to call, and can quickly cause long flood waits.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(peer) = client.resolve_username("username").await? {
    ///     println!("Found peer!: {:?}", peer.name());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn resolve_username(&self, username: &str) -> Result<Option<Peer>, InvocationError> {
        let tl::types::contacts::ResolvedPeer { peer, users, chats } = match self
            .invoke(&tl::functions::contacts::ResolveUsername {
                username: username.into(),
                referer: None,
            })
            .await
        {
            Ok(tl::enums::contacts::ResolvedPeer::Peer(p)) => p,
            Err(err) if err.is("USERNAME_NOT_OCCUPIED") => return Ok(None),
            Err(err) => return Err(err),
        };

        let mut peers = self.build_peer_map(users, chats).await;
        let peer_id = PeerId::from(peer);

        Ok(peers.take(peer_id))
    }

    /// Fetch full information about the currently logged-in user.
    ///
    /// Although this method is cheap to call, you might want to cache the results somewhere.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// println!("Displaying full user information of the logged-in user:");
    /// dbg!(client.get_me().await?);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_me(&self) -> Result<User, InvocationError> {
        let mut res = self
            .invoke(&tl::functions::users::GetUsers {
                id: vec![tl::enums::InputUser::UserSelf],
            })
            .await?;

        if res.len() != 1 {
            panic!("fetching only one user should exactly return one user");
        }

        Ok(User::from_raw(self, res.pop().unwrap()))
    }

    /// Iterate over the participants of a chat.
    ///
    /// The participants are returned in no particular order.
    ///
    /// When used to iterate the participants of "user", the iterator won't produce values.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(chat: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut participants = client.iter_participants(chat);
    ///
    /// while let Some(participant) = participants.next().await? {
    ///     println!(
    ///         "{} has role {:?}",
    ///         participant.user.first_name().unwrap_or(&participant.user.id().to_string()),
    ///         participant.role
    ///     );
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn iter_participants<C: Into<PeerRef>>(&self, chat: C) -> ParticipantIter {
        ParticipantIter::new(self, chat.into())
    }

    /// Kicks the participant from the chat.
    ///
    /// This will fail if you do not have sufficient permissions to perform said operation,
    /// or the target user is the logged-in account. Use [`Self::delete_dialog`] for the latter instead.
    ///
    /// The kicked user will be able to join after being kicked (they are not permanently banned).
    ///
    /// Kicking someone who was not in the chat prior to running this method will be able to join
    /// after as well (effectively unbanning them).
    ///
    /// When used to kick users from "user" chat, nothing will be done.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(chat: mtg_session::types::PeerRef, user: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// match client.kick_participant(chat, user).await {
    ///     Ok(_) => println!("user is no more >:D"),
    ///     Err(_) => println!("Kick failed! Are you sure you're admin?"),
    /// };
    /// # Ok(())
    /// # }
    /// ```
    pub async fn kick_participant<C: Into<PeerRef>, U: Into<PeerRef>>(
        &self,
        chat: C,
        user: U,
    ) -> Result<(), InvocationError> {
        let chat = chat.into();
        let user = user.into();
        if chat.id.kind() == PeerKind::Channel {
            self.set_banned_rights(chat, user)
                .view_messages(false)
                .duration(Duration::from_secs(KICK_BAN_DURATION as u64))
                .await?;

            self.set_banned_rights(chat, user).await
        } else if chat.id.kind() == PeerKind::Chat {
            self.invoke(&tl::functions::messages::DeleteChatUser {
                chat_id: chat.into(),
                user_id: user.into(),
                revoke_history: false,
            })
            .await
            .map(drop)
        } else {
            Ok(())
        }
    }

    /// Returns a builder to set the banned rights for a specific user.
    ///
    /// Nothing is done until it is awaited, at which point it might result in
    /// error if you do not have sufficient permissions to ban the user in the input chat.
    ///
    /// By default, the user has all rights, and you need to revoke those you want to take away
    /// from the user by setting the permissions to `false`. This means that not taking away any
    /// permissions will effectively unban someone, granting them all default user permissions.
    ///
    /// By default, the ban is applied forever, but this can be changed to a shorter duration.
    ///
    /// The default group rights are respected, despite individual restrictions.
    ///
    /// # Example
    ///
    /// ```
    /// # async fn f(chat: mtg_session::types::PeerRef, user: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // This user keeps spamming pepe stickers, take the sticker permission away from them
    /// let res = client
    ///     .set_banned_rights(chat, user)
    ///     .send_stickers(false)
    ///     .await;
    ///
    /// match res {
    ///     Ok(_) => println!("No more sticker spam!"),
    ///     Err(_) => println!("Ban failed! Are you sure you're admin?"),
    /// };
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_banned_rights<C: Into<PeerRef>, U: Into<PeerRef>>(
        &self,
        channel: C,
        user: U,
    ) -> BannedRightsBuilder<impl Future<Output = Result<(), InvocationError>>> {
        BannedRightsBuilder::new(
            self.clone(),
            channel.into(),
            user.into(),
            BannedRightsBuilderInner::invoke,
        )
    }

    /// Returns a builder to set the administrator rights for a specific user.
    ///
    /// Nothing is done until it is awaited, at which point
    /// it might result in error if you do not have sufficient permissions to grant those rights
    /// to the other user.
    ///
    /// By default, no permissions are granted, and you need to specify those you want to grant by
    /// setting the permissions to `true`. This means that not granting any permission will turn
    /// the user into a normal user again, and they will no longer be an administrator.
    ///
    /// The change is applied forever and there is no way to set a specific duration. If the user
    /// should only be an administrator for a set period of time, the administrator permissions
    /// must be manually revoked at a later point in time.
    ///
    /// # Example
    ///
    /// ```
    /// # async fn f(chat: mtg_session::types::PeerRef, user: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Let the user pin messages and ban other people
    /// let res = client.set_admin_rights(chat, user)
    ///     .load_current()
    ///     .await?
    ///     .pin_messages(true)
    ///     .ban_users(true)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_admin_rights<C: Into<PeerRef>, U: Into<PeerRef>>(
        &self,
        channel: C,
        user: U,
    ) -> AdminRightsBuilder<impl Future<Output = Result<(), InvocationError>>> {
        AdminRightsBuilder::new(
            self.clone(),
            channel.into(),
            user.into(),
            AdminRightsBuilderInner::invoke,
        )
    }

    /// Iterate over the history of profile photos for the given peer.
    ///
    /// Note that the current photo might not be present in the history, and to avoid doing more
    /// work when it's generally not needed (the photo history tends to be complete but in some
    /// cases it might not be), it's up to you to fetch this photo from the full channel.
    ///
    /// Note that you cannot use these photos to send them as messages directly. They must be
    /// downloaded first, then uploaded, and finally sent.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut photos = client.iter_profile_photos(peer);
    ///
    /// while let Some(photo) = photos.next().await? {
    ///     println!("Did you know peer has a photo with ID {}?", photo.id());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn iter_profile_photos<C: Into<PeerRef>>(&self, peer: C) -> ProfilePhotoIter {
        ProfilePhotoIter::new(self, peer.into())
    }

    /// Convert a [`PeerRef`] back into a [`Peer`].
    ///
    /// # Example
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let peer = client.resolve_peer(peer).await?;
    ///
    /// println!("Found peer: {}", peer.name().unwrap_or(&peer.id().to_string()));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn resolve_peer<C: Into<PeerRef>>(&self, peer: C) -> Result<Peer, InvocationError> {
        let peer = peer.into();
        let mut peers = match peer.id.kind() {
            PeerKind::User => {
                let res = self
                    .invoke(&tl::functions::users::GetUsers {
                        id: vec![peer.into()],
                    })
                    .await?;
                if res.len() > 1 {
                    panic!("fetching only one user should exactly return one user");
                }
                self.build_peer_map(res, Vec::new()).await
            }
            PeerKind::Chat => {
                let res = match self
                    .invoke(&tl::functions::messages::GetChats {
                        id: vec![peer.into()],
                    })
                    .await?
                {
                    tl::enums::messages::Chats::Chats(chats) => chats.chats,
                    tl::enums::messages::Chats::Slice(chat_slice) => chat_slice.chats,
                };
                if res.len() > 1 {
                    panic!("fetching only one chat should exactly return one chat");
                }
                self.build_peer_map(Vec::new(), res).await
            }
            PeerKind::Channel => {
                let res = match self
                    .invoke(&tl::functions::channels::GetChannels {
                        id: vec![peer.into()],
                    })
                    .await?
                {
                    tl::enums::messages::Chats::Chats(chats) => chats.chats,
                    tl::enums::messages::Chats::Slice(chat_slice) => chat_slice.chats,
                };
                if res.len() > 1 {
                    panic!("fetching only one channel should exactly return one chat");
                }
                self.build_peer_map(Vec::new(), res).await
            }
        };
        peers.take(peer.id).ok_or(InvocationError::Dropped(
            "response did not contain peer matching the input id",
        ))
    }

    /// Get permissions of participant `user` from chat `chat`.
    ///
    /// # Example
    ///
    /// ```
    /// # async fn f(chat: mtg_session::types::PeerRef, user: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let permissions = client.get_permissions(chat, user).await?;
    /// println!("The user {} an admin", if permissions.is_admin() { "is" } else { "is not" });
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_permissions<C: Into<PeerRef>, U: Into<PeerRef>>(
        &self,
        chat: C,
        user: U,
    ) -> Result<ParticipantPermissions, InvocationError> {
        let chat = chat.into();
        let user = user.into();

        // Get by chat
        if chat.id.kind() == PeerKind::Chat {
            // Get user id
            let user = user.into();
            let user_id = match user {
                tl::enums::InputUser::User(user) => user.user_id,
                tl::enums::InputUser::FromMessage(user) => user.user_id,
                tl::enums::InputUser::UserSelf => {
                    let me = self.get_me().await?;
                    me.id().bare_id_unchecked()
                }
                tl::enums::InputUser::Empty => {
                    return Err(InvocationError::Dropped(
                        "user ref did not actually belong to a user",
                    ));
                }
            };

            // Get chat and find user
            let chat = self
                .invoke(&tl::functions::messages::GetFullChat {
                    chat_id: chat.into(),
                })
                .await?;
            let tl::enums::messages::ChatFull::Full(chat) = chat;
            if let tl::enums::ChatFull::Full(chat) = chat.full_chat
                && let tl::enums::ChatParticipants::Participants(participants) = chat.participants
            {
                for participant in participants.participants {
                    if participant.user_id() == user_id {
                        return Ok(ParticipantPermissions(ParticipantPermissionsInner::Chat(
                            participant,
                        )));
                    }
                }
            }
            return Err(InvocationError::Rpc(RpcError {
                code: 400,
                name: "USER_NOT_PARTICIPANT".to_string(),
                value: None,
                caused_by: None,
            }));
        }

        // Get by channel
        let participant = self
            .invoke(&tl::functions::channels::GetParticipant {
                channel: chat.into(),
                participant: user.into(),
            })
            .await?;
        let tl::enums::channels::ChannelParticipant::Participant(participant) = participant;
        Ok(ParticipantPermissions(
            ParticipantPermissionsInner::Channel(participant.participant),
        ))
    }

    /// Send a message action (such as typing, uploading photo, or viewing an emoji interaction)
    ///
    /// # Examples
    ///
    /// **Do a one-shot pulse and let it fade away**
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_tl_types::enums::SendMessageAction;
    ///
    /// client
    ///     .action(peer)
    ///     .oneshot(SendMessageAction::SendMessageTypingAction)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// **Repeat request until the future is done**
    /// ```
    /// # use std::time::Duration;
    ///
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_tl_types as tl;
    ///
    /// let heavy_task = async {
    ///     tokio::time::sleep(Duration::from_secs(10)).await;
    ///
    ///     42
    /// };
    ///
    /// tokio::pin!(heavy_task);
    ///
    /// let (task_result, _) = client
    ///     .action(peer)
    ///     .repeat(
    ///         // most clients doesn't actually show progress of an action
    ///         || tl::types::SendMessageUploadDocumentAction { progress: 0 },
    ///         heavy_task
    ///     )
    ///     .await;
    ///
    /// // Note: repeat function does not cancel actions automatically, they will just fade away
    ///
    /// assert_eq!(task_result, 42);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// **Cancel any actions**
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// client.action(peer).cancel().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn action<C: Into<PeerRef>>(&self, peer: C) -> ActionSender {
        ActionSender::new(self, peer)
    }

    pub(crate) async fn build_peer_map(
        &self,
        users: Vec<tl::enums::User>,
        chats: Vec<tl::enums::Chat>,
    ) -> PeerMap {
        let map = users
            .into_iter()
            .map(|user| Peer::from_user(self, user))
            .chain(chats.into_iter().map(|chat| Peer::from_raw(self, chat)))
            .map(|peer| (peer.id(), peer))
            .collect::<HashMap<_, _>>();

        if self.0.configuration.auto_cache_peers
            && let Err(e) = self
                .0
                .session
                .cache_peers(
                    map.values()
                        .filter(|peer| peer.auth().is_some())
                        .map(mtg_session::types::PeerInfo::from)
                        .collect(),
                )
                .await
        {
            warn!(?e, "cache_peer fail")
        }

        PeerMap {
            map: Arc::new(map),
            session: Arc::clone(&self.0.session),
        }
    }

    pub(crate) fn empty_peer_map(&self) -> PeerMap {
        PeerMap {
            map: Arc::new(HashMap::new()),
            session: Arc::clone(&self.0.session),
        }
    }
}
