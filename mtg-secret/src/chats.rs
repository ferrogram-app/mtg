//! All secret chats of an account, driven by the API updates.
//!
//! The application feeds every update to [`SecretChats::handle_update`] and invokes what comes
//! back in [`ChatsOutput`]; storing chats and messages stays with the application
//! ([`SecretChats::states`] / [`SecretChats::restore`] give what to persist).
//!
//! Creating a chat: [`SecretChats::dh_config_request`] and [`SecretChats::set_dh_config`], then
//! [`SecretChats::request`] and, with the result of that call, [`SecretChats::on_request_result`].
//! The peer's `updateEncryption` completes it ([`ChatsEvent::Ready`]). For a chat started by
//! the peer, [`ChatsEvent::Requested`] arrives; answer with [`SecretChats::accept`] or
//! [`SecretChats::discard`]. After either, [`SecretChat::notify_layer`] announces our layer.

use std::collections::HashMap;

use mtg_tl_types as tl;

use crate::chat::{Event, Outgoing, SecretChat, State};
use crate::dh::{self, DhParams, Exchange};
use crate::error::{Error, Result, random_i64};

/// Bytes of server randomness asked from `messages.getDhConfig`.
const RANDOM_LENGTH: i32 = 256;

/// What happened to a chat.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ChatsEvent {
    /// The peer wants a chat: [`SecretChats::accept`] or [`SecretChats::discard`] it.
    Requested {
        /// Chat id.
        chat_id: i32,
        /// The user who asked.
        peer_id: i64,
    },
    /// The chat has its key and can be used.
    Ready {
        /// Chat id.
        chat_id: i32,
    },
    /// The chat was closed (by the peer, or because it broke the protocol).
    Discarded {
        /// Chat id.
        chat_id: i32,
        /// The peer asked to delete the history.
        history_deleted: bool,
    },
    /// Something from a chat: a message, an action, a key change.
    Chat {
        /// Chat id.
        chat_id: i32,
        /// What it is.
        event: Event,
    },
    /// A message could not be used and was dropped (bad key, bad padding, unknown layer, ...).
    Dropped {
        /// Chat id.
        chat_id: i32,
        /// Why.
        reason: String,
    },
    /// The peer is typing.
    Typing {
        /// Chat id.
        chat_id: i32,
    },
    /// The peer read the history up to `max_date`.
    Read {
        /// Chat id.
        chat_id: i32,
        /// Messages up to this date are read.
        max_date: i32,
        /// When it happened.
        date: i32,
    },
}

/// What [`SecretChats::handle_update`] produced.
#[derive(Debug, Default)]
pub struct ChatsOutput {
    /// For the application, in order.
    pub events: Vec<ChatsEvent>,
    /// Messages to send (resend requests, rekey steps).
    pub send: Vec<Outgoing>,
    /// Chats to close on the server (the peer broke the protocol).
    pub discard: Vec<tl::functions::messages::DiscardEncryption>,
    /// The `qts` of a new message: acknowledge it with [`SecretChats::received_queue`].
    pub qts: Option<i32>,
}

struct Pending {
    exchange: Exchange,
    peer_id: i64,
    /// Known once Telegram answered `requestEncryption`.
    access_hash: i64,
}

struct Incoming {
    access_hash: i64,
    peer_id: i64,
    g_a: Vec<u8>,
}

/// The chats of one account.
#[derive(Default)]
pub struct SecretChats {
    params: Option<DhParams>,
    server_random: Vec<u8>,
    chats: HashMap<i32, SecretChat>,
    /// `requestEncryption` sent, answer not yet seen (by `random_id`).
    asked: HashMap<i32, Pending>,
    /// Waiting for the peer to accept (by chat id).
    waiting: HashMap<i32, Pending>,
    /// Requests from peers (by chat id).
    incoming: HashMap<i32, Incoming>,
}

impl SecretChats {
    /// An empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// A manager with chats loaded from [`SecretChats::states`].
    pub fn restore(params: DhParams, states: impl IntoIterator<Item = State>) -> Self {
        let chats = states
            .into_iter()
            .map(|s| (s.id, SecretChat::restore(s, params.clone())))
            .collect();
        Self {
            params: Some(params),
            chats,
            ..Self::default()
        }
    }

