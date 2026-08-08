//! `.ics` iCalendar structured preview (CPE-1435, epic CPE-1433 "Structured previews"): a read-only
//! VIEWER for RFC 5545 iCalendar files — unfold the folded content lines, split the calendar into its
//! `VEVENT` / `VTODO` / `VJOURNAL` components, and decode the useful properties per component (summary,
//! start/end, location, description, organizer, attendees, status, recurrence) so a user can *look at* an
//! invitation or calendar export without a calendar app, exactly like [`crate::email_preview`] decodes an
//! `.eml` or [`crate::jwt_preview`] decodes a token without launching anything.
//!
//! **Zero new dependencies.** RFC 5545 line unfolding, the `NAME;param=value:value` content-line grammar,
//! text un-escaping (`\n`/`\,`/`\;`/`\\`), the `DATE`/`DATE-TIME` value forms, and a readable `RRULE`
//! summary are all hand-rolled — no `icalendar`/`ical` crate. A calendar is a small, well-specified line
//! format; the parse is a few hundred lines of bounded string work, far short of the weight (and the
//! transitive-dep + audit surface) a calendar-parsing crate would add for a preview.
//!
//! **Never panics.** Every step is bounds-checked and every failure mode (no `BEGIN:VCALENDAR`, a
//! truncated component, a malformed date, non-UTF-8 bytes, an unterminated quoted parameter) degrades to a
//! graceful partial result — a calendar with whatever events could be parsed, an `error` note — rather
//! than an `Err` or a panic. Covered by this module's own unit tests and wired into the crate's
//! panic-safety battery (`tests/parser_panic_safety.rs`).

use serde::{Deserialize, Serialize};

/// One decoded calendar component (`VEVENT` / `VTODO` / `VJOURNAL`), summarised for display. Every field
/// is best-effort — a component missing a property simply leaves it `None`/empty.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct IcalEvent {
    /// The component kind: `"VEVENT"`, `"VTODO"`, or `"VJOURNAL"`.
    pub component: String,
    /// `SUMMARY` — the event title, text-unescaped.
    pub summary: Option<String>,
    /// `DTSTART`, humanised to `YYYY-MM-DD` (all-day) or `YYYY-MM-DDTHH:MM:SS[Z]` (date-time), with the
    /// originating `TZID` appended when the value is a floating local time carrying one.
    pub dtstart: Option<String>,
    /// The raw `DTSTART` value exactly as it appeared (e.g. `20260807T093000Z`), for reference.
    pub dtstart_raw: Option<String>,
    /// `DTEND` (for `VEVENT`) or `DUE` (for `VTODO`), humanised like [`IcalEvent::dtstart`].
    pub dtend: Option<String>,
    /// `true` when the start was an all-day `DATE` value (`VALUE=DATE` or an 8-digit date with no time).
    pub all_day: bool,
    /// `LOCATION`, text-unescaped.
    pub location: Option<String>,
    /// `DESCRIPTION`, text-unescaped.
    pub description: Option<String>,
    /// `ORGANIZER`, resolved to its `CN` display name if present, else the bare address (`mailto:` stripped).
    pub organizer: Option<String>,
    /// `ATTENDEE` list, each resolved to its `CN` display name if present, else the bare address.
    pub attendees: Vec<String>,
    /// `STATUS` (e.g. `CONFIRMED` / `TENTATIVE` / `CANCELLED`), verbatim.
    pub status: Option<String>,
    /// A readable one-line summary of `RRULE` (e.g. "Weekly on Mon, Wed, 10 times"), or the raw rule text
    /// when it couldn't be summarised.
    pub recurrence: Option<String>,
    /// The raw `RRULE` value exactly as it appeared, for reference.
    pub rrule_raw: Option<String>,
    /// `UID`, verbatim — a stable identifier some clients show.
    pub uid: Option<String>,
}

