// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt;

use mtg_mtsender::InvocationError;
use mtg_session::types::{PeerId, PeerRef};
use mtg_session::updates::State;
use mtg_tl_types as tl;

use crate::message::InputMessage;
use crate::peer::{Peer, PeerMap, User};
use crate::{Client, utils};

/// Update that bots receive when a user performs an inline query such as `@bot query`.
#[derive(Clone)]
pub struct InlineQuery {
    pub raw: tl::enums::Update,
    pub state: State,
    pub(crate) client: Client,
    pub(crate) peers: PeerMap,
}

/// An inline query answer builder.
pub struct Answer {
    request: tl::functions::messages::SetInlineBotResults,
    client: Client,
}

impl InlineQuery {
    fn update(&self) -> &tl::types::UpdateBotInlineQuery {
        match &self.raw {
            tl::enums::Update::BotInlineQuery(update) => update,
            _ => unreachable!(),
        }
    }

    /// The [`Self::sender`]'s identifier.
    pub fn sender_id(&self) -> PeerId {
        PeerId::user_unchecked(self.update().user_id)
    }

    /// Cached reference to the [`Self::sender`], if it is in cache.
    pub async fn sender_ref(
        &self,
    ) -> Result<Option<PeerRef>, Box<dyn std::error::Error + Send + Sync>> {
        self.peers.get_ref(self.sender_id()).await
    }

    /// User that sent the query, if it is in cache.
    pub fn sender(&self) -> Option<&User> {
        match self.peers.get(self.sender_id()) {
            Some(Peer::User(user)) => Some(user),
            None => None,
            _ => unreachable!(),
        }
    }

    /// The text of the inline query.
    pub fn text(&self) -> &str {
        self.update().query.as_str()
    }

    /// The offset of the inline query.
    pub fn offset(&self) -> &str {
        self.update().offset.as_str()
    }

    /// Answer the inline query.
    // TODO: add example
    pub fn answer<T>(&self, results: impl IntoIterator<Item = T>) -> Answer
    where
        T: Into<tl::enums::InputBotInlineResult>,
    {
        Answer {
            request: tl::functions::messages::SetInlineBotResults {
                gallery: false,
                private: false,
                query_id: self.update().query_id,
                results: results.into_iter().map(Into::into).collect(),
                cache_time: 0,
                next_offset: None,
                switch_pm: None,
                switch_webview: None,
            },
            client: self.client.clone(),
        }
    }

    /// Type of the peer from which the inline query was sent.
    pub fn peer_type(&self) -> Option<tl::enums::InlineQueryPeerType> {
        self.update().peer_type.clone()
    }

    /// Query ID
    pub fn query_id(&self) -> i64 {
        self.update().query_id
    }
}

impl Answer {
    /// If set, the results will show as a gallery (grid).
    pub fn gallery(mut self) -> Self {
        self.request.gallery = true;
        self
    }

    /// If set, the results will be cached by the user's client (private) rather than by Telgram
    /// (not private).
    pub fn private(mut self) -> Self {
        self.request.private = true;
        self
    }

    /// For how long this result should be cached on the user's client. Defaults to 0 for no
    /// cache.
    pub fn cache_time(mut self, cache_time: i32) -> Self {
        self.request.cache_time = cache_time;
        self
    }

    /// The offset the client will send when the user scrolls the results and it repeats the
    /// request.
    pub fn next_offset(mut self, next_offset: impl Into<String>) -> Self {
        self.request.next_offset = Some(next_offset.into());
        self
    }

    /// If set, this text will be shown in the results to allow the user to switch to private
    /// messages.
    pub fn switch_pm(mut self, text: impl Into<String>, start_param: impl Into<String>) -> Self {
        self.request.switch_pm = Some(tl::enums::InlineBotSwitchPm::Pm(
            tl::types::InlineBotSwitchPm {
                text: text.into(),
                start_param: start_param.into(),
            },
        ));
        self
    }

    /// Answers the inline query with the given results.
    pub async fn send(self) -> Result<(), InvocationError> {
        self.client.invoke(&self.request).await?;
        Ok(())
    }
}

/// One of the possible answers to an [`InlineQuery`].
///
/// Article answers let you show an option that when clicked will cause the user to send a text message.
#[derive(Debug)]
pub struct Article {
    pub raw: tl::types::InputBotInlineResult,
}

impl Article {
    /// Creates an inline result with the given title.
    ///
    /// If selected by the user that made the inline query, the input message will be sent by them.
    pub fn new<S: Into<String>, M: Into<InputMessage>>(title: S, input_message: M) -> Self {
        let message = input_message.into();
        Self {
            raw: tl::types::InputBotInlineResult {
                id: utils::generate_random_id().to_string(),
                r#type: "article".into(),
                title: Some(title.into()),
                description: None,
                url: None,
                thumb: None,
                content: None,
                // TODO: also allow other types of messages than text
                send_message: tl::enums::InputBotInlineMessage::Text(
                    tl::types::InputBotInlineMessageText {
                        no_webpage: !message.link_preview,
                        invert_media: message.invert_media,
                        message: message.text,
                        entities: Some(message.entities),
                        reply_markup: message.reply_markup,
                    },
                ),
            },
        }
    }

