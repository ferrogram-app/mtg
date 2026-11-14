//! MTProxy settings: the `tg://proxy` / `https://t.me/proxy` link and the secret kinds.

use mtg_crypto::hex::opt_from_hex;

/// An [MTProxy](https://core.telegram.org/mtproto/mtproto-transports#transport-obfuscation)
/// to reach Telegram through, instead of the datacenter addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MtProxy {
    /// Host name or IP address of the proxy.
    pub host: String,
    /// TCP port of the proxy.
    pub port: u16,
    /// The 16 secret bytes.
    pub secret: [u8; 16],
    /// The secret had the `dd` prefix: use the padded intermediate transport.
    pub padded: bool,
}

/// Why a proxy link or secret was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MtProxyError {
    /// Not a `tg://proxy?` or `t.me/proxy?` link, or `server`/`port`/`secret` is missing or bad.
    BadLink,
    /// The secret is not hex or base64url, or has a wrong length.
    BadSecret,
    /// The secret starts with `ee` (fake-TLS), which is not supported yet.
    FakeTlsUnsupported,
}

impl std::fmt::Display for MtProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BadLink => "not a valid MTProxy link",
            Self::BadSecret => "invalid MTProxy secret",
            Self::FakeTlsUnsupported => "fake-TLS (ee) MTProxy secrets are not supported",
        })
    }
}

impl std::error::Error for MtProxyError {}

impl MtProxy {
    /// Parses a proxy link: `tg://proxy?server=H&port=P&secret=S` or the `https://t.me/proxy?...` form.
    pub fn from_link(link: &str) -> Result<Self, MtProxyError> {
        let query = ["tg://proxy?", "https://t.me/proxy?", "http://t.me/proxy?"]
            .iter()
            .find_map(|prefix| link.trim().strip_prefix(prefix))
            .ok_or(MtProxyError::BadLink)?;
        let get = |key: &str| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
        };
        let host = get("server")
            .filter(|h| !h.is_empty())
            .ok_or(MtProxyError::BadLink)?;
        let port = get("port")
            .and_then(|p| p.parse().ok())
            .ok_or(MtProxyError::BadLink)?;
        let (secret, padded) = parse_secret(get("secret").ok_or(MtProxyError::BadLink)?)?;
        Ok(Self {
            host: host.to_string(),
            port,
            secret,
            padded,
        })
    }
}

/// Decodes a secret given as hex or base64url: 16 bytes, or `dd` + 16 bytes (padded).
fn parse_secret(text: &str) -> Result<([u8; 16], bool), MtProxyError> {
    let bytes = opt_from_hex(text)
        .or_else(|| base64url(text))
        .ok_or(MtProxyError::BadSecret)?;
    let (body, padded) = match bytes.len() {
        16 => (&bytes[..], false),
        17 if bytes[0] == 0xdd => (&bytes[1..], true),
        n if n > 17 && bytes[0] == 0xee => return Err(MtProxyError::FakeTlsUnsupported),
        _ => return Err(MtProxyError::BadSecret),
    };
    Ok((body.try_into().unwrap(), padded))
}

fn base64url(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        acc = acc << 6 | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX: &str = "000102030405060708090a0b0c0d0e0f";

    #[test]
    fn plain_hex() {
        let p = MtProxy::from_link(&format!(
            "tg://proxy?server=h.example&port=443&secret={HEX}"
        ))
        .unwrap();
        assert_eq!(
            (p.host.as_str(), p.port, p.padded),
            ("h.example", 443, false)
        );
        assert_eq!(p.secret[15], 15);
    }

    #[test]
    fn padded_tme_link_any_order() {
        let p = MtProxy::from_link(&format!(
            "https://t.me/proxy?secret=dd{HEX}&port=1&server=1.2.3.4"
        ))
        .unwrap();
        assert!(p.padded);
        assert_eq!(p.secret[1], 1);
    }

    #[test]
    fn base64url_secret() {
        // 16 bytes 0..15 in base64url
        let p =
            MtProxy::from_link("tg://proxy?server=h&port=2&secret=AAECAwQFBgcICQoLDA0ODw").unwrap();
        assert_eq!(p.secret[..3], [0, 1, 2]);
        assert_eq!(p.secret[15], 15);
    }

    #[test]
    fn rejects() {
        let bad = |l: &str| MtProxy::from_link(l).unwrap_err();
        assert_eq!(bad("https://example.com/"), MtProxyError::BadLink);
        assert_eq!(bad("tg://proxy?port=1&secret=00"), MtProxyError::BadLink);
        assert_eq!(
            bad(&format!("tg://proxy?server=&port=1&secret={HEX}")),
            MtProxyError::BadLink
        );
        assert_eq!(
            bad(&format!("tg://proxy?server=h&port=x&secret={HEX}")),
            MtProxyError::BadLink
        );
        assert_eq!(bad("tg://proxy?server=h&port=1"), MtProxyError::BadLink);
        assert_eq!(
            bad("tg://proxy?server=h&port=1&secret=00ff"),
            MtProxyError::BadSecret
        );
        assert_eq!(
            bad("tg://proxy?server=h&port=1&secret=%%%"),
            MtProxyError::BadSecret
        );
        assert_eq!(
            bad(&format!("tg://proxy?server=h&port=1&secret=ee{HEX}7777")),
            MtProxyError::FakeTlsUnsupported
        );
        assert!(!bad("tg://proxy?server=h").to_string().is_empty());
    }
}