/// A structured `.ics` preview: the calendar-level metadata plus the decoded components. A malformed
/// calendar still returns whatever could be parsed, with `error` describing what wasn't.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct IcalPreview {
    /// A human-readable calendar name — `X-WR-CALNAME` if present, else the `PRODID` that wrote the file.
    pub calendar_name: Option<String>,
    /// `METHOD` (e.g. `REQUEST` / `PUBLISH` / `CANCEL`) — set on scheduling messages (invitations).
    pub method: Option<String>,
    /// The decoded components, in file order.
    pub events: Vec<IcalEvent>,
    /// Set when the input didn't look like an iCalendar file, or carried no `VEVENT`/`VTODO`/`VJOURNAL`
    /// component — the (possibly empty) calendar card still renders whatever was found.
    pub error: Option<String>,
}

/// Cap on the number of components decoded — a pathological calendar with millions of events can't make
/// the walk unbounded in memory. Real calendars are far smaller; tens of thousands already covers a full
/// multi-year export.
const MAX_EVENTS: usize = 50_000;

/// Decode an `.ics` calendar for preview. Never panics — see the module docs for the graceful-degradation
/// contract. Pure bytes→struct; the command layer does the file I/O.
pub fn ical_preview(bytes: &[u8]) -> IcalPreview {
    // Content is UTF-8 per RFC 5545; a lossy decode lets the whole parse work on `&str` and is the right
    // "show whatever we got" behaviour for a preview if the bytes aren't clean.
    let text = String::from_utf8_lossy(bytes);
    let lines = unfold(&text);

    let mut out = IcalPreview::default();
    // Component nesting: `cur` is the event being built; `sub_depth` counts sub-components nested inside it
    // (e.g. a `VALARM` inside a `VEVENT`, or `STANDARD`/`DAYLIGHT` inside a `VTIMEZONE`) whose properties
    // must NOT be attributed to the event. `stack` tracks non-event containers (`VCALENDAR`/`VTIMEZONE`).
    let mut cur: Option<IcalEvent> = None;
    let mut sub_depth: usize = 0;
    let mut stack: Vec<String> = Vec::new();

    for line in &lines {
        let cl = match parse_content_line(line) {
            Some(cl) => cl,
            None => continue,
        };
        match cl.name.as_str() {
            "BEGIN" => {
                let comp = cl.value.trim().to_ascii_uppercase();
                if cur.is_some() {
                    // Anything that begins while we're inside an event is a sub-component; skip its props.
                    sub_depth += 1;
                } else if is_event_component(&comp) {
                    if out.events.len() < MAX_EVENTS {
                        cur = Some(IcalEvent { component: comp, ..Default::default() });
                    } else {
                        // Cap reached — treat as an opaque container so its END is balanced.
                        stack.push(comp);
                    }
                } else {
                    stack.push(comp);
                }
            }
            "END" => {
                if sub_depth > 0 {
                    sub_depth -= 1;
                } else if let Some(ev) = cur.take() {
                    out.events.push(ev);
                } else {
                    stack.pop();
                }
            }
            _ => {
                if sub_depth > 0 {
                    continue; // property of a skipped sub-component
                }
                if let Some(ev) = cur.as_mut() {
                    apply_event_property(ev, &cl);
                } else if stack.last().map(String::as_str) == Some("VCALENDAR") {
                    apply_calendar_property(&mut out, &cl);
                }
                // else: a property inside VTIMEZONE / an unknown container — ignored.
            }
        }
    }

    // A component left open at EOF (a truncated calendar) is still worth showing.
    if let Some(ev) = cur.take() {
        out.events.push(ev);
    }

    if out.events.is_empty() {
        out.error = Some(if text.contains("BEGIN:VCALENDAR") {
            "no VEVENT / VTODO / VJOURNAL components found in this calendar".to_string()
        } else {
            "not a valid iCalendar file: no BEGIN:VCALENDAR found".to_string()
        });
    }
    out
}

/// `true` for the three top-level components this viewer surfaces.
fn is_event_component(name: &str) -> bool {
    matches!(name, "VEVENT" | "VTODO" | "VJOURNAL")
}

