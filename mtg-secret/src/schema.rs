//! The secret-layer objects (<https://core.telegram.org/schema/end-to-end>): what goes inside
//! the encrypted payload. They are not part of `api.tl`, so they are written by hand.
//!
//! Layers 46 and newer are spoken (MTProto 2.0). Encoding picks the constructors of the layer in
//! [`Layer::layer`]; decoding accepts every constructor of layers 46 to 151.
//! Layers below 46 (MTProto 1.0, `decryptedMessage_layer8/17`) and the legacy
//! `decryptedMessageMediaExternalDocument` are not supported.

use mtg_tl_types::enums::MessageEntity;
use mtg_tl_types::{Cursor, Deserializable, Serializable, deserialize};

use crate::error::{Error, Result};

/// The layer this crate speaks (the official Android client's `CURRENT_SECRET_CHAT_LAYER`).
pub const LAYER: i32 = 151;
/// The lowest layer a chat can use; every new chat starts here until the peer says more.
pub const MIN_LAYER: i32 = 46;
/// First layer with `decryptedMessage#91cc4674` (`silent`, `grouped_id`).
const LAYER_MESSAGE_V73: i32 = 73;
/// First layer with `decryptedMessageMediaDocument#6abd9782` (64-bit size).
const LAYER_DOCUMENT_V143: i32 = 143;

const VECTOR: u32 = 0x1cb5c415;

/// `decryptedMessageLayer`: the envelope every message travels in.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// At least 15 random bytes.
    pub random_bytes: Vec<u8>,
    /// The layer of the objects inside.
    pub layer: i32,
    /// Wire `in_seq_no` (see [`crate::chat`]).
    pub in_seq_no: i32,
    /// Wire `out_seq_no`.
    pub out_seq_no: i32,
    /// The message.
    pub message: DecryptedMessage,
}

/// A user message or a service message.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum DecryptedMessage {
    /// `decryptedMessage`.
    Message(Message),
    /// `decryptedMessageService`.
    Service(Service),
}

impl DecryptedMessage {
    /// The `random_id` of the message.
    pub fn random_id(&self) -> i64 {
        match self {
            DecryptedMessage::Message(m) => m.random_id,
            DecryptedMessage::Service(s) => s.random_id,
        }
    }
}

/// `decryptedMessage`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    /// No notification (layer 73+).
    pub silent: bool,
    /// Random id, unique per message.
    pub random_id: i64,
    /// Self-destruct time in seconds, 0 for none.
    pub ttl: i32,
    /// The text.
    pub message: String,
    /// The attachment.
    pub media: Option<Media>,
    /// Formatting.
    pub entities: Option<Vec<MessageEntity>>,
    /// Bot username for inline results.
    pub via_bot_name: Option<String>,
    /// The message this replies to.
    pub reply_to_random_id: Option<i64>,
    /// Album id (layer 73+).
    pub grouped_id: Option<i64>,
}

/// `decryptedMessageService`.
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    /// Random id, unique per message.
    pub random_id: i64,
    /// What it does.
    pub action: Action,
}

/// `DecryptedMessageAction`.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// New self-destruct time for the chat.
    SetMessageTtl {
        /// Seconds, 0 to disable.
        ttl_seconds: i32,
    },
    /// The peer read these messages (their `random_id`s).
    ReadMessages {
        /// `random_id`s.
        random_ids: Vec<i64>,
    },
    /// Delete these messages.
    DeleteMessages {
        /// `random_id`s.
        random_ids: Vec<i64>,
    },
    /// The peer took a screenshot of these messages.
    ScreenshotMessages {
        /// `random_id`s.
        random_ids: Vec<i64>,
    },
    /// Clear the whole history.
    FlushHistory,
    /// Ask for the messages with `start_seq_no..=end_seq_no` again (wire numbers).
    Resend {
        /// First wanted `out_seq_no`.
        start_seq_no: i32,
        /// Last wanted `out_seq_no`.
        end_seq_no: i32,
    },
    /// The sender now speaks this layer.
    NotifyLayer {
        /// Layer number.
        layer: i32,
    },
    /// Typing indicator.
    Typing {
        /// What the user is doing.
        action: SendAction,
    },
    /// PFS: step 1, `g_a` of a new key.
    RequestKey {
        /// Id of the exchange.
        exchange_id: i64,
        /// `g_a`.
        g_a: Vec<u8>,
    },
    /// PFS: step 2, `g_b` and the fingerprint of the new key.
    AcceptKey {
        /// Id of the exchange.
        exchange_id: i64,
        /// `g_b`.
        g_b: Vec<u8>,
        /// Fingerprint of the new key.
        key_fingerprint: i64,
    },
    /// PFS: abort the exchange.
    AbortKey {
        /// Id of the exchange.
        exchange_id: i64,
    },
    /// PFS: step 3, the new key is in use from now on.
    CommitKey {
        /// Id of the exchange.
        exchange_id: i64,
        /// Fingerprint of the new key.
        key_fingerprint: i64,
    },
    /// Does nothing (makes the peer use the new key).
    Noop,
}

