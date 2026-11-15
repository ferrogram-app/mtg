//! Encryption of attachments: AES-256-IGE with a random key and iv per file.
//!
//! The key and iv travel inside the encrypted message (`decryptedMessageMedia*`); the
//! `key_fingerprint` goes into the `InputEncryptedFile*` of the upload.

use mtg_crypto::aes::{ige_decrypt, ige_encrypt};

use crate::error::{Error, Result, random_fill};

/// The random key and iv of one file.
#[derive(Clone, PartialEq)]
pub struct FileKey {
    /// AES key.
    pub key: [u8; 32],
    /// IGE iv.
    pub iv: [u8; 32],
}

// never print key material
impl std::fmt::Debug for FileKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FileKey({:08x})", self.fingerprint())
    }
}

impl FileKey {
    /// A fresh random key and iv.
    pub fn generate() -> Self {
        let mut key = [0; 32];
        let mut iv = [0; 32];
        random_fill(&mut key);
        random_fill(&mut iv);
        Self { key, iv }
    }

    /// `key_fingerprint` of the file: `md5(key + iv)`, first 4 bytes xor last 4 bytes of the
    /// first 8.
    pub fn fingerprint(&self) -> i32 {
        let digest = md5::compute([&self.key[..], &self.iv[..]].concat()).0;
        let mut out = [0u8; 4];
        for (i, o) in out.iter_mut().enumerate() {
            *o = digest[i] ^ digest[i + 4];
        }
        i32::from_le_bytes(out)
    }

    /// Encrypts `data` in place, first padding it with random bytes to a multiple of 16. The
    /// original size must travel in the media (`size`) so the receiver can cut the padding.
    pub fn encrypt(&self, data: &mut Vec<u8>) {
        let size = data.len();
        data.resize(size.next_multiple_of(16), 0);
        random_fill(&mut data[size..]);
        ige_encrypt(data, &self.key, &self.iv);
    }

    /// Decrypts `data` in place and cuts it to `size` bytes.
    pub fn decrypt(&self, data: &mut Vec<u8>, size: usize) -> Result<()> {
        if !data.len().is_multiple_of(16) || size > data.len() {
            return Err(Error::BadLength);
        }
        ige_decrypt(data, &self.key, &self.iv);
        data.truncate(size);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_round_trips_and_is_cut_to_its_size() {
        let key = FileKey::generate();
        for len in [0usize, 1, 15, 16, 17, 1000] {
            let original: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let mut data = original.clone();
            key.encrypt(&mut data);
            assert_eq!(data.len() % 16, 0);
            if len >= 16 {
                assert_ne!(data[..16], original[..16]);
            }
            key.decrypt(&mut data, len).unwrap();
            assert_eq!(data, original);
        }
    }

    #[test]
    fn bad_sizes_are_rejected() {
        let key = FileKey::generate();
        assert_eq!(key.decrypt(&mut vec![0; 17], 1), Err(Error::BadLength));
        assert_eq!(key.decrypt(&mut vec![0; 16], 17), Err(Error::BadLength));
    }

    #[test]
    fn the_fingerprint_folds_the_md5_digest() {
        let key = FileKey {
            key: [1; 32],
            iv: [2; 32],
        };
        let d = md5::compute([[1u8; 32], [2u8; 32]].concat()).0;
        let want = [d[0] ^ d[4], d[1] ^ d[5], d[2] ^ d[6], d[3] ^ d[7]];
        assert_eq!(key.fingerprint(), i32::from_le_bytes(want));
        assert_ne!(key.fingerprint(), FileKey::generate().fingerprint());
        assert!(format!("{key:?}").starts_with("FileKey("));
    }
}
