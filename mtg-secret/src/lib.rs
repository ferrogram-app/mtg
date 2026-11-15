//! Telegram secret chats: end-to-end encrypted one-to-one chats
//! (<https://core.telegram.org/api/end-to-end>).
//!
//! The crate does no I/O. It builds the TL requests (`messages.requestEncryption`,
//! `sendEncrypted`, ...) and reads the updates (`updateEncryption`, `updateNewEncryptedMessage`,
//! ...); the application sends them over `mtg-client`, stores the chats ([`SecretChats::states`])
//! and shows the events.
//!
//! * [`dh`]: the Diffie-Hellman group, its checks and the key exchange.
//! * [`cipher`]: MTProto 2.0 encryption of one message.
//! * [`schema`]: the secret-layer objects (`decryptedMessageLayer`, messages, actions, media).
//! * [`chat`]: [`SecretChat`], the state of one chat: seq numbers, gaps and resends, layer
//!   negotiation and PFS rekeying.
//! * [`chats`]: [`SecretChats`], all chats of an account driven by the API updates.
//! * [`mod@file`]: encryption of attachments.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod chat;
pub mod chats;
pub mod cipher;
pub mod dh;
mod error;
pub mod file;
mod key;
pub mod schema;

#[cfg(test)]
mod tests;

pub use chat::{Event, Outgoing, Output, SecretChat};
pub use chats::SecretChats;
pub use error::{Error, Result};
pub use key::{Key, fingerprint_of, visualization};
