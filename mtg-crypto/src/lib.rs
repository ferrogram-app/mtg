// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This library contains a collection of functions that relate to
//! encrypting and decryption values when exchanging messages with Telegram.

#![deny(unsafe_code)]

pub mod aes;
mod auth_key;
mod factorize;
pub mod hex;
mod obfuscated;
pub mod rsa;
mod sha;
pub use sha::{hmac_sha256, sha1_parts, sha256_parts};
pub mod two_factor_auth;

use std::fmt;

pub use auth_key::AuthKey;
pub use factorize::factorize;
use glass_pumpkin::safe_prime;
use num_bigint::BigUint;
use num_traits::ToPrimitive;
pub use obfuscated::ObfuscatedCipher;

/// The error type for [`decrypt_data_v2`].
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// The ciphertext is either too small or not padded correctly.
    InvalidBuffer,

    /// The server replied with the ID of a different authorization key.
    AuthKeyMismatch,

    /// The key of the message did not match our expectations.
    MessageKeyMismatch,
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::InvalidBuffer => write!(f, "invalid ciphertext buffer length"),
            Error::AuthKeyMismatch => write!(f, "server authkey mismatches with ours"),
            Error::MessageKeyMismatch => write!(f, "server msgkey mismatches with ours"),
        }
    }
}

enum Side {
    Client,
    Server,
}

impl Side {
    // "where x = 0 for messages from client to server and x = 8 for those from server to client."
    fn x(&self) -> usize {
        match *self {
            Side::Client => 0,
            Side::Server => 8,
        }
    }
}

/// Calculate the key based on Telegram [guidelines for MTProto 2],
/// returning the pair `(key, iv)` for use in AES-IGE mode.
///
/// [guidelines for MTProto 2]: https://core.telegram.org/mtproto/description#defining-aes-key-and-initialization-vector
fn calc_key(auth_key: &AuthKey, msg_key: &[u8; 16], side: Side) -> ([u8; 32], [u8; 32]) {
    let x = side.x();

    // sha256_a = SHA256 (msg_key + substr (auth_key, x, 36));
    let sha256_a = sha256!(msg_key, &auth_key.data[x..x + 36]);

    // sha256_b = SHA256 (substr (auth_key, 40+x, 36) + msg_key);
    let sha256_b = sha256!(&auth_key.data[40 + x..40 + x + 36], msg_key);

    // aes_key = substr (sha256_a, 0, 8) + substr (sha256_b, 8, 16) + substr (sha256_a, 24, 8);
    let aes_key = {
        let mut buffer = [0; 32];
        buffer[0..8].copy_from_slice(&sha256_a[0..8]);
        buffer[8..8 + 16].copy_from_slice(&sha256_b[8..8 + 16]);
        buffer[24..24 + 8].copy_from_slice(&sha256_a[24..24 + 8]);
        buffer
    };

    // aes_iv = substr (sha256_b, 0, 8) + substr (sha256_a, 8, 16) + substr (sha256_b, 24, 8);
    let aes_iv = {
        let mut buffer = [0; 32];
        buffer[0..8].copy_from_slice(&sha256_b[0..8]);
        buffer[8..8 + 16].copy_from_slice(&sha256_a[8..8 + 16]);
        buffer[24..24 + 8].copy_from_slice(&sha256_b[24..24 + 8]);
        buffer
    };

    (aes_key, aes_iv)
}

/// Maximum amount of padding that will be returned by `determine_padding_v2_length`.
pub const MAX_PADDING_LEN: usize = 32;

/// Determines the padding length needed for a plaintext of a certain length,
/// according to the following citation:
///
/// > Note that MTProto 2.0 requires from 12 to 1024 bytes of padding
/// > [...] the resulting message length be divisible by 16 bytes
fn determine_padding_v2_length(len: usize) -> usize {
    16 + (16 - (len % 16))
}

/// Generates random padding data and returns the amount of bytes from said
/// data that need to be appended to the buffer to make it correctly-sized.
pub fn get_padding_data_v2(buffer_len: usize) -> ([u8; MAX_PADDING_LEN], usize) {
    let random_padding = {
        let mut rnd = [0; 32];
        // An OS without randomness cannot be recovered from, and weak padding would weaken
        // msg_key: abort instead of continuing.
        getrandom::fill(&mut rnd).expect("failed to generate a secure padding");
        rnd
    };
    (random_padding, determine_padding_v2_length(buffer_len))
}

