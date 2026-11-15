//! MTProto 2.0 encryption of secret chat messages
//! (<https://core.telegram.org/api/end-to-end>).
//!
//! Wire layout: `key_fingerprint` (8 bytes, little-endian) + `msg_key` (16) + AES-256-IGE
//! ciphertext of `len(4) + message + padding(12..1024)`.

use mtg_crypto::aes::{ige_decrypt, ige_encrypt};
use mtg_crypto::sha256_parts;

use crate::error::{Error, Result, random_fill};
use crate::key::Key;

const HEADER: usize = 8 + 16;
const MIN_PADDING: usize = 12;
const MAX_PADDING: usize = 1024;

/// `x` of the key derivation: 0 for messages sent by the originator of the chat, 8 for the rest.
fn x(from_originator: bool) -> usize {
    if from_originator { 0 } else { 8 }
}

fn msg_key_large(key: &Key, x: usize, plaintext: &[u8]) -> [u8; 32] {
    sha256_parts(&[&key.bytes()[88 + x..88 + x + 32], plaintext])
}

fn aes_key_iv(key: &Key, x: usize, msg_key: &[u8; 16]) -> ([u8; 32], [u8; 32]) {
    let k = key.bytes();
    let a = sha256_parts(&[msg_key, &k[x..x + 36]]);
    let b = sha256_parts(&[&k[40 + x..40 + x + 36], msg_key]);
    let mut aes_key = [0; 32];
    aes_key[..8].copy_from_slice(&a[..8]);
    aes_key[8..24].copy_from_slice(&b[8..24]);
    aes_key[24..].copy_from_slice(&a[24..]);
    let mut aes_iv = [0; 32];
    aes_iv[..8].copy_from_slice(&b[..8]);
    aes_iv[8..24].copy_from_slice(&a[8..24]);
    aes_iv[24..].copy_from_slice(&b[24..]);
    (aes_key, aes_iv)
}

/// The `key_fingerprint` a wire message starts with, to pick the key to decrypt with.
pub fn fingerprint_of_message(data: &[u8]) -> Result<i64> {
    let head: [u8; 8] = data
        .get(..8)
        .ok_or(Error::TooShort)?
        .try_into()
        .expect("8 bytes");
    Ok(i64::from_le_bytes(head))
}

/// Encrypts the serialized `decryptedMessageLayer` (`from_originator`: we started the chat).
pub fn encrypt(key: &Key, from_originator: bool, message: &[u8]) -> Vec<u8> {
    let mut random = [0u8; 1];
    random_fill(&mut random);
    let mut padding = MIN_PADDING + usize::from(random[0] % 64);
    padding += (16 - (4 + message.len() + padding) % 16) % 16;

    let mut plain = Vec::with_capacity(4 + message.len() + padding);
    plain.extend_from_slice(&(message.len() as u32).to_le_bytes());
    plain.extend_from_slice(message);
    plain.resize(4 + message.len() + padding, 0);
    random_fill(&mut plain[4 + message.len()..]);

    let x = x(from_originator);
    let msg_key: [u8; 16] = msg_key_large(key, x, &plain)[8..24]
        .try_into()
        .expect("16 bytes");
    let (aes_key, aes_iv) = aes_key_iv(key, x, &msg_key);
    ige_encrypt(&mut plain, &aes_key, &aes_iv);

    let mut out = Vec::with_capacity(HEADER + plain.len());
    out.extend_from_slice(&key.fingerprint().to_le_bytes());
    out.extend_from_slice(&msg_key);
    out.extend_from_slice(&plain);
    out
}

