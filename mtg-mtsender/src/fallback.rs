//! Fallback config for blocked datacenters: the official client reads an encrypted
//! `help.configSimple` from DNS TXT records (fetched over DNS-over-HTTPS), which lists extra
//! addresses per datacenter. This module decodes and applies it; fetching the records is up to
//! the caller (the pure part stays testable).

use std::net::{Ipv4Addr, SocketAddr};

use mtg_crypto::rsa::{Key, config_simple_key, decrypt_config_simple};
use mtg_session::Session;
use mtg_tl_types::{self as tl, Deserializable};

/// Addresses of one datacenter from the fallback config.
#[derive(Debug, PartialEq, Eq)]
pub struct AccessRule {
    /// Comma separated phone prefixes: `+7` allows, `-7` forbids, empty means everyone.
    pub phone_prefix_rules: String,
    /// Datacenter the addresses belong to.
    pub dc_id: i32,
    /// Plain addresses (entries with an MTProxy secret are skipped).
    pub addresses: Vec<SocketAddr>,
}

/// The decoded fallback config.
#[derive(Debug, PartialEq, Eq)]
pub struct SimpleConfig {
    /// Unix time the config became valid.
    pub date: i32,
    /// Unix time it stops being valid.
    pub expires: i32,
    /// Address rules.
    pub rules: Vec<AccessRule>,
}

/// Joins the TXT chunks (longest first, as the official client does) and decodes the base64.
pub fn decode_txt(mut chunks: Vec<String>) -> Option<Vec<u8>> {
    chunks.sort_by_key(|c| std::cmp::Reverse(c.len()));
    base64(&chunks.concat())
}

fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Decrypts and parses a 256-byte config block with the given key.
pub fn parse_simple_config(block: &[u8], key: &Key) -> Option<SimpleConfig> {
    let bytes = decrypt_config_simple(block, key)?;
    let tl::enums::help::ConfigSimple::Simple(config) =
        tl::enums::help::ConfigSimple::from_bytes(&bytes).ok()?;
    let rules = config
        .rules
        .0
        .into_iter()
        .map(|tl::enums::AccessPointRule::Rule(rule)| AccessRule {
            phone_prefix_rules: String::from_utf8_lossy(&rule.phone_prefix_rules).into_owned(),
            dc_id: rule.dc_id,
            addresses: rule
                .ips
                .0
                .into_iter()
                .filter_map(|ip| match ip {
                    tl::enums::IpPort::Port(p) => Some(SocketAddr::new(
                        Ipv4Addr::from(p.ipv4 as u32).into(),
                        u16::try_from(p.port).ok()?,
                    )),
                    tl::enums::IpPort::Secret(_) => None,
                })
                .collect(),
        })
        .collect();
    Some(SimpleConfig {
        date: config.date,
        expires: config.expires,
        rules,
    })
}

/// Whether `rules` (see [`AccessRule::phone_prefix_rules`]) allow `phone`.
pub fn phone_allowed(rules: &str, phone: &str) -> bool {
    if rules.is_empty() || phone.is_empty() {
        return true;
    }
    let mut found = false;
    for prefix in rules.split(',') {
        match prefix.as_bytes().first() {
            None => found = true,
            Some(b'+') if phone.starts_with(&prefix[1..]) => found = true,
            Some(b'-') if phone.starts_with(&prefix[1..]) => return false,
            _ => {}
        }
    }
    found
}

/// Decodes the TXT records and puts the addresses for `phone` in front of each datacenter's
/// address list in the session. `now` is the Unix time (use the server-corrected clock).
///
/// Returns how many datacenters were updated; `0` if the records do not decode or the config is
/// not valid at `now`.
pub async fn apply_dns_config<S: Session>(
    session: &S,
    txt_chunks: Vec<String>,
    phone: &str,
    now: i32,
) -> Result<usize, S::Error> {
    apply_with_key(session, txt_chunks, phone, now, &config_simple_key()).await
}

async fn apply_with_key<S: Session>(
    session: &S,
    txt_chunks: Vec<String>,
    phone: &str,
    now: i32,
    key: &Key,
) -> Result<usize, S::Error> {
    let Some(config) = decode_txt(txt_chunks).and_then(|b| parse_simple_config(&b, key)) else {
        return Ok(0);
    };
    if !(config.date <= now && now <= config.expires) {
        return Ok(0);
    }
    let mut updated = 0;
    for rule in config.rules {
        if rule.addresses.is_empty() || !phone_allowed(&rule.phone_prefix_rules, phone) {
            continue;
        }
        let mut list = rule.addresses;
        for old in session.dc_addresses(rule.dc_id)? {
            if !list.contains(&old) {
                list.push(old);
            }
        }
        session.set_dc_addresses(rule.dc_id, list).await?;
        updated += 1;
    }
    Ok(updated)
}

#[cfg(test)]
mod tests;
