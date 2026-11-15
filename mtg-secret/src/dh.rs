//! Diffie-Hellman for the chat key and for rekeying.
use mtg_crypto::check_dh_prime;
use mtg_tl_types::enums::messages::DhConfig;
use num_bigint::BigUint;

use crate::error::{Error, Result, random_fill};
use crate::key::Key;

/// Size of `p`, of the private exponents and of the shared key, in bytes.
const SIZE: usize = 256;

/// The `(p, g)` group of the Diffie-Hellman exchange and the config version it came with.
#[derive(Clone, Debug, PartialEq)]
pub struct DhParams {
    p: BigUint,
    g: u32,
    version: i32,
}

impl DhParams {
    /// Checks `p` is a safe 2048-bit prime and `g` a generator of its quadratic residues
    /// (security guidelines), then wraps them.
    pub fn new(p: &[u8], g: i32, version: i32) -> Result<Self> {
        let p = BigUint::from_bytes_be(p);
        if !check_dh_prime(&p, g).map_err(|_| Error::BadDhParams)? {
            return Err(Error::BadDhParams);
        }
        // `check_dh_prime` accepts only 2..=7
        Ok(Self {
            p,
            g: g as u32,
            version,
        })
    }

    /// `version` to ask `messages.getDhConfig` with.
    pub fn version(&self) -> i32 {
        self.version
    }

    /// Reads the answer of `messages.getDhConfig`. Returns the group and the server's random
    /// bytes (mixed into the private exponent by [`start`] and [`accept`]).
    ///
    /// `cached` is the group from the previous call: `dhConfigNotModified` reuses it, and a
    /// config with the same version and `p` skips the primality check.
    pub fn from_config(config: &DhConfig, cached: Option<&DhParams>) -> Result<(Self, Vec<u8>)> {
        match config {
            DhConfig::NotModified(c) => match cached {
                Some(params) => Ok((params.clone(), c.random.clone())),
                None => Err(Error::WrongState(
                    "dhConfigNotModified without a cached config",
                )),
            },
            DhConfig::Config(c) => {
                let params = match cached {
                    Some(old)
                        if old.version == c.version
                            && old.p == BigUint::from_bytes_be(&c.p)
                            && i64::from(old.g) == i64::from(c.g) =>
                    {
                        old.clone()
                    }
                    _ => Self::new(&c.p, c.g, c.version)?,
                };
                Ok((params, c.random.clone()))
            }
        }
    }

    /// `1 < x < p - 1` and `2^(2048-64) <= x <= p - 2^(2048-64)`.
    fn check_public(&self, x: &BigUint) -> Result<()> {
        let one = BigUint::from(1u8);
        let margin: BigUint = BigUint::from(1u8) << (2048 - 64);
        if x <= &one || x >= &(&self.p - &one) || x < &margin || x > &(&self.p - &margin) {
            return Err(Error::BadPublicValue);
        }
        Ok(())
    }

    /// A private exponent: system randomness mixed (xor) with the server's random bytes.
    fn private(server_random: &[u8]) -> BigUint {
        loop {
            let mut bytes = [0u8; SIZE];
            random_fill(&mut bytes);
            for (b, r) in bytes.iter_mut().zip(server_random) {
                *b ^= r;
            }
            let a = BigUint::from_bytes_be(&bytes);
            if a > BigUint::from(1u8) {
                return a;
            }
        }
    }

    fn shared(&self, theirs: &BigUint, private: &BigUint) -> Key {
        let bytes = theirs.modpow(private, &self.p).to_bytes_be();
        let mut key = [0u8; SIZE];
        key[SIZE - bytes.len()..].copy_from_slice(&bytes);
        Key::new(key)
    }
}

/// The side that started an exchange and waits for the answer (`g_a` sent, `a` kept).
pub struct Exchange {
    params: DhParams,
    a: BigUint,
}

/// Starts an exchange: returns it with `g_a` for `messages.requestEncryption` (or for the
/// `requestKey` action of a rekey).
pub fn start(params: &DhParams, server_random: &[u8]) -> (Exchange, Vec<u8>) {
    let a = DhParams::private(server_random);
    let g_a = BigUint::from(params.g).modpow(&a, &params.p).to_bytes_be();
    (
        Exchange {
            params: params.clone(),
            a,
        },
        g_a,
    )
}

impl Exchange {
    /// Computes the key from the peer's `g_b`.
    pub fn finish(&self, g_b: &[u8]) -> Result<Key> {
        let g_b = BigUint::from_bytes_be(g_b);
        self.params.check_public(&g_b)?;
        Ok(self.params.shared(&g_b, &self.a))
    }

    /// The group of this exchange.
    pub fn params(&self) -> &DhParams {
        &self.params
    }
}

