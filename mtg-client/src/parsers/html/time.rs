use html5ever::{Attribute, local_name as tag};
use jiff::fmt::temporal::DateTimeParser;
use jiff::{Timestamp, tz};
use mtg_tl_types as tl;

/// Parse a regular `<time>` HTML tag into a Telegram message entity.
///
/// Returns `None` if the `datetime` attribute value cannot be parsed for any reason.
pub(super) fn parse_time_tag(
    offset: i32,
    length: i32,
    attrs: &[Attribute],
) -> Option<tl::types::MessageEntityFormattedDate> {
    let dt_str = attrs
        .iter()
        .find(|a| a.name.local == tag!("datetime"))
        .map(|a| a.value.to_string());

    let format = attrs
        .iter()
        .find(|a| a.name.local == *"data-format")
        .map(|a| a.value.to_string())
        .unwrap_or_else(|| "".to_string());

    if let Some(dt_str) = dt_str
        && let Some(timestamp) = parse_timestamp(&dt_str)
    {
        let tg_ts: i32 = timestamp.as_second() as i32;

        Some(create_formatted_date(offset, length, tg_ts, &format))
    } else {
        None
    }
}

/// Create a message entity for a given Unix timestamp (`date`) and format string.
///
/// The format string will be parsed and set the relevant flags on the message entity.
/// Flags will be adjusted during parsing to avoid invalid format configurations.
///
/// The order of the format parsing is significant.
/// As an example, given the format string `"rDt"`, the following steps take place:
/// 1. All flags start out disabled.
/// 2. `r` enables `relative` and disables all other flags.
/// 3. `D` disables `relative` and `short_date`, and enables `long_date`.
/// 4. `t` disables `relative`, enables `short_time`, and disables `long_time`.
fn create_formatted_date(
    offset: i32,
    length: i32,
    date: i32,
    format: &str,
) -> tl::types::MessageEntityFormattedDate {
    let mut relative = false;
    let mut short_time = false;
    let mut long_time = false;
    let mut short_date = false;
    let mut long_date = false;
    let mut day_of_week = false;

    for char in format.chars() {
        match char {
            'r' => {
                relative = true;
                short_time = false;
                long_time = false;
                short_date = false;
                long_date = false;
                day_of_week = false;
            }
            't' => {
                relative = false;
                short_time = true;
                long_time = false;
            }
            'T' => {
                relative = false;
                short_time = false;
                long_time = true;
            }
            'd' => {
                relative = false;
                short_date = true;
                long_date = false;
            }
            'D' => {
                relative = false;
                short_date = false;
                long_date = true;
            }
            'w' => {
                relative = false;
                day_of_week = true;
            }
            _ => {}
        }
    }

    tl::types::MessageEntityFormattedDate {
        offset,
        length,
        date,
        relative,
        short_time,
        long_time,
        short_date,
        long_date,
        day_of_week,
    }
}

/// Parses a string into a `Timestamp`.
///
/// If the string does not have a time zone or offset, an attempt will be made to parse it as
/// a "civil" datetime and treat it as UTC.
///
/// If parsing fails, `None` is returned.
fn parse_timestamp(s: &str) -> Option<Timestamp> {
    static PARSER: DateTimeParser = DateTimeParser::new()
        .offset_conflict(tz::OffsetConflict::AlwaysTimeZone)
        .disambiguation(tz::Disambiguation::Compatible);

    if let Ok(zdt) = PARSER.parse_zoned(s) {
        return Some(zdt.timestamp());
    }

    if let Ok(ts) = PARSER.parse_timestamp(s) {
        return Some(ts);
    }

    if let Ok(cdt) = PARSER.parse_datetime(s)
        && let Ok(cdt_utc) = cdt.in_tz("UTC")
    {
        return Some(cdt_utc.timestamp());
    }

    None
}
