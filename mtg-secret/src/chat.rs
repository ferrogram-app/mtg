//! One secret chat: keys, sequence numbers, layer, resend log and PFS rekeying.
//!
//! The chat is sans-IO. Methods that send return an [`Outgoing`] (a ready TL request for the
//! caller to invoke); [`SecretChat::receive`] returns an [`Output`] with what happened and what
//! to send in reply. Time is passed in as unix seconds.
//!
//! Sequence numbers (<https://core.telegram.org/api/end-to-end/seq_no>): raw counters `out` and
//! `in` count messages; on the wire `out_seq_no = 2*out + x_out` and `in_seq_no = 2*in + x_in`,
//! where `x_out` is 1 for the chat's creator and 0 for the other side, and `x_in = 1 - x_out`.

use std::collections::{BTreeMap, VecDeque};

use mtg_tl_types as tl;

use crate::cipher;
use crate::dh::{self, DhParams, Exchange};
use crate::error::{Error, Result, random_fill, random_i64};
use crate::key::Key;
use crate::schema::{Action, DecryptedMessage, LAYER, Layer, MIN_LAYER, Message, Service};

/// Lowest layer we encode: older ones are MTProto 1.0 on the wire, which is not supported.
const MIN_SENT_LAYER: i32 = 73;
/// Most out-of-order messages kept before the chat is given up (the official client's limit).
const MAX_HELD: usize = 4;
/// Most sent messages remembered for `resend` requests.
const MAX_SENT: usize = 256;
/// A key is replaced after this many messages ...
const REKEY_MESSAGES: u32 = 100;
/// ... or this many seconds (a week), if it was used at all.
const REKEY_SECONDS: i64 = 7 * 24 * 3600;

/// A request to invoke on Telegram.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Outgoing {
    /// A message without an uploaded file.
    Message(tl::functions::messages::SendEncrypted),
    /// A message with an encrypted file.
    File(tl::functions::messages::SendEncryptedFile),
    /// A service message.
    Service(tl::functions::messages::SendEncryptedService),
}

/// Something the application should show or apply.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Event {
    /// A new message. `file` is [`tl::enums::EncryptedFile::Empty`] without an attachment.
    Message {
        /// The message.
        message: Message,
        /// The attachment on Telegram's servers (decrypt with the key inside the media).
        file: tl::enums::EncryptedFile,
        /// Server date.
        date: i32,
    },
    /// A service action of the peer: ttl, read, delete, screenshot, flush or typing.
    Action(Action),
    /// The peer's layer went up; messages are now encoded for it.
    LayerChanged(i32),
    /// The chat switched to a new key (PFS).
    KeyChanged {
        /// Fingerprint of the new key.
        fingerprint: i64,
    },
    /// A rekey was abandoned (aborted by the peer, lost a collision, or failed a check).
    RekeyAborted {
        /// Id of the exchange.
        exchange_id: i64,
    },
}

/// What [`SecretChat::receive`] produced.
#[derive(Debug, Default, PartialEq)]
pub struct Output {
    /// For the application, in order.
    pub events: Vec<Event>,
    /// Requests to invoke (resend requests, replies of a rekey, resent messages).
    pub send: Vec<Outgoing>,
}

/// A rekey in progress.
#[allow(clippy::large_enum_variant)]
enum Rekey {
    None,
    /// We sent `requestKey` and wait for `acceptKey`.
    Requested {
        exchange_id: i64,
        exchange: Exchange,
    },
    /// We sent `acceptKey` and wait for `commitKey`.
    Accepted {
        exchange_id: i64,
        key: Key,
    },
}

/// A message received ahead of its turn.
struct Held {
    layer: Layer,
    file: tl::enums::EncryptedFile,
    date: i32,
    /// `resend` actions are executed on arrival, not again when their turn comes.
    done: bool,
}

/// A sent message kept for `resend`.
struct Sent {
    layer: Layer,
    file: Option<tl::enums::InputEncryptedFile>,
}

