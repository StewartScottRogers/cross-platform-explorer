//! `.eml` structured email preview (CPE-1434, epic CPE-1433 "Structured previews"): a read-only VIEWER
//! for RFC 822 / MIME email messages — parse the headers (From/To/Cc/Subject/Date), walk the MIME part
//! tree, list attachments, and surface a **sanitized plain-text body** so a user can *look at* an `.eml`
//! without their mail client, exactly like [`crate::jwt_preview`] decodes a token or
//! [`crate::binary_preview::pe_info`] decodes a PE header without executing anything.
//!
//! **Security posture — this never renders HTML and never loads remote resources.** When a message
//! carries only an HTML body, [`strip_html`] reduces it to plain text (dropping `<script>`/`<style>`
//! blocks and all tags) so no markup, script, or remote-resource reference (`<img src>`, tracking pixels,
//! CSS `url()`) ever reaches the preview pane. The result is text, so there is nothing to load: the
//! decoder does no I/O of its own at all (the command layer reads the file; this is pure bytes→struct).
//!
//! **Zero new dependencies.** MIME structure, transfer-encoding decode (base64 / quoted-printable), and
//! RFC 2047 encoded-word header decode (`=?charset?B/Q?...?=`) are all hand-rolled, reusing only the
//! crate's existing `base64` dependency and the shared [`crate::fsutil::unix_to_rfc3339`] date humanizer —
//! no `mailparse`/`mail-parser` crate. A MIME message is a small, well-specified line format; the parse is
//! a few hundred lines of bounded string work, far short of the weight (and the transitive-dep + audit
//! surface) a mail-parsing crate would add for a preview.
//!
//! **Never panics.** Every step is bounds-checked and every failure mode (no headers, a bad boundary, bad
//! base64, a truncated part, non-UTF-8 bytes) degrades to a graceful partial result — a body-less
//! header card, an empty body, an `error` note — rather than an `Err` or a panic. Covered by this module's
//! own unit tests and wired into the crate's panic-safety battery (`tests/parser_panic_safety.rs`).

use serde::{Deserialize, Serialize};

use crate::fsutil::unix_to_rfc3339;

/// One leaf node of the MIME part tree, summarised for display: its content-type, an optional filename,
/// the decoded byte size, and whether it was treated as an attachment (vs. an inline body candidate).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MimePart {
    /// Lowercased MIME type, e.g. `"text/plain"`, `"image/png"`, `"application/pdf"`.
    pub content_type: String,
    /// The part's filename (from `Content-Disposition: …; filename=` or `Content-Type: …; name=`),
    /// decoded through the encoded-word decoder, if any.
    pub filename: Option<String>,
    /// Decoded byte length of the part's body (after undoing its transfer-encoding).
    pub size: usize,
    /// `true` if this part was classified as an attachment rather than a shown body.
    pub is_attachment: bool,
}

/// One attachment: filename + decoded size + content-type. A projection of the attachment [`MimePart`]s
/// for the pill row the frontend renders.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Attachment {
    /// The attachment's filename, or `"(unnamed)"` when the part carried none.
    pub filename: String,
    /// Decoded byte length.
    pub size: usize,
    /// Lowercased MIME type.
    pub content_type: String,
}

/// A structured `.eml` preview: the well-known headers, the MIME-part summary, the attachment list, and a
/// sanitized plain-text body. Every field is best-effort — a malformed message still returns whatever
/// could be parsed, with `error` describing what wasn't.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EmailPreview {
    /// `From:` header value, encoded-word-decoded.
    pub from: Option<String>,
    /// `To:` recipients, split on top-level commas and each encoded-word-decoded.
    pub to: Vec<String>,
    /// `Cc:` recipients.
    pub cc: Vec<String>,
    /// `Subject:` header value, encoded-word-decoded.
    pub subject: Option<String>,
    /// The raw `Date:` header value (encoded-word-decoded), preserved verbatim for display.
    pub date: Option<String>,
    /// The `Date:` header parsed and normalised to an RFC 3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) via
    /// the shared [`unix_to_rfc3339`] helper, when it parsed as an RFC 2822 date; `None` otherwise.
    pub date_rfc3339: Option<String>,
    /// The flattened MIME part tree (leaves only — multipart containers are walked, not listed).
    pub parts: Vec<MimePart>,
    /// The attachments (a projection of the attachment parts above).
    pub attachments: Vec<Attachment>,
    /// The sanitized plain-text body: the first `text/plain` part, or — if the message is HTML-only — the
    /// first `text/html` part reduced to text by [`strip_html`]. Never raw HTML, never a remote-resource
    /// reference. Capped at [`MAX_BODY_CHARS`].
    pub body: String,
    /// `true` when `body` was derived from an HTML part (so the UI can show a "shown as text" note).
    pub body_is_html: bool,
    /// `true` when `body` was truncated at [`MAX_BODY_CHARS`].
    pub body_truncated: bool,
    /// Set when the input didn't look like an email at all (no headers) — the header card still renders
    /// whatever was found.
    pub error: Option<String>,
}

