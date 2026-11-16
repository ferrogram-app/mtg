// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! [AES-Infinite Garble Extension](https://mgp25.com/blog/2015/06/21/AESIGE/) implementation.

#![allow(deprecated)] // see https://github.com/RustCrypto/block-ciphers/issues/509

use std::mem;

use aes::Aes256;
use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};

/// Encrypt the input plaintext in-place using the AES-IGE mode.
/// The buffer length must be a multiple of 16.
pub fn ige_encrypt(buffer: &mut [u8], key: &[u8; 32], iv: &[u8; 32]) {
    assert_eq!(buffer.len() % 16, 0);

    let cipher = Aes256::new(key.into());

    let mut iv1: [u8; 16] = iv[0..16].try_into().unwrap();
    let mut iv2: [u8; 16] = iv[16..32].try_into().unwrap();

    let mut next_iv2 = [0u8; 16];

    for block in buffer.chunks_mut(16) {
        // next iv2 = block (plaintext)
        next_iv2.copy_from_slice(block);

        // block (plaintext) XOR iv1 (previous ciphertext)
        for i in 0..16 {
            block[i] ^= iv1[i]
        }

        cipher.encrypt_block(block.try_into().unwrap());

        // block (ciphertext) XOR iv2 (previous plaintext)
        for i in 0..16 {
            block[i] ^= iv2[i]
        }

        // iv1 = block (ciphertext)
        iv1.copy_from_slice(block);

        // iv2 = next iv2 (plaintext)
        mem::swap(&mut iv2, &mut next_iv2);
    }
}

/// Decrypt the input ciphertext in-place using the AES-IGE mode.
/// The buffer length must be a multiple of 16.
pub fn ige_decrypt(buffer: &mut [u8], key: &[u8; 32], iv: &[u8; 32]) {
    assert_eq!(buffer.len() % 16, 0);

    let cipher = Aes256::new(key.into());

    let mut iv1: [u8; 16] = iv[0..16].try_into().unwrap();
    let mut iv2: [u8; 16] = iv[16..32].try_into().unwrap();

    let mut next_iv1 = [0u8; 16];

    for block in buffer.chunks_mut(16) {
        // next iv1 = block (ciphertext)
        next_iv1.copy_from_slice(block);

        // block (ciphertext) XOR iv2 (previous plaintext)
        for i in 0..16 {
            block[i] ^= iv2[i]
        }

        cipher.decrypt_block(block.try_into().unwrap());

        // block (plaintext) XOR iv1 (previous ciphertext)
        for i in 0..16 {
            block[i] ^= iv1[i]
        }

        // iv1 = next iv1 (ciphertext)
        mem::swap(&mut iv1, &mut next_iv1);

        // iv2 = block (plaintext)
        iv2.copy_from_slice(block);
    }
}

/// AES-256-CTR as used for CDN files (<https://core.telegram.org/cdn>): the counter starts at
/// `iv` with its last 4 bytes replaced by `offset / 16` (big-endian), so any 16-aligned part of
/// the file can be decrypted alone. Encryption and decryption are the same operation.
///
/// Returns `false`, leaving the buffer alone, if `offset` is not a multiple of 16 or its block
/// number does not fit 4 bytes.
pub fn ctr_at_offset(buffer: &mut [u8], key: &[u8; 32], iv: &[u8; 16], offset: u64) -> bool {
    use aes::cipher::{KeyIvInit, StreamCipher};
    let Ok(block) = u32::try_from(offset / 16) else {
        return false;
    };
    if !offset.is_multiple_of(16) {
        return false;
    }
    let mut counter = *iv;
    counter[12..].copy_from_slice(&block.to_be_bytes());
    ctr::Ctr128BE::<Aes256>::new(key.into(), &counter.into()).apply_keystream(buffer);
    true
}

#[cfg(test)]
mod ctr_tests {
    use super::*;

    #[test]
    fn a_part_decrypts_like_the_same_bytes_of_the_whole_stream() {
        let key = [9u8; 32];
        let mut iv = [3u8; 16];
        let mut whole = vec![0u8; 8192];
        iv[12..].copy_from_slice(&[0; 4]);
        assert!(ctr_at_offset(&mut whole, &key, &iv, 0));
        // the keystream of the whole file; the part at 4096 must match its second half
        let mut part = vec![0u8; 4096];
        assert!(ctr_at_offset(&mut part, &key, &iv, 4096));
        assert_eq!(part, whole[4096..]);
        // the garbage in the last 4 bytes of the given iv does not matter
        let mut other = iv;
        other[12..].copy_from_slice(&[0xff; 4]);
        let mut again = vec![0u8; 4096];
        assert!(ctr_at_offset(&mut again, &key, &other, 4096));
        assert_eq!(again, part);
    }

    #[test]
    fn bad_offsets_leave_the_buffer_alone() {
        let mut data = vec![1u8; 32];
        assert!(!ctr_at_offset(&mut data, &[0; 32], &[0; 16], 8));
        assert!(!ctr_at_offset(
            &mut data,
            &[0; 32],
            &[0; 16],
            (u32::MAX as u64 + 1) * 16
        ));
        assert_eq!(data, vec![1u8; 32]);
    }
}
