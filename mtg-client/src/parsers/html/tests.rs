use super::*;

#[test]
fn parse_leading() {
    // Intentionally use different casing to make sure that is handled well
    let (text, entities) = parse_html_message("<B>Hello</b> world!");
    assert_eq!(text, "Hello world!");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityBold {
                offset: 0,
                length: 5
            }
            .into()
        ]
    );
}

#[test]
fn parse_trailing() {
    let (text, entities) = parse_html_message("Hello <strong>world!</strong>");
    assert_eq!(text, "Hello world!");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityBold {
                offset: 6,
                length: 6
            }
            .into()
        ]
    );
}

#[test]
fn parse_emoji() {
    let (text, entities) = parse_html_message("A <b>little 🦀</b> here");
    assert_eq!(text, "A little 🦀 here");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityBold {
                offset: 2,
                length: 9
            }
            .into()
        ]
    );
}

#[test]
fn parse_all_entities() {
    let (text, entities) = parse_html_message(
        "Some <b>bold</b> (<strong>strong</strong>), <i>italics</i> \
        (<em>cursive</em>), inline <code>code</code>, a <pre>pre</pre> \
        block, a <a href=\"https://example.com\"><b>link</b></a>, \
        <details>spoilers</details> and <a href=\"tg://user?id=12345678\">mentions</a>",
    );

    assert_eq!(
        text,
        "Some bold (strong), italics (cursive), inline code, a pre block, a link, spoilers and mentions"
    );
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityBold {
                offset: 5,
                length: 4
            }
            .into(),
            tl::types::MessageEntityBold {
                offset: 11,
                length: 6
            }
            .into(),
            tl::types::MessageEntityItalic {
                offset: 20,
                length: 7
            }
            .into(),
            tl::types::MessageEntityItalic {
                offset: 29,
                length: 7
            }
            .into(),
            tl::types::MessageEntityCode {
                offset: 46,
                length: 4
            }
            .into(),
            tl::types::MessageEntityPre {
                offset: 54,
                length: 3,
                language: "".to_string()
            }
            .into(),
            tl::types::MessageEntityTextUrl {
                offset: 67,
                length: 4,
                url: "https://example.com".to_string()
            }
            .into(),
            tl::types::MessageEntityBold {
                offset: 67,
                length: 4,
            }
            .into(),
            tl::types::MessageEntitySpoiler {
                offset: 73,
                length: 8,
            }
            .into(),
            tl::types::MessageEntityMentionName {
                offset: 86,
                length: 8,
                user_id: 12345678
            }
            .into(),
        ]
    );
}

#[test]
fn parse_pre_with_lang() {
    let (text, entities) = parse_html_message(
        "Some <pre>pre</pre>, <code>normal</code> and \
        <pre><code class=\"language-rust\">rusty</code></pre> code",
    );

    assert_eq!(text, "Some pre, normal and rusty code");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityPre {
                offset: 5,
                length: 3,
                language: "".to_string()
            }
            .into(),
            tl::types::MessageEntityCode {
                offset: 10,
                length: 6,
            }
            .into(),
            tl::types::MessageEntityPre {
                offset: 21,
                length: 5,
                language: "rust".to_string()
            }
            .into(),
        ]
    );
}

#[test]
fn parse_empty_pre_and_lang() {
    let (text, entities) =
        parse_html_message("Some empty <pre></pre> and <code class=\"language-rust\">code</code>");

    assert_eq!(text, "Some empty  and code");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityCode {
                offset: 16,
                length: 4,
            }
            .into(),
        ]
    );
}

#[test]
fn parse_link_no_href() {
    let (text, entities) = parse_html_message("Some <a>empty link</a>, it does nothing");

    assert_eq!(text, "Some empty link, it does nothing");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityTextUrl {
                offset: 5,
                length: 10,
                url: "".to_string()
            }
            .into(),
        ]
    );
}

