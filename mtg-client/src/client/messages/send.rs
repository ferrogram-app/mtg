use super::*;

impl Client {
    /// Sends a message to the desired peer.
    ///
    /// This method can also be used to send media such as photos, videos, documents, polls, etc.
    ///
    /// If you want to send a local file as media, you will need to use
    /// [`Client::upload_file`] first.
    ///
    /// Refer to [`InputMessage`] to learn more formatting options, such as using markdown or
    /// adding buttons under your message (if you're logged in as a bot).
    ///
    /// See also: [`Message::respond`], [`Message::reply`].
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// client.send_message(peer, "Boring text message :-(").await?;
    ///
    /// use mtg_client::message::InputMessage;
    ///
    /// client.send_message(peer, InputMessage::new().text("Sneaky message").silent(true)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_message<C: Into<PeerRef>, M: Into<InputMessage>>(
        &self,
        peer: C,
        message: M,
    ) -> Result<Message, InvocationError> {
        let peer = peer.into();
        let message = message.into();
        let random_id = generate_random_id();
        let entities = parse_mention_entities(self, message.entities.clone()).await;
        let updates = if let Some(media) = message.media.clone() {
            self.invoke(&tl::functions::messages::SendMedia {
                silent: message.silent,
                background: message.background,
                clear_draft: message.clear_draft,
                peer: peer.into(),
                reply_to: message.reply_to.map(input_reply_to),
                media,
                message: message.text.clone(),
                random_id,
                reply_markup: message.reply_markup.clone(),
                entities,
                schedule_date: message.schedule_date,
                schedule_repeat_period: None,
                send_as: None,
                noforwards: false,
                update_stickersets_order: false,
                invert_media: message.invert_media,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_floodskip: false,
                allow_paid_stars: None,
                suggested_post: None,
            })
            .await
        } else {
            self.invoke(&tl::functions::messages::SendMessage {
                no_webpage: !message.link_preview,
                silent: message.silent,
                background: message.background,
                clear_draft: message.clear_draft,
                peer: peer.into(),
                reply_to: message.reply_to.map(input_reply_to),
                message: message.text.clone(),
                random_id,
                reply_markup: message.reply_markup.clone(),
                entities,
                schedule_date: message.schedule_date,
                schedule_repeat_period: None,
                send_as: None,
                noforwards: false,
                update_stickersets_order: false,
                invert_media: message.invert_media,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_floodskip: false,
                allow_paid_stars: None,
                suggested_post: None,
                rich_message: message.rich_message.clone(),
            })
            .await
        }?;

        Ok(match updates {
            tl::enums::Updates::UpdateShortSentMessage(updates) => {
                let peer = if peer.id.bare_id().is_none() {
                    // from_raw_short_updates needs the peer ID
                    self.0.session.peer_ref(peer.id).await?.unwrap()
                } else {
                    peer
                };

                Message::from_raw_short_updates(self, updates, message, peer)
            }
            updates => {
                let updates_debug = if enabled!(Level::WARN) {
                    Some(updates.clone())
                } else {
                    None
                };

                match map_random_ids_to_messages(self, peer, &[random_id], updates)
                    .await?
                    .pop()
                    .flatten()
                {
                    Some(message) => message,
                    None => {
                        if let Some(updates) = updates_debug {
                            warn!(
                                "failed to find just-sent message in response updates; please report this:"
                            );
                            warn!("{:#?}", updates);
                        }
                        Message::from_raw(
                            self,
                            tl::enums::Message::Empty(tl::types::MessageEmpty {
                                id: 0,
                                peer_id: Some(peer.id.into()),
                            }),
                            Some(peer),
                            self.empty_peer_map(),
                        )
                    }
                }
            }
        })
    }

    /// Sends a album to the desired peer.
    ///
    /// This method can also be used to send a bunch of media such as photos, videos, documents, polls, etc.
    ///
    /// If you want to send a local file as media, you will need to use
    /// [`Client::upload_file`] first.
    ///
    /// Refer to [`InputMedia`] to learn more formatting options, such as using markdown.
    ///
    /// See also: [`Message::respond_album`], [`Message::reply_album`].
    ///
    /// Note:
    /// - Empty `InputMedia` entries will be filtered out.
    /// - If an album contains more than 10 media items, Telegram will returns RpcError `400 MULTI_MEDIA_TOO_LONG`.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::media::InputMedia;
    ///
    /// client.send_album(peer, vec![InputMedia::new().caption("A album").photo_url("https://example.com/cat.jpg")]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_album<C: Into<PeerRef>>(
        &self,
        peer: C,
        mut medias: Vec<InputMedia>,
    ) -> Result<Vec<Option<Message>>, InvocationError> {
        let peer = peer.into();

        // Upload external files
        use tl::enums::InputMedia as M;

        for (index, media) in medias.iter_mut().enumerate() {
            match media.media {
                Some(M::Empty) | None => {
                    warn!(index, "skipping empty media in Client::send_album");
                    media.media = None;
                }
                Some(
                    M::UploadedPhoto(_)
                    | M::PhotoExternal(_)
                    | M::UploadedDocument(_)
                    | M::DocumentExternal(_),
                ) => {
                    media.media = self
                        .upload_media(peer, media.media.take().unwrap())
                        .await?
                        .and_then(|media| media.to_raw_input_media());
                    if media.media.is_none() {
                        warn!(
                            index,
                            "upload_media produced no media in Client::send_album; skipping"
                        );
                    }
                }
                _ => {}
            }
        }

        medias.retain(|media| media.media.is_some());

        let first_media_reply = match medias.first() {
            Some(media) => media.reply_to,
            None => return Err(InvocationError::Dropped("No media left after filtering.")),
        };

        let len = medias.len();
        let random_ids = generate_random_ids(len);

        let mut multi_media = Vec::with_capacity(len);
        for (input_media, random_id) in medias.into_iter().zip(random_ids.iter()) {
            let entities = parse_mention_entities(self, input_media.entities).await;
            let raw_media = input_media.media.unwrap();
            multi_media.push(tl::enums::InputSingleMedia::Media(
                tl::types::InputSingleMedia {
                    media: raw_media,
                    random_id: *random_id,
                    message: input_media.caption,
                    entities,
                },
            ))
        }

        let updates = self
            .invoke(&tl::functions::messages::SendMultiMedia {
                silent: false,
                background: false,
                clear_draft: false,
                peer: peer.into(),
                reply_to: first_media_reply.map(input_reply_to),
                schedule_date: None,
                multi_media,
                send_as: None,
                noforwards: false,
                update_stickersets_order: false,
                invert_media: false,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_floodskip: false,
                allow_paid_stars: None,
            })
            .await?;

        Ok(map_random_ids_to_messages(self, peer, &random_ids, updates).await?)
    }

    /// Edits an existing message.
    ///
    /// Similar to [`Client::send_message`], advanced formatting can be achieved with the
    /// options offered by [`InputMessage`].
    ///
    /// See also: [`Message::edit`].
    ///
    /// # Note
    ///
    /// An empty message text ("") is filtered out (treated as `None`), to avoid the `RpcError 400: MESSAGE_EMPTY`
    /// error. And makes it possible to edit only the reply markup (buttons) without changing the text.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(
    /// #     peer: mtg_session::types::PeerRef,
    /// #     client: mtg_client::Client,
    /// # ) -> Result<(), Box<dyn std::error::Error>> {
    /// let old_message_id = 123;
    /// client.edit_message(peer, old_message_id, "New text message").await?;
    ///
    /// use mtg_client::message::{Button, InputMessage, ReplyMarkup};
    /// // Add a button to the message without editing it's text.
    /// client
    ///     .edit_message(
    ///         peer,
    ///         old_message_id,
    ///         InputMessage::new().reply_markup(ReplyMarkup::from_buttons_row(&[
    ///             Button::url("url", "https://example.com")
    ///         ]))
    ///     )
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn edit_message<C: Into<PeerRef>, M: Into<InputMessage>>(
        &self,
        peer: C,
        message_id: i32,
        new_message: M,
    ) -> Result<(), InvocationError> {
        let new_message = new_message.into();
        let entities = parse_mention_entities(self, new_message.entities).await;
        self.invoke(&tl::functions::messages::EditMessage {
            no_webpage: !new_message.link_preview,
            invert_media: new_message.invert_media,
            peer: peer.into().into(),
            id: message_id,
            message: Some(new_message.text).filter(|text| !text.is_empty()),
            media: new_message.media,
            reply_markup: new_message.reply_markup,
            entities,
            schedule_date: new_message.schedule_date,
            schedule_repeat_period: None,
            quick_reply_shortcut_id: None,
            rich_message: None,
        })
        .await?;

        Ok(())
    }

    /// Deletes up to 100 messages in a peer.
    ///
    /// <div class="warning">
    ///
    /// When deleting messages from small group peers or private conversations, this
    /// method cannot validate that the provided message IDs actually belong to the input peer due
    /// to the way Telegram's API works. Make sure to pass correct [`Message::id`]'s.
    ///
    /// </div>
    ///
    /// The messages are deleted for both ends.
    ///
    /// The amount of deleted messages is returned (it might be less than the amount of input
    /// message IDs if some of them were already missing). It is not possible to find out which
    /// messages were actually deleted, but if the request succeeds, none of the specified message
    /// IDs will appear in the message history from that point on.
    ///
    /// See also: [`Message::delete`].
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let message_ids = [123, 456, 789];
    ///
    /// // Careful, these messages will be gone after the method succeeds!
    /// client.delete_messages(peer, &message_ids).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_messages<C: Into<PeerRef>>(
        &self,
        peer: C,
        message_ids: &[i32],
    ) -> Result<usize, InvocationError> {
        let peer = peer.into();
        let tl::enums::messages::AffectedMessages::Messages(affected) =
            if peer.id.kind() == PeerKind::Channel {
                self.invoke(&tl::functions::channels::DeleteMessages {
                    channel: peer.into(),
                    id: message_ids.to_vec(),
                })
                .await
            } else {
                self.invoke(&tl::functions::messages::DeleteMessages {
                    revoke: true,
                    id: message_ids.to_vec(),
                })
                .await
            }?;

        Ok(affected.pts_count as usize)
    }

    /// Forwards up to 100 messages from `source` into `destination`.
    ///
    /// For consistency with other methods, the peer upon which this request acts comes first
    /// (destination), and then the source peer.
    ///
    /// Returns the new forwarded messages in a list. Those messages that could not be forwarded
    /// will be `None`. The length of the resulting list is the same as the length of the input
    /// message IDs, and the indices from the list of IDs map to the indices in the result so
    /// you can find which messages were forwarded and which message they became.
    ///
    /// See also: [`Message::forward_to`].
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(destination: mtg_session::types::PeerRef, source: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let message_ids = [123, 456, 789];
    ///
    /// let messages = client.forward_messages(destination, &message_ids, source).await?;
    /// let fwd_count = messages.into_iter().filter(Option::is_some).count();
    /// println!("Forwarded {} out of {} messages!", fwd_count, message_ids.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn forward_messages<C: Into<PeerRef>, S: Into<PeerRef>>(
        &self,
        destination: C,
        message_ids: &[i32],
        source: S,
    ) -> Result<Vec<Option<Message>>, InvocationError> {
        // TODO let user customize more options
        let peer = destination.into();
        let request = tl::functions::messages::ForwardMessages {
            silent: false,
            background: false,
            with_my_score: false,
            drop_author: false,
            drop_media_captions: false,
            from_peer: source.into().into(),
            id: message_ids.to_vec(),
            random_id: generate_random_ids(message_ids.len()),
            to_peer: peer.into(),
            top_msg_id: None,
            reply_to: None,
            schedule_date: None,
            schedule_repeat_period: None,
            send_as: None,
            noforwards: false,
            quick_reply_shortcut: None,
            allow_paid_floodskip: false,
            effect: None,
            video_timestamp: None,
            allow_paid_stars: None,
            suggested_post: None,
            from_ephemeral: false,
        };
        let result = self.invoke(&request).await?;
        Ok(map_random_ids_to_messages(self, peer, &request.random_id, result).await?)
    }
}

