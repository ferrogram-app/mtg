// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// Calculate the SHA1 hash of zero or more expressions evaluating to a sequence of bytes.
#[macro_export]
macro_rules! sha1 (
    ( $( $x:expr ),* ) => ({
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        $(
            hasher.update($x);
        )+
        let sha: [u8; 20] = hasher.finalize().into();
        sha
    })
);

/// Calculate the SHA256 hash of zero or more expressions evaluating to a sequence of bytes.
#[macro_export]
macro_rules! sha256 (
    ( $( $x:expr ),* ) => ({
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        $(
            hasher.update($x);
        )+
        let sha: [u8; 32] = hasher.finalize().into();
        sha
    })
);

/// HMAC-SHA256 of `data` under `key` (used by the fake-TLS MTProxy handshake).
pub fn hmac_sha256(key: &[u8], data: &[&[u8]]) -> [u8; 32] {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac = <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(key).expect("any key length");
    for part in data {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::hmac_sha256;

    #[test]
    fn rfc4231_case_2() {
        // key "Jefe", data "what do ya want for nothing?"
        let mac = hmac_sha256(b"Jefe", &[b"what do ya want ", b"for nothing?"]);
        assert_eq!(
            crate::hex::to_hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }
}
