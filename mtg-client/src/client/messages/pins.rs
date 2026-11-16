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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{channel_ref, client, message, user_ref};

    fn msgs(m: Vec<tl::enums::Message>) -> tl::enums::messages::Messages {
        tl::types::messages::Messages {
            messages: m,
            topics: Vec::new(),
            chats: Vec::new(),
            users: Vec::new(),
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_pinned_message_of_a_private_chat() {
        let server = FakeServer::start().await;
        let peer = user_ref(5);
        server.on(move |req: tl::functions::messages::GetMessages| {
            assert!(matches!(req.id[..], [tl::enums::InputMessage::Pinned]));
            // the first message belongs to another chat and must be skipped
            Ok(msgs(vec![
                message(1, user_ref(6), "no", 0),
                message(2, peer, "pinned", 0),
            ]))
        });
        let m = client(&server)
            .get_pinned_message(peer)
            .await
            .unwrap()
            .unwrap();
        assert_eq!((m.id(), m.text()), (2, "pinned"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_channel_asks_the_channel_method_and_may_have_no_pin() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::channels::GetMessages| Ok(msgs(vec![])));
        let got = client(&server)
            .get_pinned_message(channel_ref(9))
            .await
            .unwrap();
        assert!(got.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pin_unpin_and_unpin_all_send_the_right_flags() {
        let server = FakeServer::start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        server.on(move |req: tl::functions::messages::UpdatePinnedMessage| {
            s.lock().unwrap().push((req.id, req.unpin, req.silent));
            Ok(tl::enums::Updates::TooLong)
        });
        server.on(|_: tl::functions::messages::UnpinAllMessages| {
            Ok(tl::enums::messages::AffectedHistory::from(
                tl::types::messages::AffectedHistory {
                    pts: 1,
                    pts_count: 1,
                    offset: 0,
                },
            ))
        });
        let c = client(&server);
        let peer = user_ref(5);
        c.pin_message(peer, 4).await.unwrap();
        c.unpin_message(peer, 4).await.unwrap();
        c.unpin_all_messages(peer).await.unwrap();
        assert_eq!(*seen.lock().unwrap(), [(4, false, true), (4, true, true)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_server_error_is_returned() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::messages::UpdatePinnedMessage| {
            Err::<tl::enums::Updates, _>((400, "MESSAGE_ID_INVALID".into()))
        });
        let err = client(&server)
            .pin_message(user_ref(5), 1)
            .await
            .unwrap_err();
        assert!(err.is("MESSAGE_ID_INVALID"));
    }
}