/// Apply a calendar-level property (`METHOD`, `X-WR-CALNAME`, `PRODID`) to the preview.
fn apply_calendar_property(out: &mut IcalPreview, cl: &ContentLine) {
    match cl.name.as_str() {
        "METHOD" => out.method = Some(cl.value.trim().to_string()),
        "X-WR-CALNAME" => out.calendar_name = Some(unescape_text(cl.value.trim())),
        // Only fall back to PRODID if a friendlier X-WR-CALNAME hasn't already been set.
        "PRODID" if out.calendar_name.is_none() => {
            out.calendar_name = Some(unescape_text(cl.value.trim()))
        }
        _ => {}
    }
}

/// Apply one decoded property to the component being built.
fn apply_event_property(ev: &mut IcalEvent, cl: &ContentLine) {
    match cl.name.as_str() {
        "SUMMARY" => ev.summary = Some(unescape_text(&cl.value)),
        "LOCATION" => ev.location = Some(unescape_text(&cl.value)),
        "DESCRIPTION" => ev.description = Some(unescape_text(&cl.value)),
        "STATUS" => ev.status = Some(cl.value.trim().to_string()),
        "UID" => ev.uid = Some(cl.value.trim().to_string()),
        "DTSTART" => {
            let (human, all_day) = humanize_ical_dt(cl);
            ev.all_day = all_day;
            ev.dtstart = Some(human);
            ev.dtstart_raw = Some(cl.value.trim().to_string());
        }
        // A VTODO uses DUE where a VEVENT uses DTEND; surface either as the end.
        "DTEND" | "DUE" => {
            let (human, _) = humanize_ical_dt(cl);
            ev.dtend = Some(human);
        }
        "ORGANIZER" => ev.organizer = Some(display_person(cl)),
        "ATTENDEE" => ev.attendees.push(display_person(cl)),
        "RRULE" => {
            ev.rrule_raw = Some(cl.value.trim().to_string());
            ev.recurrence = Some(summarize_rrule(cl.value.trim()));
        }
        _ => {}
    }
}

/// Resolve an `ORGANIZER`/`ATTENDEE` line to a display string: the `CN` parameter if present, else the
/// bare address with a leading `mailto:` scheme stripped.
fn display_person(cl: &ContentLine) -> String {
    if let Some(cn) = param(&cl.params, "cn") {
        let cn = cn.trim();
        if !cn.is_empty() {
            return cn.to_string();
        }
    }
    let v = cl.value.trim();
    v.strip_prefix("mailto:")
        .or_else(|| v.strip_prefix("MAILTO:"))
        .unwrap_or(v)
        .to_string()
}

/// Humanise a `DTSTART`/`DTEND`/`DUE` line into a readable timestamp, plus whether it was an all-day
/// `DATE` value. Falls back to the raw value if it doesn't fit the RFC date/date-time shape.
fn humanize_ical_dt(cl: &ContentLine) -> (String, bool) {
    let value = cl.value.trim();
    let is_date = param(&cl.params, "value").map(|v| v.eq_ignore_ascii_case("DATE")).unwrap_or(false)
        || (!value.contains('T') && value.len() == 8 && value.bytes().all(|b| b.is_ascii_digit()));

    let mut human = reformat_ical_datetime(value, is_date).unwrap_or_else(|| value.to_string());
    // A floating local time carrying a TZID isn't UTC; name the zone so the reader isn't misled.
    if !is_date && !human.ends_with('Z') {
        if let Some(tz) = param(&cl.params, "tzid") {
            let tz = tz.trim();
            if !tz.is_empty() {
                human = format!("{human} {tz}");
            }
        }
    }
    (human, is_date)
}