#[cfg(test)]
mod tests {
    use mtg_testkit::FakeServer;
    use mtg_tl_types as tl;

    use crate::test_util::{channel_ref, client, message, sent, user_ref};

    fn affected(pts_count: i32) -> tl::enums::messages::AffectedMessages {
        tl::types::messages::AffectedMessages { pts: 1, pts_count }.into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_sent_text_message_is_found_by_its_random_id() {
        let server = FakeServer::start().await;
        let peer = user_ref(5);
        server.on(move |req: tl::functions::messages::SendMessage| {
            assert_eq!(req.message, "hi");
            Ok(sent(req.random_id, message(7, peer, "hi", 2)))
        });
        let sent = client(&server).send_message(peer, "hi").await.unwrap();
        assert_eq!((sent.id(), sent.text()), (7, "hi"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn media_goes_through_send_media() {
        let server = FakeServer::start().await;
        let peer = user_ref(5);
        server.on(move |req: tl::functions::messages::SendMedia| {
            assert_eq!(req.message, "cap");
            Ok(sent(req.random_id, message(8, peer, "cap", 2)))
        });
        let input = crate::message::InputMessage::new()
            .text("cap")
            .photo_url("https://example.com/a.jpg");
        let sent = client(&server).send_message(peer, input).await.unwrap();
        assert_eq!(sent.id(), 8);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_answer_without_our_message_gives_an_empty_one() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::messages::SendMessage| {
            Ok(tl::enums::Updates::Updates(tl::types::Updates {
                updates: Vec::new(),
                users: Vec::new(),
                chats: Vec::new(),
                date: 0,
                seq: 0,
            }))
        });
        let sent = client(&server)
            .send_message(user_ref(5), "hi")
            .await
            .unwrap();
        assert_eq!(sent.id(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_short_sent_message_is_built_from_the_input() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::messages::SendMessage| {
            Ok(tl::enums::Updates::UpdateShortSentMessage(
                tl::types::UpdateShortSentMessage {
                    out: true,
                    id: 11,
                    pts: 1,
                    pts_count: 1,
                    date: 50,
                    media: None,
                    entities: None,
                    ttl_period: None,
                },
            ))
        });
        let sent = client(&server)
            .send_message(user_ref(5), "yo")
            .await
            .unwrap();
        assert_eq!((sent.id(), sent.text()), (11, "yo"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn editing_sends_text_or_nothing_when_empty() {
        let server = FakeServer::start().await;
        let texts = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = texts.clone();
        server.on(move |req: tl::functions::messages::EditMessage| {
            seen.lock().unwrap().push((req.id, req.message));
            Ok(tl::enums::Updates::Combined(tl::types::UpdatesCombined {
                updates: Vec::new(),
                users: Vec::new(),
                chats: Vec::new(),
                date: 0,
                seq_start: 0,
                seq: 0,
            }))
        });
        let c = client(&server);
        c.edit_message(user_ref(5), 3, "new").await.unwrap();
        c.edit_message(user_ref(5), 4, "").await.unwrap();
        assert_eq!(
            *texts.lock().unwrap(),
            vec![(3, Some("new".to_string())), (4, None)]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deleting_picks_the_channel_or_the_common_call() {
        let server = FakeServer::start().await;
        server.on(|req: tl::functions::channels::DeleteMessages| Ok(affected(req.id.len() as i32)));
        server.on(|req: tl::functions::messages::DeleteMessages| {
            assert!(req.revoke);
            Ok(affected(10 + req.id.len() as i32))
        });
        let c = client(&server);
        assert_eq!(c.delete_messages(channel_ref(9), &[1, 2]).await.unwrap(), 2);
        assert_eq!(c.delete_messages(user_ref(5), &[1]).await.unwrap(), 11);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn forwarding_maps_each_random_id_to_its_message() {
        let server = FakeServer::start().await;
        let dest = user_ref(6);
        server.on(move |req: tl::functions::messages::ForwardMessages| {
            assert_eq!(req.id, vec![1]);
            Ok(sent(req.random_id[0], message(21, dest, "fwd", 2)))
        });
        let got = client(&server)
            .forward_messages(dest, &[1], user_ref(5))
            .await
            .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].as_ref().unwrap().id(), 21);
    }
}