/// The persistent part of a chat (what an application stores between runs). A rekey in
/// progress and the resend log are not part of it.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// Chat id.
    pub id: i32,
    /// Access hash.
    pub access_hash: i64,
    /// The other user's id.
    pub peer_id: i64,
    /// Whether we created the chat.
    pub originator: bool,
    /// The 256-byte key.
    pub key: [u8; 256],
    /// The layer the peer speaks.
    pub peer_layer: i32,
    /// Messages we sent.
    pub out_count: i32,
    /// Messages we processed.
    pub in_count: i32,
    /// Highest `in` counter the peer has reported.
    pub peer_in_count: i32,
    /// Messages encrypted under the key.
    pub key_uses: u32,
    /// When the key was created (unix seconds).
    pub key_since: i64,
}

/// One secret chat.
pub struct SecretChat {
    id: i32,
    access_hash: i64,
    peer_id: i64,
    originator: bool,
    params: DhParams,
    key: Key,
    /// The key before a switch, until a message under the new one arrives.
    previous: Option<Key>,
    peer_layer: i32,
    out_count: i32,
    in_count: i32,
    peer_in_count: i32,
    key_uses: u32,
    key_since: i64,
    held: BTreeMap<i32, Held>,
    resend_asked: bool,
    sent: VecDeque<(i32, Sent)>,
    rekey: Rekey,
}

impl SecretChat {
    /// A chat whose key exchange just finished.
    pub fn new(
        id: i32,
        access_hash: i64,
        peer_id: i64,
        originator: bool,
        key: Key,
        params: DhParams,
        now: i64,
    ) -> Self {
        Self {
            id,
            access_hash,
            peer_id,
            originator,
            params,
            key,
            previous: None,
            peer_layer: MIN_SENT_LAYER,
            out_count: 0,
            in_count: 0,
            peer_in_count: 0,
            key_uses: 0,
            key_since: now,
            held: BTreeMap::new(),
            resend_asked: false,
            sent: VecDeque::new(),
            rekey: Rekey::None,
        }
    }

    /// Chat id.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// The other user's id.
    pub fn peer_id(&self) -> i64 {
        self.peer_id
    }

    /// Whether we created the chat.
    pub fn originator(&self) -> bool {
        self.originator
    }

    /// The current key.
    pub fn key(&self) -> &Key {
        &self.key
    }

    /// The layer messages are encoded with.
    pub fn send_layer(&self) -> i32 {
        LAYER.min(self.peer_layer.max(MIN_SENT_LAYER))
    }

    /// What to store between runs.
    pub fn state(&self) -> State {
        State {
            id: self.id,
            access_hash: self.access_hash,
            peer_id: self.peer_id,
            originator: self.originator,
            key: *self.key.bytes(),
            peer_layer: self.peer_layer,
            out_count: self.out_count,
            in_count: self.in_count,
            peer_in_count: self.peer_in_count,
            key_uses: self.key_uses,
            key_since: self.key_since,
        }
    }

    /// Rebuilds a chat from [`SecretChat::state`].
    pub fn restore(state: State, params: DhParams) -> Self {
        let mut chat = Self::new(
            state.id,
            state.access_hash,
            state.peer_id,
            state.originator,
            Key::new(state.key),
            params,
            state.key_since,
        );
        chat.peer_layer = state.peer_layer;
        chat.out_count = state.out_count;
        chat.in_count = state.in_count;
        chat.peer_in_count = state.peer_in_count;
        chat.key_uses = state.key_uses;
        chat
    }

    fn x_out(&self) -> i32 {
        i32::from(self.originator)
    }

    fn x_in(&self) -> i32 {
        1 - self.x_out()
    }

    pub(crate) fn input_chat(&self) -> tl::enums::InputEncryptedChat {
        tl::enums::InputEncryptedChat::Chat(tl::types::InputEncryptedChat {
            chat_id: self.id,
            access_hash: self.access_hash,
        })
    }