/// Reformat an RFC 5545 `DATE` (`YYYYMMDD`) or `DATE-TIME` (`YYYYMMDDTHHMMSS[Z]`) value into a readable
/// `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SS[Z]`. Returns `None` if the value doesn't match either shape (the
/// caller then keeps the raw text). Every slice is on a validated ASCII-digit boundary, so it never panics.
fn reformat_ical_datetime(raw: &str, force_date: bool) -> Option<String> {
    let s = raw.trim();
    let (date_part, time_part) = match s.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    if date_part.len() != 8 || !date_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let date = format!("{}-{}-{}", &date_part[0..4], &date_part[4..6], &date_part[6..8]);
    if force_date || time_part.is_none() {
        return Some(date);
    }
    let t = time_part.unwrap();
    let (hms, utc) = match t.strip_suffix('Z') {
        Some(rest) => (rest, true),
        None => (t, false),
    };
    // Need at least HHMM, and the leading 4 (HH MM) — plus SS when present — must be ASCII digits. Work on
    // bytes and only str-slice once digit-validated, so non-ASCII lossy input can't slice mid-codepoint.
    let hb = hms.as_bytes();
    if hb.len() < 4 || !hb[..4].iter().all(u8::is_ascii_digit) {
        return Some(date); // the date is good even if the time is garbage
    }
    let ss = if hb.len() >= 6 && hb[4..6].iter().all(u8::is_ascii_digit) { &hms[4..6] } else { "00" };
    let out = format!("{date}T{}:{}:{}{}", &hms[0..2], &hms[2..4], ss, if utc { "Z" } else { "" });
    Some(out)
}

/// Turn an `RRULE` value (`FREQ=WEEKLY;BYDAY=MO,WE;COUNT=10`) into a readable one-liner. Best-effort: the
/// common `FREQ`/`INTERVAL`/`BYDAY`/`COUNT`/`UNTIL` shape is summarised; anything it can't make sense of
/// falls back to the raw rule text so nothing is lost.
fn summarize_rrule(raw: &str) -> String {
    let mut freq: Option<&str> = None;
    let mut interval: u32 = 1;
    let mut byday: Option<&str> = None;
    let mut count: Option<&str> = None;
    let mut until: Option<&str> = None;
    for part in raw.split(';') {
        let (k, v) = match part.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        match k.trim().to_ascii_uppercase().as_str() {
            "FREQ" => freq = Some(v.trim()),
            "INTERVAL" => interval = v.trim().parse().unwrap_or(1),
            "BYDAY" => byday = Some(v.trim()),
            "COUNT" => count = Some(v.trim()),
            "UNTIL" => until = Some(v.trim()),
            _ => {}
        }
    }
    let freq = match freq {
        Some(f) => f,
        None => return raw.to_string(), // no FREQ at all — not something we can summarise
    };
    // Base cadence, pluralised when INTERVAL > 1.
    let unit = match freq.to_ascii_uppercase().as_str() {
        "SECONDLY" => "second",
        "MINUTELY" => "minute",
        "HOURLY" => "hour",
        "DAILY" => "day",
        "WEEKLY" => "week",
        "MONTHLY" => "month",
        "YEARLY" => "year",
        _ => return raw.to_string(),
    };
    let mut s = if interval > 1 {
        format!("Every {interval} {unit}s")
    } else {
        // "day" -> "Daily", "week" -> "Weekly", etc.
        match unit {
            "day" => "Daily".to_string(),
            "week" => "Weekly".to_string(),
            "month" => "Monthly".to_string(),
            "year" => "Yearly".to_string(),
            "hour" => "Hourly".to_string(),
            other => format!("Every {other}"),
        }
    };
    if let Some(days) = byday {
        let names: Vec<String> = days
            .split(',')
            .filter_map(|d| weekday_name(d.trim()))
            .collect();
        if !names.is_empty() {
            s.push_str(" on ");
            s.push_str(&names.join(", "));
        }
    }
    if let Some(c) = count {
        if let Ok(n) = c.parse::<u32>() {
            s.push_str(&format!(", {n} times"));
        }
    } else if let Some(u) = until {
        if let Some(d) = reformat_ical_datetime(u, false) {
            s.push_str(&format!(", until {d}"));
        }
    }
    s
}

