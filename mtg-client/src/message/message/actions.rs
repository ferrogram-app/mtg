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
