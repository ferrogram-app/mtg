// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::convert::TryInto;
use std::fmt;
use std::time::Duration;

use mtg_mtsender::InvocationError;
use mtg_session::types::{PeerId, PeerRef};
use mtg_session::updates::State;
use mtg_tl_types as tl;

use crate::Client;
use crate::message::{InputMessage, Message};
use crate::peer::{Peer, PeerMap};

static NO_PEER_REF_REASON: &str = "cannot invoke request without a peer ref";

/// Update that bots receive when a user presses one of the bot's inline callback buttons.
///
/// You should always [`CallbackQuery::answer`] these queries, even if you have no data to display
/// to the user, because otherwise they will think the bot is non-responsive (the button spinner
/// will timeout).
#[derive(Clone)]
pub struct CallbackQuery {
    pub raw: tl::enums::Update,
    pub state: State,
    pub(crate) client: Client,
    pub(crate) peers: PeerMap,
}

/// A callback query answer builder.
///
/// It will be executed once `.await`-ed. Modifying it after polling it once will have no effect.
pub struct Answer<'a> {
    query: &'a CallbackQuery,
    request: tl::functions::messages::SetBotCallbackAnswer,
}

impl CallbackQuery {
    /// The [`Self::peer`]'s identifier.
    pub fn peer_id(&self) -> PeerId {
        match &self.raw {
            tl::enums::Update::BotCallbackQuery(update) => update.peer.clone().into(),
            tl::enums::Update::InlineBotCallbackQuery(update) => {
                PeerId::user_unchecked(update.user_id)
            }
            _ => unreachable!(),
        }
    }

    /// Cached reference to the [`Self::peer`], if it is in cache.
    pub async fn peer_ref(
        &self,
    ) -> Result<Option<PeerRef>, Box<dyn std::error::Error + Send + Sync>> {
        self.peers.get_ref(self.peer_id()).await
    }

    /// The peer where the callback query occured, if it is in cache.
    pub fn peer(&self) -> Option<&Peer> {
        self.peers.get(self.peer_id())
    }

    /// The [`Self::sender`]'s identifier.
    pub fn sender_id(&self) -> PeerId {
        PeerId::user_unchecked(match &self.raw {
            tl::enums::Update::BotCallbackQuery(update) => update.user_id,
            tl::enums::Update::InlineBotCallbackQuery(update) => update.user_id,
            _ => unreachable!(),
        })
    }

    /// Cached reference to the [`Self::sender`], if it is in cache.
    pub async fn sender_ref(
        &self,
    ) -> Result<Option<PeerRef>, Box<dyn std::error::Error + Send + Sync>> {
        self.peers.get_ref(self.sender_id()).await
    }

    /// The user who sent this callback query, if it is in cache.
    pub fn sender(&self) -> Option<&Peer> {
        self.peers.get(self.sender_id())
    }

    /// They binary payload data contained by the inline button which was pressed.
    ///
    /// This data cannot be faked by the client, since Telegram will only accept "button presses"
    /// on data that actually existed in the buttons of the message, so you do not need to perform
    /// any sanity checks.
    ///
    /// *Trivia*: it used to be possible to fake the callback data, but a server-side check was
    /// added circa 2018 to prevent malicious clients from doing so.
    pub fn data(&self) -> &[u8] {
        match &self.raw {
            tl::enums::Update::BotCallbackQuery(update) => update.data.as_deref().unwrap_or(&[]),
            tl::enums::Update::InlineBotCallbackQuery(update) => {
                update.data.as_deref().unwrap_or(&[])
            }
            _ => unreachable!(),
        }
    }

    /// Whether the callback query was generated from an inline message.
    pub fn is_from_inline(&self) -> bool {
        matches!(self.raw, tl::enums::Update::InlineBotCallbackQuery(_))
    }

    /// Load the `Message` that contains the pressed inline button.
    pub async fn load_message(&self) -> Result<Message, InvocationError> {
        let msg_id = match &self.raw {
            tl::enums::Update::BotCallbackQuery(update) => update.msg_id,
            _ => {
                return Err(InvocationError::Dropped(
                    "cannot load message from inline callback queries",
                ));
            }
        };
        self.client
            .get_messages_by_id(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                &[msg_id],
            )
            .await?
            .pop()
            .flatten()
            .ok_or(InvocationError::Dropped("message no longer exists"))
    }

    /// Answer the callback query.
    pub fn answer(&self) -> Answer<'_> {
        let query_id = match &self.raw {
            tl::enums::Update::BotCallbackQuery(update) => update.query_id,
            tl::enums::Update::InlineBotCallbackQuery(update) => update.query_id,
            _ => unreachable!(),
        };
        Answer {
            request: tl::functions::messages::SetBotCallbackAnswer {
                alert: false,
                query_id,
                message: None,
                url: None,
                cache_time: 0,
            },
            query: self,
        }
    }
}

impl<'a> Answer<'a> {
    /// Configure the answer's text.
    ///
    /// The text will be displayed as a toast message (small popup which does not interrupt the
    /// user and fades on its own after a short period of time).
    pub fn text<T: Into<String>>(mut self, text: T) -> Self {
        self.request.message = Some(text.into());
        self.request.alert = false;
        self
    }

