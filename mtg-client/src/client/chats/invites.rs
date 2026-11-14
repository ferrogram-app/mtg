use super::*;

impl Client {
    #[cfg(feature = "parse_invite_link")]
    pub fn parse_invite_link(invite_link: &str) -> Option<String> {
        let url_parse_result = url::Url::parse(invite_link);
        if url_parse_result.is_err() {
            return None;
        }

        let url_parse = url_parse_result.unwrap();
        let scheme = url_parse.scheme();
        let path = url_parse.path();
        if url_parse.host_str().is_none() || !["https", "http"].contains(&scheme) {
            return None;
        }
        let host = url_parse.host_str().unwrap();
        let hosts = [
            "t.me",
            "telegram.me",
            "telegram.dog",
            "tg.dev",
            "telegram.me",
            "telesco.pe",
        ];

        if !hosts.contains(&host) {
            return None;
        }
        let paths = path.split("/").skip(1).collect::<Vec<&str>>();

        if paths.len() == 1 {
            if paths[0].starts_with("+") {
                return Some(paths[0].replace("+", ""));
            }
            return None;
        }

        if paths.len() > 1 {
            if paths[0].starts_with("joinchat") {
                return Some(paths[1].to_string());
            }
            if paths[0].starts_with("+") {
                return Some(paths[0].replace("+", ""));
            }
            return None;
        }

        None
    }

    /// Accept an invite link to join the corresponding private chat.
    ///
    /// If the chat is public (has a public username), [`Client::join_chat`](Client::join_chat) should be used instead.
    #[cfg(feature = "parse_invite_link")]
    pub async fn accept_invite_link(
        &self,
        invite_link: &str,
    ) -> Result<Option<Peer>, InvocationError> {
        match Self::parse_invite_link(invite_link) {
            Some(hash) => Ok(
                if let tl::enums::messages::ChatInviteJoinResult::Ok(ok) = self
                    .invoke(&tl::functions::messages::ImportChatInvite { hash })
                    .await?
                {
                    updates_to_chat(self, None, ok.updates)
                } else {
                    None
                },
            ),
            None => Err(InvocationError::Rpc(RpcError {
                code: 400,
                name: "INVITE_HASH_INVALID".to_string(),
                value: None,
                caused_by: None,
            })),
        }
    }

    #[cfg_attr(
        not(feature = "parse_invite_link"),
        allow(rustdoc::broken_intra_doc_links)
    )]
    /// Join a public group or channel.
    ///
    /// A channel is public if it has a username.
    /// To join private chats, [`Client::accept_invite_link`](Client::accept_invite_link) should be used instead.
    pub async fn join_chat<C: Into<PeerRef>>(
        &self,
        chat: C,
    ) -> Result<Option<Peer>, InvocationError> {
        let chat: PeerRef = chat.into();
        let channel = chat.into();
        Ok(
            if let tl::enums::messages::ChatInviteJoinResult::Ok(ok) = self
                .invoke(&tl::functions::channels::JoinChannel { channel })
                .await?
            {
                updates_to_chat(self, chat.id.bare_id(), ok.updates)
            } else {
                None
            },
        )
    }
}
