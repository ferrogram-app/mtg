// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use mtg_mtsender::InvocationError;
use mtg_session::types::PeerRef;
use mtg_tl_types as tl;

use super::messages::parse_mention_entities;
use super::{Client, IterBuffer};
use crate::message::InputMessage;
use crate::utils::generate_random_id;

const MAX_LIMIT: usize = 50;

/// Item returned by [`Client::inline_query`].
pub struct InlineResult {
    client: Client,
    query_id: i64,
    pub raw: tl::enums::BotInlineResult,
}

/// Iterator returned by [`Client::inline_query`].
pub type InlineResultIter = IterBuffer<tl::functions::messages::GetInlineBotResults, InlineResult>;

impl InlineResult {
    /// Send this inline result to the specified peer.
    // TODO return the produced message
    pub async fn send<C: Into<PeerRef>>(&self, peer: C) -> Result<(), InvocationError> {
        self.client
            .invoke(&tl::functions::messages::SendInlineBotResult {
                silent: false,
                background: false,
                clear_draft: false,
                hide_via: false,
                peer: peer.into().into(),
                reply_to: None,
                random_id: generate_random_id(),
                query_id: self.query_id,
                id: self.id().to_string(),
                schedule_date: None,
                send_as: None,
                quick_reply_shortcut: None,
                allow_paid_stars: None,
            })
            .await
            .map(drop)
    }

    /// The ID for this result.
    pub fn id(&self) -> &str {
        use tl::enums::BotInlineResult::*;

        match &self.raw {
            Result(r) => &r.id,
            BotInlineMediaResult(r) => &r.id,
        }
    }

    /// The title for this result, if any.
    pub fn title(&self) -> Option<&String> {
        use tl::enums::BotInlineResult::*;

        match &self.raw {
            Result(r) => r.title.as_ref(),
            BotInlineMediaResult(r) => r.title.as_ref(),
        }
    }
}

impl InlineResultIter {
    fn new(client: &Client, bot: PeerRef, query: &str) -> Self {
        Self::from_request(
            client,
            MAX_LIMIT,
            tl::functions::messages::GetInlineBotResults {
                bot: bot.into(),
                peer: tl::enums::InputPeer::Empty,
                geo_point: None,
                query: query.to_string(),
                offset: String::new(),
            },
        )
    }

    /// Indicate the bot the peer where this inline query will be sent to.
    ///
    /// Some bots use this information to return different results depending on the type of the
    /// peer, and some even "need" it to give useful results.
    pub fn peer<C: Into<PeerRef>>(mut self, peer: C) -> Self {
        self.request.peer = peer.into().into();
        self
    }

    /// Return the next `InlineResult` from the internal buffer, filling the buffer previously if
    /// it's empty.
    ///
    /// Returns `None` if the `limit` is reached or there are no results left.
    pub async fn next(&mut self) -> Result<Option<InlineResult>, InvocationError> {
        if let Some(result) = self.next_raw() {
            return result;
        }

        let tl::enums::messages::BotResults::Results(tl::types::messages::BotResults {
            query_id,
            next_offset,
            results,
            ..
        }) = self.client.invoke(&self.request).await?;

        if let Some(offset) = next_offset {
            self.request.offset = offset;
        } else {
            self.last_chunk = true;
        }

        let client = self.client.clone();
        self.buffer
            .extend(results.into_iter().map(|r| InlineResult {
                client: client.clone(),
                query_id,
                raw: r,
            }));

        Ok(self.pop_item())
    }
}

