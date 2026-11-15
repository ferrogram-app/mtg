//! Telegram one-to-one calls (<https://core.telegram.org/api/end-to-end/voice-calls>), sans-IO:
//! the signalling state machine, the key exchange, the emoji fingerprint and the relay endpoints.
//! The media stack (audio, video, WebRTC) is not part of this crate: [`call::Connected`] hands
//! over what it needs (key, endpoints, protocol).
//!
//! * [`call`]: [`Calls`], all calls of an account driven by `updatePhoneCall`.
//! * [`emoji`]: the four emojis both users compare to check the key.
//!
//! Not done: group calls (`phone.joinGroupCall` needs a WebRTC offer), conference calls.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod call;
pub mod emoji;
mod emoji_table;

#[cfg(test)]
mod tests;
pub use call::{Calls, Connected, Endpoint, Event, Output, Request};