/// `SendMessageAction` of the secret schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SendAction {
    Typing,
    Cancel,
    RecordVideo,
    UploadVideo,
    RecordAudio,
    UploadAudio,
    UploadPhoto,
    UploadDocument,
    GeoLocation,
    ChooseContact,
    RecordRound,
    UploadRound,
}

/// `DecryptedMessageMedia`. `key`/`iv` are the file's AES key and iv (see [`crate::file`]).
#[derive(Clone, Debug, PartialEq)]
#[allow(missing_docs)]
pub enum Media {
    Empty,
    Photo {
        thumb: Vec<u8>,
        thumb_w: i32,
        thumb_h: i32,
        w: i32,
        h: i32,
        size: i32,
        key: Vec<u8>,
        iv: Vec<u8>,
        caption: String,
    },
    Video {
        thumb: Vec<u8>,
        thumb_w: i32,
        thumb_h: i32,
        duration: i32,
        mime_type: String,
        w: i32,
        h: i32,
        size: i32,
        key: Vec<u8>,
        iv: Vec<u8>,
        caption: String,
    },
    Document {
        thumb: Vec<u8>,
        thumb_w: i32,
        thumb_h: i32,
        mime_type: String,
        size: i64,
        key: Vec<u8>,
        iv: Vec<u8>,
        attributes: Vec<DocumentAttribute>,
        caption: String,
    },
    Audio {
        duration: i32,
        mime_type: String,
        size: i32,
        key: Vec<u8>,
        iv: Vec<u8>,
    },
    GeoPoint {
        lat: f64,
        long: f64,
    },
    Contact {
        phone_number: String,
        first_name: String,
        last_name: String,
        user_id: i32,
    },
    Venue {
        lat: f64,
        long: f64,
        title: String,
        address: String,
        provider: String,
        venue_id: String,
    },
    WebPage {
        url: String,
    },
}

/// `DocumentAttribute` of the secret schema.
#[derive(Clone, Debug, PartialEq)]
#[allow(missing_docs)]
pub enum DocumentAttribute {
    ImageSize {
        w: i32,
        h: i32,
    },
    Animated,
    Sticker {
        alt: String,
        set: StickerSet,
    },
    Video {
        round_message: bool,
        duration: i32,
        w: i32,
        h: i32,
    },
    Audio {
        voice: bool,
        duration: i32,
        title: Option<String>,
        performer: Option<String>,
        waveform: Option<Vec<u8>>,
    },
    Filename(String),
}

/// `InputStickerSet` of the secret schema.
#[derive(Clone, Debug, PartialEq)]
pub enum StickerSet {
    /// No set.
    Empty,
    /// A set by its short name.
    ShortName(String),
}

// ---- writing

struct W(Vec<u8>);

impl W {
    fn put(&mut self, x: &impl Serializable) {
        x.serialize(&mut |b| self.0.extend_from_slice(b));
    }
    fn id(&mut self, id: u32) {
        self.put(&id);
    }
    fn bytes(&mut self, b: &[u8]) {
        self.put(&b);
    }
    fn flags(&mut self, flags: &[(u32, bool)]) {
        self.put(
            &flags
                .iter()
                .filter(|(_, on)| *on)
                .fold(0u32, |f, (bit, _)| f | 1 << bit),
        );
    }
    fn vec<T>(&mut self, items: &[T], mut f: impl FnMut(&mut W, &T) -> Result<()>) -> Result<()> {
        self.id(VECTOR);
        self.put(&(items.len() as i32));
        items.iter().try_for_each(|i| f(self, i))
    }
}

impl Layer {
    /// Serializes the envelope with the constructors of [`Layer::layer`].
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.layer < MIN_LAYER {
            return Err(Error::UnsupportedLayer(self.layer));
        }
        let mut w = W(Vec::new());
        w.id(0x1be31789);
        w.bytes(&self.random_bytes);
        w.put(&self.layer);
        w.put(&self.in_seq_no);
        w.put(&self.out_seq_no);
        match &self.message {
            DecryptedMessage::Message(m) => write_message(&mut w, m, self.layer)?,
            DecryptedMessage::Service(s) => {
                w.id(0x73164160);
                w.put(&s.random_id);
                write_action(&mut w, &s.action);
            }
        }
        Ok(w.0)
    }
}