/// This function implements the [MTProto 2.0 algorithm] for computing
/// `aes_key` and `aes_iv` from `auth_key` and `msg_key` as specified.
///
/// The buffer must've been padded with [`get_padding_data_v2`]
/// before calling this function.
///
/// The returned `msg_key` should be prepended to the buffer first.
/// Then the `auth_key.key_id` should also be prepended to the buffer.
///
/// [MTProto 2.0 algorithm]: https://core.telegram.org/mtproto/description#defining-aes-key-and-initialization-vector
#[must_use]
pub fn encrypt_data_v2(buffer: &mut [u8], auth_key: &AuthKey) -> [u8; 16] {
    debug_assert_eq!(buffer.len() % 16, 0, "buffer should have been padded");

    // Encryption is done by the client
    let side = Side::Client;
    let x = side.x();

    // msg_key_large = SHA256 (substr (auth_key, 88+x, 32) + plaintext + random_padding);
    let msg_key_large = sha256!(&auth_key.data[88 + x..88 + x + 32], &buffer[..]);

    // msg_key = substr (msg_key_large, 8, 16);
    let msg_key = {
        let mut buffer = [0; 16];
        buffer.copy_from_slice(&msg_key_large[8..8 + 16]);
        buffer
    };

    // Calculate the key
    let (key, iv) = calc_key(auth_key, &msg_key, side);

    aes::ige_encrypt(&mut buffer[..], &key, &iv);
    msg_key
}

/// This method is the inverse of `encrypt_data_v2`.
/// Returns the slice of the input buffer that contains the decrypted plaintext.
pub fn decrypt_data_v2<'b>(
    buffer: &'b mut [u8],
    auth_key: &AuthKey,
) -> Result<&'b mut [u8], Error> {
    // Decryption is done from the server
    let side = Side::Server;
    let x = side.x();

    if buffer.len() < 24 || !(buffer.len() - 24).is_multiple_of(16) {
        return Err(Error::InvalidBuffer);
    }

    // TODO Check salt, session_id and sequence_number
    let key_id = &buffer[..8];
    if auth_key.key_id() != *key_id {
        return Err(Error::AuthKeyMismatch);
    }

    let mut msg_key = [0; 16];
    msg_key.copy_from_slice(&buffer[8..8 + 16]);

    let (key, iv) = calc_key(auth_key, &msg_key, Side::Server);

    aes::ige_decrypt(&mut buffer[8 + 16..], &key, &iv);
    let plaintext = &buffer[8 + 16..];

    // https://core.telegram.org/mtproto/security_guidelines#mtproto-encrypted-messages
    let our_key = sha256!(&auth_key.data[88 + x..88 + x + 32], &plaintext);

    // constant time: a mismatch must not reveal how many leading bytes were right
    if !constant_time_eq(&msg_key, &our_key[8..8 + 16]) {
        return Err(Error::MessageKeyMismatch);
    }

    Ok(&mut buffer[8 + 16..])
}

/// Generate the AES key and initialization vector from the server nonce
/// and the new client nonce. This is done after the DH exchange.
pub fn generate_key_data_from_nonce(
    server_nonce: &[u8; 16],
    new_nonce: &[u8; 32],
) -> ([u8; 32], [u8; 32]) {
    let hash1 = sha1!(new_nonce, server_nonce);
    let hash2 = sha1!(server_nonce, new_nonce);
    let hash3 = sha1!(new_nonce, new_nonce);

    // key = hash1 + hash2[:12]
    let key = {
        let mut buffer = [0; 32];
        buffer[..hash1.len()].copy_from_slice(&hash1);
        buffer[hash1.len()..].copy_from_slice(&hash2[..12]);
        buffer
    };

    // iv = hash2[12:20] + hash3 + new_nonce[:4]
    let iv = {
        let mut buffer = [0; 32];
        buffer[..8].copy_from_slice(&hash2[12..]);
        buffer[8..28].copy_from_slice(&hash3);
        buffer[28..].copy_from_slice(&new_nonce[..4]);
        buffer
    };

    (key, iv)
}

/// Validates the `dh_prime` is a safe 2048-bit Diffie-Hellman prime.
///
/// # Errors
///
/// Returns an error when operating-system randomness is unavailable.
pub fn check_dh_prime(dh_prime: &BigUint, g: i32) -> Result<bool, glass_pumpkin::error::Error> {
    if dh_prime.bits() != 2048 {
        return Ok(false);
    }

    let valid = match g {
        2 => (dh_prime % 8u32).to_u32() == Some(7),
        3 => (dh_prime % 3u32).to_u32() == Some(2),
        4 => true,
        5 => matches!((dh_prime % 5u32).to_u32(), Some(1) | Some(4)),
        6 => matches!((dh_prime % 24u32).to_u32(), Some(19) | Some(23)),
        7 => matches!((dh_prime % 7u32).to_u32(), Some(3) | Some(5) | Some(6)),
        _ => return Ok(false),
    };
    if !valid {
        return Ok(false);
    }

    // This check is the most expensive, so leave it for last.
    safe_prime::check(dh_prime)
}

/// Compares two byte strings without stopping at the first difference.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests;
