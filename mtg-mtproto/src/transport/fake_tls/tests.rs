use super::*;

const SECRET: [u8; 16] = [7; 16];

fn counter() -> impl FnMut(&mut [u8]) {
    let mut n = 0u8;
    move |buf: &mut [u8]| {
        for b in buf {
            n = n.wrapping_mul(31).wrapping_add(17);
            *b = n;
        }
    }
}

/// Walks the hello and checks that every nested length matches what follows it.
fn assert_well_formed(hello: &[u8]) -> Vec<(u16, Vec<u8>)> {
    assert_eq!(hello[..3], [0x16, 3, 1]);
    let record = u16::from_be_bytes([hello[3], hello[4]]) as usize;
    assert_eq!(hello.len(), 5 + record);
    assert_eq!(hello[5], 1);
    let hs = u32::from_be_bytes([0, hello[6], hello[7], hello[8]]) as usize;
    assert_eq!(hello.len(), 9 + hs);
    assert_eq!(hello[9..11], [3, 3]);
    assert_eq!(hello[43], 0x20);
    assert_eq!(hello[76..78], [0, 0x20]);
    let after_suites = 78 + 32; // the 2-byte grease and 30 suite bytes, then compression
    assert_eq!(hello[after_suites..after_suites + 2], [1, 0]);
    let ext_len = u16::from_be_bytes([hello[after_suites + 2], hello[after_suites + 3]]) as usize;
    let mut rest = &hello[after_suites + 4..];
    assert_eq!(rest.len(), ext_len);
    let mut out = Vec::new();
    while !rest.is_empty() {
        let ty = u16::from_be_bytes([rest[0], rest[1]]);
        let len = u16::from_be_bytes([rest[2], rest[3]]) as usize;
        out.push((ty, rest[4..4 + len].to_vec()));
        rest = &rest[4 + len..];
    }
    out
}

#[test]
fn hello_is_well_formed_and_carries_the_domain() {
    let (hello, digest) = client_hello_with("example.com", &SECRET, 1_700_000_000, &mut counter());
    let exts = assert_well_formed(&hello);
    let sni = exts.iter().find(|(t, _)| *t == 0).unwrap();
    assert!(sni.1.windows(11).any(|w| w == b"example.com"));
    assert!(exts.iter().any(|(t, b)| *t == 0x33 && b.len() == 0x4ef));
    assert_eq!(hello[11..43], digest);
}

#[test]
fn real_randomness_gives_a_different_valid_hello_each_time() {
    let (a, _) = client_hello("a.example", &SECRET, 1);
    let (b, _) = client_hello("a.example", &SECRET, 1);
    assert_ne!(a, b);
    assert_well_formed(&a);
    assert_well_formed(&b);
}

#[test]
fn a_long_domain_is_cut_at_253_bytes() {
    let domain = "d".repeat(400);
    let (hello, _) = client_hello_with(&domain, &SECRET, 0, &mut counter());
    let exts = assert_well_formed(&hello);
    let sni = &exts.iter().find(|(t, _)| *t == 0).unwrap().1;
    assert_eq!(sni.len(), 2 + 1 + 2 + 253);
}

#[test]
fn the_digest_is_the_hmac_of_the_zeroed_hello_with_the_time_mixed_in() {
    let (hello, digest) = client_hello_with("x.org", &SECRET, 0x0102_0304, &mut counter());
    let mut zeroed = hello.clone();
    zeroed[11..43].fill(0);
    let mut want = hmac_sha256(&SECRET, &[&zeroed]);
    let t = i32::from_le_bytes(want[28..32].try_into().unwrap()) ^ 0x0102_0304;
    want[28..32].copy_from_slice(&t.to_le_bytes());
    assert_eq!(digest, want);
}

#[test]
fn x25519_keys_are_on_the_curve() {
    let p = field_prime();
    let exponent = BigUint::parse_bytes(P_HEX_MINUS_1_HALF.as_bytes(), 16).unwrap();
    for _ in 0..5 {
        let mut key = x25519_public(&mut os_random);
        key.reverse();
        let x = BigUint::from_bytes_be(&key);
        // a real point's x gives y^2 that is a square
        let y2 = curve_y2(&x, &p);
        assert!(y2 == BigUint::from(0u8) || y2.modpow(&exponent, &p) == BigUint::from(1u8));
    }
}

#[test]
fn ml_kem_key_coefficients_are_below_q() {
    let key = ml_kem_public(&mut os_random);
    for t in key[..1152].chunks(3) {
        let a = t[0] as u32 | (t[1] as u32 & 0x0f) << 8;
        let b = (t[1] as u32 >> 4) | (t[2] as u32) << 4;
        assert!(a < 3329 && b < 3329);
    }
}

#[test]
fn grease_values_are_paired_apart() {
    for _ in 0..200 {
        let g = grease(&mut os_random);
        assert!(g.iter().all(|b| b & 0x0f == 0x0a));
        for i in (1..8).step_by(2) {
            assert_ne!(g[i], g[i - 1]);
        }
    }
}