/// Map an RFC 5545 `BYDAY` weekday token (optionally with a leading ordinal like `2MO`) to a short name.
fn weekday_name(token: &str) -> Option<String> {
    // Strip a leading ordinal prefix (`+2`, `-1`, `2`) — the last two chars are the weekday.
    let day = token.trim_start_matches(|c: char| c == '+' || c == '-' || c.is_ascii_digit());
    Some(match day.to_ascii_uppercase().as_str() {
        "MO" => "Mon",
        "TU" => "Tue",
        "WE" => "Wed",
        "TH" => "Thu",
        "FR" => "Fri",
        "SA" => "Sat",
        "SU" => "Sun",
        _ => return None,
    }
    .to_string())
}

// ---------------------------------------------------------------------------------------------------
// Content-line parsing (RFC 5545 §3.1) — shared shape with vCard, kept module-local so the module is
// self-contained and independently testable.
// ---------------------------------------------------------------------------------------------------

/// One parsed content line: the upper-cased property name, its lower-cased-key parameters, and the raw
/// value (everything after the first unquoted `:`).
struct ContentLine {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

/// Unfold RFC 5545 folded lines: a line beginning with a space or tab is a continuation of the previous
/// line, and exactly one leading whitespace character is removed before the content is appended.
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if (line.starts_with(' ') || line.starts_with('\t')) && !out.is_empty() {
            // `line` starts with a single-byte ASCII whitespace, so `&line[1..]` is on a char boundary.
            out.last_mut().unwrap().push_str(&line[1..]);
        } else {
            out.push(line.to_string());
        }
    }
    out
}

/// Parse one unfolded content line into `NAME`, parameters, and value. Returns `None` for a line with no
/// unquoted colon (a blank line or garbage), which the caller skips.
fn parse_content_line(line: &str) -> Option<ContentLine> {
    // Find the first ':' that isn't inside a quoted parameter value (a `mailto:`/`CN="a:b"` colon must
    // not be mistaken for the name/value separator).
    let mut in_q = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_q = !in_q,
            ':' if !in_q => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let left = &line[..colon];
    let value = &line[colon + 1..]; // ':' is one ASCII byte, so this is a char boundary.

    let segs = split_semicolons(left);
    let mut name = segs.first().map(|s| s.trim().to_string()).unwrap_or_default();
    // Strip a group prefix (`item1.TEL` → `TEL`) — legal in vCard, harmless in iCalendar.
    if let Some(dot) = name.rfind('.') {
        name = name[dot + 1..].to_string();
    }
    let name = name.to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }

    let mut params = Vec::new();
    for seg in segs.iter().skip(1) {
        if let Some(eq) = seg.find('=') {
            let k = seg[..eq].trim().to_ascii_lowercase();
            let v = seg[eq + 1..].trim().trim_matches('"').to_string();
            if !k.is_empty() {
                params.push((k, v));
            }
        } else {
            // A bare parameter (vCard 2.1 `TEL;WORK:`) — record it as a valueless key so TYPE collection
            // can pick it up.
            let k = seg.trim().to_ascii_lowercase();
            if !k.is_empty() {
                params.push((k, String::new()));
            }
        }
    }
    Some(ContentLine { name, params, value: value.to_string() })
}

/// Split a parameter section on `;`, honouring double-quoted parameter values (so a quoted `;` doesn't
/// split). Quotes are preserved in the segments and stripped later per parameter.
fn split_semicolons(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_q = !in_q;
                cur.push(c);
            }
            ';' if !in_q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Look up a parameter value by (already-lower-cased) key.
