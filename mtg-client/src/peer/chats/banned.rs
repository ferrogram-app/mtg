use super::*;

type BannedFutGen<F> = fn(BannedRightsBuilderInner) -> F;

pub(crate) struct BannedRightsBuilderInner {
    client: Client,
    peer_ref: PeerRef,
    peer: tl::enums::InputPeer,
    user: tl::enums::InputUser,
    rights: tl::types::ChatBannedRights,
}

impl BannedRightsBuilderInner {
    // Perform the call.
    pub(crate) async fn invoke(self) -> Result<(), InvocationError> {
        if self.peer_ref.id.kind() == PeerKind::Channel {
            self.client
                .invoke(&tl::functions::channels::EditBanned {
                    channel: self.peer_ref.into(),
                    participant: self.peer.clone(),
                    banned_rights: tl::enums::ChatBannedRights::Rights(self.rights.clone()),
                })
                .await
                .map(drop)
        } else if self.peer_ref.id.kind() == PeerKind::Chat {
            if self.rights.view_messages {
                self.client
                    .invoke(&tl::functions::messages::DeleteChatUser {
                        chat_id: self.peer_ref.into(),
                        user_id: self.user.clone(),
                        revoke_history: false,
                    })
                    .await
                    .map(drop)
            } else {
                Err(InvocationError::Rpc(RpcError {
                    code: 400,
                    name: "CHAT_INVALID".to_string(),
                    value: None,
                    caused_by: None,
                }))
            }
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
    /// Builder for editing the rights of a non-admin user in a specific chat.
    ///
    /// Certain groups (small group chats) only allow banning (disallow `view_messages`). Trying to
    /// disallow other permissions in these groups will fail.
    ///
    /// Use [`Client::set_banned_rights`] to retrieve an instance of this type.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct BannedRightsBuilder<F: Future<Output = BuilderRes>> {
        inner: Option<BannedRightsBuilderInner>,
        fut_gen: BannedFutGen<F>,
        #[pin]
        fut: Option<F>,
        _phantom: PhantomPinned
    }
}

impl<F: Future<Output = BuilderRes>> Future for BannedRightsBuilder<F> {
    type Output = BuilderRes;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<BuilderRes> {
        let mut s = self.project();
        if s.fut.is_none() {
            // unwrap safety: s.inner is None only when s.fut is some
            // or s.fut is resolved
            s.fut.set(Some((s.fut_gen)(s.inner.take().unwrap())))
        }

        s.fut.as_pin_mut().unwrap().poll(cx)
    }
}

impl<F: Future<Output = BuilderRes>> BannedRightsBuilder<F> {
    pub(crate) fn new(
        client: Client,
        peer: PeerRef,
        user: PeerRef,
        fut_gen: BannedFutGen<F>,
    ) -> Self {
        Self {
            inner: Some(BannedRightsBuilderInner {
                client,
                peer_ref: peer,
                peer: user.into(),
                user: user.into(),
                rights: tl::types::ChatBannedRights {
                    view_messages: false,
                    send_messages: false,
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
                    until_date: 0,
                },
            }),
            fut_gen,
            fut: None,
            _phantom: PhantomPinned,
        }
    }

    fn inner_mut(&mut self) -> &mut BannedRightsBuilderInner {
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

            if let tl::enums::ChannelParticipant::Banned(u) = user.participant {
                s.rights = u.banned_rights.into();
            }
        }

        Ok(self)
    }

    /// Whether the user is able to view messages or not. Forbidding someone from viewing messages
    /// effectively bans (kicks) them.
    pub fn view_messages(mut self, val: bool) -> Self {
        // `true` indicates "take away", but in the builder it makes more sense that `false` means
        // "they won't have this permission". All methods perform this negation for that reason.
        self.inner_mut().rights.view_messages = !val;
        self
    }

    /// Whether the user is able to send messages or not. The user will remain in the chat, and
    /// can still read the conversation.
    pub fn send_messages(mut self, val: bool) -> Self {
        self.inner_mut().rights.send_messages = !val;
        self
    }

    /// Whether the user is able to send any form of media or not, such as photos or voice notes.
    pub fn send_media(mut self, val: bool) -> Self {
        self.inner_mut().rights.send_media = !val;
        self
    }

    /// Whether the user is able to send stickers or not.
    pub fn send_stickers(mut self, val: bool) -> Self {
        self.inner_mut().rights.send_stickers = !val;
        self
    }

    /// Whether the user is able to send animated gifs or not.
    pub fn send_gifs(mut self, val: bool) -> Self {
        self.inner_mut().rights.send_gifs = !val;
        self
    }

    /// Whether the user is able to send games or not.
    pub fn send_games(mut self, val: bool) -> Self {
        self.inner_mut().rights.send_games = !val;
        self
    }

    /// Whether the user is able to use inline bots or not.
    pub fn send_inline(mut self, val: bool) -> Self {
        self.inner_mut().rights.send_inline = !val;
        self
    }

    /// Whether the user is able to enable the link preview in the messages they send.
    ///
    /// Note that the user will still be able to send messages with links if this permission is
    /// taken away from the user, but these links won't display a link preview.
    pub fn embed_link_previews(mut self, val: bool) -> Self {
        self.inner_mut().rights.embed_links = !val;
        self
    }

    /// Whether the user is able to send polls or not.
    pub fn send_polls(mut self, val: bool) -> Self {
        self.inner_mut().rights.send_polls = !val;
        self
    }

    /// Whether the user is able to change information about the chat such as group description or
    /// not.
    pub fn change_info(mut self, val: bool) -> Self {
        self.inner_mut().rights.change_info = !val;
        self
    }

    /// Whether the user is able to invite other users or not.
    pub fn invite_users(mut self, val: bool) -> Self {
        self.inner_mut().rights.invite_users = !val;
        self
    }

    /// Whether the user is able to pin messages or not.
    pub fn pin_messages(mut self, val: bool) -> Self {
        self.inner_mut().rights.pin_messages = !val;
        self
    }

    /// Apply the restrictions until the given epoch time.
    ///
    /// Note that this is absolute time (i.e current time is not added).
    ///
    /// By default, the restriction is permanent.
    pub fn until(mut self, val: i32) -> Self {
        // TODO this should take a date, not int
        self.inner_mut().rights.until_date = val;
        self
    }

    /// Apply the restriction for a given duration.
    pub fn duration(mut self, val: Duration) -> Self {
        // TODO this should account for the server time instead (via sender's offset)
        self.inner_mut().rights.until_date = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is before epoch")
            .as_secs() as i32
            + val.as_secs() as i32;

        self
    }
}