    // ---- sending

    /// Sends a message without an attachment (or with media that needs no file: location,
    /// contact, venue, web page). A zero `random_id` is filled in.
    pub fn send_message(&mut self, mut message: Message) -> Result<Outgoing> {
        if message.random_id == 0 {
            message.random_id = random_i64();
        }
        self.seal(DecryptedMessage::Message(message), None)
    }

    /// Sends a message whose media was encrypted with [`crate::file::FileKey`] and uploaded
    /// (`file` is what `upload.saveFilePart` produced).
    pub fn send_file(
        &mut self,
        mut message: Message,
        file: tl::enums::InputEncryptedFile,
    ) -> Result<Outgoing> {
        if message.random_id == 0 {
            message.random_id = random_i64();
        }
        self.seal(DecryptedMessage::Message(message), Some(file))
    }

    /// Sends a service action. Use the dedicated methods for layer and rekey actions.
    pub fn send_action(&mut self, action: Action) -> Result<Outgoing> {
        let service = Service {
            random_id: random_i64(),
            action,
        };
        self.seal(DecryptedMessage::Service(service), None)
    }

    /// Tells the peer which layer we speak (do this once the chat exists).
    pub fn notify_layer(&mut self) -> Result<Outgoing> {
        self.send_action(Action::NotifyLayer { layer: LAYER })
    }

    /// `messages.setEncryptedTyping`.
    pub fn typing(&self, typing: bool) -> tl::functions::messages::SetEncryptedTyping {
        tl::functions::messages::SetEncryptedTyping {
            peer: self.input_chat(),
            typing,
        }
    }

    /// `messages.readEncryptedHistory`.
    pub fn read_history(&self, max_date: i32) -> tl::functions::messages::ReadEncryptedHistory {
        tl::functions::messages::ReadEncryptedHistory {
            peer: self.input_chat(),
            max_date,
        }
    }

    /// `messages.discardEncryption`.
    pub fn discard(&self, delete_history: bool) -> tl::functions::messages::DiscardEncryption {
        tl::functions::messages::DiscardEncryption {
            delete_history,
            chat_id: self.id,
        }
    }

    fn seal(
        &mut self,
        message: DecryptedMessage,
        file: Option<tl::enums::InputEncryptedFile>,
    ) -> Result<Outgoing> {
        let mut random_bytes = vec![0; 16];
        random_fill(&mut random_bytes);
        let layer = Layer {
            random_bytes,
            layer: self.send_layer(),
            in_seq_no: 2 * self.in_count + self.x_in(),
            out_seq_no: 2 * self.out_count + self.x_out(),
            message,
        };
        let out = self.encode(&layer, file.clone())?;
        self.sent.push_back((self.out_count, Sent { layer, file }));
        if self.sent.len() > MAX_SENT {
            self.sent.pop_front();
        }
        self.out_count += 1;
        self.key_uses += 1;
        Ok(out)
    }

    fn encode(
        &self,
        layer: &Layer,
        file: Option<tl::enums::InputEncryptedFile>,
    ) -> Result<Outgoing> {
        let data = cipher::encrypt(&self.key, self.originator, &layer.encode()?);
        let peer = self.input_chat();
        Ok(match (&layer.message, file) {
            (DecryptedMessage::Service(s), _) => {
                Outgoing::Service(tl::functions::messages::SendEncryptedService {
                    peer,
                    random_id: s.random_id,
                    data,
                })
            }
            (DecryptedMessage::Message(m), None) => {
                Outgoing::Message(tl::functions::messages::SendEncrypted {
                    silent: m.silent,
                    peer,
                    random_id: m.random_id,
                    data,
                })
            }
            (DecryptedMessage::Message(m), Some(file)) => {
                Outgoing::File(tl::functions::messages::SendEncryptedFile {
                    silent: m.silent,
                    peer,
                    random_id: m.random_id,
                    data,
                    file,
                })
            }
        })
    }