/// The answering side: checks `g_a`, returns `g_b` and the key.
pub fn accept(params: &DhParams, g_a: &[u8], server_random: &[u8]) -> Result<(Vec<u8>, Key)> {
    let g_a = BigUint::from_bytes_be(g_a);
    params.check_public(&g_a)?;
    let b = DhParams::private(server_random);
    let g_b = BigUint::from(params.g).modpow(&b, &params.p).to_bytes_be();
    Ok((g_b, params.shared(&g_a, &b)))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::OnceLock;

    use mtg_tl_types as tl;

    use super::*;

    /// RFC 3526 group 14: a 2048-bit safe prime, p mod 8 == 7, so g = 2 is valid.
    pub(crate) const P14: &str = "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aacaa68ffffffffffffffff";

    pub(crate) fn p14() -> Vec<u8> {
        BigUint::parse_bytes(P14.as_bytes(), 16)
            .unwrap()
            .to_bytes_be()
    }

    /// Checked once: the primality test is the slow part.
    pub(crate) fn params() -> DhParams {
        static P: OnceLock<DhParams> = OnceLock::new();
        P.get_or_init(|| DhParams::new(&p14(), 2, 7).unwrap())
            .clone()
    }

    #[test]
    fn both_sides_compute_the_same_key() {
        let params = params();
        let (exchange, g_a) = start(&params, &[]);
        let (g_b, key_b) = accept(&params, &g_a, &[1, 2, 3]).unwrap();
        let key_a = exchange.finish(&g_b).unwrap();
        assert_eq!(key_a, key_b);
        assert_eq!(key_a.fingerprint(), key_b.fingerprint());
        assert_eq!(exchange.params(), &params);
        assert_eq!(params.version(), 7);
    }

    #[test]
    fn bad_groups_are_rejected() {
        // composite, wrong size, unsupported g
        let mut p = p14();
        *p.last_mut().unwrap() ^= 2;
        assert_eq!(DhParams::new(&p, 2, 1), Err(Error::BadDhParams));
        assert_eq!(DhParams::new(&p14()[1..], 2, 1), Err(Error::BadDhParams));
        assert_eq!(DhParams::new(&p14(), 9, 1), Err(Error::BadDhParams));
    }

    #[test]
    fn public_values_outside_the_range_are_rejected() {
        let params = params();
        let p = BigUint::from_bytes_be(&p14());
        let margin: BigUint = BigUint::from(1u8) << (2048 - 64);
        let bad = [
            BigUint::from(0u8),
            BigUint::from(1u8),
            &p - 1u8,
            p.clone(),
            &margin - 1u8,
            &p - &margin + 1u8,
        ];
        for x in bad {
            assert_eq!(
                accept(&params, &x.to_bytes_be(), &[]).map(|_| ()),
                Err(Error::BadPublicValue),
                "{x}"
            );
            assert_eq!(
                start(&params, &[]).0.finish(&x.to_bytes_be()).map(|_| ()),
                Err(Error::BadPublicValue)
            );
        }
        // the bounds themselves are fine
        assert!(accept(&params, &margin.to_bytes_be(), &[]).is_ok());
        assert!(accept(&params, &(&p - &margin).to_bytes_be(), &[]).is_ok());
    }

    #[test]
    fn short_keys_are_left_padded_to_256_bytes() {
        // g^x with x = 0 would be 1: the padded key keeps its length
        let params = params();
        let key = params.shared(&BigUint::from(3u8), &BigUint::from(0u8));
        assert_eq!(key.bytes()[255], 1);
        assert!(key.bytes()[..255].iter().all(|b| *b == 0));
    }

    #[test]
    fn the_config_is_read_with_and_without_a_cache() {
        let config = tl::enums::messages::DhConfig::Config(tl::types::messages::DhConfig {
            g: 2,
            p: p14(),
            version: 7,
            random: vec![5; 8],
        });
        let (params, random) = DhParams::from_config(&config, None).unwrap();
        assert_eq!((params.version(), random), (7, vec![5; 8]));
        // same version and p: taken from the cache
        let (again, _) = DhParams::from_config(&config, Some(&params)).unwrap();
        assert_eq!(again, params);

        let same =
            tl::enums::messages::DhConfig::NotModified(tl::types::messages::DhConfigNotModified {
                random: vec![6],
            });
        let (cached, random) = DhParams::from_config(&same, Some(&params)).unwrap();
        assert_eq!((cached, random), (params.clone(), vec![6]));
        assert!(matches!(
            DhParams::from_config(&same, None),
            Err(Error::WrongState(_))
        ));

        // a different version with a broken p is checked again and fails
        let mut bad_p = p14();
        *bad_p.last_mut().unwrap() ^= 2;
        let bad = tl::enums::messages::DhConfig::Config(tl::types::messages::DhConfig {
            g: 2,
            p: bad_p,
            version: 8,
            random: vec![],
        });
        assert_eq!(
            DhParams::from_config(&bad, Some(&params)).map(|_| ()),
            Err(Error::BadDhParams)
        );
    }
}
