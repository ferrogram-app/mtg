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

#[cfg(test)]
mod tests {
    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{channel, channel_ref, chat, client};

    fn joined(chats: Vec<tl::enums::Chat>) -> tl::enums::messages::ChatInviteJoinResult {
        tl::types::messages::ChatInviteJoinResultOk {
            updates: tl::enums::Updates::Updates(tl::types::Updates {
                updates: Vec::new(),
                users: Vec::new(),
                chats,
                date: 0,
                seq: 0,
            }),
        }
        .into()
    }

    fn web_view() -> tl::enums::messages::ChatInviteJoinResult {
        tl::types::messages::ChatInviteJoinResultWebView {
            bot_id: 1,
            query_id: 2,
            users: Vec::new(),
        }
        .into()
    }

    #[cfg(feature = "parse_invite_link")]
    #[test]
    fn invite_links_yield_their_hash() {
        let parse = Client::parse_invite_link;
        assert_eq!(parse("https://t.me/+abc"), Some("abc".into()));
        assert_eq!(parse("http://telegram.me/joinchat/xyz"), Some("xyz".into()));
        assert_eq!(parse("https://t.me/+abc/more"), Some("abc".into()));
        assert_eq!(parse("https://telesco.pe/+q"), Some("q".into()));
        // a plain username link, an unknown host, a bad scheme or no link at all
        assert_eq!(parse("https://t.me/durov"), None);
        assert_eq!(parse("https://t.me/durov/12"), None);
        assert_eq!(parse("https://t.me/"), None);
        assert_eq!(parse("https://example.com/+abc"), None);
        assert_eq!(parse("ftp://t.me/+abc"), None);
        assert_eq!(parse("tg:join?invite=abc"), None);
        assert_eq!(parse("not a url"), None);
    }

    #[cfg(feature = "parse_invite_link")]
    #[tokio::test(flavor = "multi_thread")]
    async fn accepting_an_invite_returns_the_joined_chat() {
        let server = FakeServer::start().await;
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        let mode = std::sync::Arc::new(std::sync::Mutex::new(0));
        let m = mode.clone();
        server.on(move |req: tl::functions::messages::ImportChatInvite| {
            log.lock().unwrap().push(req.hash);
            Ok(match *m.lock().unwrap() {
                0 => joined(vec![chat(3)]),
                1 => joined(vec![]),
                _ => web_view(),
            })
        });
        let c = client(&server);
        let peer = c
            .accept_invite_link("https://t.me/+hash1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(peer.id().bare_id(), Some(3));
        *mode.lock().unwrap() = 1;
        assert!(
            c.accept_invite_link("https://t.me/+hash1")
                .await
                .unwrap()
                .is_none()
        );
        *mode.lock().unwrap() = 2;
        assert!(
            c.accept_invite_link("https://t.me/+hash1")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(*seen.lock().unwrap(), ["hash1"; 3]);

        let bad = c.accept_invite_link("https://example.com/x").await;
        assert!(matches!(bad, Err(InvocationError::Rpc(e)) if e.name == "INVITE_HASH_INVALID"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn joining_a_channel_returns_that_channel() {
        let server = FakeServer::start().await;
        let mode = std::sync::Arc::new(std::sync::Mutex::new(0));
        let m = mode.clone();
        server.on(move |_: tl::functions::channels::JoinChannel| {
            Ok(match *m.lock().unwrap() {
                0 => joined(vec![channel(10), channel(9)]),
                1 => joined(vec![channel(10)]),
                _ => web_view(),
            })
        });
        let c = client(&server);
        let peer = c.join_chat(channel_ref(9)).await.unwrap().unwrap();
        assert_eq!(peer.id().bare_id(), Some(9));
        *mode.lock().unwrap() = 1;
        assert!(c.join_chat(channel_ref(9)).await.unwrap().is_none());
        *mode.lock().unwrap() = 2;
        assert!(c.join_chat(channel_ref(9)).await.unwrap().is_none());
    }
}