fn write_message(w: &mut W, m: &Message, layer: i32) -> Result<()> {
    let v73 = layer >= LAYER_MESSAGE_V73;
    w.id(if v73 { 0x91cc4674 } else { 0x36b091de });
    w.flags(&[
        (5, v73 && m.silent),
        (9, m.media.is_some()),
        (7, m.entities.is_some()),
        (11, m.via_bot_name.is_some()),
        (3, m.reply_to_random_id.is_some()),
        (17, v73 && m.grouped_id.is_some()),
    ]);
    w.put(&m.random_id);
    w.put(&m.ttl);
    w.put(&m.message);
    if let Some(media) = &m.media {
        write_media(w, media, layer)?;
    }
    if let Some(entities) = &m.entities {
        w.put(entities);
    }
    if let Some(name) = &m.via_bot_name {
        w.put(name);
    }
    if let Some(id) = m.reply_to_random_id {
        w.put(&id);
    }
    if v73 && let Some(id) = m.grouped_id {
        w.put(&id);
    }
    Ok(())
}

fn write_action(w: &mut W, action: &Action) {
    match action {
        Action::SetMessageTtl { ttl_seconds } => {
            w.id(0xa1733aec);
            w.put(ttl_seconds);
        }
        Action::ReadMessages { random_ids } => {
            w.id(0x0c4f40be);
            w.put(random_ids);
        }
        Action::DeleteMessages { random_ids } => {
            w.id(0x65614304);
            w.put(random_ids);
        }
        Action::ScreenshotMessages { random_ids } => {
            w.id(0x8ac1f475);
            w.put(random_ids);
        }
        Action::FlushHistory => w.id(0x6719e45c),
        Action::Resend {
            start_seq_no,
            end_seq_no,
        } => {
            w.id(0x511110b0);
            w.put(start_seq_no);
            w.put(end_seq_no);
        }
        Action::NotifyLayer { layer } => {
            w.id(0xf3048883);
            w.put(layer);
        }
        Action::Typing { action } => {
            w.id(0xccb27641);
            w.id(send_action_id(*action));
        }
        Action::RequestKey { exchange_id, g_a } => {
            w.id(0xf3c9611b);
            w.put(exchange_id);
            w.bytes(g_a);
        }
        Action::AcceptKey {
            exchange_id,
            g_b,
            key_fingerprint,
        } => {
            w.id(0x6fe1735b);
            w.put(exchange_id);
            w.bytes(g_b);
            w.put(key_fingerprint);
        }
        Action::AbortKey { exchange_id } => {
            w.id(0xdd05ec6b);
            w.put(exchange_id);
        }
        Action::CommitKey {
            exchange_id,
            key_fingerprint,
        } => {
            w.id(0xec2e0b9b);
            w.put(exchange_id);
            w.put(key_fingerprint);
        }
        Action::Noop => w.id(0xa82fdd63),
    }
}

const SEND_ACTIONS: [(u32, SendAction); 12] = [
    (0x16bf744e, SendAction::Typing),
    (0xfd5ec8f5, SendAction::Cancel),
    (0xa187d66f, SendAction::RecordVideo),
    (0x92042ff7, SendAction::UploadVideo),
    (0xd52f73f7, SendAction::RecordAudio),
    (0xe6ac8a6f, SendAction::UploadAudio),
    (0x990a3c1a, SendAction::UploadPhoto),
    (0x8faee98e, SendAction::UploadDocument),
    (0x176f8ba1, SendAction::GeoLocation),
    (0x628cbc6f, SendAction::ChooseContact),
    (0x88f27fbc, SendAction::RecordRound),
    (0xbb718624, SendAction::UploadRound),
];

fn send_action_id(action: SendAction) -> u32 {
    SEND_ACTIONS
        .iter()
        .find(|(_, a)| *a == action)
        .map(|(id, _)| *id)
        .expect("every action is listed")
}

