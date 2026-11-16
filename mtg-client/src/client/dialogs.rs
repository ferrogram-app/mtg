// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use mtg_mtsender::InvocationError;
use mtg_session::types::{PeerKind, PeerRef, UpdateState};
use mtg_tl_types as tl;

use super::{Client, IterBuffer};
use crate::peer::Dialog;

const MAX_LIMIT: usize = 100;

/// Iterator returned by [`Client::iter_dialogs`].
pub type DialogIter = IterBuffer<tl::functions::messages::GetDialogs, Dialog>;

impl DialogIter {
    fn new(client: &Client) -> Self {
        // TODO let users tweak all the options from the request
        Self::from_request(
            client,
            MAX_LIMIT,
            tl::functions::messages::GetDialogs {
                exclude_pinned: false,
                folder_id: None,
                offset_date: 0,
                offset_id: 0,
                offset_peer: tl::enums::InputPeer::Empty,
                limit: 0,
                hash: 0,
            },
        )
    }

    /// Determines how many dialogs there are in total.
    ///
    /// This only performs a network call if `next` has not been called before.
    pub async fn total(&mut self) -> Result<usize, InvocationError> {
        if let Some(total) = self.total {
            return Ok(total);
        }

        use tl::enums::messages::Dialogs;

        self.request.limit = 1;
        let total = match self.client.invoke(&self.request).await? {
            Dialogs::Dialogs(dialogs) => dialogs.dialogs.len(),
            Dialogs::Slice(dialogs) => dialogs.count as usize,
            Dialogs::NotModified(dialogs) => dialogs.count as usize,
        };
        self.total = Some(total);
        Ok(total)
    }

    /// Return the next `Dialog` from the internal buffer, filling the buffer previously if it's
    /// empty.
    ///
    /// Returns `None` if the `limit` is reached or there are no dialogs left.
    pub async fn next(&mut self) -> Result<Option<Dialog>, InvocationError> {
        if let Some(result) = self.next_raw() {
            return result;
        }

        use tl::enums::messages::Dialogs;

        self.request.limit = self.determine_limit(MAX_LIMIT);
        let (dialogs, mut messages, users, chats) = match self.client.invoke(&self.request).await? {
            Dialogs::Dialogs(d) => {
                self.last_chunk = true;
                self.total = Some(d.dialogs.len());
                (d.dialogs, d.messages, d.users, d.chats)
            }
            Dialogs::Slice(d) => {
                self.last_chunk = d.dialogs.len() < self.request.limit as usize;
                self.total = Some(d.count as usize);
                (d.dialogs, d.messages, d.users, d.chats)
            }
            Dialogs::NotModified(_) => {
                return Err(InvocationError::Dropped("unexpected Dialogs::NotModified"));
            }
        };

        let peers = self.client.build_peer_map(users, chats).await;

        for dialog in dialogs.iter() {
            if let tl::enums::Dialog::Dialog(tl::types::Dialog {
                peer: tl::enums::Peer::Channel(channel),
                pts: Some(pts),
                ..
            }) = &dialog
            {
                self.client
                    .0
                    .session
                    .set_update_state(UpdateState::Channel {
                        id: channel.channel_id,
                        pts: *pts,
                    })
                    .await?;
            }
        }

        self.buffer.extend(
            dialogs.into_iter().filter_map(|dialog| {
                Dialog::new(&self.client, dialog, &mut messages, peers.handle())
            }),
        );

        // Don't bother updating offsets if this is the last time stuff has to be fetched.
        if !self.last_chunk && !self.buffer.is_empty() {
            self.request.exclude_pinned = true;
            if let Some(last_message) = self
                .buffer
                .iter()
                .rev()
                .find_map(|dialog| dialog.last_message.as_ref())
            {
                self.request.offset_date = last_message.date_timestamp();
                self.request.offset_id = last_message.id();
            }
            self.request.offset_peer = self.buffer[self.buffer.len() - 1].peer_ref().into();
        }

        Ok(self.pop_item())
    }
}

