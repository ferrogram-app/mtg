// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;
use std::time::SystemTime;

use jiff::Timestamp;
use mtg_tl_types as tl;

// This atomic isn't for anything critical, just to generate unique IDs without locks.
// The worst that can happen if the load and store orderings are wrong is that the IDs
// are not actually unique which could confuse some of the API results.
static LAST_ID: AtomicI64 = AtomicI64::new(0);

/// Generate a "random" ID suitable for sending messages or media.
pub(crate) fn generate_random_id() -> i64 {
    while LAST_ID.load(Ordering::SeqCst) == 0 {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before epoch")
            .as_nanos() as i64;

        if LAST_ID
            .compare_exchange(0, now, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            thread::yield_now();
        }
    }

    LAST_ID.fetch_add(1, Ordering::SeqCst)
}

pub(crate) fn generate_random_ids(n: usize) -> Vec<i64> {
    (0..n).map(|_| generate_random_id()).collect()
}

pub(crate) fn date(date: i32) -> Timestamp {
    Timestamp::from_second(date as i64).expect("date out of range")
}

/// `(salt1, salt2, p, g)` of the 2FA KDF.
pub(crate) type PasswordParams<'a> = (&'a Vec<u8>, &'a Vec<u8>, &'a Vec<u8>, &'a i32);

pub(crate) fn extract_password_parameters(
    current_algo: &tl::enums::PasswordKdfAlgo,
) -> Option<PasswordParams<'_>> {
    let tl::types::PasswordKdfAlgoSha256Sha256Pbkdf2Hmacsha512iter100000Sha256ModPow {
        salt1,
        salt2,
        p,
        g,
    } = match current_algo {
        // most likely the client is outdated and does not support the KDF algorithm
        tl::enums::PasswordKdfAlgo::Unknown => return None,
        tl::enums::PasswordKdfAlgo::Sha256Sha256Pbkdf2Hmacsha512iter100000Sha256ModPow(alg) => alg,
    };
    Some((salt1, salt2, p, g))
}

pub(crate) fn peer_from_message(message: &tl::enums::Message) -> Option<tl::enums::Peer> {
    match &message {
        tl::enums::Message::Empty(message) => message.peer_id.clone(),
        tl::enums::Message::Message(message) => Some(message.peer_id.clone()),
        tl::enums::Message::Service(message) => Some(message.peer_id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{message, user_ref};

    #[test]
    fn random_ids_are_distinct_and_counted() {
        let ids = generate_random_ids(3);
        assert_eq!(ids.len(), 3);
        assert!(ids[0] != ids[1] && ids[1] != ids[2]);
        assert!(generate_random_ids(0).is_empty());
    }

    #[test]
    fn dates_are_unix_seconds() {
        assert_eq!(date(60).as_second(), 60);
    }

    #[test]
    fn password_parameters_exist_only_for_the_known_algorithm() {
        assert!(extract_password_parameters(&tl::enums::PasswordKdfAlgo::Unknown).is_none());
        let algo = tl::enums::PasswordKdfAlgo::Sha256Sha256Pbkdf2Hmacsha512iter100000Sha256ModPow(
            tl::types::PasswordKdfAlgoSha256Sha256Pbkdf2Hmacsha512iter100000Sha256ModPow {
                salt1: vec![1],
                salt2: vec![2],
                p: vec![3],
                g: 4,
            },
        );
        let (s1, s2, p, g) = extract_password_parameters(&algo).unwrap();
        assert_eq!(
            (s1.as_slice(), s2.as_slice(), p.as_slice(), *g),
            (&[1][..], &[2][..], &[3][..], 4)
        );
    }

    #[test]
    fn the_peer_of_a_message_is_found_for_every_kind() {
        let peer = user_ref(5);
        let want = tl::enums::Peer::from(peer.id);
        assert_eq!(
            peer_from_message(&message(1, peer, "x", 0)),
            Some(want.clone())
        );
        let empty = tl::enums::Message::Empty(tl::types::MessageEmpty {
            id: 1,
            peer_id: Some(want.clone()),
        });
        assert_eq!(peer_from_message(&empty), Some(want));
        let none = tl::enums::Message::Empty(tl::types::MessageEmpty {
            id: 1,
            peer_id: None,
        });
        assert_eq!(peer_from_message(&none), None);
    }
}