fn write_media(w: &mut W, media: &Media, layer: i32) -> Result<()> {
    match media {
        Media::Empty => w.id(0x089f5c4a),
        Media::Photo {
            thumb,
            thumb_w,
            thumb_h,
            w: width,
            h,
            size,
            key,
            iv,
            caption,
        } => {
            w.id(0xf1fa8d78);
            w.bytes(thumb);
            w.put(thumb_w);
            w.put(thumb_h);
            w.put(width);
            w.put(h);
            w.put(size);
            w.bytes(key);
            w.bytes(iv);
            w.put(caption);
        }
        Media::Video {
            thumb,
            thumb_w,
            thumb_h,
            duration,
            mime_type,
            w: width,
            h,
            size,
            key,
            iv,
            caption,
        } => {
            w.id(0x970c8c0e);
            w.bytes(thumb);
            w.put(thumb_w);
            w.put(thumb_h);
            w.put(duration);
            w.put(mime_type);
            w.put(width);
            w.put(h);
            w.put(size);
            w.bytes(key);
            w.bytes(iv);
            w.put(caption);
        }
        Media::Document {
            thumb,
            thumb_w,
            thumb_h,
            mime_type,
            size,
            key,
            iv,
            attributes,
            caption,
        } => {
            let wide = layer >= LAYER_DOCUMENT_V143;
            w.id(if wide { 0x6abd9782 } else { 0x7afe8ae2 });
            w.bytes(thumb);
            w.put(thumb_w);
            w.put(thumb_h);
            w.put(mime_type);
            if wide {
                w.put(size);
            } else {
                // before layer 143 the size is 32-bit
                w.put(&i32::try_from(*size).map_err(|_| Error::UnsupportedLayer(layer))?);
            }
            w.bytes(key);
            w.bytes(iv);
            w.vec(attributes, |w, a| {
                write_attribute(w, a);
                Ok(())
            })?;
            w.put(caption);
        }
        Media::Audio {
            duration,
            mime_type,
            size,
            key,
            iv,
        } => {
            w.id(0x57e0a9cb);
            w.put(duration);
            w.put(mime_type);
            w.put(size);
            w.bytes(key);
            w.bytes(iv);
        }
        Media::GeoPoint { lat, long } => {
            w.id(0x35480a59);
            w.put(lat);
            w.put(long);
        }
        Media::Contact {
            phone_number,
            first_name,
            last_name,
            user_id,
        } => {
            w.id(0x588a0a97);
            w.put(phone_number);
            w.put(first_name);
            w.put(last_name);
            w.put(user_id);
        }
        Media::Venue {
            lat,
            long,
            title,
            address,
            provider,
            venue_id,
        } => {
            w.id(0x8a0df56f);
            w.put(lat);
            w.put(long);
            w.put(title);
            w.put(address);
            w.put(provider);
            w.put(venue_id);
        }
        Media::WebPage { url } => {
            w.id(0xe50511d8);
            w.put(url);
        }
    }
    Ok(())
}

fn write_attribute(w: &mut W, attribute: &DocumentAttribute) {
    match attribute {
        DocumentAttribute::ImageSize { w: width, h } => {
            w.id(0x6c37c15c);
            w.put(width);
            w.put(h);
        }
        DocumentAttribute::Animated => w.id(0x11b58939),
        DocumentAttribute::Sticker { alt, set } => {
            w.id(0x3a556302);
            w.put(alt);
            match set {
                StickerSet::Empty => w.id(0xffb62b95),
                StickerSet::ShortName(name) => {
                    w.id(0x861cc8a0);
                    w.put(name);
                }
            }
        }
        DocumentAttribute::Video {
            round_message,
            duration,
            w: width,
            h,
        } => {
            w.id(0x0ef02ce6);
            w.flags(&[(0, *round_message)]);
            w.put(duration);
            w.put(width);
            w.put(h);
        }
        DocumentAttribute::Audio {
            voice,
            duration,
            title,
            performer,
            waveform,
        } => {
            w.id(0x9852f9c6);
            w.flags(&[
                (10, *voice),
                (0, title.is_some()),
                (1, performer.is_some()),
                (2, waveform.is_some()),
            ]);
            w.put(duration);
            if let Some(t) = title {
                w.put(t);
            }
            if let Some(p) = performer {
                w.put(p);
            }
            if let Some(b) = waveform {
                w.bytes(b);
            }
        }
        DocumentAttribute::Filename(name) => {
            w.id(0x15590068);
            w.put(name);
        }
    }
}

// ---- reading

type C<'a, 'b> = &'a mut Cursor<'b>;

fn unexpected(id: u32) -> Error {
    Error::Deserialize(deserialize::Error::UnexpectedConstructor { id })
}

fn read_vec<T>(c: C, mut f: impl FnMut(C) -> Result<T>) -> Result<Vec<T>> {
    let id = u32::deserialize(c)?;
    if id != VECTOR {
        return Err(unexpected(id));
    }
    let len = i32::deserialize(c)?;
    let len = usize::try_from(len).map_err(|_| Error::BadLength)?;
    // no pre-allocation: `len` is untrusted and every item takes at least 4 bytes
    let mut out = Vec::new();
    for _ in 0..len {
        out.push(f(c)?);
    }
    Ok(out)
}

