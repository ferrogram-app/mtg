// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::{Error, Tagged, Transport, UnpackedOffset};
use crate::DequeBuffer;

/// A light MTProto transport protocol that guarantees data padded to 4 bytes.
///
/// This is an implementation of the [intermediate transport].
///
/// * Overhead: small.
/// * Minimum envelope length: 4 bytes.
/// * Maximum envelope length: 4 bytes.
///
/// It serializes the input payload as follows:
///
/// ```text
/// +----+----...----+
/// | len|  payload  |
/// +----+----...----+
///  ^^^^ 4 bytes
/// ```
///
/// [intermediate transport]: https://core.telegram.org/mtproto/mtproto-transports#intermediate
pub struct Intermediate {
    init: bool,
}

#[allow(clippy::new_without_default)]
impl Intermediate {
    const TAG: [u8; 4] = 0xee_ee_ee_ee_u32.to_le_bytes();

    /// Creates the transport.
    pub fn new() -> Self {
        Self { init: false }
    }
}

impl Transport for Intermediate {
    fn pack(&mut self, buffer: &mut DequeBuffer<u8>) {
        let len = buffer.len();
        // contract: `Mtp` pads every packet to 4 bytes
        assert_eq!(len % 4, 0);

        buffer.extend_front(&(len as i32).to_le_bytes());

        if !self.init {
            buffer.extend_front(&Self::TAG);
            self.init = true;
        }
    }

    fn unpack(&mut self, buffer: &mut [u8]) -> Result<UnpackedOffset, Error> {
        if buffer.len() < 4 {
            return Err(Error::MissingBytes);
        }

        // infallible: at least 4 bytes were checked above
        let len = i32::from_le_bytes(buffer[0..4].try_into().unwrap());
        if len < 4 {
            return Err(Error::BadLen { got: len });
        }

        // `len` does not count the 4-byte header itself
        let len = len as usize;
        if buffer.len() < 4 + len {
            return Err(Error::MissingBytes);
        }

        if len == 4 {
            // infallible: length was checked above
            let data = i32::from_le_bytes(buffer[4..8].try_into().unwrap());
            return Err(Error::BadStatus {
                status: data.unsigned_abs(),
            });
        }

        Ok(UnpackedOffset {
            data_range: 4..4 + len,
            next_offset: 4 + len,
        })
    }
}

impl Tagged for Intermediate {
    fn init_tag(&mut self) -> [u8; 4] {
        self.init = true;
        Self::TAG
    }
}

/// The [padded intermediate transport]: like [`Intermediate`], but every packet carries 0 to 15
/// random bytes after the payload, which hides the real length. Used by MTProxy (`dd` secrets).
///
/// Telegram's servers never pad what they send, so reading works as in [`Intermediate`].
///
/// [padded intermediate transport]: https://core.telegram.org/mtproto/mtproto-transports#padded-intermediate
pub struct PaddedIntermediate {
    inner: Intermediate,
}

#[allow(clippy::new_without_default)]
impl PaddedIntermediate {
    const TAG: [u8; 4] = 0xdd_dd_dd_dd_u32.to_le_bytes();

    /// Creates the transport.
    pub fn new() -> Self {
        Self {
            inner: Intermediate::new(),
        }
    }
}

impl Transport for PaddedIntermediate {
    fn pack(&mut self, buffer: &mut DequeBuffer<u8>) {
        let mut pad = [0u8; 16];
        // no OS randomness: padding is not a secret, but the length must not be predictable
        getrandom::fill(&mut pad).unwrap();
        let pad_len = usize::from(pad[0] & 0x0f);
        buffer.extend_back(&pad[1..1 + pad_len.min(15)]);
        let len = buffer.len();
        buffer.extend_front(&(len as i32).to_le_bytes());
        if !self.inner.init {
            buffer.extend_front(&Self::TAG);
            self.inner.init = true;
        }
    }

    fn unpack(&mut self, buffer: &mut [u8]) -> Result<UnpackedOffset, Error> {
        self.inner.unpack(buffer)
    }
}