/// Method implementations related to open conversations.
impl Client {
    /// Returns a new iterator over the dialogs.
    ///
    /// While iterating, the update state for any broadcast channel or megagroup will be set if it was unknown before.
    /// When the update state is set for these peers, the library can actively check to make sure it's not missing any
    /// updates from them (as long as the queue limit for updates is larger than zero).
    ///
    /// Bot accounts will receive an error response when using this method, as they do not have dialogs.
    /// The closest a bot account can get to listing the peers it has interacted with is by querying the
    /// session cache, but that approach can be misleading, as it often contains both additional peers,
    /// and peers that have since blocked the bot ("deleted the dialog" with it).
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut dialogs = client.iter_dialogs();
    ///
    /// while let Some(dialog) = dialogs.next().await? {
    ///     let peer = dialog.peer();
    ///     println!("{} ({})", peer.name().unwrap_or_default(), peer.id());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn iter_dialogs(&self) -> DialogIter {
        DialogIter::new(self)
    }

    /// Deletes a dialog, effectively removing it from your list of open conversations.
    ///
    /// The dialog is only deleted for yourself.
    ///
    /// Deleting a dialog effectively clears the message history and "kicks" you from it.
    ///
    /// For groups and channels, this is the same as leaving said chat. This method does **not**
    /// delete the chat itself (the chat still exists and the other members will remain inside).
    ///
    /// Bot accounts can use this method to leave a channel or group, but attempting
    /// to leave the dialog with a user will fail, as bots do not actually have dialogs.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Consider making a backup before, you will lose access to the messages in peer!
    /// client.delete_dialog(peer).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_dialog<C: Into<PeerRef>>(&self, peer: C) -> Result<(), InvocationError> {
        let peer = peer.into();
        if peer.id.kind() == PeerKind::Channel {
            self.invoke(&tl::functions::channels::LeaveChannel {
                channel: peer.into(),
            })
            .await
            .map(drop)
        } else if peer.id.kind() == PeerKind::Chat {
            self.invoke(&tl::functions::messages::DeleteChatUser {
                chat_id: peer.into(),
                user_id: tl::enums::InputUser::UserSelf,
                revoke_history: false,
            })
            .await
            .map(drop)
        } else {
            self.invoke(&tl::functions::messages::DeleteHistory {
                just_clear: false,
                revoke: false,
                peer: peer.into(),
                max_id: 0,
                min_date: None,
                max_date: None,
            })
            .await
            .map(drop)
        }
    }

    /// Mark a peer as read.
    ///
    /// If you want to get rid of all the mentions (for example, a voice note that you have not
    /// listened to yet), you need to also use [`Client::clear_mentions`].
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// client.mark_as_read(peer).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn mark_as_read<C: Into<PeerRef>>(&self, peer: C) -> Result<(), InvocationError> {
        let peer = peer.into();
        if peer.id.kind() == PeerKind::Channel {
            self.invoke(&tl::functions::channels::ReadHistory {
                channel: peer.into(),
                max_id: 0,
            })
            .await
            .map(drop)
        } else {
            self.invoke(&tl::functions::messages::ReadHistory {
                peer: peer.into(),
                max_id: 0,
            })
            .await
            .map(drop)
        }
    }

    /// Clears all pending mentions from a peer, marking them as read.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// client.clear_mentions(peer).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn clear_mentions<C: Into<PeerRef>>(&self, peer: C) -> Result<(), InvocationError> {
        self.invoke(&tl::functions::messages::ReadMentions {
            peer: peer.into().into(),
            top_msg_id: None,
        })
        .await
        .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use mtg_session::Session;
    use mtg_session::types::PeerId;
    use mtg_testkit::FakeServer;
    use mtg_tl_types as tl;

    use crate::test_util::{
        channel, channel_ref, chat_ref, client, client_with, dialog, message, user, user_ref,
    };

    fn dialogs(
        dialogs: Vec<tl::enums::Dialog>,
        messages: Vec<tl::enums::Message>,
        chats: Vec<tl::enums::Chat>,
        users: Vec<tl::enums::User>,
        count: Option<i32>,
    ) -> tl::enums::messages::Dialogs {
        match count {
            None => tl::types::messages::Dialogs {
                dialogs,
                messages,
                chats,
                users,
            }
            .into(),
            Some(count) => tl::types::messages::DialogsSlice {
                count,
                dialogs,
                messages,
                chats,
                users,
            }
            .into(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dialogs_are_listed_and_the_channel_pts_is_remembered() {
        let server = FakeServer::start().await;
        let (u, ch) = (user_ref(5), channel_ref(9));
        server.on(move |_: tl::functions::messages::GetDialogs| {
            Ok(dialogs(
                vec![dialog(u, 1, None), dialog(ch, 2, Some(77))],
                vec![message(1, u, "a", 0), message(2, ch, "b", 0)],
                vec![channel(9)],
                vec![user(5)],
                Some(2),
            ))
        });
        let (c, session) = client_with(server.session());
        let mut it = c.iter_dialogs();
        assert_eq!(it.total().await.unwrap(), 2);
        assert_eq!(it.total().await.unwrap(), 2); // cached
        it.total = None;
        let mut peers = Vec::new();
        while let Some(d) = it.next().await.unwrap() {
            peers.push(d.peer_id());
        }
        assert_eq!(
            peers,
            [PeerId::user_unchecked(5), PeerId::channel_unchecked(9)]
        );
        let state = session.updates_state().await.unwrap();
        assert!(state.channels.iter().any(|c| c.id == 9 && c.pts == 77));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_full_page_asks_for_the_next_one_after_the_last_message() {
        let server = FakeServer::start().await;
        let u = user_ref(5);
        server.on(move |req: tl::functions::messages::GetDialogs| {
            Ok(if req.offset_id == 0 {
                dialogs(
                    vec![dialog(u, 1, None)],
                    vec![message(1, u, "a", 0)],
                    Vec::new(),
                    vec![user(5)],
                    Some(9),
                )
            } else {
                assert!(req.exclude_pinned);
                assert_eq!(req.offset_id, 1);
                dialogs(Vec::new(), Vec::new(), Vec::new(), Vec::new(), None)
            })
        });
        let mut it = client(&server).iter_dialogs().limit(1);
        // limit 1 and one dialog: the iterator stops after it without a second request
        assert!(it.next().await.unwrap().is_some());
        assert!(it.next().await.unwrap().is_none());
        // without a limit a one-dialog slice is shorter than a full page: the last one
        let mut it = client(&server).iter_dialogs();
        assert!(it.next().await.unwrap().is_some());
        assert!(it.next().await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn not_modified_dialogs_are_an_error_with_a_count_for_total() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::messages::GetDialogs| {
            Ok(tl::types::messages::DialogsNotModified { count: 4 }.into())
        });
        let mut it = client(&server).iter_dialogs();
        assert_eq!(it.total().await.unwrap(), 4);
        assert!(client(&server).iter_dialogs().next().await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn plain_dialogs_are_counted_by_length() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::messages::GetDialogs| {
            Ok(dialogs(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
            ))
        });
        let mut it = client(&server).iter_dialogs();
        assert_eq!(it.total().await.unwrap(), 0);
        assert!(
            client(&server)
                .iter_dialogs()
                .next()
                .await
                .unwrap()
                .is_none()
        );
    }

    fn ack() -> tl::enums::messages::AffectedHistory {
        tl::types::messages::AffectedHistory {
            pts: 1,
            pts_count: 1,
            offset: 0,
        }
        .into()
    }

    fn updates() -> tl::enums::Updates {
        tl::enums::Updates::TooLong
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deleting_a_dialog_uses_the_call_of_its_kind() {
        let server = FakeServer::start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = |name: &'static str| {
            let seen = seen.clone();
            move || seen.lock().unwrap().push(name)
        };
        let leave = log("leave");
        server.on(move |_: tl::functions::channels::LeaveChannel| {
            leave();
            Ok(updates())
        });
        let kick = log("chat");
        server.on(move |_: tl::functions::messages::DeleteChatUser| {
            kick();
            Ok(updates())
        });
        let history = log("history");
        server.on(move |_: tl::functions::messages::DeleteHistory| {
            history();
            Ok(ack())
        });
        let c = client(&server);
        c.delete_dialog(channel_ref(9)).await.unwrap();
        c.delete_dialog(chat_ref(8)).await.unwrap();
        c.delete_dialog(user_ref(5)).await.unwrap();
        assert_eq!(*seen.lock().unwrap(), ["leave", "chat", "history"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reading_and_clearing_mentions() {
        let server = FakeServer::start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        server.on(move |_: tl::functions::channels::ReadHistory| {
            s.lock().unwrap().push("channel");
            Ok(true)
        });
        let s = seen.clone();
        server.on(move |_: tl::functions::messages::ReadHistory| {
            s.lock().unwrap().push("common");
            Ok(tl::types::messages::AffectedMessages {
                pts: 1,
                pts_count: 1,
            }
            .into())
        });
        let s = seen.clone();
        server.on(move |_: tl::functions::messages::ReadMentions| {
            s.lock().unwrap().push("mentions");
            Ok(ack())
        });
        let c = client(&server);
        c.mark_as_read(channel_ref(9)).await.unwrap();
        c.mark_as_read(user_ref(5)).await.unwrap();
        c.clear_mentions(user_ref(5)).await.unwrap();
        assert_eq!(*seen.lock().unwrap(), ["channel", "common", "mentions"]);
    }
}