impl Layer {
    /// Parses a decrypted payload. Checks `random_bytes` is at least 15 bytes long.
    pub fn decode(data: &[u8]) -> Result<Layer> {
        let c = &mut Cursor::from_slice(data);
        let id = u32::deserialize(c)?;
        if id != 0x1be31789 {
            return Err(unexpected(id));
        }
        let random_bytes = Vec::<u8>::deserialize(c)?;
        if random_bytes.len() < 15 {
            return Err(Error::BadRandomBytes);
        }
        let layer = i32::deserialize(c)?;
        let in_seq_no = i32::deserialize(c)?;
        let out_seq_no = i32::deserialize(c)?;
        let message = match u32::deserialize(c)? {
            id @ (0x91cc4674 | 0x36b091de) => {
                DecryptedMessage::Message(read_message(c, id == 0x91cc4674)?)
            }
            0x73164160 => DecryptedMessage::Service(Service {
                random_id: i64::deserialize(c)?,
                action: read_action(c)?,
            }),
            id => return Err(unexpected(id)),
        };
        Ok(Layer {
            random_bytes,
            layer,
            in_seq_no,
            out_seq_no,
            message,
        })
    }
}

fn read_message(c: C, v73: bool) -> Result<Message> {
    let flags = u32::deserialize(c)?;
    let has = |bit: u32| flags & (1 << bit) != 0;
    let mut m = Message {
        silent: v73 && has(5),
        random_id: i64::deserialize(c)?,
        ttl: i32::deserialize(c)?,
        message: String::deserialize(c)?,
        ..Message::default()
    };
    if has(9) {
        m.media = Some(read_media(c)?);
    }
    if has(7) {
        m.entities = Some(Vec::<MessageEntity>::deserialize(c)?);
    }
    if has(11) {
        m.via_bot_name = Some(String::deserialize(c)?);
    }
    if has(3) {
        m.reply_to_random_id = Some(i64::deserialize(c)?);
    }
    if v73 && has(17) {
        m.grouped_id = Some(i64::deserialize(c)?);
    }
    Ok(m)
}

fn read_action(c: C) -> Result<Action> {
    Ok(match u32::deserialize(c)? {
        0xa1733aec => Action::SetMessageTtl {
            ttl_seconds: i32::deserialize(c)?,
        },
        0x0c4f40be => Action::ReadMessages {
            random_ids: Vec::<i64>::deserialize(c)?,
        },
        0x65614304 => Action::DeleteMessages {
            random_ids: Vec::<i64>::deserialize(c)?,
        },
        0x8ac1f475 => Action::ScreenshotMessages {
            random_ids: Vec::<i64>::deserialize(c)?,
        },
        0x6719e45c => Action::FlushHistory,
        0x511110b0 => Action::Resend {
            start_seq_no: i32::deserialize(c)?,
            end_seq_no: i32::deserialize(c)?,
        },
        0xf3048883 => Action::NotifyLayer {
            layer: i32::deserialize(c)?,
        },
        0xccb27641 => {
            let id = u32::deserialize(c)?;
            let action = SEND_ACTIONS
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, a)| *a)
                .ok_or_else(|| unexpected(id))?;
            Action::Typing { action }
        }
        0xf3c9611b => Action::RequestKey {
            exchange_id: i64::deserialize(c)?,
            g_a: Vec::<u8>::deserialize(c)?,
        },
        0x6fe1735b => Action::AcceptKey {
            exchange_id: i64::deserialize(c)?,
            g_b: Vec::<u8>::deserialize(c)?,
            key_fingerprint: i64::deserialize(c)?,
        },
        0xdd05ec6b => Action::AbortKey {
            exchange_id: i64::deserialize(c)?,
        },
        0xec2e0b9b => Action::CommitKey {
            exchange_id: i64::deserialize(c)?,
            key_fingerprint: i64::deserialize(c)?,
        },
        0xa82fdd63 => Action::Noop,
        id => return Err(unexpected(id)),
    })
}

