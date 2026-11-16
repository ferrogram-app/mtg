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

use crate::Client;
use crate::message::InputMessage;
use crate::peer::{Peer, PeerMap, User};

/// Update that bots receive when a user chooses an inline query result.
///
/// To receive this update, "Inline Feedback" under "Bot Settings" must be enabled via [@BotFather](https://t.me/BotFather).
#[derive(Clone)]
pub struct InlineSend {
    pub raw: tl::enums::Update,
    pub state: State,
    pub(crate) client: Client,
    pub(crate) peers: PeerMap,
}

impl InlineSend {
    fn update(&self) -> &tl::types::UpdateBotInlineSend {
        match &self.raw {
            tl::enums::Update::BotInlineSend(update) => update,
            _ => unreachable!(),
        }
    }

    /// The query that was used to obtain the result.
    pub fn text(&self) -> &str {
        self.update().query.as_str()
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

    /// The user that chose the result, if it is in cache.
    pub fn sender(&self) -> Option<&User> {
        match self.peers.get(self.sender_id()) {
            Some(Peer::User(user)) => Some(user),
            None => None,
            _ => unreachable!(),
        }
    }

    /// The unique identifier for the result that was chosen
    pub fn result_id(&self) -> &str {
        self.update().id.as_str()
    }

    /// Identifier of sent inline message.
    /// Available only if there is an inline keyboard attached.
    /// Will be also received in callback queries and can be used to edit the message.
    pub fn message_id(&self) -> Option<tl::enums::InputBotInlineMessageId> {
        self.update().msg_id.clone()
    }

    /// Edits this inline message.
    ///
    /// **This method will return Ok(None) if message id is None (e.g. if an inline keyboard is not attached)**
    pub async fn edit_message(
        &self,
        input_message: impl Into<InputMessage>,
    ) -> Result<Option<bool>, InvocationError> {
        let msg_id = match self.update().msg_id.clone() {
            None => return Ok(None),
            Some(msg_id) => msg_id,
        };

        Ok(Some(
            self.client
                .edit_inline_message(msg_id, input_message.into())
                .await?,
        ))
    }
}

impl fmt::Debug for InlineSend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InlineSend")
            .field("text", &self.text())
            .field("sender", &self.sender())
            .field("result_id", &self.result_id())
            .field("message_id", &self.message_id())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{client, user, user_ref};

    async fn send(
        c: &Client,
        user_id: i64,
        msg_id: Option<tl::enums::InputBotInlineMessageId>,
    ) -> InlineSend {
        InlineSend {
            raw: tl::types::UpdateBotInlineSend {
                user_id,
                query: "cats".into(),
                geo: None,
                id: "r1".into(),
                msg_id,
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

    fn inline_id() -> tl::enums::InputBotInlineMessageId {
        tl::types::InputBotInlineMessageId {
            dc_id: 2,
            id: 1,
            access_hash: 2,
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_chosen_result_exposes_its_fields() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let s = send(&c, 5, Some(inline_id())).await;
        assert_eq!((s.text(), s.result_id()), ("cats", "r1"));
        assert_eq!(s.sender_id(), PeerId::user_unchecked(5));
        assert!(s.sender().is_some());
        assert_eq!(s.sender_ref().await.unwrap(), Some(user_ref(5)));
        assert!(s.message_id().is_some());
        assert!(format!("{s:?}").contains("cats"));
        assert!(send(&c, 6, None).await.sender().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn editing_needs_an_inline_message_id() {
        let server = FakeServer::start().await;
        server.on(|req: tl::functions::messages::EditInlineBotMessage| {
            assert_eq!(req.message.as_deref(), Some("new"));
            Ok(true)
        });
        let c = client(&server);
        assert_eq!(
            send(&c, 5, Some(inline_id()))
                .await
                .edit_message("new")
                .await
                .unwrap(),
            Some(true)
        );
        assert_eq!(
            send(&c, 5, None).await.edit_message("new").await.unwrap(),
            None
        );
    }
}
