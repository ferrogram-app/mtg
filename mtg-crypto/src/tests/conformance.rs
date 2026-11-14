use crate::*;

/// RFC 3526 group 14: a 2048-bit safe prime, p mod 8 == 7.
const P14: &str = "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aacaa68ffffffffffffffff";

pub(crate) fn p14_bytes() -> Vec<u8> {
    p14().to_bytes_be()
}

fn p14() -> BigUint {
    BigUint::parse_bytes(P14.as_bytes(), 16).unwrap()
}

#[test]
fn dh_prime_checks() {
    let p = p14();
    assert!(check_dh_prime(&p, 2).unwrap());
    assert!(check_dh_prime(&p, 4).unwrap());
    // unsupported g
    for g in [1, 8, -2] {
        assert!(!check_dh_prime(&p, g).unwrap(), "g={g}");
    }
    // not 2048 bits
    assert!(!check_dh_prime(&(&p >> 1), 2).unwrap());
    // 2048 bits, p mod 8 == 7, but composite: not a safe prime
    assert!(!check_dh_prime(&(&p - 8u32), 2).unwrap());
    // g = 3, 5, 6, 7 are decided by p mod m, then the primality check
    for (g, m, allowed) in [
        (3, 3u32, &[2u32][..]),
        (5, 5, &[1, 4][..]),
        (6, 24, &[19, 23][..]),
        (7, 7, &[3, 5, 6][..]),
    ] {
        let r = (&p % m).to_u32().unwrap();
        assert_eq!(
            check_dh_prime(&p, g).unwrap(),
            allowed.contains(&r),
            "g={g}"
        );
    }
}

#[test]
fn decrypt_rejects_bad_buffers() {
    let key = AuthKey::from_bytes([7; 256]);
    assert!(matches!(
        decrypt_data_v2(&mut [0; 23], &key),
        Err(Error::InvalidBuffer)
    ));
    assert!(matches!(
        decrypt_data_v2(&mut [0; 41], &key),
        Err(Error::InvalidBuffer)
    ));
    assert!(matches!(
        decrypt_data_v2(&mut [0; 40], &key),
        Err(Error::AuthKeyMismatch)
    ));
    let mut buf = vec![0u8; 40];
    buf[..8].copy_from_slice(&key.key_id());
    assert!(matches!(
        decrypt_data_v2(&mut buf, &key),
        Err(Error::MessageKeyMismatch)
    ));
}

#[test]
fn errors_display() {
    for e in [
        Error::InvalidBuffer,
        Error::AuthKeyMismatch,
        Error::MessageKeyMismatch,
    ] {
        assert!(!e.to_string().is_empty());
    }
}

#[test]
fn padding_is_12_to_32_and_aligns_to_16() {
    for len in 0..64 {
        let (_, pad) = get_padding_data_v2(len);
        assert!((12..=MAX_PADDING_LEN).contains(&pad));
        assert_eq!((len + pad) % 16, 0);
    }
    assert_ne!(get_padding_data_v2(0).0, get_padding_data_v2(0).0);
}
