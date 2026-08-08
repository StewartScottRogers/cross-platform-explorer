//! `.vcf` vCard structured preview (CPE-1436, epic CPE-1433 "Structured previews"): a read-only VIEWER
//! for vCard 2.1 / 3.0 / 4.0 contact files — unfold the folded content lines, split the file into its
//! `VCARD` blocks, and decode the useful properties per contact (formatted + structured name, org, title,
//! phones, emails, addresses, URLs, birthday) so a user can *look at* a contact card without an address
//! book, exactly like [`crate::ical_preview`] decodes a calendar or [`crate::email_preview`] decodes an
//! `.eml` without launching anything.
//!
//! **Photo is presence-only.** A `PHOTO` property is reported as a boolean + its encoded byte length —
//! the image bytes are **never** returned over IPC, consistent with the app's rule against shipping heavy
//! blobs through a preview command.
//!
//! **Zero new dependencies.** Line unfolding, the `NAME;param=value:value` content-line grammar (shared in
//! shape with iCalendar), text un-escaping (`\n`/`\,`/`\;`/`\\`), the structured `N`/`ADR` component
//! split, and `TYPE` parameter collection (including bare vCard-2.1 type parameters) are all hand-rolled —
//! no `vcard`/`ical` crate. A vCard is a small, well-specified line format; the parse is a couple hundred
//! lines of bounded string work, far short of the weight a contact-parsing crate would add for a preview.
//!
//! **Never panics.** Every step is bounds-checked and every failure mode (no `BEGIN:VCARD`, a truncated
//! block, non-UTF-8 bytes, an unterminated quoted parameter) degrades to a graceful partial result — a
//! card with whatever properties could be parsed, an `error` note — rather than an `Err` or a panic.
//! Covered by this module's own unit tests and wired into the crate's panic-safety battery
//! (`tests/parser_panic_safety.rs`).

use serde::{Deserialize, Serialize};

/// One telephone number with its (lower-cased) `TYPE` labels (e.g. `["work", "cell"]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct VcardPhone {
    /// The dialling number, verbatim.
    pub number: String,
    /// Lower-cased `TYPE` labels, in declaration order.
    pub types: Vec<String>,
}

/// One email address with its (lower-cased) `TYPE` labels.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct VcardEmail {
    /// The email address, verbatim.
    pub address: String,
    /// Lower-cased `TYPE` labels, in declaration order.
    pub types: Vec<String>,
}

/// One postal address, flattened to a single readable line plus its (lower-cased) `TYPE` labels.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct VcardAddress {
    /// The address components (street, locality, region, postal code, country) joined with `, `.
    pub label: String,
    /// Lower-cased `TYPE` labels, in declaration order.
    pub types: Vec<String>,
}

/// One decoded contact (`VCARD` block). Every field is best-effort — a card missing a property leaves it
/// `None`/empty.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct VcardEntry {
    /// `FN` — the formatted display name.
    pub formatted_name: Option<String>,
    /// `N` — the structured name assembled into a readable order (prefix given additional family suffix).
    pub name: Option<String>,
    /// `ORG` — organisation (the `;`-separated units joined with `, `).
    pub org: Option<String>,
    /// `TITLE` — job title.
    pub title: Option<String>,
    /// `TEL` list.
    pub phones: Vec<VcardPhone>,
    /// `EMAIL` list.
    pub emails: Vec<VcardEmail>,
    /// `ADR` list.
    pub addresses: Vec<VcardAddress>,
    /// `URL` list.
    pub urls: Vec<String>,
    /// `BDAY` — birthday, verbatim (dates come in several vCard forms; shown as-is).
    pub birthday: Option<String>,
    /// `true` when the card carried a `PHOTO` property (whose bytes are deliberately never returned).
    pub has_photo: bool,
    /// The `PHOTO` value's encoded byte length (base64 text or a URI), for a "photo present (~N)" note.
    /// `0` when there is no photo.
    pub photo_size: usize,
}