/// Decrypts a wire message sent by the originator (`from_originator`) or by the other side and
/// returns the serialized `decryptedMessageLayer`. The fingerprint is not checked here.
pub fn decrypt(key: &Key, from_originator: bool, data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < HEADER + 16 || !(data.len() - HEADER).is_multiple_of(16) {
        return Err(Error::TooShort);
    }
    let msg_key: [u8; 16] = data[8..HEADER].try_into().expect("16 bytes");
    let x = x(from_originator);
    let (aes_key, aes_iv) = aes_key_iv(key, x, &msg_key);
    let mut plain = data[HEADER..].to_vec();
    ige_decrypt(&mut plain, &aes_key, &aes_iv);

    // the check runs before anything is read from the plaintext
    let large = msg_key_large(key, x, &plain);
    if !constant_time_eq(&large[8..24], &msg_key) {
        return Err(Error::BadMsgKey);
    }
    let len = u32::from_le_bytes(plain[..4].try_into().expect("4 bytes")) as usize;
    let padding = (plain.len() - 4).checked_sub(len).ok_or(Error::BadLength)?;
    if !len.is_multiple_of(4) || !(MIN_PADDING..=MAX_PADDING).contains(&padding) {
        return Err(Error::BadLength);
    }
    Ok(plain[4..4 + len].to_vec())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> Key {
        Key::new(std::array::from_fn(|i| (i * 7 + 3) as u8))
    }

    #[test]
    fn a_message_round_trips_in_both_directions() {
        for from_originator in [true, false] {
            for len in [0usize, 4, 8, 100, 1000] {
                let message: Vec<u8> = (0..len).map(|i| i as u8).collect();
                let wire = encrypt(&key(), from_originator, &message);
                assert_eq!(fingerprint_of_message(&wire).unwrap(), key().fingerprint());
                assert_eq!((wire.len() - HEADER) % 16, 0);
                let plain_len = wire.len() - HEADER;
                assert!(plain_len - 4 - len >= MIN_PADDING && plain_len - 4 - len <= 1024);
                assert_eq!(decrypt(&key(), from_originator, &wire).unwrap(), message);
            }
        }
    }

    #[test]
    fn the_wrong_direction_or_key_fails_the_msg_key_check() {
        let wire = encrypt(&key(), true, &[1, 2, 3, 4]);
        assert_eq!(decrypt(&key(), false, &wire), Err(Error::BadMsgKey));
        let other = Key::new([9; 256]);
        assert_eq!(decrypt(&other, true, &wire), Err(Error::BadMsgKey));
    }

    #[test]
    fn any_flipped_byte_fails_the_msg_key_check() {
        let wire = encrypt(&key(), true, &[1, 2, 3, 4]);
        for i in 8..wire.len() {
            let mut bad = wire.clone();
            bad[i] ^= 1;
            assert_eq!(decrypt(&key(), true, &bad), Err(Error::BadMsgKey), "{i}");
        }
    }

    #[test]
    fn short_or_misaligned_data_is_rejected() {
        assert_eq!(fingerprint_of_message(&[0; 7]), Err(Error::TooShort));
        for len in [0, 8, 24, 39, 41] {
            assert_eq!(
                decrypt(&key(), true, &vec![0; len]),
                Err(Error::TooShort),
                "{len}"
            );
        }
    }

    /// Builds a wire message from raw plaintext, to feed the length checks.
    fn seal(plain: &[u8]) -> Vec<u8> {
        let mut plain = plain.to_vec();
        let msg_key: [u8; 16] = msg_key_large(&key(), 0, &plain)[8..24].try_into().unwrap();
        let (k, iv) = aes_key_iv(&key(), 0, &msg_key);
        ige_encrypt(&mut plain, &k, &iv);
        [&key().fingerprint().to_le_bytes()[..], &msg_key, &plain].concat()
    }

    #[test]
    fn a_bad_inner_length_or_padding_is_rejected() {
        let body = |len: u32, total: usize| {
            let mut v = len.to_le_bytes().to_vec();
            v.resize(total, 0);
            v
        };
        // length larger than the data, not divisible by 4, padding too small, padding too big
        for plain in [body(100, 32), body(6, 32), body(20, 32), body(4, 1056)] {
            assert_eq!(decrypt(&key(), true, &seal(&plain)), Err(Error::BadLength));
        }
        // the exact limits are accepted
        assert!(decrypt(&key(), true, &seal(&body(0, 16))).is_ok());
        assert!(decrypt(&key(), true, &seal(&body(12, 1040))).is_ok());
    }
}
