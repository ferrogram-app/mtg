use std::fmt;

use mtg_crypto::{sha1_parts, sha256_parts};

/// The 256-byte shared key of a secret chat and its fingerprint.
#[derive(Clone, PartialEq)]
pub struct Key {
    bytes: [u8; 256],
    fingerprint: i64,
}

impl Key {
    /// Wraps the key bytes and computes the fingerprint.
    pub fn new(bytes: [u8; 256]) -> Self {
        Self {
            fingerprint: fingerprint_of(&bytes),
            bytes,
        }
    }

    /// The key bytes.
    pub fn bytes(&self) -> &[u8; 256] {
        &self.bytes
    }

    /// `key_fingerprint`: the 64 low-order bits of SHA-1 of the key.
    pub fn fingerprint(&self) -> i64 {
        self.fingerprint
    }
}

// never print key material
impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Key({:016x})", self.fingerprint)
    }
}

/// The last 8 bytes of SHA-1 of the key, read little-endian (like an auth key id).
pub fn fingerprint_of(key: &[u8; 256]) -> i64 {
    let sha = sha1_parts(&[key]);
    i64::from_le_bytes(sha[12..20].try_into().expect("8 bytes"))
}

/// The bytes users compare to verify a chat: the first 128 bits of SHA-1 of the `initial` key
/// followed by the first 160 bits of SHA-256 of the key `at_layer_46` (the initial key itself for
/// a chat created at layer 46 or later). Drawing the picture or the hex text is up to the UI.
pub fn visualization(initial: &Key, at_layer_46: &Key) -> [u8; 36] {
    let mut out = [0u8; 36];
    out[..16].copy_from_slice(&sha1_parts(&[&initial.bytes])[..16]);
    out[16..].copy_from_slice(&sha256_parts(&[&at_layer_46.bytes])[..20]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_is_the_low_bits_of_sha1() {
        let key = Key::new([7; 256]);
        let sha = sha1_parts(&[&[7u8; 256]]);
        assert_eq!(key.fingerprint().to_le_bytes(), sha[12..20]);
        assert_eq!(key.bytes(), &[7; 256]);
        assert_eq!(fingerprint_of(&[7; 256]), key.fingerprint());
    }

    #[test]
    fn debug_shows_the_fingerprint_not_the_key() {
        let text = format!("{:?}", Key::new([7; 256]));
        assert!(text.starts_with("Key(") && text.len() < 30, "{text}");
    }

    #[test]
    fn the_visualization_mixes_sha1_of_the_first_and_sha256_of_the_second_key() {
        let a = Key::new([1; 256]);
        let b = Key::new([2; 256]);
        let v = visualization(&a, &b);
        assert_eq!(v[..16], sha1_parts(&[&[1u8; 256]])[..16]);
        assert_eq!(v[16..], sha256_parts(&[&[2u8; 256]])[..20]);
        assert_ne!(v, visualization(&b, &b));
    }
}