    /// Unique identifier of the result.
    ///
    /// By default, a random string will be used.
    pub fn id(mut self, result_id: impl Into<String>) -> Self {
        self.raw.id = result_id.into();
        self
    }

    /// Short description of the result.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.raw.description = Some(description.into());
        self
    }

    /// URL of the result.
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.raw.url = Some(url.into());
        self
    }

    /// URL of the thumbnail for the result.
    ///
    /// Must point to a suitable JPEG image.
    pub fn thumb_url(mut self, thumb_url: impl Into<String>) -> Self {
        self.raw.thumb = Some(tl::enums::InputWebDocument::Document(
            tl::types::InputWebDocument {
                url: thumb_url.into(),
                size: 0,
                mime_type: "image/jpeg".into(),
                attributes: vec![],
            },
        ));
        self
    }
}

impl From<Article> for tl::enums::InputBotInlineResult {
    fn from(article: Article) -> Self {
        tl::enums::InputBotInlineResult::Result(article.raw)
    }
}

impl fmt::Debug for InlineQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InlineQuery")
            .field("text", &self.text())
            .field("peer_type", &self.peer_type())
            .field("sender", &self.sender())
            .field("query_id", &self.query_id())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{client, user, user_ref};

    async fn query(c: &Client, user_id: i64) -> InlineQuery {
        InlineQuery {
            raw: tl::types::UpdateBotInlineQuery {
                query_id: 9,
                user_id,
                query: "cats".into(),
                geo: None,
                peer_type: Some(tl::enums::InlineQueryPeerType::Pm),
                offset: "o1".into(),
            }
            .into(),
            state: State {
                date: 0,
                seq: 0,
                message_box: None,
            },
            client: c.clone(),
            peers: c.build_peer_map(vec![user(5)], vec![]).await,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_query_exposes_its_fields_and_sender() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let q = query(&c, 5).await;
        assert_eq!((q.text(), q.offset(), q.query_id()), ("cats", "o1", 9));
        assert!(matches!(
            q.peer_type(),
            Some(tl::enums::InlineQueryPeerType::Pm)
        ));
        assert_eq!(q.sender_id(), PeerId::user_unchecked(5));
        assert!(q.sender().is_some());
        assert_eq!(q.sender_ref().await.unwrap(), Some(user_ref(5)));
        assert!(format!("{q:?}").contains("cats"));
        // somebody the update did not bring along
        let stranger = query(&c, 6).await;
        assert!(stranger.sender().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_answer_sends_every_option() {
        let server = FakeServer::start().await;
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::SetInlineBotResults| {
            log.lock().unwrap().push(req);
            Ok(true)
        });
        let c = client(&server);
        let q = query(&c, 5).await;
        q.answer(Vec::<Article>::new()).send().await.unwrap();
        q.answer([Article::new("t", "body").id("a")])
            .gallery()
            .private()
            .cache_time(60)
            .next_offset("o2")
            .switch_pm("go", "start")
            .send()
            .await
            .unwrap();
        let seen = seen.lock().unwrap();
        assert!(seen[0].results.is_empty() && !seen[0].gallery && !seen[0].private);
        let r = &seen[1];
        assert_eq!((r.query_id, r.cache_time), (9, 60));
        assert!(r.gallery && r.private);
        assert_eq!(r.next_offset.as_deref(), Some("o2"));
        let Some(tl::enums::InlineBotSwitchPm::Pm(pm)) = &r.switch_pm else {
            panic!("no switch_pm")
        };
        assert_eq!((pm.text.as_str(), pm.start_param.as_str()), ("go", "start"));
        assert_eq!(r.results.len(), 1);
    }

    #[test]
    fn an_article_is_built_step_by_step() {
        let plain = Article::new(
            "title",
            InputMessage::new().text("hello").link_preview(false),
        );
        assert_eq!(plain.raw.title.as_deref(), Some("title"));
        assert!(plain.raw.id.parse::<i64>().is_ok());
        assert!(
            plain.raw.description.is_none() && plain.raw.url.is_none() && plain.raw.thumb.is_none()
        );
        let tl::enums::InputBotInlineMessage::Text(m) = &plain.raw.send_message else {
            panic!("not a text message")
        };
        assert_eq!(m.message, "hello");
        assert!(m.no_webpage);

        let full = Article::new("t", "x")
            .id("custom")
            .description("d")
            .url("https://example.com")
            .thumb_url("https://example.com/t.jpg");
        assert_eq!(full.raw.id, "custom");
        assert_eq!(full.raw.description.as_deref(), Some("d"));
        assert_eq!(full.raw.url.as_deref(), Some("https://example.com"));
        let Some(tl::enums::InputWebDocument::Document(thumb)) = &full.raw.thumb else {
            panic!("no thumb")
        };
        assert_eq!(thumb.url, "https://example.com/t.jpg");
        let result: tl::enums::InputBotInlineResult = full.into();
        assert!(matches!(result, tl::enums::InputBotInlineResult::Result(r) if r.id == "custom"));
    }
}