/// A structured `.vcf` preview: the decoded contacts. A malformed file still returns whatever could be
/// parsed, with `error` describing what wasn't.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct VcardPreview {
    /// The decoded contacts, in file order.
    pub cards: Vec<VcardEntry>,
    /// Set when the input carried no `VCARD` block — the (empty) list still renders with this note.
    pub error: Option<String>,
}

/// Cap on the number of contacts decoded — a pathological file can't make the walk unbounded in memory.
/// Real address-book exports are far smaller; tens of thousands already covers a full contacts dump.
const MAX_CARDS: usize = 50_000;

/// Decode a `.vcf` file for preview. Never panics — see the module docs for the graceful-degradation
/// contract. Pure bytes→struct; the command layer does the file I/O.
pub fn vcard_preview(bytes: &[u8]) -> VcardPreview {
    // vCard is UTF-8 (3.0/4.0) or ASCII-ish (2.1); a lossy decode lets the whole parse work on `&str` and
    // is the right "show whatever we got" behaviour for a preview.
    let text = String::from_utf8_lossy(bytes);
    let lines = unfold(&text);

    let mut out = VcardPreview::default();
    let mut cur: Option<VcardEntry> = None;

    for line in &lines {
        let cl = match parse_content_line(line) {
            Some(cl) => cl,
            None => continue,
        };
        match cl.name.as_str() {
            "BEGIN" if cl.value.trim().eq_ignore_ascii_case("VCARD") => {
                if cur.is_some() {
                    // An unterminated card followed by a new BEGIN — flush the previous one first.
                    if let Some(prev) = cur.take() {
                        out.cards.push(prev);
                    }
                }
                if out.cards.len() < MAX_CARDS {
                    cur = Some(VcardEntry::default());
                }
            }
            "END" if cl.value.trim().eq_ignore_ascii_case("VCARD") => {
                if let Some(card) = cur.take() {
                    out.cards.push(card);
                }
            }
            _ => {
                if let Some(card) = cur.as_mut() {
                    apply_property(card, &cl);
                }
            }
        }
    }
    // A card left open at EOF (truncated file) is still worth showing.
    if let Some(card) = cur.take() {
        out.cards.push(card);
    }

    if out.cards.is_empty() {
        out.error = Some("not a valid vCard file: no BEGIN:VCARD block found".to_string());
    }
    out
}

/// Apply one decoded property to the contact being built.
fn apply_property(card: &mut VcardEntry, cl: &ContentLine) {
    match cl.name.as_str() {
        "FN" => card.formatted_name = Some(unescape_text(&cl.value)),
        "N" => card.name = assemble_structured_name(&cl.value),
        "ORG" => card.org = Some(join_components(&cl.value, ", ")),
        "TITLE" => card.title = Some(unescape_text(&cl.value)),
        "TEL" => card.phones.push(VcardPhone {
            number: unescape_text(cl.value.trim()),
            types: collect_types(cl),
        }),
        "EMAIL" => card.emails.push(VcardEmail {
            address: unescape_text(cl.value.trim()),
            types: collect_types(cl),
        }),
        "ADR" => card.addresses.push(VcardAddress {
            label: assemble_address(&cl.value),
            types: collect_types(cl),
        }),
        "URL" => {
            let u = cl.value.trim();
            if !u.is_empty() {
                card.urls.push(u.to_string());
            }
        }
        "BDAY" => card.birthday = Some(cl.value.trim().to_string()),
        // Presence-only: flag it and record the encoded value's length; the bytes are never surfaced.
        "PHOTO" => {
            card.has_photo = true;
            card.photo_size = cl.value.trim().len();
        }
        _ => {}
    }
}

