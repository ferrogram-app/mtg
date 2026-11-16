use super::*;

impl Message {
    /// Fetch the message that this message is replying to, or `None` if this message is not a
    /// reply to a previous message.
    ///
    /// Shorthand for `Client::get_reply_to_message`.
    pub async fn get_reply(&self) -> Result<Option<Self>, InvocationError> {
        self.client
            .clone() // TODO don't clone
            .get_reply_to_message(self)
            .await
    }

    /// Respond to this message by sending a new message to the same peer, but without directly
    /// replying to it.
    ///
    /// Shorthand for `Client::send_message`.
    pub async fn respond<M: Into<InputMessage>>(
        &self,
        message: M,
    ) -> Result<Self, InvocationError> {
        self.client
            .send_message(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                message,
            )
            .await
    }

    /// Respond to this message by sending a album in the same peer, but without directly
    /// replying to it.
    ///
    /// Shorthand for `Client::send_album`.
    pub async fn respond_album(
        &self,
        medias: Vec<InputMedia>,
    ) -> Result<Vec<Option<Self>>, InvocationError> {
        self.client
            .send_album(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                medias,
            )
            .await
    }

    /// Directly reply to this message by sending a new message to the same peer that replies to
    /// it. This methods overrides the `reply_to` on the `InputMessage` to point to `self`.
    ///
    /// Shorthand for `Client::send_message`.
    pub async fn reply<M: Into<InputMessage>>(&self, message: M) -> Result<Self, InvocationError> {
        let message = message.into();
        self.client
            .send_message(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                message.reply_to(Some(self.id())),
            )
            .await
    }

    /// Directly reply to this message by sending a album to the same peer that replies to
    /// it. This methods overrides the `reply_to` on the first `InputMedia` to point to `self`.
    ///
    /// Shorthand for `Client::send_album`.
    pub async fn reply_album(
        &self,
        mut medias: Vec<InputMedia>,
    ) -> Result<Vec<Option<Self>>, InvocationError> {
        if let Some(first) = medias.first_mut() {
            first.reply_to = Some(self.id());
        }
        self.client
            .send_album(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                medias,
            )
            .await
    }

    /// Forward this message to another (or the same) peer.
    ///
    /// Shorthand for `Client::forward_messages`. If you need to forward multiple messages
    /// at once, consider using that method instead.
    pub async fn forward_to<C: Into<PeerRef>>(&self, chat: C) -> Result<Self, InvocationError> {
        // TODO return `Message`
        // When forwarding a single message, if it fails, Telegram should respond with RPC error.
        // If it succeeds we will have the single forwarded message present which we can unwrap.
        self.client
            .forward_messages(
                chat,
                &[self.id()],
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
            )
            .await?
            .pop()
            .flatten()
            .ok_or(InvocationError::Dropped(
                "forwarded message is missing from the response",
            ))
    }

    /// Edit this message to change its text or media.
    ///
    /// Shorthand for `Client::edit_message`.
    pub async fn edit<M: Into<InputMessage>>(&self, new_message: M) -> Result<(), InvocationError> {
        self.client
            .edit_message(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                self.id(),
                new_message,
            )
            .await
    }

    /// Delete this message for everyone.
    ///
    /// Shorthand for `Client::delete_messages`. If you need to delete multiple messages
    /// at once, consider using that method instead.
    pub async fn delete(&self) -> Result<(), InvocationError> {
        self.client
            .delete_messages(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                &[self.id()],
            )
            .await
            .map(drop)
    }

    /// Mark this message and all messages above it as read.
    ///
    /// Unlike `Client::mark_as_read`, this method only will mark the conversation as read up to
    /// this message, not the entire conversation.
    pub async fn mark_as_read(&self) -> Result<(), InvocationError> {
        let peer = self
            .peer_ref()
            .await?
            .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?;
        if peer.id.kind() == PeerKind::Channel {
            self.client
                .invoke(&tl::functions::channels::ReadHistory {
                    channel: peer.into(),
                    max_id: self.id(),
                })
                .await
                .map(drop)
        } else {
            self.client
                .invoke(&tl::functions::messages::ReadHistory {
                    peer: peer.into(),
                    max_id: self.id(),
                })
                .await
                .map(drop)
        }
    }