    // ---- rekeying

    /// Whether the key is worn out: used for over 100 messages, or older than a week and used
    /// at all, and no rekey is running.
    pub fn needs_rekey(&self, now: i64) -> bool {
        matches!(self.rekey, Rekey::None)
            && (self.key_uses > REKEY_MESSAGES
                || (self.key_uses > 0 && now - self.key_since > REKEY_SECONDS))
    }

    /// Starts a rekey: returns the `requestKey` message. Fails if one is already running.
    pub fn request_key(&mut self) -> Result<Outgoing> {
        if !matches!(self.rekey, Rekey::None) {
            return Err(Error::WrongState("a rekey is already running"));
        }
        let (exchange, g_a) = dh::start(&self.params, &[]);
        let exchange_id = random_i64();
        self.rekey = Rekey::Requested {
            exchange_id,
            exchange,
        };
        self.send_action(Action::RequestKey { exchange_id, g_a })
    }

    /// Gives up the running rekey, if any, and tells the peer.
    pub fn abort_rekey(&mut self) -> Result<Option<Outgoing>> {
        let exchange_id = match self.rekey {
            Rekey::Requested { exchange_id, .. } | Rekey::Accepted { exchange_id, .. } => {
                exchange_id
            }
            Rekey::None => return Ok(None),
        };
        self.rekey = Rekey::None;
        self.send_action(Action::AbortKey { exchange_id }).map(Some)
    }

    fn switch_key(&mut self, key: Key, now: i64) {
        self.previous = Some(std::mem::replace(&mut self.key, key));
        self.key_uses = 0;
        self.key_since = now;
    }

    // ---- receiving

    /// Handles a message of this chat from `updateNewEncryptedMessage` or `getDifference`.
    ///
    /// An `Err` is fatal for the chat only if it is [`Error::Protocol`] (the official client
    /// discards the chat then); other errors mean this one message was bad and is dropped.
    pub fn receive(&mut self, now: i64, message: &tl::enums::EncryptedMessage) -> Result<Output> {
        let (bytes, file, date) = match message {
            tl::enums::EncryptedMessage::Message(m) => (&m.bytes, m.file.clone(), m.date),
            tl::enums::EncryptedMessage::Service(m) => {
                (&m.bytes, tl::enums::EncryptedFile::Empty, m.date)
            }
        };
        let fingerprint = cipher::fingerprint_of_message(bytes)?;
        let key = self.key_for(fingerprint)?;
        let plain = cipher::decrypt(&key, !self.originator, bytes)?;
        let layer = Layer::decode(&plain)?;
        if layer.layer < MIN_LAYER {
            return Err(Error::UnsupportedLayer(layer.layer));
        }
        let mut out = Output::default();
        if layer.layer > self.peer_layer {
            self.peer_layer = layer.layer;
            out.events.push(Event::LayerChanged(layer.layer));
        }
        if fingerprint == self.key.fingerprint() {
            // the peer uses the new key: nothing can arrive under the old one any more
            // once the gaps are filled
            if self.held.is_empty() {
                self.previous = None;
            }
        }
        self.sequence(now, layer, file, date, &mut out)?;
        Ok(out)
    }

    fn key_for(&self, fingerprint: i64) -> Result<Key> {
        if fingerprint == self.key.fingerprint() {
            return Ok(self.key.clone());
        }
        if let Rekey::Accepted { key, .. } = &self.rekey
            && key.fingerprint() == fingerprint
        {
            return Ok(key.clone());
        }
        match &self.previous {
            Some(key) if key.fingerprint() == fingerprint => Ok(key.clone()),
            _ => Err(Error::UnknownKey),
        }
    }