fn read_media(c: C) -> Result<Media> {
    Ok(match u32::deserialize(c)? {
        0x089f5c4a => Media::Empty,
        0xf1fa8d78 => Media::Photo {
            thumb: Vec::<u8>::deserialize(c)?,
            thumb_w: i32::deserialize(c)?,
            thumb_h: i32::deserialize(c)?,
            w: i32::deserialize(c)?,
            h: i32::deserialize(c)?,
            size: i32::deserialize(c)?,
            key: Vec::<u8>::deserialize(c)?,
            iv: Vec::<u8>::deserialize(c)?,
            caption: String::deserialize(c)?,
        },
        0x970c8c0e => Media::Video {
            thumb: Vec::<u8>::deserialize(c)?,
            thumb_w: i32::deserialize(c)?,
            thumb_h: i32::deserialize(c)?,
            duration: i32::deserialize(c)?,
            mime_type: String::deserialize(c)?,
            w: i32::deserialize(c)?,
            h: i32::deserialize(c)?,
            size: i32::deserialize(c)?,
            key: Vec::<u8>::deserialize(c)?,
            iv: Vec::<u8>::deserialize(c)?,
            caption: String::deserialize(c)?,
        },
        id @ (0x6abd9782 | 0x7afe8ae2) => Media::Document {
            thumb: Vec::<u8>::deserialize(c)?,
            thumb_w: i32::deserialize(c)?,
            thumb_h: i32::deserialize(c)?,
            mime_type: String::deserialize(c)?,
            size: if id == 0x6abd9782 {
                i64::deserialize(c)?
            } else {
                i64::from(i32::deserialize(c)?)
            },
            key: Vec::<u8>::deserialize(c)?,
            iv: Vec::<u8>::deserialize(c)?,
            attributes: read_vec(c, read_attribute)?,
            caption: String::deserialize(c)?,
        },
        0x57e0a9cb => Media::Audio {
            duration: i32::deserialize(c)?,
            mime_type: String::deserialize(c)?,
            size: i32::deserialize(c)?,
            key: Vec::<u8>::deserialize(c)?,
            iv: Vec::<u8>::deserialize(c)?,
        },
        0x35480a59 => Media::GeoPoint {
            lat: f64::deserialize(c)?,
            long: f64::deserialize(c)?,
        },
        0x588a0a97 => Media::Contact {
            phone_number: String::deserialize(c)?,
            first_name: String::deserialize(c)?,
            last_name: String::deserialize(c)?,
            user_id: i32::deserialize(c)?,
        },
        0x8a0df56f => Media::Venue {
            lat: f64::deserialize(c)?,
            long: f64::deserialize(c)?,
            title: String::deserialize(c)?,
            address: String::deserialize(c)?,
            provider: String::deserialize(c)?,
            venue_id: String::deserialize(c)?,
        },
        0xe50511d8 => Media::WebPage {
            url: String::deserialize(c)?,
        },
        id => return Err(unexpected(id)),
    })
}

