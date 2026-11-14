//! Fake-TLS for MTProxy (`ee` secrets): the connection looks like TLS 1.3 to a web site.
//!
//! Sans-IO pieces only: the ClientHello with its secret-keyed digest, the check of the server's
//! answer, and the TLS record framing the obfuscated stream travels in. See
//! [`Obfuscated`](super::Obfuscated) for the stream itself. The hello follows the one the
//! official Android client sends (`TLSHello.cpp`: GREASE values, shuffled extensions, a
//! post-quantum key share, X25519 keys that are valid curve points).

use mtg_crypto::hmac_sha256;
use num_bigint::BigUint;

/// A source of random bytes; `getrandom` unless a test says otherwise.
pub type Fill<'a> = &'a mut dyn FnMut(&mut [u8]);

fn os_random(buf: &mut [u8]) {
    // no OS randomness means no safe handshake: refusing to connect is the only correct outcome
    getrandom::fill(buf).unwrap();
}

/// Largest payload put in one TLS record (what the official client sends per write).
pub const MAX_RECORD_PAYLOAD: usize = 2878;

/// Why a fake-TLS exchange was rejected. The connection must be dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// More bytes are needed before the answer can be judged.
    MissingBytes,
    /// The server's answer is not the expected sequence of TLS records.
    BadRecord,
    /// The digest in the server's answer does not match: wrong secret or not our proxy.
    BadDigest,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingBytes => "fake-TLS: incomplete server hello",
            Self::BadRecord => "fake-TLS: unexpected TLS record",
            Self::BadDigest => "fake-TLS: server digest mismatch (wrong secret?)",
        })
    }
}

impl std::error::Error for Error {}

/// Length-prefixed scopes written in place, like the nested lengths of TLS structures.
#[derive(Default)]
struct Builder {
    buf: Vec<u8>,
    open: Vec<usize>,
}

impl Builder {
    fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    fn begin(&mut self) -> &mut Self {
        self.open.push(self.buf.len());
        self.buf.extend_from_slice(&[0, 0]);
        self
    }

    fn end(&mut self) -> &mut Self {
        let at = self.open.pop().expect("unbalanced scope");
        let len = (self.buf.len() - at - 2) as u16;
        self.buf[at..at + 2].copy_from_slice(&len.to_be_bytes());
        self
    }

    fn random(&mut self, n: usize, fill: Fill) -> &mut Self {
        let at = self.buf.len();
        self.buf.resize(at + n, 0);
        fill(&mut self.buf[at..]);
        self
    }
}

const P_HEX_MINUS_1_HALF: &str = "3ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff6";

fn field_prime() -> BigUint {
    (BigUint::from(1u8) << 255) - 19u8
}

/// x^3 + 486662 x^2 + x
fn curve_y2(x: &BigUint, p: &BigUint) -> BigUint {
    ((x + 486662u32) * x % p + 1u8) * x % p
}

/// x of the doubled point.
fn double_x(x: &BigUint, p: &BigUint) -> BigUint {
    let denominator = curve_y2(x, p) * 4u8 % p;
    let numerator = (x * x % p + p - 1u8) % p;
    let numerator = &numerator * &numerator % p;
    let inverse = denominator.modpow(&(p - 2u8), p);
    numerator * inverse % p
}

/// 32 bytes that are the x coordinate of a real X25519 point, so that a DPI which tests
/// the key share for being on the curve does not tell it from a browser's.
fn x25519_public(fill: Fill) -> [u8; 32] {
    let p = field_prime();
    let exponent = BigUint::parse_bytes(P_HEX_MINUS_1_HALF.as_bytes(), 16).unwrap();
    let mut x = loop {
        let mut key = [0u8; 32];
        fill(&mut key);
        key[31] &= 127;
        let x = BigUint::from_bytes_be(&key);
        let x = &x * &x % &p;
        // y^2 must be a square (Euler's criterion)
        if curve_y2(&x, &p).modpow(&exponent, &p) == BigUint::from(1u8) {
            break x;
        }
    };
    for _ in 0..3 {
        x = double_x(&x, &p);
    }
    let mut out = [0u8; 32];
    let be = x.to_bytes_be();
    out[32 - be.len()..].copy_from_slice(&be);
    out.reverse();
    out
}