    fn sequence(
        &mut self,
        now: i64,
        layer: Layer,
        file: tl::enums::EncryptedFile,
        date: i32,
        out: &mut Output,
    ) -> Result<()> {
        let (wire_out, wire_in) = (layer.out_seq_no, layer.in_seq_no);
        if wire_out < 0 || wire_out & 1 != self.x_in() || wire_in < 0 || wire_in & 1 != self.x_out()
        {
            return Err(Error::Protocol("wrong sequence number parity"));
        }
        let (raw_out, raw_in) = (wire_out >> 1, wire_in >> 1);
        if raw_in < self.peer_in_count || raw_in > self.out_count {
            return Err(Error::Protocol("in_seq_no out of range"));
        }
        self.peer_in_count = raw_in;
        while self.sent.front().is_some_and(|(n, _)| *n < raw_in) {
            self.sent.pop_front();
        }
        if raw_out < self.in_count {
            return Ok(()); // duplicate
        }
        if raw_out > self.in_count {
            return self.hold(raw_out, layer, file, date, out);
        }
        self.apply(now, layer, file, date, false, out)?;
        while let Some(h) = self.held.remove(&self.in_count) {
            self.apply(now, h.layer, h.file, h.date, h.done, out)?;
        }
        if self.held.is_empty() {
            self.resend_asked = false;
        }
        Ok(())
    }

    fn hold(
        &mut self,
        raw_out: i32,
        layer: Layer,
        file: tl::enums::EncryptedFile,
        date: i32,
        out: &mut Output,
    ) -> Result<()> {
        if self.held.contains_key(&raw_out) {
            return Ok(()); // duplicate of a held one
        }
        if self.held.len() >= MAX_HELD {
            return Err(Error::Protocol("too many messages out of order"));
        }
        let wire_out = layer.out_seq_no;
        let mut done = false;
        if let DecryptedMessage::Service(Service {
            action:
                Action::Resend {
                    start_seq_no,
                    end_seq_no,
                },
            ..
        }) = &layer.message
        {
            self.resend(*start_seq_no, *end_seq_no, out)?;
            done = true;
        }
        self.held.insert(
            raw_out,
            Held {
                layer,
                file,
                date,
                done,
            },
        );
        if !self.resend_asked {
            self.resend_asked = true;
            let start = 2 * self.in_count + self.x_in();
            out.send.push(self.send_action(Action::Resend {
                start_seq_no: start,
                end_seq_no: wire_out - 2,
            })?);
        }
        Ok(())
    }

    fn resend(&mut self, start: i32, end: i32, out: &mut Output) -> Result<()> {
        let x = self.x_out();
        if start > end || start & 1 != x || end & 1 != x || start < 0 {
            return Err(Error::Protocol("bad resend range"));
        }
        let (first, last) = (start >> 1, end >> 1);
        let wanted: Vec<_> = (first..=last).collect();
        let mut replay = Vec::new();
        for n in &wanted {
            match self.sent.iter().find(|(c, _)| c == n) {
                Some((_, s)) => replay.push((s.layer.clone(), s.file.clone())),
                None => return Err(Error::Protocol("resend of a message we no longer have")),
            }
        }
        for (layer, file) in replay {
            out.send.push(self.encode(&layer, file)?);
        }
        Ok(())
    }

    fn apply(
        &mut self,
        now: i64,
        layer: Layer,
        file: tl::enums::EncryptedFile,
        date: i32,
        done: bool,
        out: &mut Output,
    ) -> Result<()> {
        self.in_count += 1;
        self.key_uses += 1;
        match layer.message {
            DecryptedMessage::Message(message) => out.events.push(Event::Message {
                message,
                file,
                date,
            }),
            DecryptedMessage::Service(s) => self.action(now, s.action, done, out)?,
        }
        Ok(())
    }