    /// The state of every chat, to store.
    pub fn states(&self) -> Vec<State> {
        self.chats.values().map(SecretChat::state).collect()
    }

    /// A chat by id.
    pub fn chat(&self, id: i32) -> Option<&SecretChat> {
        self.chats.get(&id)
    }

    /// A chat by id, to send in it.
    pub fn chat_mut(&mut self, id: i32) -> Option<&mut SecretChat> {
        self.chats.get_mut(&id)
    }

    /// `messages.getDhConfig` for the cached version (0 the first time).
    pub fn dh_config_request(&self) -> tl::functions::messages::GetDhConfig {
        tl::functions::messages::GetDhConfig {
            version: self.params.as_ref().map_or(0, DhParams::version),
            random_length: RANDOM_LENGTH,
        }
    }

    /// Takes the answer of [`SecretChats::dh_config_request`]; checks the prime.
    pub fn set_dh_config(&mut self, config: &tl::enums::messages::DhConfig) -> Result<()> {
        let (params, random) = DhParams::from_config(config, self.params.as_ref())?;
        self.params = Some(params);
        self.server_random = random;
        Ok(())
    }

    fn params(&self) -> Result<&DhParams> {
        self.params
            .as_ref()
            .ok_or(Error::WrongState("no DH config: call set_dh_config first"))
    }

    /// Starts a chat with a user: returns `messages.requestEncryption` to invoke.
    pub fn request(
        &mut self,
        user: tl::enums::InputUser,
        peer_id: i64,
    ) -> Result<tl::functions::messages::RequestEncryption> {
        let (exchange, g_a) = dh::start(self.params()?, &self.server_random);
        let random_id = random_id();
        self.asked.insert(
            random_id,
            Pending {
                exchange,
                peer_id,
                access_hash: 0,
            },
        );
        Ok(tl::functions::messages::RequestEncryption {
            user_id: user,
            random_id,
            g_a,
        })
    }

    /// Takes the result of the [`SecretChats::request`] call (made with that `random_id`).
    /// Returns the chat id.
    pub fn on_request_result(
        &mut self,
        random_id: i32,
        chat: &tl::enums::EncryptedChat,
    ) -> Result<i32> {
        let tl::enums::EncryptedChat::Waiting(w) = chat else {
            return Err(Error::WrongState(
                "requestEncryption did not return a waiting chat",
            ));
        };
        let mut pending = self
            .asked
            .remove(&random_id)
            .ok_or(Error::WrongState("no such request"))?;
        pending.access_hash = w.access_hash;
        self.waiting.insert(w.id, pending);
        Ok(w.id)
    }

    /// Accepts a chat from [`ChatsEvent::Requested`]: returns `messages.acceptEncryption`. The
    /// chat is usable at once.
    pub fn accept(
        &mut self,
        chat_id: i32,
        now: i64,
    ) -> Result<tl::functions::messages::AcceptEncryption> {
        let params = self.params()?.clone();
        let req = self
            .incoming
            .get(&chat_id)
            .ok_or(Error::NoSuchChat(chat_id))?;
        let (g_b, key) = dh::accept(&params, &req.g_a, &self.server_random)?;
        let req = self
            .incoming
            .remove(&chat_id)
            .ok_or(Error::NoSuchChat(chat_id))?;
        let key_fingerprint = key.fingerprint();
        let chat = SecretChat::new(
            chat_id,
            req.access_hash,
            req.peer_id,
            false,
            key,
            params,
            now,
        );
        let peer = chat.input_chat();
        self.chats.insert(chat_id, chat);
        Ok(tl::functions::messages::AcceptEncryption {
            peer,
            g_b,
            key_fingerprint,
        })
    }

    /// Closes a chat (or declines a request) and forgets it: returns the request to invoke.
    pub fn discard(
        &mut self,
        chat_id: i32,
        delete_history: bool,
    ) -> tl::functions::messages::DiscardEncryption {
        self.forget(chat_id);
        tl::functions::messages::DiscardEncryption {
            delete_history,
            chat_id,
        }
    }

    fn forget(&mut self, chat_id: i32) {
        self.chats.remove(&chat_id);
        self.waiting.remove(&chat_id);
        self.incoming.remove(&chat_id);
    }

