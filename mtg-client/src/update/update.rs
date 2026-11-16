// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use mtg_session::updates::State;
use mtg_tl_types as tl;

use super::{
    CallbackQuery, GuestChatQuery, InlineQuery, InlineSend, Message, MessageDeletion, Raw,
};
use crate::{Client, peer::PeerMap, utils};

/// An update that indicates some event, which may be of interest to the logged-in account, has occured.
///
/// Only updates pertaining to messages are guaranteed to be delivered, and can be fetched on-demand if
/// they occured while the client was offline by enabling [`catch_up`](mtg_mtsender::UpdatesConfiguration::catch_up).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Update {
    /// Occurs whenever a new text message or a message with media is produced.
    NewMessage(Message),
    /// Occurs when a message is updated.
    MessageEdited(Message),
    /// Occurs when a message is deleted.
    MessageDeleted(MessageDeletion),
    /// Occurs when Telegram calls back into your bot because an inline callback
    /// button was pressed.
    CallbackQuery(CallbackQuery),
    /// Occurs whenever you sign in as a bot and a user sends an inline query
    /// such as `@bot query`.
    InlineQuery(InlineQuery),
    /// Represents an update of user choosing the result of inline query and sending it to their peer partner.
    InlineSend(InlineSend),
    /// Occurs whenever you sign in as a bot and a user sends a message mention you in a public group
    /// such as `hi @bot`.
    GuestChatQuery(GuestChatQuery),
    /// Raw events are not actual events.
    /// Instead, they are the raw Update object that Telegram sends. You
    /// normally shouldn’t need these.
    ///
    /// <div class="warning">The library can "move" raw update types into
    /// a separate update wrapper variant during minor version bumps, so use
    /// this only as a workaround when such variant is not available yet.</div>
    Raw(Raw),
}