/// Assemble a structured `N` value (`Family;Given;Additional;Prefix;Suffix`) into a readable name in
/// natural order: `Prefix Given Additional Family Suffix`, with empty parts dropped and whitespace
/// collapsed. Returns `None` if every component is empty.
fn assemble_structured_name(value: &str) -> Option<String> {
    let parts: Vec<String> = split_structured(value).into_iter().map(|c| unescape_text(c.trim())).collect();
    // N order is Family;Given;Additional;Prefix;Suffix — reorder to natural reading order.
    let get = |i: usize| parts.get(i).map(String::as_str).unwrap_or("");
    let ordered = [get(3), get(1), get(2), get(0), get(4)]; // prefix given additional family suffix
    let joined = ordered.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ");
    let trimmed = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Assemble a structured `ADR` value (`PO;Ext;Street;Locality;Region;Postal;Country`) into a single
/// readable line, joining the meaningful components (street, locality, region, postal, country) with `, `.
fn assemble_address(value: &str) -> String {
    let parts: Vec<String> = split_structured(value).into_iter().map(|c| unescape_text(c.trim())).collect();
    let get = |i: usize| parts.get(i).map(String::as_str).unwrap_or("");
    [get(2), get(3), get(4), get(5), get(6)] // street, locality, region, postal, country
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
}

/// Join a `;`-separated structured value's non-empty components with `sep` (used for `ORG`).
fn join_components(value: &str, sep: &str) -> String {
    split_structured(value)
        .into_iter()
        .map(|c| unescape_text(c.trim()))
        .filter(|c| !c.is_empty())
        .collect::<Vec<_>>()
        .join(sep)
}

/// Collect the (lower-cased) `TYPE` labels off a property line: every `TYPE=a,b` parameter's comma-split
/// values, plus any bare vCard-2.1 type parameter (`TEL;WORK;VOICE:` → `work`, `voice`). Non-type bare
/// parameters that are clearly not types (`pref`, `encoding`, `charset`, `value`, `language`) are dropped.
fn collect_types(cl: &ContentLine) -> Vec<String> {
    let mut types = Vec::new();
    for (k, v) in &cl.params {
        if k == "type" {
            for t in v.split(',') {
                let t = t.trim().to_ascii_lowercase();
                if !t.is_empty() && !types.contains(&t) {
                    types.push(t);
                }
            }
        } else if v.is_empty() && !NON_TYPE_BARE_PARAMS.contains(&k.as_str()) {
            // A bare vCard-2.1 parameter (no `=`) — treat as a type label.
            let t = k.to_ascii_lowercase();
            if !types.contains(&t) {
                types.push(t);
            }
        }
    }
    types
}

/// Bare parameter keys that are NOT type labels (so a vCard-2.1 `TEL;PREF;WORK:` yields only `work`).
const NON_TYPE_BARE_PARAMS: &[&str] = &["pref", "encoding", "charset", "value", "language", "quoted-printable", "base64"];

// ---------------------------------------------------------------------------------------------------
// Content-line parsing — the same RFC 5545/6350 §3 shape iCalendar uses, kept module-local so this
// module is self-contained and independently testable.
// ---------------------------------------------------------------------------------------------------

/// One parsed content line: the upper-cased property name, its lower-cased-key parameters, and the raw
/// value (everything after the first unquoted `:`).
struct ContentLine {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

/// Unfold folded lines: a line beginning with a space or tab continues the previous line, with exactly one
/// leading whitespace character removed. (vCard 2.1 also allows `=`-soft-wrapped quoted-printable, which
/// this preview does not attempt to rejoin — a rare, legacy shape; the value is shown as-is.)
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
/// unquoted colon.
fn parse_content_line(line: &str) -> Option<ContentLine> {
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
    // Strip a group prefix (`item1.TEL` → `TEL`).
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
            let k = seg.trim().to_ascii_lowercase();
            if !k.is_empty() {
                params.push((k, String::new()));
            }
        }
    }
    Some(ContentLine { name, params, value: value.to_string() })
}

/// Split a parameter section on `;`, honouring double-quoted parameter values.
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