/// Cap on the number of MIME parts walked — a pathological deeply-fanned message can't make the walk
/// unbounded. Real mail is a handful of parts; hundreds already covers anything legitimate.
const MAX_PARTS: usize = 200;
/// Cap on multipart nesting depth (`multipart/mixed` → `multipart/alternative` → … ). Guards against a
/// crafted message with absurd nesting.
const MAX_DEPTH: usize = 20;
/// Cap on the sanitized body length returned. A preview doesn't need a multi-megabyte body in memory or on
/// the wire; 512 KiB of text is far more than any pane shows at once.
const MAX_BODY_CHARS: usize = 512 * 1024;

/// Decode a `.eml` message for preview. Never panics — see the module docs for the graceful-degradation
/// contract. Pure bytes→struct; the command layer does the file I/O.
pub fn email_preview(bytes: &[u8]) -> EmailPreview {
    // Headers are ASCII (non-ASCII text is RFC-2047-encoded); bodies carry their own charset. A single
    // lossy decode up front lets the whole structural parse work on `&str`; base64/QP bodies survive it
    // intact (they're ASCII), and a raw 8-bit body only matters for display, where lossy is already the
    // right "show whatever we got" behaviour for a preview.
    let text = String::from_utf8_lossy(bytes);
    let (header_block, body) = split_headers_body(&text);
    let headers = parse_headers(header_block);

    let mut out = EmailPreview {
        from: header_get(&headers, "from").map(|v| decode_encoded_words(&v)),
        to: header_get(&headers, "to").map(split_addresses).unwrap_or_default(),
        cc: header_get(&headers, "cc").map(split_addresses).unwrap_or_default(),
        subject: header_get(&headers, "subject").map(|v| decode_encoded_words(&v)),
        ..Default::default()
    };
    if let Some(d) = header_get(&headers, "date") {
        let d = decode_encoded_words(&d);
        out.date_rfc3339 = parse_rfc2822_date(&d).map(unix_to_rfc3339);
        out.date = Some(d);
    }

    let mut ctx = WalkCtx::default();
    walk(&headers, body, &mut ctx, 0);
    out.parts = ctx.parts;
    out.attachments = ctx.attachments;

    let (body_text, is_html) = match (ctx.plain_body, ctx.html_body) {
        (Some(p), _) => (p, false),
        (None, Some(h)) => (strip_html(&h), true),
        (None, None) => (String::new(), false),
    };
    let (capped, truncated) = truncate_chars(&body_text, MAX_BODY_CHARS);
    out.body = capped;
    out.body_is_html = is_html;
    out.body_truncated = truncated;

    if headers.is_empty() {
        out.error = Some("not a valid email message: no RFC 822 headers found".to_string());
    }
    out
}

/// Accumulator threaded through the recursive [`walk`]: the flattened parts, the attachments, and the
/// first plain/html body candidate seen (later ones are ignored — an alternative's first-listed part is
/// the least-rich, but for a *preview* the first text/plain is the safe canonical body).
#[derive(Default)]
struct WalkCtx {
    parts: Vec<MimePart>,
    attachments: Vec<Attachment>,
    plain_body: Option<String>,
    html_body: Option<String>,
}