impl Update {
    /// Create new friendly to use `Update`` from its raw version and peer map.
    pub fn from_raw(
        client: &Client,
        update: tl::enums::Update,
        state: State,
        peers: PeerMap,
    ) -> Self {
        match &update {
            // NewMessage
            tl::enums::Update::NewMessage(raw) => {
                if utils::peer_from_message(&raw.message).is_none() {
                    return Self::Raw(Raw { raw: update, state });
                }

                Self::NewMessage(Message {
                    msg: crate::message::Message::from_raw(
                        client,
                        raw.message.clone(),
                        None,
                        peers,
                    ),
                    raw: update,
                    state,
                })
            }

            tl::enums::Update::NewChannelMessage(raw) => {
                if utils::peer_from_message(&raw.message).is_none() {
                    return Self::Raw(Raw { raw: update, state });
                }

                Self::NewMessage(Message {
                    msg: crate::message::Message::from_raw(
                        client,
                        raw.message.clone(),
                        None,
                        peers,
                    ),
                    raw: update,
                    state,
                })
            }

            // MessageEdited
            tl::enums::Update::EditMessage(raw) => {
                if utils::peer_from_message(&raw.message).is_none() {
                    return Self::Raw(Raw { raw: update, state });
                }

                Self::MessageEdited(Message {
                    msg: crate::message::Message::from_raw(
                        client,
                        raw.message.clone(),
                        None,
                        peers,
                    ),
                    raw: update,
                    state,
                })
            }
            tl::enums::Update::EditChannelMessage(raw) => Self::MessageEdited(Message {
                msg: crate::message::Message::from_raw(client, raw.message.clone(), None, peers),
                raw: update,
                state,
            }),

            // MessageDeleted
            tl::enums::Update::DeleteMessages(_) => {
                Self::MessageDeleted(MessageDeletion { raw: update, state })
            }
            tl::enums::Update::DeleteChannelMessages(_) => {
                Self::MessageDeleted(MessageDeletion { raw: update, state })
            }

            // CallbackQuery
            tl::enums::Update::BotCallbackQuery(_) => Self::CallbackQuery(CallbackQuery {
                raw: update,
                state,
                client: client.clone(),
                peers,
            }),

            // InlineCallbackQuery
            tl::enums::Update::InlineBotCallbackQuery(_) => Self::CallbackQuery(CallbackQuery {
                raw: update,
                state,
                client: client.clone(),
                peers,
            }),

            // InlineQuery
            tl::enums::Update::BotInlineQuery(_) => Self::InlineQuery(InlineQuery {
                raw: update,
                state,
                client: client.clone(),
                peers,
            }),

            // InlineSend
            tl::enums::Update::BotInlineSend(_) => Self::InlineSend(InlineSend {
                raw: update,
                state,
                client: client.clone(),
                peers,
            }),

            // GuestChatQuery
            tl::enums::Update::BotGuestChatQuery(raw) => Self::GuestChatQuery(GuestChatQuery {
                message: crate::message::Message::from_raw(
                    client,
                    raw.message.clone(),
                    None,
                    peers.clone(),
                ),
                reference_messages: raw
                    .reference_messages
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|msg| crate::message::Message::from_raw(client, msg, None, peers.clone()))
                    .collect(),
                raw: update,
                state,
            }),

            // Raw
            _ => Self::Raw(Raw { raw: update, state }),
        }
    }

    /// Update state.
    pub fn state(&self) -> &State {
        match self {
            Update::NewMessage(update) => &update.state,
            Update::MessageEdited(update) => &update.state,
            Update::MessageDeleted(update) => &update.state,
            Update::CallbackQuery(update) => &update.state,
            Update::InlineQuery(update) => &update.state,
            Update::InlineSend(update) => &update.state,
            Update::GuestChatQuery(update) => &update.state,
            Update::Raw(update) => &update.state,
        }
    }

    /// Raw update, as sent by Telegram.
    ///
    /// Only contains the individual [`Update`](tl::enums::Update),
    /// not the [`Updates`](tl::enums::Updates) container from which it may have come from.
    pub fn raw(&self) -> &tl::enums::Update {
        match self {
            Update::NewMessage(update) => &update.raw,
            Update::MessageEdited(update) => &update.raw,
            Update::MessageDeleted(update) => &update.raw,
            Update::CallbackQuery(update) => &update.raw,
            Update::InlineQuery(update) => &update.raw,
            Update::InlineSend(update) => &update.raw,
            Update::GuestChatQuery(update) => &update.raw,
            Update::Raw(update) => &update.raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{client, message, user_ref};

    fn state() -> State {
        State {
            date: 3,
            seq: 4,
            message_box: None,
        }
    }

    fn msg(id: i32) -> tl::enums::Message {
        message(id, user_ref(5), "hi", 0)
    }

    fn no_peer() -> tl::enums::Message {
        tl::types::MessageEmpty {
            id: 1,
            peer_id: None,
        }
        .into()
    }

    async fn convert(raw: tl::enums::Update) -> Update {
        let server = FakeServer::start().await;
        let c = client(&server);
        Update::from_raw(&c, raw, state(), c.empty_peer_map())
    }

    fn new_message(m: tl::enums::Message) -> tl::enums::Update {
        tl::types::UpdateNewMessage {
            message: m,
            pts: 1,
            pts_count: 1,
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn message_updates_become_typed_updates() {
        let new = convert(new_message(msg(1))).await;
        assert!(matches!(&new, Update::NewMessage(m) if m.id() == 1));
        let channel = convert(
            tl::types::UpdateNewChannelMessage {
                message: msg(2),
                pts: 1,
                pts_count: 1,
            }
            .into(),
        )
        .await;
        assert!(matches!(&channel, Update::NewMessage(m) if m.id() == 2));
        let edit = convert(
            tl::types::UpdateEditMessage {
                message: msg(3),
                pts: 1,
                pts_count: 1,
            }
            .into(),
        )
        .await;
        assert!(matches!(&edit, Update::MessageEdited(m) if m.id() == 3));
        let edit_channel = convert(
            tl::types::UpdateEditChannelMessage {
                message: msg(4),
                pts: 1,
                pts_count: 1,
            }
            .into(),
        )
        .await;
        assert!(matches!(&edit_channel, Update::MessageEdited(m) if m.id() == 4));
        for u in [&new, &channel, &edit, &edit_channel] {
            assert_eq!(u.state().date, 3);
            assert!(matches!(
                u.raw(),
                tl::enums::Update::NewMessage(_)
                    | tl::enums::Update::NewChannelMessage(_)
                    | tl::enums::Update::EditMessage(_)
                    | tl::enums::Update::EditChannelMessage(_)
            ));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn messages_without_a_chat_stay_raw() {
        let raw = convert(new_message(no_peer())).await;
        assert!(matches!(raw, Update::Raw(_)));
        let raw = convert(
            tl::types::UpdateNewChannelMessage {
                message: no_peer(),
                pts: 1,
                pts_count: 1,
            }
            .into(),
        )
        .await;
        assert!(matches!(raw, Update::Raw(_)));
        let raw = convert(
            tl::types::UpdateEditMessage {
                message: no_peer(),
                pts: 1,
                pts_count: 1,
            }
            .into(),
        )
        .await;
        assert!(matches!(raw, Update::Raw(_)));
        // unlike the others, an edited channel message is accepted as it is
        let edited = convert(
            tl::types::UpdateEditChannelMessage {
                message: no_peer(),
                pts: 1,
                pts_count: 1,
            }
            .into(),
        )
        .await;
        assert!(matches!(edited, Update::MessageEdited(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn other_updates_keep_their_kind() {
        let deleted = convert(
            tl::types::UpdateDeleteMessages {
                messages: vec![1, 2],
                pts: 1,
                pts_count: 2,
            }
            .into(),
        )
        .await;
        assert!(matches!(&deleted, Update::MessageDeleted(d) if d.messages() == [1, 2]));
        let deleted = convert(
            tl::types::UpdateDeleteChannelMessages {
                channel_id: 9,
                messages: vec![3],
                pts: 1,
                pts_count: 1,
            }
            .into(),
        )
        .await;
        assert!(matches!(&deleted, Update::MessageDeleted(d) if d.channel_id() == Some(9)));

        let press = convert(
            tl::types::UpdateBotCallbackQuery {
                query_id: 1,
                user_id: 5,
                peer: tl::types::PeerUser { user_id: 5 }.into(),
                msg_id: 2,
                chat_instance: 0,
                data: None,
                game_short_name: None,
            }
            .into(),
        )
        .await;
        assert!(matches!(press, Update::CallbackQuery(_)));
        let inline_press = convert(
            tl::types::UpdateInlineBotCallbackQuery {
                query_id: 1,
                user_id: 5,
                msg_id: tl::types::InputBotInlineMessageId {
                    dc_id: 2,
                    id: 1,
                    access_hash: 2,
                }
                .into(),
                chat_instance: 0,
                data: None,
                game_short_name: None,
            }
            .into(),
        )
        .await;
        assert!(matches!(inline_press, Update::CallbackQuery(_)));
        let query = convert(
            tl::types::UpdateBotInlineQuery {
                query_id: 1,
                user_id: 5,
                query: String::new(),
                geo: None,
                peer_type: None,
                offset: String::new(),
            }
            .into(),
        )
        .await;
        assert!(matches!(query, Update::InlineQuery(_)));
        let send = convert(
            tl::types::UpdateBotInlineSend {
                user_id: 5,
                query: String::new(),
                geo: None,
                id: "r".into(),
                msg_id: None,
            }
            .into(),
        )
        .await;
        assert!(matches!(send, Update::InlineSend(_)));
        let guest = convert(
            tl::types::UpdateBotGuestChatQuery {
                query_id: 1,
                message: msg(1),
                reference_messages: Some(vec![msg(2), msg(3)]),
                qts: 1,
            }
            .into(),
        )
        .await;
        let Update::GuestChatQuery(g) = &guest else {
            panic!("not a guest chat query")
        };
        assert_eq!((g.message.id(), g.reference_messages.len()), (1, 2));
        let bare = convert(
            tl::types::UpdateBotGuestChatQuery {
                query_id: 1,
                message: msg(1),
                reference_messages: None,
                qts: 1,
            }
            .into(),
        )
        .await;
        assert!(matches!(&bare, Update::GuestChatQuery(g) if g.reference_messages.is_empty()));

        let other = convert(tl::enums::Update::Config).await;
        assert!(matches!(other.raw(), tl::enums::Update::Config));
        assert!(matches!(other, Update::Raw(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn wrappers_dereference_to_what_they_hold() {
        let Update::NewMessage(mut m) = convert(new_message(msg(7))).await else {
            panic!("not a message")
        };
        assert_eq!(m.text(), "hi");
        m.raw = tl::enums::Update::Config;
        let inner = m.clone().into_inner();
        assert_eq!(inner.id(), 7);
        // DerefMut reaches the inner message
        m.fetched_in = Some(user_ref(5));

        let Update::Raw(mut raw) = convert(tl::enums::Update::Config).await else {
            panic!("not raw")
        };
        assert!(matches!(*raw, tl::enums::Update::Config));
        *raw = tl::enums::Update::ChannelTooLong(tl::types::UpdateChannelTooLong {
            channel_id: 1,
            pts: None,
        });
        assert!(matches!(&*raw, tl::enums::Update::ChannelTooLong(_)));
    }
}