/// Method implementations related to dealing with bots.
impl Client {
    /// Perform an inline query to the specified bot.
    ///
    /// The query text may be empty.
    ///
    /// Executing the query will fail if the input peer does not actually represent a bot account
    /// supporting inline mode.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(bot: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // This is equivalent to writing `@bot inline query` in a Telegram app.
    /// let mut inline_results = client.inline_query(bot, "inline query");
    ///
    /// while let Some(result) = inline_results.next().await? {
    ///     println!("{}", result.title().unwrap());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn inline_query<C: Into<PeerRef>>(&self, bot: C, query: &str) -> InlineResultIter {
        InlineResultIter::new(self, bot.into(), query)
    }

    /// Edits an inline message sent by a bot.
    ///
    /// Similar to [`Client::edit_message`], advanced formatting can be achieved with the
    /// options offered by [`InputMessage`].
    pub(crate) async fn edit_inline_message(
        &self,
        message_id: tl::enums::InputBotInlineMessageId,
        message: InputMessage,
    ) -> Result<bool, InvocationError> {
        let text = Some(message.text).filter(|text| !text.is_empty());
        let entities = parse_mention_entities(self, message.entities).await;
        if message.media.as_ref().is_some_and(|media| {
            !matches!(
                media,
                tl::enums::InputMedia::PhotoExternal(_)
                    | tl::enums::InputMedia::DocumentExternal(_),
            )
        }) {
            let dc_id = message_id.dc_id();
            self.invoke_in_dc(
                dc_id,
                &tl::functions::messages::EditInlineBotMessage {
                    id: message_id,
                    message: text,
                    media: message.media,
                    entities,
                    no_webpage: !message.link_preview,
                    reply_markup: message.reply_markup,
                    invert_media: message.invert_media,
                    rich_message: None,
                },
            )
            .await
        } else {
            self.invoke(&tl::functions::messages::EditInlineBotMessage {
                id: message_id,
                message: text,
                media: message.media,
                entities,
                no_webpage: !message.link_preview,
                reply_markup: message.reply_markup,
                invert_media: message.invert_media,
                rich_message: None,
            })
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use mtg_testkit::FakeServer;
    use mtg_tl_types as tl;

    use crate::test_util::{client, user_ref};

    fn result(id: &str, title: Option<&str>) -> tl::enums::BotInlineResult {
        tl::enums::BotInlineResult::Result(tl::types::BotInlineResult {
            id: id.into(),
            r#type: "article".into(),
            title: title.map(Into::into),
            description: None,
            url: None,
            thumb: None,
            content: None,
            send_message: tl::enums::BotInlineMessage::Text(tl::types::BotInlineMessageText {
                no_webpage: false,
                invert_media: false,
                message: "m".into(),
                entities: None,
                reply_markup: None,
            }),
        })
    }

    fn media_result(id: &str) -> tl::enums::BotInlineResult {
        let tl::enums::BotInlineResult::Result(r) = result(id, None) else {
            unreachable!()
        };
        tl::enums::BotInlineResult::BotInlineMediaResult(tl::types::BotInlineMediaResult {
            id: id.into(),
            r#type: "photo".into(),
            photo: None,
            document: None,
            title: Some("pic".into()),
            description: None,
            send_message: r.send_message,
        })
    }

    fn results(
        next_offset: Option<&str>,
        results: Vec<tl::enums::BotInlineResult>,
    ) -> tl::enums::messages::BotResults {
        tl::types::messages::BotResults {
            gallery: false,
            query_id: 42,
            next_offset: next_offset.map(Into::into),
            switch_pm: None,
            switch_webview: None,
            results,
            cache_time: 0,
            users: Vec::new(),
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn inline_results_are_paged_by_the_offset_and_can_be_sent() {
        let server = FakeServer::start().await;
        server.on(|req: tl::functions::messages::GetInlineBotResults| {
            assert_eq!(req.query, "cats");
            assert!(matches!(req.peer, tl::enums::InputPeer::User(_)));
            Ok(match req.offset.as_str() {
                "" => results(Some("next"), vec![result("a", Some("A"))]),
                "next" => results(None, vec![media_result("b")]),
                other => panic!("unexpected offset {other}"),
            })
        });
        server.on(|req: tl::functions::messages::SendInlineBotResult| {
            assert_eq!((req.query_id, req.id.as_str()), (42, "a"));
            Ok(tl::enums::Updates::TooLong)
        });
        let c = client(&server);
        let mut it = c.inline_query(user_ref(7), "cats").peer(user_ref(5));
        let first = it.next().await.unwrap().unwrap();
        assert_eq!(
            (first.id(), first.title().map(String::as_str)),
            ("a", Some("A"))
        );
        first.send(user_ref(5)).await.unwrap();
        let second = it.next().await.unwrap().unwrap();
        assert_eq!(
            (second.id(), second.title().map(String::as_str)),
            ("b", Some("pic"))
        );
        assert!(it.next().await.unwrap().is_none());
    }

    fn inline_id() -> tl::enums::InputBotInlineMessageId {
        tl::types::InputBotInlineMessageId {
            dc_id: 2,
            id: 1,
            access_hash: 2,
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn editing_an_inline_message_goes_to_its_own_dc_for_uploaded_media() {
        let server = FakeServer::start().await;
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let s = seen.clone();
        server.on(move |req: tl::functions::messages::EditInlineBotMessage| {
            s.lock().unwrap().push((req.message, req.media.is_some()));
            Ok(true)
        });
        let c = client(&server);
        // plain text: the home DC
        assert!(
            c.edit_inline_message(inline_id(), "new".into())
                .await
                .unwrap()
        );
        // external media is also sent from the home DC, without any text
        let ext = crate::message::InputMessage::new().photo_url("https://example.com/a.jpg");
        assert!(c.edit_inline_message(inline_id(), ext).await.unwrap());
        // other media goes to the DC of the message
        let doc = crate::message::InputMessage::new().media(tl::types::InputMediaGeoPoint {
            geo_point: tl::enums::InputGeoPoint::Empty,
        });
        assert!(c.edit_inline_message(inline_id(), doc).await.unwrap());
        assert_eq!(
            *seen.lock().unwrap(),
            [(Some("new".to_string()), false), (None, true), (None, true)]
        );
    }
}
