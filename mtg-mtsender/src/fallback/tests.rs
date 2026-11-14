use std::sync::Arc;

use mtg_crypto::rsa::Key;
use mtg_session::Session;
use mtg_session::storages::MemorySession;

use super::*;

const N: &str = "21090230903822316117654422484263147060077195495712566810909462356540098787204385939845527215199589878095439591511042318004631157659799851603209389212371066772770536174925195809160416844957003354779569485140676522749105197516930345499632998410855484261066996873879058971590474903556114562718720857625274712183787264272034243957991556472979142799513915266131837093399629940124689657002432229705423195752220251538829538821981326538633461800714560687255943553174887594181651789183072577155773364072660430324422152875686170044554635966684624923973651859643526859172705527823801855226517827523056261492365261902034386537703";
// A block signed with the private half of `N`: rule "" -> dc 2 1.2.3.4:443, rule "+7" -> dc 4
// 5.6.7.8:80 (plus an address with a proxy secret). Valid from 1000 to 2000.
const BLOCK_B64: [&str; 2] = [
    "ig+sH5RhadXF558CiSuWrijZvgu66DkGyfV4dr7SQScRCIM54pZutGv8sww/I/mT5XgP4BWEgq/QS+aumJyYjMdB9bbi+v6obg5E2SW2KXZVGmoxvOHxicW7r+8MaQMthkEcn3hA4ANEyJavPf7T8kiZodu1ipQn2fsf2JTD/loi3U7YCJ1Y7LVxqEJZ4jTFjGXAmhof",
    "eeOgl0IzaiDZv3j6HhrJduj7oot7EZ9dto7ajriUzrvPXG5Gp+IC9uwmH+k4ilbDoGXI4RjnVXbO1eI5Gw4i8l0noVRF+SQwRazMq6MjQEzrxutOb4OIG5m7Izlg3CAa+gostZz3bC5Adg==",
];

fn key() -> Key {
    Key::new(N, "65537").unwrap()
}

/// Chunks in the wrong order, as DNS may return them.
fn chunks() -> Vec<String> {
    BLOCK_B64.iter().rev().map(|s| s.to_string()).collect()
}

fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap()
}

#[test]
fn txt_chunks_are_joined_longest_first_and_decoded() {
    assert_eq!(decode_txt(vec!["AAAA".into()]), Some(vec![0, 0, 0]));
    assert_eq!(decode_txt(vec!["TQ==".into()]), Some(vec![b'M']));
    // the longer chunk goes first
    assert_eq!(
        decode_txt(vec!["Fu".into(), "TWFu".into()]),
        Some(b"Man\x16".to_vec())
    );
    assert_eq!(decode_txt(vec!["!!".into()]), None);
}

#[test]
fn the_config_is_parsed_and_proxy_entries_are_skipped() {
    let bytes = decode_txt(chunks()).unwrap();
    let config = parse_simple_config(&bytes, &key()).unwrap();
    assert_eq!((config.date, config.expires), (1000, 2000));
    assert_eq!(
        config.rules,
        vec![
            AccessRule {
                phone_prefix_rules: "".into(),
                dc_id: 2,
                addresses: vec![addr("1.2.3.4:443")]
            },
            AccessRule {
                phone_prefix_rules: "+7".into(),
                dc_id: 4,
                addresses: vec![addr("5.6.7.8:80")]
            },
        ]
    );
    assert!(parse_simple_config(&bytes[..255], &key()).is_none());
}

#[test]
fn phone_rules_follow_the_official_client() {
    assert!(phone_allowed("", "123"));
    assert!(phone_allowed("+7", ""));
    assert!(phone_allowed("+7", "7900"));
    assert!(!phone_allowed("+7", "1900"));
    assert!(!phone_allowed("+7,-79", "7900"));
    assert!(phone_allowed("+7,-79", "7800"));
    assert!(phone_allowed(",+7", "1"));
    assert!(!phone_allowed("-1", "1900"));
}

#[tokio::test]
async fn the_addresses_for_the_phone_go_in_front_of_the_stored_ones() {
    let session = Arc::new(MemorySession::default());
    session
        .set_dc_addresses(2, vec![addr("8.8.8.8:443")])
        .await
        .unwrap();

    let n = apply_with_key(&*session, chunks(), "7900", 1500, &key())
        .await
        .unwrap();
    assert_eq!(n, 2);
    assert_eq!(
        session.dc_addresses(2).unwrap(),
        vec![addr("1.2.3.4:443"), addr("8.8.8.8:443")]
    );
    assert_eq!(session.dc_addresses(4).unwrap(), vec![addr("5.6.7.8:80")]);

    // not matching the "+7" rule, applying twice does not duplicate
    let session = Arc::new(MemorySession::default());
    for _ in 0..2 {
        assert_eq!(
            apply_with_key(&*session, chunks(), "1555", 1500, &key())
                .await
                .unwrap(),
            1
        );
    }
    assert_eq!(session.dc_addresses(2).unwrap(), vec![addr("1.2.3.4:443")]);
    assert!(session.dc_addresses(4).unwrap().is_empty());
}

#[tokio::test]
async fn expired_or_unverifiable_configs_change_nothing() {
    let session = MemorySession::default();
    for now in [999, 2001] {
        assert_eq!(
            apply_with_key(&session, chunks(), "7900", now, &key())
                .await
                .unwrap(),
            0
        );
    }
    assert_eq!(
        apply_with_key(&session, vec!["zz".into()], "7900", 1500, &key())
            .await
            .unwrap(),
        0
    );
    // the real key did not sign this block
    assert_eq!(
        apply_dns_config(&session, chunks(), "7900", 1500)
            .await
            .unwrap(),
        0
    );
    assert!(session.dc_addresses(2).unwrap().is_empty());
}
