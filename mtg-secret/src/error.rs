use std::fmt;

use mtg_tl_types::deserialize;

/// Everything that can go wrong in a secret chat.
///
/// Only [`Error::Protocol`] means the chat is broken for good: the application should discard it
/// (`messages.discardEncryption`). Every other error concerns one message or one call; the chat
/// stays usable.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// `p`/`g` of the Diffie-Hellman group failed the checks of the security guidelines.
    BadDhParams,
    /// `g_a`/`g_b` is outside the allowed range (or the server sent a bad group).
    BadPublicValue,
    /// The fingerprint the peer reports does not match the key we computed.
    FingerprintMismatch,
    /// A message is encrypted with a key we do not have.
    UnknownKey,
    /// The encrypted data is too short or not a whole number of AES blocks.
    TooShort,
    /// The `msg_key` does not match the decrypted data (the message was altered or is not ours).
    BadMsgKey,
    /// The inner length prefix or the padding is invalid.
    BadLength,
    /// `decryptedMessageLayer.random_bytes` is shorter than 15 bytes.
    BadRandomBytes,
    /// The layer is below 46 (or the value does not fit the constructor of the negotiated layer).
    UnsupportedLayer(i32),
    /// The decrypted payload is not a valid secret chat object.
    Deserialize(deserialize::Error),
    /// The peer broke the protocol (bad seq_no, bad key exchange, ...): discard the chat.
    Protocol(&'static str),
    /// The call does not fit the current state of the chat or exchange.
    WrongState(&'static str),
    /// No chat with this id.
    NoSuchChat(i32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "secret chat: ")?;
        match self {
            Error::BadDhParams => write!(f, "bad Diffie-Hellman parameters"),
            Error::BadPublicValue => write!(f, "g_a/g_b out of range"),
            Error::FingerprintMismatch => write!(f, "key fingerprint mismatch"),
            Error::UnknownKey => write!(f, "message encrypted with an unknown key"),
            Error::TooShort => write!(f, "encrypted data too short"),
            Error::BadMsgKey => write!(f, "msg_key mismatch"),
            Error::BadLength => write!(f, "bad length or padding"),
            Error::BadRandomBytes => write!(f, "random_bytes shorter than 15"),
            Error::UnsupportedLayer(l) => write!(f, "unsupported layer {l}"),
            Error::Deserialize(e) => write!(f, "{e}"),
            Error::Protocol(why) => write!(f, "protocol violation: {why}"),
            Error::WrongState(why) => write!(f, "wrong state: {why}"),
            Error::NoSuchChat(id) => write!(f, "no chat {id}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<deserialize::Error> for Error {
    fn from(e: deserialize::Error) -> Self {
        Error::Deserialize(e)
    }
}

/// Result of the secret chat functions.
pub type Result<T> = std::result::Result<T, Error>;

/// Fills `buf` with operating-system randomness; no randomness means no safe key, so stop.
pub(crate) fn random_fill(buf: &mut [u8]) {
    getrandom::fill(buf).expect("operating system randomness is required");
}

/// A random `i64` (`random_id`).
pub(crate) fn random_i64() -> i64 {
    let mut b = [0u8; 8];
    random_fill(&mut b);
    i64::from_le_bytes(b)
}