    /// `messages.receivedQueue` for the `qts` of [`ChatsOutput`].
    pub fn received_queue(max_qts: i32) -> tl::functions::messages::ReceivedQueue {
        tl::functions::messages::ReceivedQueue { max_qts }
    }

    /// Handles an update; updates that are not about secret chats give an empty output.
    pub fn handle_update(&mut self, now: i64, update: &tl::enums::Update) -> ChatsOutput {
        let mut out = ChatsOutput::default();
        match update {
            tl::enums::Update::Encryption(u) => self.on_encryption(now, &u.chat, &mut out),
            tl::enums::Update::NewEncryptedMessage(u) => {
                out.qts = Some(u.qts);
                self.on_message(now, &u.message, &mut out);
            }
            tl::enums::Update::EncryptedChatTyping(u) => {
                out.events.push(ChatsEvent::Typing { chat_id: u.chat_id });
            }
            tl::enums::Update::EncryptedMessagesRead(u) => out.events.push(ChatsEvent::Read {
                chat_id: u.chat_id,
                max_date: u.max_date,
                date: u.date,
            }),
            _ => {}
        }
        out
    }

    fn on_encryption(&mut self, now: i64, chat: &tl::enums::EncryptedChat, out: &mut ChatsOutput) {
        use tl::enums::EncryptedChat as E;
        match chat {
            E::Requested(r) => {
                self.incoming.insert(
                    r.id,
                    Incoming {
                        access_hash: r.access_hash,
                        peer_id: r.admin_id,
                        g_a: r.g_a.clone(),
                    },
                );
                out.events.push(ChatsEvent::Requested {
                    chat_id: r.id,
                    peer_id: r.admin_id,
                });
            }
            E::Chat(c) => {
                // for a chat we accepted this is only the confirmation
                let Some(pending) = self.waiting.remove(&c.id) else {
                    return;
                };
                let key = pending
                    .exchange
                    .finish(&c.g_a_or_b)
                    .ok()
                    .filter(|k| k.fingerprint() == c.key_fingerprint);
                match key {
                    Some(key) => {
                        let params = pending.exchange.params().clone();
                        let chat = SecretChat::new(
                            c.id,
                            pending.access_hash,
                            pending.peer_id,
                            true,
                            key,
                            params,
                            now,
                        );
                        self.chats.insert(c.id, chat);
                        out.events.push(ChatsEvent::Ready { chat_id: c.id });
                    }
                    None => {
                        out.discard.push(self.discard(c.id, false));
                        out.events.push(ChatsEvent::Discarded {
                            chat_id: c.id,
                            history_deleted: false,
                        });
                    }
                }
            }
            E::Discarded(d) => {
                self.forget(d.id);
                out.events.push(ChatsEvent::Discarded {
                    chat_id: d.id,
                    history_deleted: d.history_deleted,
                });
            }
            E::Empty(e) => self.forget(e.id),
            E::Waiting(_) => {}
        }
    }

    fn on_message(
        &mut self,
        now: i64,
        message: &tl::enums::EncryptedMessage,
        out: &mut ChatsOutput,
    ) {
        let chat_id = match message {
            tl::enums::EncryptedMessage::Message(m) => m.chat_id,
            tl::enums::EncryptedMessage::Service(m) => m.chat_id,
        };
        let Some(chat) = self.chats.get_mut(&chat_id) else {
            return;
        };
        match chat.receive(now, message) {
            Ok(o) => {
                out.send.extend(o.send);
                out.events.extend(
                    o.events
                        .into_iter()
                        .map(|event| ChatsEvent::Chat { chat_id, event }),
                );
            }
            Err(Error::Protocol(why)) => {
                out.discard.push(self.discard(chat_id, false));
                out.events.push(ChatsEvent::Dropped {
                    chat_id,
                    reason: why.to_string(),
                });
                out.events.push(ChatsEvent::Discarded {
                    chat_id,
                    history_deleted: false,
                });
            }
            Err(e) => out.events.push(ChatsEvent::Dropped {
                chat_id,
                reason: e.to_string(),
            }),
        }
    }
}

fn random_id() -> i32 {
    random_i64() as i32
}