/// Recursively walk one MIME entity (its headers + body). `multipart/*` entities are split on their
/// boundary and each child walked; every other entity is a leaf that's recorded as a part and classified
/// as an attachment or a body candidate.
fn walk(headers: &[(String, String)], body: &str, ctx: &mut WalkCtx, depth: usize) {
    if depth > MAX_DEPTH || ctx.parts.len() >= MAX_PARTS {
        return;
    }

    let ct_raw = header_get(headers, "content-type").unwrap_or_else(|| "text/plain".to_string());
    let (mime, ct_params) = parse_content_type(&ct_raw);
    let cte = header_get(headers, "content-transfer-encoding")
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let disp_raw = header_get(headers, "content-disposition").unwrap_or_default();
    let (disposition, disp_params) = parse_content_type(&disp_raw); // same `token; k=v` grammar
    let filename = param(&disp_params, "filename")
        .or_else(|| param(&ct_params, "name"))
        .map(|f| decode_encoded_words(&f));

    if mime.starts_with("multipart/") {
        if let Some(boundary) = param(&ct_params, "boundary") {
            for sub in split_parts(body, &boundary) {
                if ctx.parts.len() >= MAX_PARTS {
                    break;
                }
                let (h, b) = split_headers_body(sub);
                let sub_headers = parse_headers(h);
                walk(&sub_headers, b, ctx, depth + 1);
            }
            return;
        }
        // A multipart with no boundary can't be split — fall through and treat it as an opaque leaf.
    }

    let decoded = decode_transfer(body, &cte);
    let size = decoded.len();
    // Attachment iff it declares itself one, carries a filename, or isn't textual content we'd show.
    let is_attachment = disposition == "attachment"
        || filename.is_some()
        || !(mime.is_empty() || mime.starts_with("text/"));

    ctx.parts.push(MimePart {
        content_type: mime.clone(),
        filename: filename.clone(),
        size,
        is_attachment,
    });

    if is_attachment {
        ctx.attachments.push(Attachment {
            filename: filename.unwrap_or_else(|| "(unnamed)".to_string()),
            size,
            content_type: mime,
        });
    } else if mime == "text/plain" && ctx.plain_body.is_none() {
        ctx.plain_body = Some(String::from_utf8_lossy(&decoded).into_owned());
    } else if mime == "text/html" && ctx.html_body.is_none() {
        ctx.html_body = Some(String::from_utf8_lossy(&decoded).into_owned());
    }
}

// ---------------------------------------------------------------------------------------------------
// Header + structural parsing
// ---------------------------------------------------------------------------------------------------

/// Split a message (or a MIME part) into its header block and body at the first blank line. A message with
/// no blank line is treated as all-headers (body empty) — the common shape of a bare header dump.
fn split_headers_body(s: &str) -> (&str, &str) {
    if let Some(i) = s.find("\r\n\r\n") {
        (&s[..i], &s[i + 4..])
    } else if let Some(i) = s.find("\n\n") {
        (&s[..i], &s[i + 2..])
    } else {
        (s, "")
    }
}

/// Parse a header block into ordered `(name, value)` pairs, unfolding RFC 822 continuation lines (a line
/// beginning with space/tab continues the previous header's value). Names keep their original case; look
/// them up case-insensitively via [`header_get`]. A line with no colon that isn't a continuation is
/// ignored rather than fatal.
fn parse_headers(block: &str) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut cur: Option<(String, String)> = None;
    for raw_line in block.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, v)) = cur.as_mut() {
                v.push(' ');
                v.push_str(line.trim_start());
            }
        } else if let Some(colon) = line.find(':') {
            if let Some(h) = cur.take() {
                headers.push(h);
            }
            let name = line[..colon].trim().to_string();
            let value = line[colon + 1..].trim_start().to_string();
            cur = Some((name, value));
        }
        // else: a non-continuation line without a colon — malformed; skip it.
    }
    if let Some(h) = cur.take() {
        headers.push(h);
    }
    headers
}

/// Case-insensitive header lookup, returning the first match's value as an owned `String` (values are
/// small, and owning sidesteps borrow tangles at the call sites).
fn header_get(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

/// Split a multipart body on its boundary into the child part slices. The preamble (before the first
/// boundary) and epilogue (after the closing `--boundary--`) are dropped, and the CRLF that follows each
/// boundary delimiter line is stripped so each returned slice begins at the child's own headers.
fn split_parts<'a>(body: &'a str, boundary: &str) -> Vec<&'a str> {
    let delim = format!("--{boundary}");
    let mut parts = Vec::new();
    let mut segs = body.split(delim.as_str());
    segs.next(); // preamble before the first boundary
    for seg in segs {
        // The closing delimiter is `--boundary--`; the split leaves a segment starting with "--".
        if seg.starts_with("--") {
            break;
        }
        let seg = seg
            .strip_prefix("\r\n")
            .or_else(|| seg.strip_prefix('\n'))
            .unwrap_or(seg);
        parts.push(seg);
    }
    parts
}