    fn action(&mut self, now: i64, action: Action, done: bool, out: &mut Output) -> Result<()> {
        match action {
            Action::Resend {
                start_seq_no,
                end_seq_no,
            } => {
                if !done {
                    self.resend(start_seq_no, end_seq_no, out)?;
                }
            }
            Action::NotifyLayer { layer } => {
                if layer > self.peer_layer {
                    self.peer_layer = layer;
                    out.events.push(Event::LayerChanged(layer));
                }
            }
            Action::Noop => {}
            Action::RequestKey { exchange_id, g_a } => {
                self.on_request_key(exchange_id, &g_a, out)?;
            }
            Action::AcceptKey {
                exchange_id,
                g_b,
                key_fingerprint,
            } => self.on_accept_key(now, exchange_id, &g_b, key_fingerprint, out)?,
            Action::CommitKey {
                exchange_id,
                key_fingerprint,
            } => {
                if let Rekey::Accepted {
                    exchange_id: id,
                    key,
                } = &self.rekey
                    && *id == exchange_id
                    && key.fingerprint() == key_fingerprint
                {
                    let key = key.clone();
                    self.rekey = Rekey::None;
                    out.events.push(Event::KeyChanged {
                        fingerprint: key.fingerprint(),
                    });
                    self.switch_key(key, now);
                }
            }
            Action::AbortKey { exchange_id } => {
                let running = match self.rekey {
                    Rekey::Requested {
                        exchange_id: id, ..
                    }
                    | Rekey::Accepted {
                        exchange_id: id, ..
                    } => id,
                    Rekey::None => return Ok(()),
                };
                if running == exchange_id {
                    self.rekey = Rekey::None;
                    out.events.push(Event::RekeyAborted { exchange_id });
                }
            }
            other => out.events.push(Event::Action(other)),
        }
        Ok(())
    }

    fn on_request_key(&mut self, exchange_id: i64, g_a: &[u8], out: &mut Output) -> Result<()> {
        match &self.rekey {
            Rekey::None => {}
            Rekey::Requested {
                exchange_id: ours, ..
            } => {
                // both sides asked at once: the smaller id gives way, equal ids cancel both
                if *ours > exchange_id {
                    return Ok(());
                }
                let ours = *ours;
                self.rekey = Rekey::None;
                out.events.push(Event::RekeyAborted { exchange_id: ours });
                if ours == exchange_id {
                    return Ok(());
                }
            }
            Rekey::Accepted { .. } => {
                out.send
                    .push(self.send_action(Action::AbortKey { exchange_id })?);
                return Ok(());
            }
        }
        match dh::accept(&self.params, g_a, &[]) {
            Ok((g_b, key)) => {
                let key_fingerprint = key.fingerprint();
                self.rekey = Rekey::Accepted { exchange_id, key };
                out.send.push(self.send_action(Action::AcceptKey {
                    exchange_id,
                    g_b,
                    key_fingerprint,
                })?);
            }
            Err(_) => out
                .send
                .push(self.send_action(Action::AbortKey { exchange_id })?),
        }
        Ok(())
    }

    fn on_accept_key(
        &mut self,
        now: i64,
        exchange_id: i64,
        g_b: &[u8],
        key_fingerprint: i64,
        out: &mut Output,
    ) -> Result<()> {
        let Rekey::Requested {
            exchange_id: id,
            exchange,
        } = &self.rekey
        else {
            return Ok(());
        };
        if *id != exchange_id {
            return Ok(());
        }
        let key = exchange
            .finish(g_b)
            .ok()
            .filter(|k| k.fingerprint() == key_fingerprint);
        self.rekey = Rekey::None;
        match key {
            Some(key) => {
                // the commit goes out under the old key, everything after it under the new
                out.send.push(self.send_action(Action::CommitKey {
                    exchange_id,
                    key_fingerprint,
                })?);
                out.events.push(Event::KeyChanged {
                    fingerprint: key.fingerprint(),
                });
                self.switch_key(key, now);
            }
            None => {
                out.events.push(Event::RekeyAborted { exchange_id });
                out.send
                    .push(self.send_action(Action::AbortKey { exchange_id })?);
            }
        }
        Ok(())
    }
}
