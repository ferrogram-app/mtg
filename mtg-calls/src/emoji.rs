//! The emoji fingerprint of a call key.

use mtg_crypto::sha256_parts;
use mtg_secret::Key;

use crate::emoji_table::EMOJIS;

/// The four emojis of the key: each is picked by 8 bytes of `SHA256(key)` (top bit cleared) modulo
/// the table size, as the official client does (`EncryptionKeyEmojifier.emojifyForCall`).
pub fn emoji(key: &Key) -> [&'static str; 4] {
    let hash = sha256_parts(&[key.bytes()]);
    std::array::from_fn(|i| {
        let mut word = [0u8; 8];
        word.copy_from_slice(&hash[8 * i..8 * i + 8]);
        let n = u64::from_be_bytes(word) & (u64::MAX >> 1);
        EMOJIS[(n % EMOJIS.len() as u64) as usize]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_333_distinct_emojis() {
        let mut all = EMOJIS.to_vec();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), 333);
    }

    #[test]
    fn the_emoji_are_a_function_of_the_key() {
        let a = Key::new([1; 256]);
        assert_eq!(emoji(&a), emoji(&Key::new([1; 256])));
        assert_ne!(emoji(&a), emoji(&Key::new([2; 256])));
        // worked by hand for a fixed hash: the index is word % 333
        let hash = sha256_parts(&[a.bytes()]);
        let mut w = [0u8; 8];
        w.copy_from_slice(&hash[..8]);
        let first = (u64::from_be_bytes(w) & (u64::MAX >> 1)) % 333;
        assert_eq!(emoji(&a)[0], EMOJIS[first as usize]);
    }
}