/// Split a structured property VALUE on unescaped `;` into its components, leaving each component's own
/// `\`-escapes intact for a later [`unescape_text`] pass.
fn split_structured(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut esc = false;
    for c in s.chars() {
        if esc {
            // Preserve the escape sequence verbatim so unescape_text can process it later.
            cur.push('\\');
            cur.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == ';' {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    if esc {
        cur.push('\\'); // a dangling backslash at end of value
    }
    out.push(cur);
    out
}

/// Un-escape vCard TEXT: `\n`/`\N` → newline, `\,` → comma, `\;` → semicolon, `\\` → backslash; an unknown
/// `\x` keeps the literal `x`, and a trailing lone `\` is preserved.
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

    /// A vCard 3.0 with a folded ADR, multiple TEL (TYPE + a v2.1-style bare type), multiple EMAIL, an
    /// ORG with a unit, a photo (base64, presence-only), and a structured N.
    fn sample_vcard() -> &'static str {
        "BEGIN:VCARD\r\n\
         VERSION:3.0\r\n\
         FN:Dr. Alice P. Example\r\n\
         N:Example;Alice;Pat;Dr.;PhD\r\n\
         ORG:Acme Corp;Research\r\n\
         TITLE:Principal Engineer\r\n\
         TEL;TYPE=WORK,VOICE:+1-555-0100\r\n\
         TEL;TYPE=CELL:+1-555-0199\r\n\
         EMAIL;TYPE=INTERNET,WORK:alice@example.com\r\n\
         EMAIL;TYPE=HOME:alice.p@example.net\r\n\
         ADR;TYPE=WORK:;;123 Main St\\, Suite 400;Springfield;IL;62704;USA\r\n\
         URL:https://example.com/alice\r\n\
         BDAY:19850407\r\n\
         PHOTO;ENCODING=b;TYPE=JPEG:/9j/4AAQSkZJRgABAQ==\r\n\
         END:VCARD\r\n"
    }

    #[test]
    fn parses_contact_fully() {
        let p = vcard_preview(sample_vcard().as_bytes());
        assert!(p.error.is_none(), "well-formed vcard must not error: {:?}", p.error);
        assert_eq!(p.cards.len(), 1);
        let c = &p.cards[0];
        assert_eq!(c.formatted_name.as_deref(), Some("Dr. Alice P. Example"));
        // N assembled into natural order (prefix given additional family suffix).
        assert_eq!(c.name.as_deref(), Some("Dr. Alice Pat Example PhD"));
        assert_eq!(c.org.as_deref(), Some("Acme Corp, Research"));
        assert_eq!(c.title.as_deref(), Some("Principal Engineer"));
        // Two phones, TYPE labels collected + lower-cased.
        assert_eq!(c.phones.len(), 2);
        assert_eq!(c.phones[0].number, "+1-555-0100");
        assert_eq!(c.phones[0].types, vec!["work", "voice"]);
        assert_eq!(c.phones[1].types, vec!["cell"]);
        // Two emails.
        assert_eq!(c.emails.len(), 2);
        assert_eq!(c.emails[0].address, "alice@example.com");
        assert_eq!(c.emails[0].types, vec!["internet", "work"]);
        // Address flattened; the escaped comma inside "Main St, Suite 400" survives as one component.
        assert_eq!(c.addresses.len(), 1);
        assert_eq!(c.addresses[0].label, "123 Main St, Suite 400, Springfield, IL, 62704, USA");
        assert_eq!(c.addresses[0].types, vec!["work"]);
        assert_eq!(c.urls, vec!["https://example.com/alice"]);
        assert_eq!(c.birthday.as_deref(), Some("19850407"));
        // Photo is presence-only — flagged, sized, bytes never surfaced.
        assert!(c.has_photo);
        assert_eq!(c.photo_size, "/9j/4AAQSkZJRgABAQ==".len());
    }

    #[test]
    fn multiple_cards_in_one_file() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:First Person\r\nEND:VCARD\r\n\
                   BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Second Person\r\nEND:VCARD\r\n";
        let p = vcard_preview(vcf.as_bytes());
        assert_eq!(p.cards.len(), 2);
        assert_eq!(p.cards[0].formatted_name.as_deref(), Some("First Person"));
        assert_eq!(p.cards[1].formatted_name.as_deref(), Some("Second Person"));
    }

    #[test]
    fn vcard_21_bare_type_params_are_collected() {
        // vCard 2.1 uses bare type params (no TYPE=), and PREF/ENCODING must NOT be treated as types.
        let vcf = "BEGIN:VCARD\r\nVERSION:2.1\r\n\
                   FN:Bob\r\n\
                   TEL;WORK;VOICE:+1-555-0000\r\n\
                   TEL;PREF;HOME:+1-555-1111\r\n\
                   END:VCARD\r\n";
        let p = vcard_preview(vcf.as_bytes());
        let c = &p.cards[0];
        assert_eq!(c.phones[0].types, vec!["work", "voice"]);
        // PREF dropped (not a type), HOME kept.
        assert_eq!(c.phones[1].types, vec!["home"]);
    }

    #[test]
    fn folded_adr_line_is_rejoined() {
        // A physically folded ADR (continuation line begins with a space).
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Carol\r\n\
                   ADR:;;123 Very Long Street Name That Was\r\n  Folded;Metropolis;NY;10001;USA\r\n\
                   END:VCARD\r\n";
        let p = vcard_preview(vcf.as_bytes());
        let c = &p.cards[0];
        assert_eq!(c.addresses.len(), 1);
        assert_eq!(
            c.addresses[0].label,
            "123 Very Long Street Name That Was Folded, Metropolis, NY, 10001, USA"
        );
    }

    #[test]
    fn photo_bytes_are_never_in_the_output() {
        let vcf = "BEGIN:VCARD\r\nFN:X\r\nPHOTO;ENCODING=b:QUJDREVGRw==\r\nEND:VCARD\r\n";
        let p = vcard_preview(vcf.as_bytes());
        assert!(p.cards[0].has_photo);
        assert_eq!(p.cards[0].photo_size, "QUJDREVGRw==".len());
        // The struct has no field carrying the base64 payload — presence + size only, by construction.
    }

    #[test]
    fn malformed_input_degrades_gracefully_no_panic() {
        // Not a vCard at all.
        let p = vcard_preview(b"\xff\xfe not a vcard \x00\x01");
        assert!(p.error.is_some());
        assert!(p.cards.is_empty());
        // Non-UTF-8 bytes everywhere.
        let _ = vcard_preview(&[0xff; 1024]);
        // Empty.
        assert!(vcard_preview(b"").error.is_some());
        // A truncated card (BEGIN, some props, no END) must not panic and must still be shown.
        let truncated = "BEGIN:VCARD\r\nFN:Cut Off\r\nTEL:+1-555-9999";
        let p2 = vcard_preview(truncated.as_bytes());
        assert_eq!(p2.cards.len(), 1);
        assert_eq!(p2.cards[0].formatted_name.as_deref(), Some("Cut Off"));
        // A line with no colon must be skipped, not panic.
        let _ = vcard_preview(b"BEGIN:VCARD\r\nGARBAGE NO COLON\r\nEND:VCARD\r\n");
    }

    #[test]
    fn group_prefixed_property_names_are_stripped() {
        let vcf = "BEGIN:VCARD\r\nFN:Dana\r\nitem1.TEL;TYPE=WORK:+1-555-7777\r\nitem1.X-ABLABEL:Direct\r\nEND:VCARD\r\n";
        let p = vcard_preview(vcf.as_bytes());
        assert_eq!(p.cards[0].phones.len(), 1);
        assert_eq!(p.cards[0].phones[0].number, "+1-555-7777");
    }
}
