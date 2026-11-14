use super::*;

impl Client {
    /// Get the latest pin from a peer.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(message) = client.get_pinned_message(peer).await? {
    ///     println!("There is a message pinned: {}", message.text());
    /// } else {
    ///     println!("There are no messages pinned");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_pinned_message<C: Into<PeerRef>>(
        &self,
        peer: C,
    ) -> Result<Option<Message>, InvocationError> {
        let peer = peer.into();
        // TODO return types::Message and print its text in the example
        let id = vec![tl::enums::InputMessage::Pinned];

        let result = if peer.id.kind() == PeerKind::Channel {
            self.invoke(&tl::functions::channels::GetMessages {
                channel: peer.into(),
                id,
            })
            .await
        } else {
            self.invoke(&tl::functions::messages::GetMessages { id })
                .await
        }?;

        let (messages, users, chats) = unpack_messages(result);

        let peers = self.build_peer_map(users, chats).await;
        Ok(messages
            .into_iter()
            .map(|m| Message::from_raw(self, m, Some(peer), peers.handle()))
            .find(|m| m.peer_id() == peer.id))
    }

    /// Pin a message in the peer. This will not notify any users.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let message_id = 123;
    /// client.pin_message(peer, message_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    // TODO return produced Option<service message>
    pub async fn pin_message<C: Into<PeerRef>>(
        &self,
        peer: C,
        message_id: i32,
    ) -> Result<(), InvocationError> {
        self.update_pinned(peer.into(), message_id, true).await
    }

    /// Unpin a message from the peer.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let message_id = 123;
    /// client.unpin_message(peer, message_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn unpin_message<C: Into<PeerRef>>(
        &self,
        peer: C,
        message_id: i32,
    ) -> Result<(), InvocationError> {
        self.update_pinned(peer.into(), message_id, false).await
    }

    async fn update_pinned(
        &self,
        peer: PeerRef,
        id: i32,
        pin: bool,
    ) -> Result<(), InvocationError> {
        self.invoke(&tl::functions::messages::UpdatePinnedMessage {
            silent: true,
            unpin: !pin,
            pm_oneside: false,
            peer: peer.into(),
            id,
        })
        .await
        .map(drop)
    }

    /// Unpin all currently-pinned messages from the peer.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// client.unpin_all_messages(peer).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn unpin_all_messages<C: Into<PeerRef>>(
        &self,
        peer: C,
    ) -> Result<(), InvocationError> {
        self.invoke(&tl::functions::messages::UnpinAllMessages {
            peer: peer.into().into(),
            top_msg_id: None,
            saved_peer_id: None,
        })
        .await?;
        Ok(())
    }
}