    /// Pin this message in the conversation.
    ///
    /// Shorthand for `Client::pin_message`.
    pub async fn pin(&self) -> Result<(), InvocationError> {
        self.client
            .pin_message(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                self.id(),
            )
            .await
    }

    /// Unpin this message from the conversation.
    ///
    /// Shorthand for `Client::unpin_message`.
    pub async fn unpin(&self) -> Result<(), InvocationError> {
        self.client
            .unpin_message(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                self.id(),
            )
            .await
    }

    /// Refetch this message, mutating all of its properties in-place.
    ///
    /// No changes will be made to the message if it fails to be fetched.
    ///
    /// Shorthand for `Client::get_messages_by_id`.
    pub async fn refetch(&mut self) -> Result<(), InvocationError> {
        // When fetching a single message, if it fails, Telegram should respond with RPC error.
        // A deleted message comes back as `None`.
        let message = self
            .client
            .get_messages_by_id(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                &[self.id()],
            )
            .await?
            .pop()
            .flatten()
            .ok_or(InvocationError::Dropped("message no longer exists"))?;
        *self = message;
        Ok(())
    }

    /// Download the message media in this message if applicable.
    ///
    /// Returns `true` if there was media to download, or `false` otherwise.
    ///
    /// Shorthand for `Client::download_media`.
    #[cfg(feature = "fs")]
    pub async fn download_media<P: AsRef<Path>>(&self, path: P) -> Result<bool, InvocationError> {
        // TODO probably encode failed download in error
        if let Some(media) = self.media() {
            self.client.download_media(&media, path).await.map(|_| true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use mtg_testkit::FakeServer;
    use mtg_tl_types as tl;

    use super::*;
    use crate::test_util::{channel_ref, client, message, sent, user_ref};

    fn msg(c: &Client, peer: PeerRef, id: i32) -> Message {
        Message::from_raw(c, message(id, peer, "x", 0), Some(peer), c.empty_peer_map())
    }

    fn no_updates() -> tl::enums::Updates {
        tl::enums::Updates::Updates(tl::types::Updates {
            updates: Vec::new(),
            users: Vec::new(),
            chats: Vec::new(),
            date: 0,
            seq: 0,
        })
    }

    fn msgs(m: Vec<tl::enums::Message>) -> tl::enums::messages::Messages {
        tl::types::messages::Messages {
            messages: m,
            topics: Vec::new(),
            chats: Vec::new(),
            users: Vec::new(),
        }
        .into()
    }

    fn affected() -> tl::enums::messages::AffectedMessages {
        tl::types::messages::AffectedMessages {
            pts: 1,
            pts_count: 1,
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn respond_and_reply_send_to_the_same_chat() {
        let server = FakeServer::start().await;
        let peer = user_ref(5);
        let replies = Arc::new(Mutex::new(Vec::new()));
        let seen = replies.clone();
        server.on(move |req: tl::functions::messages::SendMessage| {
            let reply = match &req.reply_to {
                Some(tl::enums::InputReplyTo::Message(r)) => Some(r.reply_to_msg_id),
                _ => None,
            };
            seen.lock().unwrap().push((req.message.clone(), reply));
            Ok(sent(req.random_id, message(30, peer, &req.message, 2)))
        });
        let c = client(&server);
        let m = msg(&c, peer, 4);
        assert_eq!(m.respond("a").await.unwrap().id(), 30);
        assert_eq!(m.reply("b").await.unwrap().id(), 30);
        assert_eq!(
            *replies.lock().unwrap(),
            vec![("a".to_string(), None), ("b".to_string(), Some(4))]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn albums_without_media_are_refused_locally() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let m = msg(&c, user_ref(5), 4);
        assert!(matches!(
            m.respond_album(vec![]).await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(
            m.reply_album(vec![]).await,
            Err(InvocationError::Dropped(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn forwarding_returns_the_copy_or_complains() {
        let server = FakeServer::start().await;
        let (from, to) = (user_ref(5), user_ref(6));
        let empty = Arc::new(Mutex::new(false));
        let flag = empty.clone();
        server.on(move |req: tl::functions::messages::ForwardMessages| {
            assert_eq!(req.id, vec![4]);
            if *flag.lock().unwrap() {
                Ok(no_updates())
            } else {
                Ok(sent(req.random_id[0], message(40, to, "fwd", 2)))
            }
        });
        let c = client(&server);
        let m = msg(&c, from, 4);
        assert_eq!(m.forward_to(to).await.unwrap().id(), 40);
        *empty.lock().unwrap() = true;
        assert!(matches!(
            m.forward_to(to).await,
            Err(InvocationError::Dropped(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn edit_delete_pin_and_unpin_hit_the_right_methods() {
        let server = FakeServer::start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::EditMessage| {
            log.lock()
                .unwrap()
                .push(format!("edit {} {:?}", req.id, req.message));
            Ok(no_updates())
        });
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::DeleteMessages| {
            log.lock().unwrap().push(format!("delete {:?}", req.id));
            Ok(affected())
        });
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::UpdatePinnedMessage| {
            log.lock()
                .unwrap()
                .push(format!("pin {} {}", req.id, req.unpin));
            Ok(no_updates())
        });
        let c = client(&server);
        let m = msg(&c, user_ref(5), 4);
        m.edit("new").await.unwrap();
        m.delete().await.unwrap();
        m.pin().await.unwrap();
        m.unpin().await.unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            [
                "edit 4 Some(\"new\")",
                "delete [4]",
                "pin 4 false",
                "pin 4 true"
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn marking_read_uses_the_channel_method_for_channels() {
        let server = FakeServer::start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        server.on(move |req: tl::functions::channels::ReadHistory| {
            log.lock().unwrap().push(format!("channel {}", req.max_id));
            Ok(true)
        });
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::ReadHistory| {
            log.lock().unwrap().push(format!("chat {}", req.max_id));
            Ok(tl::types::messages::AffectedMessages {
                pts: 1,
                pts_count: 1,
            }
            .into())
        });
        let c = client(&server);
        msg(&c, channel_ref(9), 7).mark_as_read().await.unwrap();
        msg(&c, user_ref(5), 8).mark_as_read().await.unwrap();
        assert_eq!(*seen.lock().unwrap(), ["channel 7", "chat 8"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn refetch_replaces_the_message_or_complains() {
        let server = FakeServer::start().await;
        let peer = user_ref(5);
        let gone = Arc::new(Mutex::new(false));
        let flag = gone.clone();
        server.on(move |_: tl::functions::messages::GetMessages| {
            Ok(msgs(if *flag.lock().unwrap() {
                vec![]
            } else {
                vec![message(4, peer, "edited", 0)]
            }))
        });
        let c = client(&server);
        let mut m = msg(&c, peer, 4);
        m.refetch().await.unwrap();
        assert_eq!(m.text(), "edited");
        *gone.lock().unwrap() = true;
        assert!(matches!(
            m.refetch().await,
            Err(InvocationError::Dropped(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_with_no_known_chat_cannot_act() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let m = Message::from_raw(
            &c,
            message(4, user_ref(5), "x", 0),
            None,
            c.empty_peer_map(),
        );
        assert!(matches!(
            m.respond("a").await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(
            m.reply("a").await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(
            m.respond_album(vec![]).await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(
            m.reply_album(vec![]).await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(
            m.forward_to(user_ref(6)).await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(
            m.edit("a").await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(m.delete().await, Err(InvocationError::Dropped(_))));
        assert!(matches!(
            m.mark_as_read().await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(m.pin().await, Err(InvocationError::Dropped(_))));
        assert!(matches!(m.unpin().await, Err(InvocationError::Dropped(_))));
        let mut m = m;
        assert!(matches!(
            m.refetch().await,
            Err(InvocationError::Dropped(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_reply_to_nothing_is_none_and_media_less_messages_download_nothing() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let m = msg(&c, user_ref(5), 4);
        assert!(m.get_reply().await.unwrap().is_none());
        #[cfg(feature = "fs")]
        assert!(
            !m.download_media("/nonexistent/never-written")
                .await
                .unwrap()
        );
    }
}