fn read_attribute(c: C) -> Result<DocumentAttribute> {
    Ok(match u32::deserialize(c)? {
        0x6c37c15c => DocumentAttribute::ImageSize {
            w: i32::deserialize(c)?,
            h: i32::deserialize(c)?,
        },
        0x11b58939 => DocumentAttribute::Animated,
        0x3a556302 => DocumentAttribute::Sticker {
            alt: String::deserialize(c)?,
            set: match u32::deserialize(c)? {
                0xffb62b95 => StickerSet::Empty,
                0x861cc8a0 => StickerSet::ShortName(String::deserialize(c)?),
                id => return Err(unexpected(id)),
            },
        },
        0x0ef02ce6 => {
            let flags = u32::deserialize(c)?;
            DocumentAttribute::Video {
                round_message: flags & 1 != 0,
                duration: i32::deserialize(c)?,
                w: i32::deserialize(c)?,
                h: i32::deserialize(c)?,
            }
        }
        0x9852f9c6 => {
            let flags = u32::deserialize(c)?;
            let has = |bit: u32| flags & (1 << bit) != 0;
            let duration = i32::deserialize(c)?;
            DocumentAttribute::Audio {
                voice: has(10),
                duration,
                title: if has(0) {
                    Some(String::deserialize(c)?)
                } else {
                    None
                },
                performer: if has(1) {
                    Some(String::deserialize(c)?)
                } else {
                    None
                },
                waveform: if has(2) {
                    Some(Vec::<u8>::deserialize(c)?)
                } else {
                    None
                },
            }
        }
        0x15590068 => DocumentAttribute::Filename(String::deserialize(c)?),
        id => return Err(unexpected(id)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(layer: i32, message: DecryptedMessage) -> Layer {
        Layer {
            random_bytes: vec![7; 15],
            layer,
            in_seq_no: 3,
            out_seq_no: 4,
            message,
        }
    }

    fn round(layer_no: i32, message: DecryptedMessage) -> Layer {
        let l = layer(layer_no, message);
        let back = Layer::decode(&l.encode().unwrap()).unwrap();
        assert_eq!(back, l);
        back
    }

    fn msg(media: Option<Media>) -> Message {
        Message {
            silent: true,
            random_id: -5,
            ttl: 60,
            message: "text".into(),
            media,
            entities: Some(vec![MessageEntity::Bold(
                mtg_tl_types::types::MessageEntityBold {
                    offset: 0,
                    length: 2,
                },
            )]),
            via_bot_name: Some("bot".into()),
            reply_to_random_id: Some(9),
            grouped_id: Some(10),
        }
    }

    #[test]
    fn every_service_action_round_trips() {
        let actions = [
            Action::SetMessageTtl { ttl_seconds: 5 },
            Action::ReadMessages {
                random_ids: vec![1, -2],
            },
            Action::DeleteMessages { random_ids: vec![] },
            Action::ScreenshotMessages {
                random_ids: vec![3],
            },
            Action::FlushHistory,
            Action::Resend {
                start_seq_no: 2,
                end_seq_no: 8,
            },
            Action::NotifyLayer { layer: LAYER },
            Action::RequestKey {
                exchange_id: 1,
                g_a: vec![1, 2, 3],
            },
            Action::AcceptKey {
                exchange_id: 1,
                g_b: vec![4; 256],
                key_fingerprint: -1,
            },
            Action::AbortKey { exchange_id: 2 },
            Action::CommitKey {
                exchange_id: 2,
                key_fingerprint: 77,
            },
            Action::Noop,
        ];
        for action in actions {
            let m = DecryptedMessage::Service(Service {
                random_id: 8,
                action,
            });
            assert_eq!(m.random_id(), 8);
            round(LAYER, m);
        }
    }

    #[test]
    fn every_typing_action_round_trips() {
        for (_, action) in SEND_ACTIONS {
            round(
                LAYER,
                DecryptedMessage::Service(Service {
                    random_id: 1,
                    action: Action::Typing { action },
                }),
            );
        }
    }

    #[test]
    fn every_media_round_trips() {
        let attributes = vec![
            DocumentAttribute::ImageSize { w: 1, h: 2 },
            DocumentAttribute::Animated,
            DocumentAttribute::Sticker {
                alt: "x".into(),
                set: StickerSet::Empty,
            },
            DocumentAttribute::Sticker {
                alt: "y".into(),
                set: StickerSet::ShortName("set".into()),
            },
            DocumentAttribute::Video {
                round_message: true,
                duration: 3,
                w: 4,
                h: 5,
            },
            DocumentAttribute::Video {
                round_message: false,
                duration: 3,
                w: 4,
                h: 5,
            },
            DocumentAttribute::Audio {
                voice: true,
                duration: 9,
                title: Some("t".into()),
                performer: Some("p".into()),
                waveform: Some(vec![1, 2]),
            },
            DocumentAttribute::Audio {
                voice: false,
                duration: 9,
                title: None,
                performer: None,
                waveform: None,
            },
            DocumentAttribute::Filename("a.txt".into()),
        ];
        let media = [
            Media::Empty,
            Media::Photo {
                thumb: vec![1],
                thumb_w: 2,
                thumb_h: 3,
                w: 4,
                h: 5,
                size: 6,
                key: vec![7; 32],
                iv: vec![8; 32],
                caption: "c".into(),
            },
            Media::Video {
                thumb: vec![],
                thumb_w: 2,
                thumb_h: 3,
                duration: 4,
                mime_type: "video/mp4".into(),
                w: 5,
                h: 6,
                size: 7,
                key: vec![8; 32],
                iv: vec![9; 32],
                caption: String::new(),
            },
            Media::Document {
                thumb: vec![1],
                thumb_w: 1,
                thumb_h: 1,
                mime_type: "a/b".into(),
                size: 5_000_000_000,
                key: vec![1; 32],
                iv: vec![2; 32],
                attributes: attributes.clone(),
                caption: "d".into(),
            },
            Media::Audio {
                duration: 1,
                mime_type: "audio/ogg".into(),
                size: 2,
                key: vec![3; 32],
                iv: vec![4; 32],
            },
            Media::GeoPoint {
                lat: 55.5,
                long: 37.5,
            },
            Media::Contact {
                phone_number: "1".into(),
                first_name: "a".into(),
                last_name: "b".into(),
                user_id: 4,
            },
            Media::Venue {
                lat: 1.0,
                long: 2.0,
                title: "t".into(),
                address: "a".into(),
                provider: "p".into(),
                venue_id: "v".into(),
            },
            Media::WebPage {
                url: "https://t.me".into(),
            },
        ];
        for m in media {
            round(LAYER, DecryptedMessage::Message(msg(Some(m))));
        }
    }

    #[test]
    fn a_plain_message_round_trips_and_older_layers_drop_new_fields() {
        let plain = Message {
            random_id: 1,
            message: "hi".into(),
            ..Message::default()
        };
        round(LAYER, DecryptedMessage::Message(plain));
        // layer 46..72: no silent, no grouped_id on the wire
        let old = Layer::decode(
            &layer(46, DecryptedMessage::Message(msg(None)))
                .encode()
                .unwrap(),
        )
        .unwrap();
        let DecryptedMessage::Message(m) = old.message else {
            panic!()
        };
        assert!(!m.silent);
        assert_eq!(m.grouped_id, None);
        assert_eq!(m.reply_to_random_id, Some(9));
        assert_eq!(m.via_bot_name.as_deref(), Some("bot"));
    }

    #[test]
    fn a_document_before_layer_143_has_a_32_bit_size() {
        let doc = |size| {
            DecryptedMessage::Message(msg(Some(Media::Document {
                thumb: vec![],
                thumb_w: 0,
                thumb_h: 0,
                mime_type: "a/b".into(),
                size,
                key: vec![],
                iv: vec![],
                attributes: vec![],
                caption: String::new(),
            })))
        };
        round(100, doc(1234));
        assert!(matches!(
            layer(100, doc(5_000_000_000)).encode(),
            Err(Error::UnsupportedLayer(100))
        ));
        assert!(matches!(
            layer(45, doc(1)).encode(),
            Err(Error::UnsupportedLayer(45))
        ));
    }

    #[test]
    fn bad_input_is_an_error() {
        let good = layer(LAYER, DecryptedMessage::Message(msg(Some(Media::Empty))))
            .encode()
            .unwrap();
        // every truncation fails cleanly
        for n in 0..good.len() {
            assert!(Layer::decode(&good[..n]).is_err(), "{n}");
        }
        let unexpected = |data: &[u8]| matches!(Layer::decode(data), Err(Error::Deserialize(_)));
        assert!(unexpected(&0u32.to_le_bytes()));
        // short random_bytes
        let short = layer(
            LAYER,
            DecryptedMessage::Service(Service {
                random_id: 1,
                action: Action::Noop,
            }),
        );
        let mut raw = short.encode().unwrap();
        raw[4] = 14;
        assert!(matches!(Layer::decode(&raw), Err(Error::BadRandomBytes)));
        // unknown message / action / media / attribute / sticker set / typing / vector ids
        let patch = |raw: &[u8], at: usize| {
            let mut r = raw.to_vec();
            r[at..at + 4].copy_from_slice(&0xdeadbeefu32.to_le_bytes());
            Layer::decode(&r)
        };
        let base = short.encode().unwrap();
        let at_message = 4 + 16 + 12; // id, random_bytes (15 + length byte), layer, in, out
        assert!(unexpected_id(patch(&base, at_message)));
        assert!(unexpected_id(patch(&base, at_message + 12)));
        let mut typing = layer(
            LAYER,
            DecryptedMessage::Service(Service {
                random_id: 1,
                action: Action::Typing {
                    action: SendAction::Typing,
                },
            }),
        )
        .encode()
        .unwrap();
        let n = typing.len();
        typing[n - 4..].copy_from_slice(&0xdeadbeefu32.to_le_bytes());
        assert!(unexpected_id(Layer::decode(&typing)));
        // media, attribute, sticker set and a vector without its marker
        let media = |m| {
            layer(
                LAYER,
                DecryptedMessage::Message(Message {
                    media: Some(m),
                    ..Message::default()
                }),
            )
        };
        let raw = media(Media::Empty).encode().unwrap();
        let pos = raw
            .windows(4)
            .position(|w| w == 0x089f5c4au32.to_le_bytes())
            .unwrap();
        assert!(unexpected_id(patch(&raw, pos)));
        let doc = media(Media::Document {
            thumb: vec![],
            thumb_w: 0,
            thumb_h: 0,
            mime_type: String::new(),
            size: 0,
            key: vec![],
            iv: vec![],
            attributes: vec![DocumentAttribute::Sticker {
                alt: String::new(),
                set: StickerSet::Empty,
            }],
            caption: String::new(),
        })
        .encode()
        .unwrap();
        for id in [0x1cb5c415u32, 0x3a556302, 0xffb62b95] {
            let pos = doc.windows(4).position(|w| w == id.to_le_bytes()).unwrap();
            assert!(unexpected_id(patch(&doc, pos)), "{id:x}");
        }
        // a negative vector length
        let pos = doc
            .windows(4)
            .position(|w| w == 0x1cb5c415u32.to_le_bytes())
            .unwrap();
        let mut neg = doc.clone();
        neg[pos + 4..pos + 8].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(matches!(Layer::decode(&neg), Err(Error::BadLength)));
        // a huge vector length runs out of data instead of allocating
        let mut huge = doc;
        huge[pos + 4..pos + 8].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(Layer::decode(&huge).is_err());
    }

    fn unexpected_id(r: Result<Layer>) -> bool {
        matches!(
            r,
            Err(Error::Deserialize(
                deserialize::Error::UnexpectedConstructor { id: 0xdeadbeef }
            ))
        )
    }
}