    /// For how long should the answer be considered valid. It will be cached by the client for
    /// the given duration, so subsequent callback queries with the same data will not reach the
    /// bot.
    pub fn cache_time(mut self, time: Duration) -> Self {
        self.request.cache_time = time.as_secs().try_into().unwrap_or(i32::MAX);
        self
    }

    /// Configure the answer's text.
    ///
    /// The text will be displayed as an alert (popup modal window with the text, which the user
    /// needs to close before performing other actions).
    pub fn alert<T: Into<String>>(mut self, text: T) -> Self {
        self.request.message = Some(text.into());
        self.request.alert = true;
        self
    }

    /// Send the answer back to Telegram, and then relayed to the user who pressed the inline
    /// button.
    pub async fn send(self) -> Result<(), InvocationError> {
        self.query.client.invoke(&self.request).await?;
        Ok(())
    }

    /// [`Self::send`] the answer, and also edit the message that contained the button.
    pub async fn edit<M: Into<InputMessage>>(self, new_message: M) -> Result<(), InvocationError> {
        self.query.client.invoke(&self.request).await?;
        let peer = self
            .query
            .peer_ref()
            .await?
            .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?;
        match &self.query.raw {
            tl::enums::Update::BotCallbackQuery(update) => {
                self.query
                    .client
                    .edit_message(peer, update.msg_id, new_message)
                    .await
            }
            tl::enums::Update::InlineBotCallbackQuery(update) => self
                .query
                .client
                .edit_inline_message(update.msg_id.clone(), new_message.into())
                .await
                .map(drop),
            _ => unreachable!(),
        }
    }

    /// [`Self::send`] the answer, and also respond in the peer where the button was clicked.
    pub async fn respond<M: Into<InputMessage>>(
        self,
        message: M,
    ) -> Result<Message, InvocationError> {
        self.query.client.invoke(&self.request).await?;
        let peer = self
            .query
            .peer_ref()
            .await?
            .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?;
        self.query.client.send_message(peer, message).await
    }

    /// [`Self::send`] the answer, and also reply to the message that contained the button.
    pub async fn reply<M: Into<InputMessage>>(
        self,
        message: M,
    ) -> Result<Message, InvocationError> {
        let msg_id = match &self.query.raw {
            tl::enums::Update::BotCallbackQuery(update) => update.msg_id,
            _ => {
                return Err(InvocationError::Dropped(
                    "cannot reply to inline callback query",
                ));
            }
        };
        self.query.client.invoke(&self.request).await?;
        let peer = self
            .query
            .peer_ref()
            .await?
            .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?;
        let message = message.into();
        self.query
            .client
            .send_message(peer, message.reply_to(Some(msg_id)))
            .await
    }
}