#[test]
fn parse_time_with_zone() {
    let (text, entities) = parse_html_message(
        "A <time datetime=\"2026-09-27 12:13:14[Europe/Stockholm]\">time</time>",
    );

    assert_eq!(text, "A time");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityFormattedDate {
                offset: 2,
                length: 4,
                date: 1790503994,
                relative: false,
                short_time: false,
                long_time: false,
                short_date: false,
                long_date: false,
                day_of_week: false,
            }
            .into(),
        ]
    );
}

#[test]
fn parse_time_with_timestamp() {
    let (text, entities) =
        parse_html_message("A <time datetime=\"2026-09-27T12:13:14+02:00\">time</time>");

    assert_eq!(text, "A time");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityFormattedDate {
                offset: 2,
                length: 4,
                date: 1790503994,
                relative: false,
                short_time: false,
                long_time: false,
                short_date: false,
                long_date: false,
                day_of_week: false,
            }
            .into(),
        ]
    );
}

#[test]
fn parse_time_with_invalid_date() {
    let (text, entities) = parse_html_message("A <time datetime=\"invalid\">time</time>");

    assert_eq!(text, "A time");
    assert_eq!(entities, vec![]);
}

#[test]
fn parse_time_with_both_valid_and_invalid_date() {
    let (text, entities) = parse_html_message(
        "A <time datetime=\"2026-09-27T12:13:14+02:00\">valid</time> and <time datetime=\"invalid\">invalid</time>",
    );

    assert_eq!(text, "A valid and invalid");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityFormattedDate {
                offset: 2,
                length: 5,
                date: 1790503994,
                relative: false,
                short_time: false,
                long_time: false,
                short_date: false,
                long_date: false,
                day_of_week: false,
            }
            .into(),
        ]
    );
}

#[test]
fn parse_nested_entities() {
    let (text, entities) = parse_html_message("Some <b>bold <i>both</b> italics</i>");
    assert_eq!(text, "Some bold both italics");
    assert_eq!(
        entities,
        vec![
            tl::types::MessageEntityBold {
                offset: 5,
                length: 9
            }
            .into(),
            tl::types::MessageEntityItalic {
                offset: 10,
                length: 12
            }
            .into(),
        ]
    );
}

#[test]
fn parse_then_unparse() {
    let html = "Some <b>bold</b>, <i>italics</i> inline <code>code</code>, \
    a <pre>pre</pre> block <pre><code class=\"language-rs\">use rust;</code></pre>, \
    a <a href=\"https://example.com\"><b>link</b></a>, <details>spoilers</details> and \
    <a href=\"tg://user?id=12345678\">mentions</a>";
    let (text, entities) = parse_html_message(html);
    let generated = generate_html_message(&text, &entities);
    assert_eq!(generated, html);
}

#[test]
fn parse_then_unparse_overlapping() {
    let markdown = "<i>a</i><a href=\"https://example.com\"><b>b</b></a><code>c</code>";
    let (text, entities) = parse_html_message(markdown);
    let generated = generate_html_message(&text, &entities);
    assert_eq!(generated, markdown);
}

#[test]
fn parse_then_unparse_time() {
    let html = "Some <time datetime=\"2026-09-27T12:13:14+02:00\">time</time>";
    let (text, entities) = parse_html_message(html);
    let generated = generate_html_message(&text, &entities);
    let expected = "Some <time datetime=\"2026-09-27T10:13:14Z\">time</time>";
    assert_eq!(generated, expected);
}

#[test]
fn parse_then_unparse_time_with_format() {
    let html = "Some <time datetime=\"2026-09-27T12:13:14+02:00\" data-format=\"wDt\">time</time>";
    let (text, entities) = parse_html_message(html);
    let generated = generate_html_message(&text, &entities);
    let expected = "Some <time datetime=\"2026-09-27T10:13:14Z\" data-format=\"wDt\">time</time>";
    assert_eq!(generated, expected);
}