fn exchange() -> ([u8; 32], Vec<u8>) {
    let (hello, digest) = client_hello("example.com", &SECRET, 5);
    (digest, server_hello(&SECRET, &hello, &mut os_random))
}

#[test]
fn the_server_hello_checks_out() {
    let (digest, answer) = exchange();
    assert_eq!(
        check_server_hello(&SECRET, &digest, &answer),
        Ok(answer.len())
    );
    // extra bytes after the answer are not part of it
    let mut more = answer.clone();
    more.extend_from_slice(&[1, 2, 3]);
    assert_eq!(
        check_server_hello(&SECRET, &digest, &more),
        Ok(answer.len())
    );
}

#[test]
fn a_partial_server_hello_asks_for_more() {
    let (digest, answer) = exchange();
    for cut in [0, 3, 5, 40, answer.len() - 1] {
        assert_eq!(
            check_server_hello(&SECRET, &digest, &answer[..cut]),
            Err(Error::MissingBytes),
            "cut at {cut}"
        );
    }
}

#[test]
fn a_wrong_secret_or_a_tampered_answer_is_a_digest_mismatch() {
    let (digest, mut answer) = exchange();
    assert_eq!(
        check_server_hello(&[8; 16], &digest, &answer),
        Err(Error::BadDigest)
    );
    let last = answer.len() - 1;
    answer[last] ^= 1;
    assert_eq!(
        check_server_hello(&SECRET, &digest, &answer),
        Err(Error::BadDigest)
    );
}

#[test]
fn something_that_is_not_our_answer_is_a_bad_record() {
    let (digest, answer) = exchange();
    assert_eq!(
        check_server_hello(&SECRET, &digest, b"HTTP/1.1 400"),
        Err(Error::BadRecord)
    );
    let mut no_ccs = answer.clone();
    let first = 5 + u16::from_be_bytes([answer[3], answer[4]]) as usize;
    no_ccs[first] = 0x15;
    assert_eq!(
        check_server_hello(&SECRET, &digest, &no_ccs),
        Err(Error::BadRecord)
    );
    // a ServerHello too short to hold a digest
    let tiny = [0x16, 3, 3, 0, 0, 0x14, 3, 3, 0, 1, 1, 0x17, 3, 3, 0, 0];
    assert_eq!(
        check_server_hello(&SECRET, &digest, &tiny),
        Err(Error::BadRecord)
    );
}

#[test]
fn records_roundtrip_with_a_single_ccs_first() {
    let mut enc = RecordEncoder::default();
    let mut wire = Vec::new();
    enc.encode(&[1, 2, 3], &mut wire);
    assert_eq!(wire[..6], [0x14, 3, 3, 0, 1, 1]);
    let first_len = wire.len();
    enc.encode(&[4; 5], &mut wire);
    assert_eq!(
        wire.len() - first_len,
        5 + 5,
        "no second change-cipher-spec"
    );

    // the receiving side sees only application records (the peer's own ccs came in its hello)
    let mut dec = RecordDecoder::default();
    let mut data = wire[6..].to_vec();
    assert_eq!(dec.decode(&mut data), Ok(8));
    assert_eq!(data[..8], [1, 2, 3, 4, 4, 4, 4, 4]);
}

#[test]
fn an_established_encoder_sends_no_change_cipher_spec() {
    let mut wire = Vec::new();
    RecordEncoder::established().encode(&[9], &mut wire);
    assert_eq!(wire, [0x17, 3, 3, 0, 1, 9]);
}

#[test]
fn the_decoder_keeps_state_across_arbitrary_splits() {
    let mut enc = RecordEncoder::default();
    let mut wire = Vec::new();
    for i in 0..4u8 {
        enc.encode(&[i; 100], &mut wire);
    }
    let wire = wire[6..].to_vec();
    let want: Vec<u8> = (0..4u8).flat_map(|i| [i; 100]).collect();
    for step in [1, 2, 3, 7, 50, 105, 1000] {
        let mut dec = RecordDecoder::default();
        let mut got = Vec::new();
        for chunk in wire.chunks(step) {
            let mut c = chunk.to_vec();
            let n = dec.decode(&mut c).unwrap();
            got.extend_from_slice(&c[..n]);
        }
        assert_eq!(got, want, "step {step}");
    }
}

#[test]
fn the_decoder_rejects_other_records() {
    let mut dec = RecordDecoder::default();
    assert_eq!(
        dec.decode(&mut [0x15, 3, 3, 0, 2, 1, 1]),
        Err(Error::BadRecord)
    );
}

#[test]
fn an_oversized_payload_is_a_bug_not_a_record() {
    let r = std::panic::catch_unwind(|| {
        RecordEncoder::default().encode(&vec![0; MAX_RECORD_PAYLOAD + 1], &mut Vec::new())
    });
    assert!(r.is_err());
}

#[test]
fn errors_print() {
    for e in [Error::MissingBytes, Error::BadRecord, Error::BadDigest] {
        assert!(e.to_string().starts_with("fake-TLS"));
    }
}