fn param(params: &[(String, String)], key: &str) -> Option<String> {
    params.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

/// Un-escape RFC 5545 / vCard TEXT: `\n`/`\N` → newline, `\,` → comma, `\;` → semicolon, `\\` → backslash;
/// an unknown `\x` keeps the literal `x`, and a trailing lone `\` is preserved.
fn unescape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full VCALENDAR with a folded DESCRIPTION line, two attendees (one with a CN), a UTC DTSTART, a
    /// weekly RRULE, and a nested VALARM whose properties must NOT leak onto the event.
    fn sample_calendar() -> &'static str {
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//CPE//Sample//EN\r\n\
         METHOD:REQUEST\r\n\
         X-WR-CALNAME:Team Calendar\r\n\
         BEGIN:VEVENT\r\n\
         UID:evt-1@example.com\r\n\
         SUMMARY:Weekly sync\r\n\
         DTSTART:20260807T093000Z\r\n\
         DTEND:20260807T100000Z\r\n\
         LOCATION:Room 42\r\n\
         DESCRIPTION:Line one\\nand a folded\r\n \x20continuation.\r\n\
         ORGANIZER;CN=Alice Example:mailto:alice@example.com\r\n\
         ATTENDEE;CN=Bob Example:mailto:bob@example.com\r\n\
         ATTENDEE:mailto:carol@example.com\r\n\
         STATUS:CONFIRMED\r\n\
         RRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=10\r\n\
         BEGIN:VALARM\r\n\
         ACTION:DISPLAY\r\n\
         SUMMARY:This alarm summary must not overwrite the event\r\n\
         TRIGGER:-PT15M\r\n\
         END:VALARM\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR\r\n"
    }

    #[test]
    fn parses_calendar_event_fully() {
        let p = ical_preview(sample_calendar().as_bytes());
        assert!(p.error.is_none(), "well-formed calendar must not error: {:?}", p.error);
        // X-WR-CALNAME beats PRODID for the display name.
        assert_eq!(p.calendar_name.as_deref(), Some("Team Calendar"));
        assert_eq!(p.method.as_deref(), Some("REQUEST"));
        assert_eq!(p.events.len(), 1);
        let ev = &p.events[0];
        assert_eq!(ev.component, "VEVENT");
        // The nested VALARM's SUMMARY must NOT have overwritten the event's.
        assert_eq!(ev.summary.as_deref(), Some("Weekly sync"));
        assert_eq!(ev.uid.as_deref(), Some("evt-1@example.com"));
        // UTC date-time humanised to RFC-3339-ish.
        assert_eq!(ev.dtstart.as_deref(), Some("2026-08-07T09:30:00Z"));
        assert_eq!(ev.dtend.as_deref(), Some("2026-08-07T10:00:00Z"));
        assert!(!ev.all_day);
        assert_eq!(ev.location.as_deref(), Some("Room 42"));
        // Folded continuation joined, and the \n escape decoded.
        assert_eq!(ev.description.as_deref(), Some("Line one\nand a folded continuation."));
        // Organizer + attendees resolved to CN where present, mailto stripped otherwise.
        assert_eq!(ev.organizer.as_deref(), Some("Alice Example"));
        assert_eq!(ev.attendees, vec!["Bob Example", "carol@example.com"]);
        assert_eq!(ev.status.as_deref(), Some("CONFIRMED"));
        assert_eq!(ev.recurrence.as_deref(), Some("Weekly on Mon, Wed, 10 times"));
        assert_eq!(ev.rrule_raw.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=10"));
    }

    #[test]
    fn all_day_date_value_is_flagged() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n\
                   SUMMARY:Holiday\r\n\
                   DTSTART;VALUE=DATE:20261225\r\n\
                   DTEND;VALUE=DATE:20261226\r\n\
                   END:VEVENT\r\nEND:VCALENDAR\r\n";
        let p = ical_preview(ics.as_bytes());
        let ev = &p.events[0];
        assert!(ev.all_day, "VALUE=DATE start must be flagged all-day");
        assert_eq!(ev.dtstart.as_deref(), Some("2026-12-25"));
        assert_eq!(ev.dtend.as_deref(), Some("2026-12-26"));
    }

    #[test]
    fn bare_eight_digit_date_is_all_day_without_value_param() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20261225\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let p = ical_preview(ics.as_bytes());
        assert!(p.events[0].all_day);
        assert_eq!(p.events[0].dtstart.as_deref(), Some("2026-12-25"));
    }

    #[test]
    fn floating_time_with_tzid_names_the_zone() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n\
                   DTSTART;TZID=America/New_York:20260807T093000\r\n\
                   END:VEVENT\r\nEND:VCALENDAR\r\n";
        let p = ical_preview(ics.as_bytes());
        assert_eq!(p.events[0].dtstart.as_deref(), Some("2026-08-07T09:30:00 America/New_York"));
        assert!(!p.events[0].all_day);
    }

    #[test]
    fn multiple_components_including_vtodo_and_vjournal() {
        let ics = "BEGIN:VCALENDAR\r\n\
                   BEGIN:VEVENT\r\nSUMMARY:An event\r\nEND:VEVENT\r\n\
                   BEGIN:VTODO\r\nSUMMARY:A task\r\nDUE:20260808T170000Z\r\nEND:VTODO\r\n\
                   BEGIN:VJOURNAL\r\nSUMMARY:A note\r\nEND:VJOURNAL\r\n\
                   END:VCALENDAR\r\n";
        let p = ical_preview(ics.as_bytes());
        assert_eq!(p.events.len(), 3);
        assert_eq!(p.events[0].component, "VEVENT");
        assert_eq!(p.events[1].component, "VTODO");
        // VTODO's DUE surfaces as the end.
        assert_eq!(p.events[1].dtend.as_deref(), Some("2026-08-08T17:00:00Z"));
        assert_eq!(p.events[2].component, "VJOURNAL");
    }

    #[test]
    fn quoted_cn_with_comma_and_colon_is_not_mis_split() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n\
                   ATTENDEE;CN=\"Doe, John: VP\";ROLE=REQ-PARTICIPANT:mailto:john@example.com\r\n\
                   END:VEVENT\r\nEND:VCALENDAR\r\n";
        let p = ical_preview(ics.as_bytes());
        assert_eq!(p.events[0].attendees, vec!["Doe, John: VP"]);
    }

    #[test]
    fn interval_and_until_rrule_summarised() {
        assert_eq!(summarize_rrule("FREQ=DAILY;INTERVAL=3"), "Every 3 days");
        assert_eq!(summarize_rrule("FREQ=MONTHLY;INTERVAL=2;UNTIL=20271231"), "Every 2 months, until 2027-12-31");
        assert_eq!(summarize_rrule("FREQ=YEARLY"), "Yearly");
        // Unsummarisable rule falls back to raw.
        assert_eq!(summarize_rrule("BYMONTH=3;COUNT=2"), "BYMONTH=3;COUNT=2");
    }

    #[test]
    fn malformed_input_degrades_gracefully_no_panic() {
        // Not a calendar at all.
        let p = ical_preview(b"\xff\xfe not a calendar \x00\x01");
        assert!(p.error.is_some());
        assert!(p.events.is_empty());
        // Non-UTF-8 bytes everywhere.
        let _ = ical_preview(&[0xff; 1024]);
        // Empty.
        assert!(ical_preview(b"").error.is_some());
        // A truncated calendar (BEGIN with no END) must not panic and must not lose the event.
        let truncated = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:cut off";
        let p2 = ical_preview(truncated.as_bytes());
        assert_eq!(p2.events.len(), 1);
        assert_eq!(p2.events[0].summary.as_deref(), Some("cut off"));
        // A line with no colon must be skipped, not panic.
        let _ = ical_preview(b"BEGIN:VCALENDAR\r\nGARBAGE LINE WITH NO COLON\r\nEND:VCALENDAR\r\n");
        // A bad date keeps the (valid) date and doesn't panic.
        let baddate = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:2026ZZZZThhmmss\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let _ = ical_preview(baddate.as_bytes());
    }

    #[test]
    fn calendar_with_only_vtimezone_reports_no_events() {
        let ics = "BEGIN:VCALENDAR\r\n\
                   BEGIN:VTIMEZONE\r\nTZID:UTC\r\n\
                   BEGIN:STANDARD\r\nTZOFFSETTO:+0000\r\nEND:STANDARD\r\n\
                   END:VTIMEZONE\r\nEND:VCALENDAR\r\n";
        let p = ical_preview(ics.as_bytes());
        assert!(p.events.is_empty());
        assert!(p.error.is_some(), "a calendar with no VEVENT/VTODO/VJOURNAL must note it");
    }
}