/// 1184 bytes shaped like an ML-KEM-768 public key: 384 coefficients below 3329, packed, + seed.
fn ml_kem_public(fill: Fill) -> [u8; 1184] {
    const Q: u32 = 3329;
    let mut raw = [0u8; 384 * 2 * 4];
    fill(&mut raw);
    let value = |i: usize| u32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap()) % Q;
    let mut key = [0u8; 1184];
    for i in 0..384 {
        let (a, b) = (value(i * 2), value(i * 2 + 1));
        key[i * 3] = a as u8;
        key[i * 3 + 1] = (a >> 8) as u8 | ((b & 0x0f) << 4) as u8;
        key[i * 3 + 2] = (b >> 4) as u8;
    }
    fill(&mut key[1152..]);
    key
}

/// GREASE bytes: `0x?A`, with the two of a pair never equal.
fn grease(fill: Fill) -> [u8; 8] {
    let mut g = [0u8; 8];
    fill(&mut g);
    for b in &mut g {
        *b = (*b & 0xf0) + 0x0a;
    }
    for i in (1..8).step_by(2) {
        if g[i] == g[i - 1] {
            g[i] ^= 0x10;
        }
    }
    g
}

fn extensions(domain: &str, g: &[u8; 8], fill: Fill) -> Vec<Vec<u8>> {
    let gg = |i: usize| [g[i], g[i]];
    let mut list: Vec<Vec<u8>> = Vec::new();
    let mut ext = |f: &mut dyn FnMut(&mut Builder)| {
        let mut b = Builder::default();
        f(&mut b);
        list.push(b.buf);
    };
    let domain = &domain.as_bytes()[..domain.len().min(253)];
    ext(&mut |b| {
        b.raw(&[0, 0])
            .begin()
            .begin()
            .raw(&[0])
            .begin()
            .raw(domain)
            .end()
            .end()
            .end();
    });
    ext(&mut |b| {
        b.raw(&[0, 5, 0, 5, 1, 0, 0, 0, 0]);
    });
    ext(&mut |b| {
        b.raw(&[0, 0x0a, 0, 0x0c, 0, 0x0a])
            .raw(&gg(4))
            .raw(&[0x11, 0xec, 0, 0x1d, 0, 0x17, 0, 0x18]);
    });
    ext(&mut |b| {
        b.raw(&[0, 0x0b, 0, 2, 1, 0]);
    });
    ext(&mut |b| {
        b.raw(&[
            0, 0x0d, 0, 0x18, 0, 0x16, 9, 4, 9, 5, 9, 6, 4, 3, 8, 4, 4, 1, 5, 3, 8, 5, 5, 1,
        ])
        .raw(&[8, 6, 6, 1]);
    });
    ext(&mut |b| {
        b.raw(&[0, 0x10, 0, 0x0e, 0, 0x0c, 2, b'h', b'2', 8])
            .raw(b"http/1.1");
    });
    ext(&mut |b| {
        b.raw(&[0, 0x12, 0, 0]);
    });
    ext(&mut |b| {
        b.raw(&[0, 0x17, 0, 0]);
    });
    ext(&mut |b| {
        b.raw(&[0, 0x1b, 0, 3, 2, 0, 2]);
    });
    ext(&mut |b| {
        b.raw(&[0, 0x23, 0, 0]);
    });
    ext(&mut |b| {
        b.raw(&[0, 0x2b, 0, 7, 6]).raw(&gg(6)).raw(&[3, 4, 3, 3]);
    });
    ext(&mut |b| {
        b.raw(&[0, 0x2d, 0, 2, 1, 1]);
    });
    let mut share = Builder::default();
    share
        .raw(&[0, 0x33, 4, 0xef, 4, 0xed])
        .raw(&gg(4))
        .raw(&[0, 1, 0, 0x11, 0xec, 4, 0xc0])
        .raw(&ml_kem_public(fill))
        .raw(&x25519_public(fill))
        .raw(&[0, 0x1d, 0, 0x20])
        .raw(&x25519_public(fill));
    list.push(share.buf);
    list.push(vec![0x44, 0xcd, 0, 5, 0, 3, 2, b'h', b'2']);
    let mut ech = Builder::default();
    ech.raw(&[0xfe, 0x0d])
        .begin()
        .raw(&[0, 0, 1, 0, 1])
        .random(1, fill)
        .raw(&[0, 0x20]);
    ech.raw(&x25519_public(fill));
    let mut len = [0u8];
    fill(&mut len);
    ech.begin()
        .random(144 + 32 * (len[0] & 3) as usize, fill)
        .end()
        .end();
    list.push(ech.buf);
    list.push(vec![0xff, 1, 0, 1, 0]);
    list
}