impl Tagged for PaddedIntermediate {
    fn init_tag(&mut self) -> [u8; 4] {
        self.inner.init = true;
        Self::TAG
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a full intermediate transport, and `n` bytes of input data for it.
    fn setup_pack(n: usize) -> (Intermediate, DequeBuffer<u8>) {
        let mut buffer = DequeBuffer::with_capacity(n, 0);
        for i in 0..n {
            buffer.push((i & 0xff) as u8);
        }
        (Intermediate::new(), buffer)
    }

    #[test]
    fn pack_empty() {
        let (mut transport, mut buffer) = setup_pack(0);
        transport.pack(&mut buffer);
        assert_eq!(&buffer[..], &[0xee, 0xee, 0xee, 0xee, 0, 0, 0, 0]);
    }

    #[test]
    #[should_panic]
    fn pack_non_padded() {
        let (mut transport, mut buffer) = setup_pack(7);
        transport.pack(&mut buffer);
    }

    #[test]
    fn pack_normal() {
        let (mut transport, mut buffer) = setup_pack(128);
        let orig = buffer.clone();
        transport.pack(&mut buffer);
        assert_eq!(&buffer[..8], &[0xee, 0xee, 0xee, 0xee, 128, 0, 0, 0]);
        assert_eq!(&buffer[8..buffer.len()], &orig[..]);
    }

    #[test]
    fn unpack_small() {
        let mut transport = Intermediate::new();
        let mut buffer = DequeBuffer::with_capacity(1, 0);
        buffer.extend_back(&[1]);
        assert_eq!(transport.unpack(&mut buffer[..],), Err(Error::MissingBytes));
    }

    #[test]
    fn unpack_normal() {
        let (mut transport, mut buffer) = setup_pack(128);
        let orig = buffer.clone();
        transport.pack(&mut buffer);
        let n = 4; // init bytes
        let offset = transport.unpack(&mut buffer[n..]).unwrap();
        assert_eq!(&buffer[n..][offset.data_range], &orig[..]);
    }

    #[test]
    fn unpack_two_at_once() {
        let (mut transport, mut buffer) = setup_pack(128);
        let orig = buffer.clone();

        let mut two_buffer = DequeBuffer::with_capacity(0, 0);
        transport.pack(&mut buffer);
        two_buffer.extend_back(&buffer[4..]); // init bytes
        let single_size = two_buffer.len();

        buffer = orig.clone();
        transport.pack(&mut buffer);
        two_buffer.extend_back(&buffer[..]);

        let offset = transport.unpack(&mut two_buffer[..]).unwrap();
        assert_eq!(&buffer[offset.data_range], &orig[..]);
        assert_eq!(offset.next_offset, single_size);

        let n = offset.next_offset;
        let offset = transport.unpack(&mut two_buffer[n..]).unwrap();
        assert_eq!(&buffer[offset.data_range], &orig[..]);
    }

    #[test]
    fn unpack_bad_status() {
        let mut transport = Intermediate::new();
        let mut buffer = DequeBuffer::with_capacity(8, 0);
        buffer.extend_back(&(4_i32).to_le_bytes());
        buffer.extend_back(&(-404_i32).to_le_bytes());

        assert_eq!(
            transport.unpack(&mut buffer[..]),
            Err(Error::BadStatus { status: 404 })
        );
    }

    #[test]
    fn padded_packets_carry_up_to_15_random_bytes_counted_in_the_length() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let mut transport = PaddedIntermediate::new();
            let mut buffer = DequeBuffer::with_capacity(8, 0);
            buffer.extend_back(&[1, 2, 3, 4, 5, 6, 7, 8]);
            transport.pack(&mut buffer);
            assert_eq!(&buffer[..4], &[0xdd; 4]);
            let len = i32::from_le_bytes(buffer[4..8].try_into().unwrap()) as usize;
            assert_eq!(buffer.len(), 8 + len);
            assert!((8..=23).contains(&len), "{len}");
            assert_eq!(&buffer[8..16], &[1, 2, 3, 4, 5, 6, 7, 8]);
            seen.insert(len);
            // the tag is sent once
            let mut again = DequeBuffer::with_capacity(4, 0);
            again.extend_back(&[0; 4]);
            transport.pack(&mut again);
            assert_ne!(&again[..4], &[0xdd; 4]);
        }
        assert!(seen.len() > 1, "padding length never varies");
    }

    #[test]
    fn padded_reads_like_intermediate_and_tags_for_obfuscation() {
        let mut transport = PaddedIntermediate::new();
        assert_eq!(transport.init_tag(), [0xdd; 4]);
        let mut buffer = vec![8, 0, 0, 0, 9, 9, 9, 9, 9, 9, 9, 9];
        let offset = transport.unpack(&mut buffer).unwrap();
        assert_eq!(&buffer[offset.data_range], &[9; 8]);
    }
}