/// Parse a structured header value of the form `token; key=value; key2="quoted value"` (used for both
/// `Content-Type` and `Content-Disposition`). Returns the lowercased leading token and the lowercased-key,
/// original-case-value parameter list. Quotes around a value are stripped.
fn parse_content_type(value: &str) -> (String, Vec<(String, String)>) {
    let mut segs = value.split(';');
    let token = segs.next().unwrap_or("").trim().to_ascii_lowercase();
    let mut params = Vec::new();
    for seg in segs {
        if let Some(eq) = seg.find('=') {
            let k = seg[..eq].trim().to_ascii_lowercase();
            let v = seg[eq + 1..].trim().trim_matches('"').to_string();
            if !k.is_empty() {
                params.push((k, v));
            }
        }
    }
    (token, params)
}

/// Look up a parameter value by (already-lowercased) key.
fn param(params: &[(String, String)], key: &str) -> Option<String> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
}

/// Split a `To:`/`Cc:` header into individual recipients on top-level commas, decoding each through the
/// encoded-word decoder and dropping empties. (A display name containing a literal comma inside quotes is
/// a rare edge this naive split doesn't honour — acceptable for a preview.)
fn split_addresses(value: String) -> Vec<String> {
    value
        .split(',')
        .map(|a| decode_encoded_words(a.trim()))
        .filter(|a| !a.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------------------------------
// Transfer-encoding decode (base64 / quoted-printable)
// ---------------------------------------------------------------------------------------------------

/// Undo a part's `Content-Transfer-Encoding`. `base64` and `quoted-printable` are decoded; `7bit`/`8bit`/
/// `binary`/unknown are passed through as raw bytes. A base64 body that won't decode falls back to its raw
/// bytes rather than failing.
fn decode_transfer(body: &str, cte: &str) -> Vec<u8> {
    match cte {
        "base64" => base64_decode_loose(body).unwrap_or_else(|| body.as_bytes().to_vec()),
        "quoted-printable" => decode_qp(body),
        _ => body.as_bytes().to_vec(),
    }
}

/// Whitespace-tolerant base64 decode (MIME base64 is wrapped at 76 columns, so newlines are expected).
/// Tries padded standard base64 first, then unpadded, returning `None` only if both fail.
fn base64_decode_loose(s: &str) -> Option<Vec<u8>> {
    use base64::{
        engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
        Engine as _,
    };
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() {
        return Some(Vec::new());
    }
    STANDARD
        .decode(&cleaned)
        .or_else(|_| STANDARD_NO_PAD.decode(cleaned.trim_end_matches('=')))
        .ok()
}

/// Decode a quoted-printable body: `=XX` hex escapes become their byte, `=`-at-end-of-line soft breaks are
/// removed, everything else is literal. Malformed `=` sequences are passed through verbatim.
fn decode_qp(input: &str) -> Vec<u8> {
    let b = input.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'=' {
            // Soft line break: "=\r\n" or "=\n".
            if i + 2 < b.len() && b[i + 1] == b'\r' && b[i + 2] == b'\n' {
                i += 3;
                continue;
            }
            if i + 1 < b.len() && b[i + 1] == b'\n' {
                i += 2;
                continue;
            }
            // Hex escape "=XX".
            if i + 2 < b.len() {
                if let (Some(h), Some(l)) = (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                    out.push(h * 16 + l);
                    i += 3;
                    continue;
                }
            }
            // Not a recognised escape — literal '='.
            out.push(b'=');
            i += 1;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// One hex nibble, or `None` for a non-hex byte.
fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------------
// RFC 2047 encoded-word header decode (=?charset?B/Q?text?=)
// ---------------------------------------------------------------------------------------------------

/// Decode any RFC 2047 encoded-words (`=?charset?B?base64?=` / `=?charset?Q?quoted?=`) embedded in a
/// header value, leaving surrounding literal text intact. Per the RFC, whitespace *between two adjacent
/// encoded-words* is not significant and is dropped. The charset label is not honoured beyond UTF-8
/// (decoded bytes are read lossily as UTF-8) — enough for the overwhelmingly common `utf-8` case and a
/// safe, panic-free fallback for anything else in a preview.
fn decode_encoded_words(input: &str) -> String {
    let mut out = String::new();
    let mut remaining = input;
    let mut prev_was_encoded = false;
    while let Some(idx) = remaining.find("=?") {
        let (before, from_marker) = remaining.split_at(idx);
        if let Some((decoded, rest)) = parse_one_encoded_word(from_marker) {
            // Drop whitespace that merely separated two encoded-words; keep any real intervening text.
            if !(prev_was_encoded && before.trim().is_empty()) {
                out.push_str(before);
            }
            out.push_str(&decoded);
            remaining = rest;
            prev_was_encoded = true;
        } else {
            // Not actually an encoded-word — emit the literal "=?" and move past it.
            out.push_str(before);
            out.push_str("=?");
            remaining = &from_marker[2..];
            prev_was_encoded = false;
        }
    }
    out.push_str(remaining);
    out
}

/// Parse a single encoded-word at the start of `s` (which begins with `=?`). Returns the decoded text and
/// the remainder after the closing `?=`, or `None` if `s` isn't a well-formed encoded-word.
fn parse_one_encoded_word(s: &str) -> Option<(String, &str)> {
    let inner = s.strip_prefix("=?")?;
    let q1 = inner.find('?')?;
    let charset = &inner[..q1];
    let after_charset = &inner[q1 + 1..];
    let q2 = after_charset.find('?')?;
    let enc = &after_charset[..q2];
    let after_enc = &after_charset[q2 + 1..];
    let end = after_enc.find("?=")?;
    let text = &after_enc[..end];
    let rest = &after_enc[end + 2..];
    // A charset label is short; an empty or absurd one means this isn't a real encoded-word.
    if charset.is_empty() || charset.len() > 100 {
        return None;
    }
    let bytes = match enc {
        "B" | "b" => base64_decode_loose(text)?,
        "Q" | "q" => decode_q(text),
        _ => return None,
    };
    Some((String::from_utf8_lossy(&bytes).into_owned(), rest))
}

/// Decode RFC 2047 "Q" encoding (a header-specific quoted-printable variant): `_` is a space, `=XX` is a
/// hex byte, everything else literal.
fn decode_q(input: &str) -> Vec<u8> {
    let b = input.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'_' => {
                out.push(b' ');
                i += 1;
            }
            b'=' if i + 2 < b.len() => {
                if let (Some(h), Some(l)) = (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                    out.push(h * 16 + l);
                    i += 3;
                } else {
                    out.push(b'=');
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// HTML → text sanitization (no markup, no remote resources reach the preview)
// ---------------------------------------------------------------------------------------------------

/// Reduce an HTML body to plain text: drop `<script>`/`<style>` element *contents* entirely, strip every
/// remaining tag, decode a handful of common entities, and collapse runaway blank lines. The output is
/// plain text, so no tag, script, or remote-resource reference (`<img src>`, CSS `url()`, tracking pixel)
/// survives — nothing for the pane to load. Bounds-checked; never panics on malformed markup.
fn strip_html(input: &str) -> String {
    let without_blocks = remove_element(&remove_element(input, "script"), "style");

    // Strip tags. A `<` with no matching `>` (truncated markup) is emitted literally rather than eating
    // the rest of the string.
    let mut text = String::with_capacity(without_blocks.len());
    let bytes = without_blocks.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some(close) = without_blocks[i..].find('>') {
                // Turn common block-level tags into a newline so paragraphs don't run together.
                let tag = without_blocks[i + 1..i + close].trim_start_matches('/');
                let name: String = tag
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();
                if matches!(name.as_str(), "br" | "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                    text.push('\n');
                }
                i += close + 1;
                continue;
            }
            // No closing '>' — literal '<'.
            text.push('<');
            i += 1;
        } else {
            // Copy one whole char (not byte) to stay on UTF-8 boundaries.
            let ch = without_blocks[i..].chars().next().unwrap_or('\u{FFFD}');
            text.push(ch);
            i += ch.len_utf8();
        }
    }

    collapse_blank_lines(&decode_entities(&text))
}

/// Remove every `<tag …>…</tag>` element (including its contents) for a given tag name, case-insensitively.
/// Used to excise `<script>`/`<style>` bodies before the generic tag strip.
fn remove_element(input: &str, tag: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}");
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if lower[i..].starts_with(&open) {
            // Find the end of this element (`</tag...>`), or the string end if unterminated.
            if let Some(rel) = lower[i..].find(&close) {
                let after_close = i + rel;
                if let Some(gt) = lower[after_close..].find('>') {
                    i = after_close + gt + 1;
                    continue;
                }
            }
            break; // unterminated — drop the remainder
        }
        let ch = input[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Decode the small set of HTML entities that actually matter for readable text.
fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

/// Collapse 3+ consecutive newlines down to a blank-line gap, and trim leading/trailing whitespace, so a
/// tag-stripped HTML body doesn't render as a tower of blank lines.
fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0;
    for ch in s.chars() {
        if ch == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push('\n');
            }
        } else {
            newlines = 0;
            out.push(ch);
        }
    }
    out.trim().to_string()
}

// ---------------------------------------------------------------------------------------------------
// RFC 2822 date parsing (best-effort) + misc
// ---------------------------------------------------------------------------------------------------

/// Parse an RFC 2822 `Date:` value (`[Dow, ]D Mon YYYY HH:MM[:SS] ±ZZZZ`) into Unix-epoch seconds (UTC),
/// or `None` if it doesn't fit the shape. Best-effort: named time zones other than `GMT`/`UT`/`Z` are
/// treated as UTC (their offset unknown without a table), which is close enough for a humanised preview.
fn parse_rfc2822_date(value: &str) -> Option<i64> {
    let mut toks: Vec<&str> = value.split_whitespace().collect();
    // Drop an optional leading day-of-week token ("Mon," or "Mon").
    if let Some(first) = toks.first() {
        if first.ends_with(',') || matches!(*first, "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun") {
            toks.remove(0);
        }
    }
    if toks.len() < 4 {
        return None;
    }
    let day: u32 = toks[0].parse().ok()?;
    let month = month_num(toks[1])?;
    let mut year: i64 = toks[2].parse().ok()?;
    if year < 100 {
        // RFC 2822 obsolete 2-digit year: 0-49 → 2000s, 50-99 → 1900s.
        year += if year < 50 { 2000 } else { 1900 };
    }
    let (hh, mm, ss) = parse_hms(toks[3])?;
    let zone_offset = toks.get(4).map(|z| parse_zone(z)).unwrap_or(0);

    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hh * 3600 + mm * 60 + ss - zone_offset;
    Some(secs)
}

/// 3-letter English month → 1..=12.
fn month_num(s: &str) -> Option<u32> {
    Some(match s {
        "Jan" => 1, "Feb" => 2, "Mar" => 3, "Apr" => 4, "May" => 5, "Jun" => 6,
        "Jul" => 7, "Aug" => 8, "Sep" => 9, "Oct" => 10, "Nov" => 11, "Dec" => 12,
        _ => return None,
    })
}

/// Parse `HH:MM` or `HH:MM:SS` into `(hours, minutes, seconds)` as `i64`s.
fn parse_hms(s: &str) -> Option<(i64, i64, i64)> {
    let mut it = s.split(':');
    let hh: i64 = it.next()?.parse().ok()?;
    let mm: i64 = it.next()?.parse().ok()?;
    let ss: i64 = match it.next() {
        Some(v) => v.parse().ok()?,
        None => 0,
    };
    Some((hh, mm, ss))
}

/// Parse a `±HHMM` numeric zone (or `GMT`/`UT`/`UTC`/`Z`) into an offset in seconds east of UTC. Unknown
/// alphabetic zones default to 0 (UTC).
fn parse_zone(z: &str) -> i64 {
    let b = z.as_bytes();
    if (b.first() == Some(&b'+') || b.first() == Some(&b'-')) && z.len() >= 5 {
        if let (Ok(h), Ok(m)) = (z[1..3].parse::<i64>(), z[3..5].parse::<i64>()) {
            let mag = h * 3600 + m * 60;
            return if b[0] == b'-' { -mag } else { mag };
        }
    }
    0
}

/// (year, month, day) → days since 1970-01-01. Howard Hinnant's `days_from_civil`
/// (<https://howardhinnant.github.io/date_algorithms.html>) — the inverse of
/// [`crate::fsutil`]'s `civil_from_days`; all-`i64` arithmetic, no overflow across the representable range.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let m = m as u64;
    let d = d as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe as i64 - 719_468
}

/// Truncate a string to at most `max` chars on a char boundary (never a byte-offset slice, which would
/// panic mid-codepoint). Returns the (possibly truncated) string and whether truncation happened.
fn truncate_chars(s: &str, max: usize) -> (String, bool) {
    if s.chars().count() <= max {
        (s.to_string(), false)
    } else {
        (s.chars().take(max).collect(), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full multipart/mixed message: encoded-word Subject, a quoted-printable text/plain body, and a
    /// base64 attachment. Exercises the whole happy path end-to-end.
    fn sample_mixed() -> &'static str {
        // Subject encoded-word decodes to "Héllo —". Body QP "=C3=A9" → é. Attachment "hello attach\n".
        "From: Alice <alice@example.com>\r\n\
         To: Bob <bob@example.com>, carol@example.com\r\n\
         Cc: dave@example.com\r\n\
         Subject: =?utf-8?B?SMOpbGxvIOKAlA==?=\r\n\
         Date: Mon, 07 Aug 2026 09:30:00 +0000\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"BOUND\"\r\n\
         \r\n\
         preamble ignored\r\n\
         --BOUND\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: quoted-printable\r\n\
         \r\n\
         Caf=C3=A9 time.\r\n\
         --BOUND\r\n\
         Content-Type: application/octet-stream; name=\"note.txt\"\r\n\
         Content-Transfer-Encoding: base64\r\n\
         Content-Disposition: attachment; filename=\"note.txt\"\r\n\
         \r\n\
         aGVsbG8gYXR0YWNo\r\n\
         --BOUND--\r\n"
    }

    #[test]
    fn parses_headers_body_and_attachment() {
        let p = email_preview(sample_mixed().as_bytes());
        assert_eq!(p.from.as_deref(), Some("Alice <alice@example.com>"));
        assert_eq!(p.to, vec!["Bob <bob@example.com>", "carol@example.com"]);
        assert_eq!(p.cc, vec!["dave@example.com"]);
        // Encoded-word subject decoded.
        assert_eq!(p.subject.as_deref(), Some("Héllo —"));
        // Date humanized via the shared helper.
        assert_eq!(p.date_rfc3339.as_deref(), Some("2026-08-07T09:30:00Z"));
        // QP body decoded (=C3=A9 → é).
        assert!(p.body.contains("Café time."), "body was: {:?}", p.body);
        assert!(!p.body_is_html);
        // One attachment, base64-decoded to 12 bytes ("hello attach").
        assert_eq!(p.attachments.len(), 1);
        assert_eq!(p.attachments[0].filename, "note.txt");
        assert_eq!(p.attachments[0].size, b"hello attach".len());
        assert_eq!(p.attachments[0].content_type, "application/octet-stream");
        // Two leaf parts (the plain body + the attachment); the container isn't listed.
        assert_eq!(p.parts.len(), 2);
        assert!(p.error.is_none());
    }

    #[test]
    fn html_only_message_is_stripped_to_text_no_markup() {
        let msg = "From: x@example.com\r\n\
                   Subject: HTML only\r\n\
                   Content-Type: text/html; charset=utf-8\r\n\
                   \r\n\
                   <html><head><style>.a{color:red}</style></head><body>\
                   <script>alert('x')</script>\
                   <p>Hello <b>world</b> &amp; welcome</p>\
                   <img src=\"http://tracker.example/pixel.gif\">\
                   </body></html>";
        let p = email_preview(msg.as_bytes());
        assert!(p.body_is_html, "an html-only body must be flagged is_html");
        // No tags, no script, no style, no remote-resource URL survives.
        assert!(!p.body.contains('<'), "no tags: {:?}", p.body);
        assert!(!p.body.to_lowercase().contains("script"));
        assert!(!p.body.contains("color:red"));
        assert!(!p.body.contains("tracker.example"), "remote URL must be dropped: {:?}", p.body);
        // The readable text (with entity decoded) survives.
        assert!(p.body.contains("Hello"));
        assert!(p.body.contains("world"));
        assert!(p.body.contains("& welcome"), "entity decoded: {:?}", p.body);
    }

    #[test]
    fn prefers_plain_text_over_html_in_multipart_alternative() {
        let msg = "From: a@b.com\r\n\
                   Subject: alt\r\n\
                   Content-Type: multipart/alternative; boundary=\"X\"\r\n\
                   \r\n\
                   --X\r\n\
                   Content-Type: text/plain\r\n\
                   \r\n\
                   plain wins\r\n\
                   --X\r\n\
                   Content-Type: text/html\r\n\
                   \r\n\
                   <p>html loses</p>\r\n\
                   --X--\r\n";
        let p = email_preview(msg.as_bytes());
        assert!(p.body.contains("plain wins"));
        assert!(!p.body.contains("html loses"));
        assert!(!p.body_is_html);
    }

    #[test]
    fn base64_encoded_word_q_variant_decodes() {
        // Q-encoding: underscore is space, =XX hex.
        let decoded = decode_encoded_words("=?utf-8?Q?Hello_World_=E2=82=AC?=");
        assert_eq!(decoded, "Hello World €");
    }

    #[test]
    fn adjacent_encoded_words_drop_separating_whitespace() {
        // Two encoded-words separated only by a space → the space is not significant (RFC 2047).
        let decoded = decode_encoded_words("=?utf-8?B?SGVsbG8=?= =?utf-8?B?V29ybGQ=?=");
        assert_eq!(decoded, "HelloWorld");
    }

    #[test]
    fn simple_single_part_message_has_body() {
        let msg = "From: solo@example.com\r\nSubject: plain\r\n\r\nJust a body.\r\n";
        let p = email_preview(msg.as_bytes());
        assert_eq!(p.from.as_deref(), Some("solo@example.com"));
        assert!(p.body.contains("Just a body."));
        assert_eq!(p.attachments.len(), 0);
        assert!(p.error.is_none());
    }

    #[test]
    fn malformed_input_degrades_gracefully_no_panic() {
        // No headers at all.
        let p = email_preview(b"\xff\xfe not an email at all \x00\x01");
        assert!(p.error.is_some() || p.from.is_none());
        // Non-UTF-8 bytes everywhere.
        let _ = email_preview(&[0xff; 1024]);
        // Empty input.
        let p2 = email_preview(b"");
        assert!(p2.error.is_some());
        // A truncated multipart (boundary declared, body cut off mid-part) must not panic.
        let truncated = "Content-Type: multipart/mixed; boundary=\"B\"\r\n\r\n--B\r\nContent-Type: text/plain\r\n\r\nhi";
        let _ = email_preview(truncated.as_bytes());
    }

    #[test]
    fn attachment_without_filename_is_labelled_unnamed() {
        let msg = "Content-Type: multipart/mixed; boundary=\"B\"\r\n\r\n\
                   --B\r\n\
                   Content-Type: image/png\r\n\
                   Content-Transfer-Encoding: base64\r\n\
                   \r\n\
                   iVBORw0KGgo=\r\n\
                   --B--\r\n";
        let p = email_preview(msg.as_bytes());
        assert_eq!(p.attachments.len(), 1);
        assert_eq!(p.attachments[0].filename, "(unnamed)");
        assert_eq!(p.attachments[0].content_type, "image/png");
    }

    #[test]
    fn date_round_trips_known_value() {
        // Sanity on the RFC 2822 → epoch → RFC 3339 pipeline with a non-UTC zone.
        // 2023-11-14 22:13:20 +0000 == unix 1_700_000_000.
        assert_eq!(parse_rfc2822_date("Tue, 14 Nov 2023 22:13:20 +0000"), Some(1_700_000_000));
        // +0100 shifts the epoch back an hour.
        assert_eq!(parse_rfc2822_date("14 Nov 2023 23:13:20 +0100"), Some(1_700_000_000));
    }

    #[test]
    fn days_from_civil_inverts_civil_from_days() {
        // Round-trip a handful of epochs against fsutil's forward converter (via the RFC 3339 render).
        assert_eq!(unix_to_rfc3339(days_from_civil(1970, 1, 1) * 86_400), "1970-01-01T00:00:00Z");
        assert_eq!(unix_to_rfc3339(days_from_civil(2026, 8, 7) * 86_400), "2026-08-07T00:00:00Z");
    }
}