/// Builds the ClientHello record for `domain`, with the random field holding the digest keyed
/// by `secret` (its last four bytes mixed with `unix_time`). Returns the bytes and that field.
pub fn client_hello(domain: &str, secret: &[u8; 16], unix_time: i32) -> (Vec<u8>, [u8; 32]) {
    client_hello_with(domain, secret, unix_time, &mut os_random)
}

pub(crate) fn client_hello_with(
    domain: &str,
    secret: &[u8; 16],
    unix_time: i32,
    fill: Fill,
) -> (Vec<u8>, [u8; 32]) {
    let g = grease(fill);
    let mut shuffled = extensions(domain, &g, fill);
    // Fisher-Yates, like the reference
    for i in 0..shuffled.len() - 1 {
        let mut r = [0u8; 4];
        fill(&mut r);
        let j = i + u32::from_le_bytes(r) as usize % (shuffled.len() - i);
        shuffled.swap(i, j);
    }

    let mut b = Builder::default();
    b.raw(&[0x16, 3, 1])
        .begin()
        .raw(&[1, 0])
        .begin()
        .raw(&[3, 3])
        .raw(&[0; 32])
        .raw(&[0x20]);
    b.random(32, fill).raw(&[0, 0x20]).raw(&[g[0], g[0]]);
    b.raw(&[
        0x13, 1, 0x13, 2, 0x13, 3, 0xc0, 0x2b, 0xc0, 0x2f, 0xc0, 0x2c, 0xc0, 0x30,
    ]);
    b.raw(&[
        0xcc, 0xa9, 0xcc, 0xa8, 0xc0, 0x13, 0xc0, 0x14, 0, 0x9c, 0, 0x9d, 0, 0x2f, 0, 0x35,
    ]);
    b.raw(&[1, 0]);
    b.begin().raw(&[g[2], g[2], 0, 0]);
    for ext in &shuffled {
        b.raw(ext);
    }
    b.raw(&[g[3], g[3], 0, 1, 0]);
    // the reference pads short hellos up to 513 bytes; with the post-quantum share ours never are
    b.end().end().end();

    let mut hello = b.buf;
    let mut digest = hmac_sha256(secret, &[&hello]);
    let mixed = i32::from_le_bytes(digest[28..32].try_into().unwrap()) ^ unix_time;
    digest[28..32].copy_from_slice(&mixed.to_le_bytes());
    hello[11..43].copy_from_slice(&digest);
    (hello, digest)
}

/// Digest of a server answer: HMAC over our hello digest and the answer with its own digest
/// field (bytes 11..43) zeroed.
fn server_digest(secret: &[u8; 16], client_digest: &[u8; 32], answer: &[u8]) -> [u8; 32] {
    let mut zeroed = answer.to_vec();
    zeroed[11..43].fill(0);
    hmac_sha256(secret, &[client_digest, &zeroed])
}

