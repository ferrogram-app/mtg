// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use mtg_mtsender::InvocationError;
use mtg_session::updates::State;
use mtg_tl_types as tl;

use crate::Client;
use crate::message::{InputMessage, Message};

#[derive(Clone, Debug)]
pub struct GuestChatQuery {
    pub message: Message,
    pub reference_messages: Vec<Message>,
    pub raw: tl::enums::Update,
    pub state: State,
}

impl GuestChatQuery {
    fn update(&self) -> &tl::types::UpdateBotGuestChatQuery {
        match &self.raw {
            tl::enums::Update::BotGuestChatQuery(update) => update,
            _ => unreachable!(),
        }
    }

    /// Query ID
    pub fn query_id(&self) -> i64 {
        self.update().query_id
    }

    pub async fn answer<T>(&self, result: T) -> Result<AnsweredMessage, InvocationError>
    where
        T: Into<tl::enums::InputBotInlineResult>,
    {
        let client = self.message.client.clone();
        let id = client
            .invoke(&tl::functions::messages::SetBotGuestChatResult {
                query_id: self.query_id(),
                result: result.into(),
            })
            .await?;
        Ok(AnsweredMessage { raw: id, client })
    }
}

pub struct AnsweredMessage {
    pub raw: tl::enums::InputBotInlineMessageId,
    pub(crate) client: Client,
}

impl AnsweredMessage {
    pub async fn edit(
        &self,
        input_message: impl Into<InputMessage>,
    ) -> Result<bool, InvocationError> {
        self.client
            .edit_inline_message(self.raw.clone(), input_message.into())
            .await
    }
}

#[cfg(test)]
mod tests {
    use mtg_testkit::FakeServer;

    use super::*;
    use crate::message::InputMessage;
    use crate::test_util::{client, message, user_ref};
    use crate::update::inline_query::Article;

    #[tokio::test(flavor = "multi_thread")]
    async fn answering_gives_a_message_that_can_be_edited() {
        let server = FakeServer::start().await;
        server.on(|req: tl::functions::messages::SetBotGuestChatResult| {
            assert_eq!(req.query_id, 4);
            Ok(tl::enums::InputBotInlineMessageId::from(
                tl::types::InputBotInlineMessageId {
                    dc_id: 2,
                    id: 1,
                    access_hash: 2,
                },
            ))
        });
        server.on(|req: tl::functions::messages::EditInlineBotMessage| {
            assert_eq!(req.message.as_deref(), Some("later"));
            Ok(true)
        });
        let c = client(&server);
        let msg = crate::message::Message::from_raw(
            &c,
            message(1, user_ref(5), "q", 0),
            Some(user_ref(5)),
            c.empty_peer_map(),
        );
        let query = GuestChatQuery {
            message: msg,
            reference_messages: Vec::new(),
            raw: tl::types::UpdateBotGuestChatQuery {
                query_id: 4,
                message: message(1, user_ref(5), "q", 0),
                reference_messages: None,
                qts: 1,
            }
            .into(),
            state: State {
                date: 0,
                seq: 0,
                message_box: None,
            },
        };
        assert_eq!(query.query_id(), 4);
        let answered = query.answer(Article::new("t", "body")).await.unwrap();
        assert!(
            answered
                .edit(InputMessage::new().text("later"))
                .await
                .unwrap()
        );
    }
}
