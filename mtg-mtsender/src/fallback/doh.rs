//! DNS-over-HTTPS transport for the fallback config: asks Google, then Mozilla, for the TXT
//! records of the configured domain, as the official client does.

use std::time::Duration;

/// Default domain of the fallback config (`dc_txt_domain_name` of `help.config` overrides it).
pub const DEFAULT_DOMAIN: &str = "apv3.stel.com";

/// URLs to ask, in order.
pub fn urls(domain: &str, padding: &str) -> [String; 2] {
    [
        format!("https://dns.google.com/resolve?name={domain}&type=ANY&random_padding={padding}"),
        format!("https://mozilla.cloudflare-dns.com/dns-query?name={domain}&type=TXT"),
    ]
}

/// The TXT strings of a DoH JSON answer; empty if the body is not the expected shape.
pub fn parse_answer(body: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    json["Answer"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["data"].as_str())
        .map(|d| d.trim_matches('"').to_string())
        .collect()
}

/// Padding that hides the length of the domain from an observer; not secret, so no real RNG.
// shortcut: clock-derived padding, not random; upgrade only if the length leaks something real.
pub fn padding() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    "a".repeat(13 + (n % 20) as usize)
}

/// Blocking fetch of the TXT strings from one URL.
fn fetch_blocking(url: &str) -> Vec<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    agent
        .get(url)
        .header("Accept", "application/dns-json")
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_to_string().ok())
        .map(|b| parse_answer(&b))
        .unwrap_or_default()
}

/// TXT strings from the first resolver that answers with any.
pub async fn fetch_txt(domain: &str) -> Vec<String> {
    for url in urls(domain, &padding()) {
        // a plain blocking client on a blocking thread: no async TLS stack needed
        let chunks = tokio::task::spawn_blocking(move || fetch_blocking(&url))
            .await
            .unwrap_or_default();
        if !chunks.is_empty() {
            return chunks;
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_parsed_and_unquoted() {
        let body = r#"{"Status":0,"Answer":[{"name":"a","type":16,"data":"\"AAAA\""},{"data":"BB"},{"x":1}]}"#;
        assert_eq!(parse_answer(body), vec!["AAAA", "BB"]);
        assert!(parse_answer("{}").is_empty());
        assert!(parse_answer("not json").is_empty());
    }

    #[test]
    fn urls_name_both_resolvers() {
        let u = urls("d.example", "aaa");
        assert!(u[0].starts_with("https://dns.google.com/resolve?name=d.example&type=ANY"));
        assert!(u[0].ends_with("random_padding=aaa"));
        assert!(u[1].starts_with("https://mozilla.cloudflare-dns.com/dns-query?name=d.example"));
        assert!((13..33).contains(&padding().len()));
    }
}