/// The server's answer is `16 03 03 len` (ServerHello), `14 03 03 00 01 01` and `17 03 03 len`
/// with a dummy payload. Returns how many bytes it spans, once its digest checks out.
pub fn check_server_hello(
    secret: &[u8; 16],
    client_digest: &[u8; 32],
    data: &[u8],
) -> Result<usize, Error> {
    const CCS_AND_DATA: [u8; 9] = [0x14, 3, 3, 0, 1, 1, 0x17, 3, 3];
    let head = |at: usize| data.get(at..at + 3);
    // judge what is already there before waiting for more
    if head(0).is_some_and(|h| h != [0x16, 3, 3]) {
        return Err(Error::BadRecord);
    }
    if data.len() < 5 {
        return Err(Error::MissingBytes);
    }
    let first = 5 + u16::from_be_bytes([data[3], data[4]]) as usize;
    let have = &data[first.min(data.len())..];
    if have[..have.len().min(9)] != CCS_AND_DATA[..have.len().min(9)] {
        return Err(Error::BadRecord);
    }
    if data.len() < first + 11 {
        return Err(Error::MissingBytes);
    }
    let total = first + 11 + u16::from_be_bytes([data[first + 9], data[first + 10]]) as usize;
    if data.len() < total {
        return Err(Error::MissingBytes);
    }
    if first < 43 {
        return Err(Error::BadRecord);
    }
    let expected = server_digest(secret, client_digest, &data[..total]);
    // constant-time enough: the digest is not a secret, a mismatch just ends the connection
    if expected != data[11..43] {
        return Err(Error::BadDigest);
    }
    Ok(total)
}

/// What a proxy answers to a valid `client_hello` (for tests and fake servers): ServerHello,
/// change-cipher-spec and one dummy application record, digest-signed with `secret`.
pub fn server_hello(secret: &[u8; 16], client_hello: &[u8], fill: Fill) -> Vec<u8> {
    let client_digest: [u8; 32] = client_hello[11..43].try_into().unwrap();
    let session_id = &client_hello[44..76];
    let mut b = Builder::default();
    b.raw(&[0x16, 3, 3])
        .begin()
        .raw(&[2, 0])
        .begin()
        .raw(&[3, 3])
        .raw(&[0; 32])
        .raw(&[0x20]);
    b.raw(session_id).raw(&[0x13, 1, 0]).begin();
    b.raw(&[0, 0x2b, 0, 2, 3, 4, 0, 0x33, 0, 0x24, 0, 0x1d, 0, 0x20]);
    b.raw(&x25519_public(fill));
    b.end().end().end();
    b.raw(&[0x14, 3, 3, 0, 1, 1, 0x17, 3, 3])
        .begin()
        .random(1 + 8 * 40, fill)
        .end();
    let digest = server_digest(secret, &client_digest, &b.buf);
    b.buf[11..43].copy_from_slice(&digest);
    b.buf
}

/// Wraps outgoing bytes in TLS application-data records (the first write is preceded by a
/// change-cipher-spec, as in a real session).
#[derive(Default)]
pub struct RecordEncoder {
    started: bool,
}

impl RecordEncoder {
    /// An encoder for the server side, whose change-cipher-spec went out inside its hello.
    pub fn established() -> Self {
        Self { started: true }
    }

    /// Frames `payload` (at most [`MAX_RECORD_PAYLOAD`] bytes) into `out`.
    pub fn encode(&mut self, payload: &[u8], out: &mut Vec<u8>) {
        assert!(payload.len() <= MAX_RECORD_PAYLOAD);
        if !self.started {
            self.started = true;
            out.extend_from_slice(&[0x14, 3, 3, 0, 1, 1]);
        }
        out.extend_from_slice(&[0x17, 3, 3]);
        out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        out.extend_from_slice(payload);
    }
}

/// Strips the record headers off incoming bytes.
#[derive(Default)]
pub struct RecordDecoder {
    header: [u8; 5],
    header_len: usize,
    left: usize,
}

impl RecordDecoder {
    /// Removes the headers from `buf` in place and returns how many payload bytes it now
    /// starts with. Any state spanning two calls (a split header, a record continued) is kept.
    /// Records that are not application data (`0x17 03 03`) are an error.
    pub fn decode(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let (mut read, mut write) = (0, 0);
        while read < buf.len() {
            if self.left == 0 {
                self.header[self.header_len] = buf[read];
                self.header_len += 1;
                read += 1;
                if self.header_len == 5 {
                    if self.header[..3] != [0x17, 3, 3] {
                        return Err(Error::BadRecord);
                    }
                    self.left = u16::from_be_bytes([self.header[3], self.header[4]]) as usize;
                    self.header_len = 0;
                }
            } else {
                let n = self.left.min(buf.len() - read);
                buf.copy_within(read..read + n, write);
                read += n;
                write += n;
                self.left -= n;
            }
        }
        Ok(write)
    }
}

#[cfg(test)]
mod tests;