impl fmt::Debug for CallbackQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallbackQuery")
            .field("data", &self.data())
            .field("sender", &self.sender())
            .field("peer", &self.peer())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{client, message, sent, user, user_ref};

    type Log = Arc<Mutex<Vec<String>>>;

    fn state() -> State {
        State {
            date: 0,
            seq: 0,
            message_box: None,
        }
    }

    async fn query(c: &Client, inline: bool, data: Option<Vec<u8>>) -> CallbackQuery {
        let raw = if inline {
            tl::types::UpdateInlineBotCallbackQuery {
                query_id: 7,
                user_id: 5,
                msg_id: tl::types::InputBotInlineMessageId {
                    dc_id: 2,
                    id: 1,
                    access_hash: 2,
                }
                .into(),
                chat_instance: 0,
                data,
                game_short_name: None,
            }
            .into()
        } else {
            tl::types::UpdateBotCallbackQuery {
                query_id: 7,
                user_id: 5,
                peer: tl::types::PeerUser { user_id: 5 }.into(),
                msg_id: 12,
                chat_instance: 0,
                data,
                game_short_name: None,
            }
            .into()
        };
        CallbackQuery {
            raw,
            state: state(),
            client: c.clone(),
            peers: c.build_peer_map(vec![user(5)], vec![]).await,
        }
    }

    fn answered(server: &FakeServer, log: &Log) {
        let log = log.clone();
        server.on(move |req: tl::functions::messages::SetBotCallbackAnswer| {
            log.lock().unwrap().push(format!(
                "answer {} {:?} alert={} cache={}",
                req.query_id, req.message, req.alert, req.cache_time
            ));
            Ok(true)
        });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_button_press_in_a_chat_exposes_its_data() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let q = query(&c, false, Some(vec![1, 2])).await;
        assert_eq!(q.data(), [1, 2]);
        assert!(!q.is_from_inline());
        assert_eq!(q.peer_id(), PeerId::user_unchecked(5));
        assert_eq!(q.sender_id(), PeerId::user_unchecked(5));
        assert!(q.peer().is_some() && q.sender().is_some());
        assert_eq!(q.peer_ref().await.unwrap(), Some(user_ref(5)));
        assert_eq!(q.sender_ref().await.unwrap(), Some(user_ref(5)));
        assert!(format!("{q:?}").starts_with("CallbackQuery"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_inline_press_belongs_to_the_user_and_may_have_no_data() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let q = query(&c, true, None).await;
        assert!(q.is_from_inline() && q.data().is_empty());
        assert_eq!(q.peer_id(), PeerId::user_unchecked(5));
        assert_eq!(q.sender_id(), PeerId::user_unchecked(5));
        let q = query(&c, false, None).await;
        assert!(q.data().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_pressed_message_is_loaded_or_missing() {
        let server = FakeServer::start().await;
        let found = Arc::new(Mutex::new(true));
        let flag = found.clone();
        server.on(move |_: tl::functions::messages::GetMessages| {
            Ok(tl::types::messages::Messages {
                messages: if *flag.lock().unwrap() {
                    vec![message(12, user_ref(5), "pressed", 0)]
                } else {
                    vec![]
                },
                topics: Vec::new(),
                chats: Vec::new(),
                users: Vec::new(),
            }
            .into())
        });
        let c = client(&server);
        let q = query(&c, false, None).await;
        assert_eq!(q.load_message().await.unwrap().text(), "pressed");
        *found.lock().unwrap() = false;
        assert!(matches!(
            q.load_message().await,
            Err(InvocationError::Dropped(_))
        ));
        let inline = query(&c, true, None).await;
        assert!(matches!(
            inline.load_message().await,
            Err(InvocationError::Dropped(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn answers_carry_text_alert_and_cache_time() {
        let server = FakeServer::start().await;
        let log: Log = Arc::default();
        answered(&server, &log);
        let c = client(&server);
        let q = query(&c, false, None).await;
        q.answer().send().await.unwrap();
        q.answer()
            .text("hi")
            .cache_time(Duration::from_secs(30))
            .send()
            .await
            .unwrap();
        q.answer().alert("careful").send().await.unwrap();
        q.answer()
            .cache_time(Duration::from_secs(u64::MAX))
            .send()
            .await
            .unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            [
                "answer 7 None alert=false cache=0",
                "answer 7 Some(\"hi\") alert=false cache=30",
                "answer 7 Some(\"careful\") alert=true cache=0",
                "answer 7 None alert=false cache=2147483647",
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn answering_can_also_edit_respond_or_reply() {
        let server = FakeServer::start().await;
        let log: Log = Arc::default();
        answered(&server, &log);
        let edits = log.clone();
        server.on(move |req: tl::functions::messages::EditMessage| {
            edits
                .lock()
                .unwrap()
                .push(format!("edit {} {:?}", req.id, req.message));
            Ok(tl::enums::Updates::Updates(tl::types::Updates {
                updates: Vec::new(),
                users: Vec::new(),
                chats: Vec::new(),
                date: 0,
                seq: 0,
            }))
        });
        let sends = log.clone();
        server.on(move |req: tl::functions::messages::SendMessage| {
            let reply = match &req.reply_to {
                Some(tl::enums::InputReplyTo::Message(r)) => r.reply_to_msg_id,
                _ => 0,
            };
            sends
                .lock()
                .unwrap()
                .push(format!("send {} reply={reply}", req.message));
            Ok(sent(
                req.random_id,
                message(30, user_ref(5), &req.message, 2),
            ))
        });
        let inline = log.clone();
        server.on(move |req: tl::functions::messages::EditInlineBotMessage| {
            inline
                .lock()
                .unwrap()
                .push(format!("inline {:?}", req.message));
            Ok(true)
        });
        let c = client(&server);
        let q = query(&c, false, None).await;
        q.answer().edit("new").await.unwrap();
        assert_eq!(q.answer().respond("r").await.unwrap().id(), 30);
        assert_eq!(q.answer().reply("p").await.unwrap().id(), 30);
        let i = query(&c, true, None).await;
        i.answer().edit("in").await.unwrap();
        assert!(matches!(
            i.answer().reply("p").await,
            Err(InvocationError::Dropped(_))
        ));
        assert_eq!(
            log.lock()
                .unwrap()
                .iter()
                .filter(|l| !l.starts_with("answer"))
                .collect::<Vec<_>>(),
            [
                "edit 12 Some(\"new\")",
                "send r reply=0",
                "send p reply=12",
                "inline Some(\"in\")"
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn answers_to_unknown_peers_are_dropped_after_the_server_call() {
        let server = FakeServer::start().await;
        let log: Log = Arc::default();
        answered(&server, &log);
        let c = client(&server);
        let mut q = query(&c, false, None).await;
        q.peers = c.empty_peer_map();
        // user 5 sits in the session cache after the first query: ask about a stranger
        let tl::enums::Update::BotCallbackQuery(u) = &mut q.raw else {
            unreachable!()
        };
        u.peer = tl::types::PeerUser { user_id: 99 }.into();
        assert!(matches!(
            q.answer().edit("x").await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(
            q.answer().respond("x").await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(
            q.answer().reply("x").await,
            Err(InvocationError::Dropped(_))
        ));
        assert!(matches!(
            q.load_message().await,
            Err(InvocationError::Dropped(_))
        ));
    }
}
