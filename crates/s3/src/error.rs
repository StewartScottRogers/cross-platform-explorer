//! Turn a non-2xx S3 HTTP response into an error that names the real cause (CPE-1682, epic CPE-1503).
//!
//! S3 answers a misconfiguration with an HTTP status and an XML body:
//!
//! ```xml
//! <Error><Code>SignatureDoesNotMatch</Code><Message>…</Message><RequestId>…</RequestId></Error>
//! ```
//!
//! A provider that reports `s3: HTTP 403` collapses **SignatureDoesNotMatch** (the clock is skewed, or
//! the secret is wrong), **AccessDenied** (the credential is fine, the policy is not) and
//! **InvalidAccessKeyId** (the key does not exist) into one indistinguishable string, leaving the user to
//! guess which of three unrelated fixes to try. [`map_s3_error`] is the one place in this crate that turns
//! a status + body into an error string, so CPE-1683 (`ListObjectsV2`) and CPE-1684 (object ops) call it
//! rather than each inventing their own status-code handling.
//!
//! # Bounding the parse — an error path is still an attack surface
//! Guards run before any text is trusted:
//!
//! - **[`MAX_ERROR_BODY_BYTES`]** caps how many bytes are ever looked at, so a server that answers with a
//!   huge body (deliberately, or as a misconfigured proxy's giant HTML error page) costs no more than a
//!   normal one.
//! - **[`MAX_ELEMENT_DEPTH`]** caps how many levels of XML element nesting [`scan_elements`] will recurse
//!   into. `cpe-webdav` had to add the same kind of guard ahead of `roxmltree` (CPE-1398) because a
//!   hostile deeply-nested document can stack-overflow the process — and a hand-rolled byte scan it tried
//!   first turned out to have a quote-unaware evasion bug (a `>` hidden inside a quoted attribute value
//!   could make a real, child-bearing element look self-closing to the scanner, silently under-counting
//!   depth and letting the *separate*, unguarded recursive parse afterward walk arbitrarily deep anyway).
//!
//!   This module closes that whole bug *class* rather than patching one instance of it: depth-checking and
//!   recursion are the same function here, not a pre-scan ahead of a second, unguarded parse. `depth` is
//!   incremented on every recursive call this function ever makes and checked before any of those calls
//!   happens, so no matter how the tag/attribute syntax is mangled, the number of stack frames can never
//!   exceed `MAX_ELEMENT_DEPTH + 1`. The trade-off that buys this: [`scan_elements`] never tries to detect
//!   a self-closing `<tag/>` at all — every `<name` is treated as opening a level, full stop. That costs a
//!   little precision on documents that use self-closing tags (S3's own error body never does), but it is
//!   exactly what removes the self-closing heuristic a quoted `>` could otherwise fool.
//! - **Comments, CDATA and processing instructions are skipped as opaque spans**, not scanned for tags
//!   ([`skip_past`]). XML permits a bare `>` inside exactly those three constructs, and stopping at the
//!   first `>` instead of the real terminator (`-->`, `]]>`, `?>`) lets a `<Code>` hidden inside
//!   `<!-- a > <Code>Fake</Code> -->` be walked as if it were live markup — either shadowing the real
//!   `<Code>` that follows, or (worse, as a leading comment) preventing the real body from ever being
//!   reached at all. A confident answer with the wrong cause, from the one module whose entire subject is
//!   confident answers with wrong causes.
//! - **The parsed `code` is shape-checked, not merely echoed** ([`is_plausible_code`]): a real S3 code is a
//!   short token of `[A-Za-z0-9]`, so anything else — `<![CDATA[...]]>`, `"><`, an embedded `<Message>`, an
//!   embedded comment, or simply markup that slipped through as literal text — is treated as "no usable
//!   code was read" rather than surfaced as if it were one. `message`, which is expected to be free text,
//!   is instead passed through [`sanitize_remote`]: this crate already holds *outbound* header/credential
//!   text to "no control characters, they would split a signed line" (`validate_structural_text`,
//!   `sigv4::reject_framing_bytes`); nothing enforced the mirror rule on text arriving *from* a hostile
//!   endpoint that a caller will render as this app's own error text. A CR/LF/NUL there can forge a fake
//!   second log line or a fake UI block; an ANSI escape can clear the screen; an unbounded length can
//!   exhaust a caller that doesn't itself truncate. [`sanitize_remote`] neutralises the first two and
//!   bounds the third.
//! - **A nested `<Code>`/`<Message>` cannot hijack the real one.** Once a `<Code>` or `<Message>` element
//!   has been opened, [`scan_elements`] suppresses capture for everything found *inside* its content — so
//!   `<Message>text <Code>Fake</Code> more</Message>` appearing before the real `<Code>` in document order
//!   cannot plant a value that "first field wins" would otherwise lock in. This was found while fixing the
//!   comment/CDATA issue above, by the same reasoning applied one level further: any construct that lets
//!   attacker text be walked as if it were structure is the same bug in a different costume.

/// Upper bound on how many bytes of an error response body [`map_s3_error`] will look at. A real S3
/// `<Error>` document is a few hundred bytes; this is generous headroom above that while keeping a huge or
/// hostile body from costing more than a bounded, constant amount of memory and scan time to handle.
///
/// **Pinned by its own test** (`max_error_body_bytes_is_pinned_to_16_kib_not_merely_bounded`), not only by
/// tests that scale their fixture from this constant: a fixture built as `MAX_ERROR_BODY_BYTES + N` still
/// passes if this constant is silently widened (an independent reviewer found this could move to 64 MiB —
/// 4096x — with every existing test green). If you deliberately change this value, update that test's
/// expectation in the same commit.
const MAX_ERROR_BODY_BYTES: usize = 16 * 1024;

/// Upper bound on how many levels of XML element nesting [`scan_elements`] will recurse into before
/// refusing to go deeper. A real S3 error body nests two levels deep (`Error` -> `Code`/`Message`); this
/// is wide headroom above that — enough for a gateway that wraps the body in an extra envelope element —
/// while still refusing a document nested far beyond anything real. See this module's top doc for why
/// going deeper is refused at all rather than merely made expensive.
///
/// **This bound is stack-bound, not a nicety.** [`scan_elements`] recurses once per level, and each call
/// frame costs real call-stack space — measured at roughly 2.7 KiB per frame in a debug build, so depth 32
/// costs on the order of 90 KiB: a ~2.8x margin under this repo's 256 KiB small-stack test standard (see
/// `map_s3_error_never_stack_overflows_on_deep_nesting_on_a_256kib_stack` below, which proves the margin
/// empirically rather than leaving it as an assertion in a comment). Raising this constant materially —
/// say, to 256 "for a chatty gateway" — would cost on the order of 700 KiB and can overflow a small stack;
/// widen it only alongside a fresh measurement and a re-run of that test, not as a bare number bump.
const MAX_ELEMENT_DEPTH: usize = 32;

/// The two fields this module ever extracts from an S3 error body. `None` means "not found" — including
/// "found, but the closing tag never matched" (a truncated read) and "found, but nested inside another
/// captured field" (see [`scan_elements`]).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
struct ParsedFields {
    code: Option<String>,
    message: Option<String>,
}

/// A genuine `</name>` closing tag [`scan_elements`] found, versus running out of buffer without one —
/// the distinction a truncated read needs (see that function's doc). Named rather than an anonymous tuple
/// so the return type of a recursive function stays readable.
#[derive(Debug)]
struct ClosedAt {
    name: Vec<u8>,
    tag_start: usize,
}

/// Turn a non-2xx S3 response into an error that names the real cause instead of a bare status code.
///
/// `body` is whatever bytes the transport read for the response, whether or not it looks like XML at all
/// — a proxy's HTML 502 page, an empty body from a connection that dropped mid-response, or a truncated
/// read are all routed through here rather than special-cased by the caller. Each of those produces an
/// honest "the body could not be read" error that names the status but never guesses a code (this
/// ticket's AC2) — [`scan_elements`] only reports a field as found when its closing tag genuinely matched,
/// so a truncated `<Code>NoSuchBu` (cut off mid-value, no `</Code>`) is treated exactly like no `<Code>` at
/// all rather than surfacing the partial text as if it had been read in full. A `<Code>` that *is* found
/// but is empty, whitespace-only, or does not have the shape of a real S3 code ([`is_plausible_code`]) is
/// treated the same way, and the "could not be read" message names the byte cap when it is the reason
/// nothing further could be examined, rather than claiming something about the whole body when only a
/// prefix of it was ever looked at.
///
/// A code this module does not name explicitly (this crate talks to gateways beyond AWS proper, which
/// have their own extension codes) still comes through with its code and message, just without the
/// explanatory suffix [`known_code_explanation`] adds for the five named ones — "unknown codes pass
/// through verbatim rather than being flattened", per the ticket's scope. `message` is passed through
/// [`sanitize_remote`] first: it is expected free text (S3's own messages are natural-language sentences),
/// so it is not shape-checked like `code`, but it is still remote-controlled text about to be rendered as
/// this app's own error output.
pub fn map_s3_error(status: u16, body: &[u8]) -> String {
    let capped = &body[..body.len().min(MAX_ERROR_BODY_BYTES)];
    let mut fields = ParsedFields::default();
    if let Err(reason) = scan_elements(capped, 0, 0, false, &mut fields) {
        return format!("s3: HTTP {status} — {reason}");
    }

    let plausible_code = fields.code.as_deref().filter(|c| is_plausible_code(c));

    match plausible_code {
        Some(code) => {
            let message = sanitize_remote(fields.message.as_deref().unwrap_or(""), 512);
            let explanation = known_code_explanation(code).map(|e| format!(" — {e}")).unwrap_or_default();
            if message.is_empty() {
                format!("s3: HTTP {status} {code}{explanation}")
            } else {
                format!("s3: HTTP {status} {code}: {message}{explanation}")
            }
        }
        None => {
            let truncation_note = if body.len() > MAX_ERROR_BODY_BYTES {
                format!(
                    " (only the first {MAX_ERROR_BODY_BYTES} bytes of a {}-byte body were examined)",
                    body.len()
                )
            } else {
                String::new()
            };
            format!(
                "s3: HTTP {status} and the response body could not be read as an S3 error (no non-empty \
                 <Code> element was found in it{truncation_note}); refusing to guess which cause applies"
            )
        }
    }
}

/// Whether `code` has the shape a real S3 error code always has: a short token of `[A-Za-z0-9]`, 1-64
/// characters — never markup, never punctuation. Every AWS-documented code (`NoSuchBucket`,
/// `SignatureDoesNotMatch`, ...) fits this; nothing legitimate does not. Anything else — `<![CDATA[...]]>`,
/// `"><`, an embedded `<Message>`, an embedded comment, or a byte-cap-truncated fragment — is treated by
/// [`map_s3_error`] as "no usable code was read" rather than echoed as if it were one.
fn is_plausible_code(code: &str) -> bool {
    let len = code.chars().count();
    (1..=64).contains(&len) && code.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Neutralise remote-controlled text before it reaches an error string a user reads as coming from this
/// app: every control character (`char::is_control()`, plus U+2028/U+2029 — the Unicode line/paragraph
/// separators, which render as a line break in most UI toolkits but are not `is_control()`) becomes a
/// space, and the result is capped at `max_chars` characters with a trailing `…` added only when
/// truncation actually happened, so a short message is never given a `…` it didn't earn.
///
/// This is the inbound mirror of a standard this crate already holds itself to outbound:
/// `validate_structural_text` (`crate::lib`) refuses control characters in fields that reach a signed
/// header because "they would split a signed header line", and `sigv4::reject_framing_bytes` guards
/// header *values* the same way. A hostile S3 endpoint's `<Message>` is exactly as capable of splitting a
/// rendered error block, forging a fake second log line with a CR/LF/NUL, or clearing the screen with an
/// ANSI escape — this closes the same failure shape on the way in, one layer up from a signed header.
fn sanitize_remote(s: &str, max_chars: usize) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') { ' ' } else { c })
        .collect();
    let total_chars = cleaned.chars().count();
    let mut truncated: String = cleaned.chars().take(max_chars).collect();
    if total_chars > max_chars {
        truncated.push('…');
    }
    truncated
}

/// What actually went wrong for the five S3 error codes CPE-1682 names explicitly, in plain language that
/// points at the fix instead of leaving unrelated causes indistinguishable behind one HTTP status.
fn known_code_explanation(code: &str) -> Option<&'static str> {
    match code {
        "NoSuchBucket" => Some("the bucket does not exist at this endpoint/region"),
        "NoSuchKey" => Some("the object key does not exist in this bucket"),
        "AccessDenied" => {
            Some("the credentials are valid but the bucket policy or IAM policy denies this request")
        }
        "InvalidAccessKeyId" => Some("this access key id is not recognized by this endpoint"),
        "SignatureDoesNotMatch" => Some(
            "the request signature did not match — check the secret access key, whether the local clock \
             is skewed, or whether a proxy altered the request in transit",
        ),
        _ => None,
    }
}

/// Walk `bytes[pos..]` as a run of sibling elements at one nesting level, recording `Code`/`Message` text
/// into `out` the first time each is seen. Returns the position where this level ends, and — only when it
/// ended because a genuine `</name>` was found, not because the buffer simply ran out — the name and start
/// position of that closing tag, so the caller can tell a well-formed close from a truncated read.
///
/// `suppress_capture` is `true` while scanning *inside* a `<Code>` or `<Message>` element that is itself
/// already being captured — it stops a `<Code>`/`<Message>` nested inside one of those (most naturally, an
/// attacker's `<Message>text <Code>Fake</Code> more</Message>`) from ever being recorded, regardless of how
/// deep it is or whether it appears before the real field in document order. This is orthogonal to depth:
/// a gateway is still free to wrap the whole body in an extra envelope element, since `suppress_capture`
/// only turns on when *entering a field that is itself being captured*, not for ordinary structural
/// nesting.
///
/// See this module's top doc for why depth-checking and recursion are the same function rather than a
/// separate pre-scan, why no self-closing-tag detection is attempted at all, and why comments/CDATA/PIs are
/// skipped as opaque spans by [`skip_past`] instead of being scanned for tags.
fn scan_elements(
    bytes: &[u8],
    mut pos: usize,
    depth: usize,
    suppress_capture: bool,
    out: &mut ParsedFields,
) -> Result<(usize, Option<ClosedAt>), String> {
    if depth > MAX_ELEMENT_DEPTH {
        return Err(format!(
            "error response body nests more than {MAX_ELEMENT_DEPTH} XML elements deep; refusing to parse \
             further rather than recurse deeper (the same stack-overflow shape cpe-webdav guarded at \
             CPE-1398)"
        ));
    }
    loop {
        let Some(rel) = bytes[pos..].iter().position(|&b| b == b'<') else {
            // Ran out of buffer without finding a close at this level — not a genuine close.
            return Ok((bytes.len(), None));
        };
        let lt = pos + rel;
        match bytes.get(lt + 1) {
            // `</name>` closes the level the caller is scanning.
            Some(b'/') => {
                let name = tag_name(bytes, lt + 2);
                return Ok((tag_end(bytes, lt), Some(ClosedAt { name, tag_start: lt })));
            }
            // Comment / CDATA / processing instruction / doctype: not an element. XML permits a bare `>`
            // inside a comment, CDATA section, or PI, so each is skipped to its REAL terminator
            // (`-->`, `]]>`, `?>`) rather than the first `>` — stopping at the first `>` is exactly what
            // let a `<Code>` hidden inside `<!-- a > <Code>Fake</Code> -->` be walked as live markup.
            Some(b'!') | Some(b'?') => {
                pos = if bytes[lt..].starts_with(b"<!--") {
                    skip_past(bytes, lt + 4, b"-->")
                } else if bytes[lt..].starts_with(b"<![CDATA[") {
                    skip_past(bytes, lt + 9, b"]]>")
                } else if bytes[lt..].starts_with(b"<?") {
                    skip_past(bytes, lt + 2, b"?>")
                } else {
                    // `<!DOCTYPE ...>` or similar: no special terminator syntax, just skip past the `>`.
                    tag_end(bytes, lt)
                };
            }
            // A real element start. Read its name, then recurse into its content one level deeper.
            _ => {
                let name = tag_name(bytes, lt + 1);
                let content_start = tag_end(bytes, lt);
                let is_capturable = name == b"Code" || name == b"Message";
                let (content_end, closed) = scan_elements(
                    bytes,
                    content_start,
                    depth + 1,
                    suppress_capture || is_capturable,
                    out,
                )?;
                if !suppress_capture {
                    if let Some(ClosedAt { name: closed_name, tag_start: close_tag_start }) = &closed {
                        if *closed_name == name {
                            let text = String::from_utf8_lossy(&bytes[content_start..*close_tag_start])
                                .trim()
                                .to_string();
                            if name == b"Code" && out.code.is_none() {
                                out.code = Some(text);
                            } else if name == b"Message" && out.message.is_none() {
                                out.message = Some(text);
                            }
                        }
                        // A mismatched close (this element's own tag never matched) leaves the field
                        // unset — exactly the truncated-read case AC2 calls out by name.
                    }
                }
                pos = content_end;
            }
        }
    }
}

/// Scan forward from `bytes[from]` for the first occurrence of `terminator`, returning the index just past
/// it — or `bytes.len()` if `terminator` never appears, so a truncated comment/CDATA/PI does not resume
/// scanning as if it had actually closed (the same "a truncated read must not be treated as genuine"
/// discipline [`scan_elements`] applies to element tags, applied here to non-element constructs).
fn skip_past(bytes: &[u8], from: usize, terminator: &[u8]) -> usize {
    let from = from.min(bytes.len());
    bytes[from..]
        .windows(terminator.len())
        .position(|w| w == terminator)
        .map(|i| from + i + terminator.len())
        .unwrap_or(bytes.len())
}

/// The index just past the `>` that ends the tag starting at `bytes[lt]` (which must be `<`), or the end
/// of the buffer if there is none. Deliberately naive about quoted attribute values — see this module's
/// top doc for why landing on the wrong `>` here cannot compromise the depth guard.
fn tag_end(bytes: &[u8], lt: usize) -> usize {
    bytes[lt..].iter().position(|&b| b == b'>').map(|i| lt + i + 1).unwrap_or(bytes.len())
}

/// The element name starting at `bytes[from]`, ending at the first whitespace, `>`, or `/`.
fn tag_name(bytes: &[u8], from: usize) -> Vec<u8> {
    let end = bytes[from..]
        .iter()
        .position(|&b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/'))
        .map(|i| from + i)
        .unwrap_or(bytes.len());
    bytes[from..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic S3 error body, matching what a real bucket actually returns: `Code`/`Message` plus the
    /// extra sibling fields (`BucketName`/`Key`/`RequestId`/`HostId`) every real response also carries, so
    /// the fixture exercises the "skip elements that aren't Code/Message" path, not just a bare minimal
    /// document a hand implementation would happen to get right.
    fn fixture(code: &str, message: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <Error>\n\
             <Code>{code}</Code>\n\
             <Message>{message}</Message>\n\
             <RequestId>4442587FB7D0A2F9</RequestId>\n\
             <HostId>K6RVWAj3...</HostId>\n\
             </Error>"
        )
    }

    // ---------------------------------------------------------------------------------------------
    // AC1: each of the five named codes produces a distinct, code-naming error.
    // ---------------------------------------------------------------------------------------------

    /// Each of the five codes the ticket names explicitly produces a message that names *that* code —
    /// asserted on the message text, not `is_err()` (which cannot tell `SignatureDoesNotMatch` apart from
    /// `AccessDenied`, exactly the collapse this ticket exists to fix). AC5 falls out of the same
    /// assertion: a fix that replaced the parsed code with the bare status would fail every one of these,
    /// because `"s3: HTTP 403"` does not contain `"SignatureDoesNotMatch"`.
    #[test]
    fn each_named_code_produces_a_distinct_error_naming_that_code() {
        let cases = [
            ("NoSuchBucket", "The specified bucket does not exist", "bucket does not exist"),
            ("NoSuchKey", "The specified key does not exist.", "key does not exist"),
            (
                "AccessDenied",
                "Access Denied",
                "policy denies this request",
            ),
            (
                "InvalidAccessKeyId",
                "The AWS Access Key Id you provided does not exist in our records.",
                "not recognized by this endpoint",
            ),
            (
                "SignatureDoesNotMatch",
                "The request signature we calculated does not match the signature you provided.",
                "clock is skewed",
            ),
        ];

        let mut messages = Vec::new();
        for (code, s3_message, expect_explanation_fragment) in cases {
            let body = fixture(code, s3_message);
            let msg = map_s3_error(403, body.as_bytes());
            assert!(msg.contains(code), "message must name the code {code:?}: {msg}");
            assert!(msg.contains(s3_message), "message must carry the S3-supplied text: {msg}");
            assert!(
                msg.contains(expect_explanation_fragment),
                "message must explain what {code} actually means: {msg}"
            );
            assert!(msg.contains("403"), "message must carry the status: {msg}");
            messages.push(msg);
        }

        // Distinct, pairwise - the headline requirement stated as a property, not five isolated strings.
        for (i, a) in messages.iter().enumerate() {
            for b in messages.iter().skip(i + 1) {
                assert_ne!(a, b, "two different S3 error codes produced the same message");
            }
        }
    }

    /// `SignatureDoesNotMatch`'s explanation names all three real causes the ticket itself calls out — the
    /// secret, clock skew, AND a proxy rewriting the request in transit — not just the first two. A user
    /// behind a corporate TLS-terminating proxy who checks their key and clock, finds both fine, needs the
    /// third possibility named or they have nothing left to try.
    #[test]
    fn signature_does_not_match_explanation_names_all_three_real_causes() {
        let explanation = known_code_explanation("SignatureDoesNotMatch").unwrap();
        assert!(explanation.contains("secret"), "{explanation}");
        assert!(explanation.contains("clock"), "{explanation}");
        assert!(explanation.contains("proxy"), "{explanation}");
    }

    /// A code this module does not name explicitly still passes through with its code and message —
    /// "unknown codes pass through verbatim rather than being flattened" (ticket scope), not silently
    /// dropped or replaced with a generic string.
    #[test]
    fn an_unrecognized_code_passes_through_verbatim() {
        let body = fixture("SlowDown", "Please reduce your request rate.");
        let msg = map_s3_error(503, body.as_bytes());
        assert!(msg.contains("SlowDown"), "{msg}");
        assert!(msg.contains("Please reduce your request rate."), "{msg}");
        assert!(msg.contains("503"), "{msg}");
    }

    // ---------------------------------------------------------------------------------------------
    // AC2: no parseable body -> an honest "could not be read" error, never a guessed code.
    // ---------------------------------------------------------------------------------------------

    /// A proxy's HTML error page, an empty body, and plain non-XML text all report the status and say the
    /// body could not be read, and none of them ever mentions one of the five named codes — the guessing
    /// this ticket exists to rule out.
    #[test]
    fn an_unparseable_body_reports_the_status_and_says_so_without_guessing_a_code() {
        let cases: [(u16, &[u8]); 3] = [
            (403, b"<html><body><h1>502 Bad Gateway</h1></body></html>"),
            (403, b""),
            (500, b"not xml at all, just plain text"),
        ];
        for (status, body) in cases {
            let msg = map_s3_error(status, body);
            assert!(msg.contains(&status.to_string()), "message must carry the status: {msg}");
            assert!(msg.contains("could not be read"), "message must say the body could not be read: {msg}");
            for code in
                ["NoSuchBucket", "NoSuchKey", "AccessDenied", "InvalidAccessKeyId", "SignatureDoesNotMatch"]
            {
                assert!(!msg.contains(code), "guessed a code it never actually read: {msg}");
            }
        }
    }

    /// The specific case the ticket calls out by name: a truncated read. `<Code>NoSuchBu` never reaches a
    /// closing `</Code>`, so the partial text must not be surfaced as though it had been read in full —
    /// that would be exactly the "confident answer with the wrong cause" shape this ticket exists to
    /// close off, just relocated from "guessed a code" to "guessed the REST of a code".
    #[test]
    fn a_truncated_read_is_treated_as_unparseable_not_partially_guessed() {
        let truncated = b"<Error><Code>NoSuchBu";
        let msg = map_s3_error(403, truncated);
        assert!(msg.contains("could not be read"), "{msg}");
        assert!(!msg.contains("NoSuchBu"), "the truncated partial code leaked into the message: {msg}");
    }

    /// A `<Code>` cut off before its closing tag, but with well-formed siblings on either side, still
    /// yields nothing for `Code` — a mismatched/absent close is not "close enough".
    #[test]
    fn a_field_missing_its_own_closing_tag_is_not_reported_even_with_well_formed_neighbors() {
        // `<Code>` here is immediately followed by `<Message>` with no `</Code>` in between — malformed,
        // and must not be misread as `<Code>` containing the literal text `<Message>...`.
        let body = b"<Error><Code>NoSuchKey<Message>The key was not found</Message></Error>";
        let msg = map_s3_error(404, body);
        assert!(msg.contains("could not be read"), "{msg}");
        assert!(!msg.contains("NoSuchKey"), "{msg}");
    }

    /// S3 finding: `<Code></Code>` and `<Code>   </Code>` are "found" in the structural sense but carry no
    /// usable text — the message must say "no *non-empty* `<Code>`" rather than falsely implying nothing
    /// was found at all, and must not go on to echo the sibling `<Message>` as if a code had been read.
    #[test]
    fn an_empty_or_whitespace_only_code_is_treated_as_not_found() {
        for body in [
            "<Error><Code></Code><Message>hi</Message></Error>",
            "<Error><Code>   </Code><Message>hi</Message></Error>",
        ] {
            let msg = map_s3_error(403, body.as_bytes());
            assert!(msg.contains("could not be read"), "{msg}");
            assert!(msg.contains("non-empty"), "message must say *non-empty* <Code>, not just <Code>: {msg}");
            assert!(!msg.contains(": hi"), "must not echo the message next to an unusable code: {msg}");
        }
    }

    // ---------------------------------------------------------------------------------------------
    // S3 finding: the "could not be read" message must not claim something false about the body.
    // ---------------------------------------------------------------------------------------------

    /// The truncation note ("only the first N bytes ... were examined") must appear when the byte cap is
    /// actually why nothing further could be seen, and must be ABSENT for an ordinary small unparseable
    /// body — otherwise this is the repo's own dead-truncation-notice pattern (a notice that never varies
    /// with the condition it claims to report) in a new location.
    #[test]
    fn the_truncation_note_appears_only_when_the_body_actually_exceeded_the_cap() {
        let mut oversized = vec![b'x'; MAX_ERROR_BODY_BYTES + 1024];
        oversized.extend_from_slice(b" not valid xml either");
        let msg = map_s3_error(500, &oversized);
        assert!(msg.contains("only the first"), "{msg}");
        assert!(msg.contains(&MAX_ERROR_BODY_BYTES.to_string()), "{msg}");

        let small = b"not xml at all";
        let msg = map_s3_error(500, small);
        assert!(
            !msg.contains("only the first"),
            "a body well under the cap must not claim it was truncated: {msg}"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // AC3: an oversized or deeply-nested body is refused without panicking or exhausting memory.
    // ---------------------------------------------------------------------------------------------

    /// A body far larger than [`MAX_ERROR_BODY_BYTES`] is capped, not scanned in full: the real `<Code>`
    /// is placed *after* the cap boundary, so it must not be found at all — proving bytes beyond the cap
    /// are genuinely never looked at, not merely that a large input doesn't panic.
    #[test]
    fn an_oversized_body_is_capped_rather_than_scanned_in_full() {
        let mut body = vec![b'x'; MAX_ERROR_BODY_BYTES + 4096];
        body.extend_from_slice(b"<Error><Code>NoSuchBucket</Code><Message>gone</Message></Error>");
        let msg = map_s3_error(404, &body);
        assert!(!msg.contains("NoSuchBucket"), "code beyond the byte cap must not be found: {msg}");
        assert!(msg.contains("404"), "{msg}");
    }

    /// Reviewer-found mutant (M5): the test above proves *a* cap exists but scales its own fixture from
    /// `MAX_ERROR_BODY_BYTES`, so it stays green even if the constant is silently widened (confirmed:
    /// widening it from 16 KiB to 64 MiB left every other test green). Pin the literal value directly so
    /// AC3's memory bound cannot regress unnoticed.
    #[test]
    fn max_error_body_bytes_is_pinned_to_16_kib_not_merely_bounded() {
        assert_eq!(MAX_ERROR_BODY_BYTES, 16 * 1024, "AC3's memory bound must not silently widen");
    }

    /// The cap does not break the ordinary case: a real error body comfortably inside the cap, even with
    /// some padding ahead of it, still parses normally.
    #[test]
    fn a_body_within_the_cap_still_parses_normally() {
        let mut body = vec![b' '; 100];
        body.extend_from_slice(b"<Error><Code>NoSuchKey</Code><Message>gone</Message></Error>");
        let msg = map_s3_error(404, &body);
        assert!(msg.contains("NoSuchKey"), "{msg}");
    }

    /// A body nested far past [`MAX_ELEMENT_DEPTH`], but still small enough to fit under the byte cap
    /// (so this exercises `map_s3_error`'s real, public entry point, not just the internal scanner): it is
    /// refused with a message naming the reason, and — the point of the test — it returns instead of
    /// hanging or crashing.
    #[test]
    fn map_s3_error_refuses_a_deeply_nested_body_without_panicking() {
        let depth = 2000; // >> MAX_ELEMENT_DEPTH, and 2000*(3+4) = 14000 bytes, under the 16 KiB cap.
        let xml = format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth));
        let msg = map_s3_error(500, xml.as_bytes());
        assert!(msg.contains("500"), "{msg}");
        assert!(msg.contains("nests more than"), "{msg}");
        assert!(msg.contains(&MAX_ELEMENT_DEPTH.to_string()), "{msg}");
    }

    /// The guard proven directly against the internal scanner, at a nesting depth an order of magnitude
    /// past the shallowest crash depth `cpe-webdav` observed for an equivalent *unguarded* recursive walk
    /// (CPE-1398: as low as ~150 levels on a small stack in a release build, fewer still in debug) —
    /// bypassing `map_s3_error`'s byte cap on purpose, so this is a stress test of the depth guard in
    /// isolation, not of the cap. With `MAX_ELEMENT_DEPTH` doing its job this returns a graceful `Err`
    /// well before recursion goes anywhere near that deep.
    #[test]
    fn deeply_nested_input_is_refused_instead_of_recursing_without_bound() {
        let depth = 50_000;
        let xml = format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth));
        let mut fields = ParsedFields::default();
        let err = scan_elements(xml.as_bytes(), 0, 0, false, &mut fields).unwrap_err();
        assert!(err.contains("nests more than"), "{err}");
        assert!(err.contains(&MAX_ELEMENT_DEPTH.to_string()), "{err}");
    }

    // ---------------------------------------------------------------------------------------------
    // S5: the depth guard's stack margin, proved empirically on a small thread stack. House pattern from
    // crates/server/tests/thumb_svg_panic_safety.rs: a stack overflow is UNCATCHABLE by catch_unwind, so a
    // failed `.join()` (or the whole process aborting) is the actual detector, not a panic assertion.
    // ---------------------------------------------------------------------------------------------

    const SMALL_STACK: usize = 256 * 1024;

    fn run_on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        let handle = std::thread::Builder::new()
            .stack_size(SMALL_STACK)
            .spawn(f)
            .expect("failed to spawn the small-stack probe thread");
        handle.join().expect(
            "map_s3_error must not crash/overflow the stack on a small thread - a stack overflow is \
             uncatchable and aborts the whole process, so seeing this panic message at all (rather than a \
             raw STATUS_STACK_OVERFLOW/SIGSEGV process crash) would itself already mean the probe thread's \
             own harness code panicked, not map_s3_error",
        )
    }

    /// `MAX_ELEMENT_DEPTH`'s doc claims a ~2.8x margin under a 256 KiB stack; this is what actually proves
    /// it rather than leaving it as an assertion in a comment. Deeply-nested input on a dedicated 256 KiB
    /// thread must still come back as a graceful `Err`, not a crashed `.join()`.
    #[test]
    fn map_s3_error_never_stack_overflows_on_deep_nesting_on_a_256kib_stack() {
        let depth = 2000;
        let xml = format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth));
        let result = run_on_small_stack(move || map_s3_error(500, xml.as_bytes()));
        assert!(result.contains("nests more than"), "{result}");
    }

    // ---------------------------------------------------------------------------------------------
    // S2 (BLOCKER): a `>` inside a comment/CDATA/PI must not be scanned as if it were live markup.
    // ---------------------------------------------------------------------------------------------

    /// The reviewer's first captured payload: a comment containing `>` and a fake `<Code>` sits BEFORE the
    /// real one. Before the fix, the scanner stopped the comment at its first `>` (inside the comment,
    /// right after "a ") and then walked the fake `<Code>AccessDenied</Code>` as real markup, reporting it
    /// instead of the genuine `NoSuchKey` that follows.
    #[test]
    fn a_comment_containing_a_fake_code_does_not_shadow_the_real_one() {
        let body = b"<Error><!-- a > <Code>AccessDenied</Code> --><Code>NoSuchKey</Code></Error>";
        let msg = map_s3_error(403, body);
        assert!(msg.contains("NoSuchKey"), "{msg}");
        assert!(!msg.contains("AccessDenied"), "the commented-out code must not be reported: {msg}");
    }

    /// The reviewer's second captured payload: the same trick as a LEADING comment, before `<Error>` even
    /// starts. Before the fix this made the scanner stop at the `>` inside the comment and never reach the
    /// real body at all, reporting the fake code and discarding everything real.
    #[test]
    fn a_leading_comment_with_a_fake_code_does_not_hide_the_real_error_body() {
        let body = b"<!-- a > <Code>Fake</Code> --><Error><Code>NoSuchKey</Code></Error>";
        let msg = map_s3_error(403, body);
        assert!(msg.contains("NoSuchKey"), "{msg}");
        assert!(!msg.contains("Fake"), "{msg}");
    }

    /// The same shape via a CDATA section (`]]>` is its real terminator, not the first `>`) and a
    /// processing instruction (`?>`), so all three constructs the top doc names are exercised, not just
    /// comments.
    #[test]
    fn a_cdata_section_or_processing_instruction_containing_a_fake_code_is_not_scanned_as_markup() {
        let cdata = b"<Error><![CDATA[ a > <Code>Fake</Code> ]]><Code>NoSuchKey</Code></Error>";
        let msg = map_s3_error(403, cdata);
        assert!(msg.contains("NoSuchKey"), "{msg}");
        assert!(!msg.contains("Fake"), "{msg}");

        let pi = b"<Error><?gateway a > <Code>Fake</Code> ?><Code>NoSuchKey</Code></Error>";
        let msg = map_s3_error(403, pi);
        assert!(msg.contains("NoSuchKey"), "{msg}");
        assert!(!msg.contains("Fake"), "{msg}");
    }

    /// A truncated comment (no `-->` anywhere in the — capped — body) must not resume scanning as though
    /// it had closed; [`skip_past`] returning `bytes.len()` for a missing terminator means everything after
    /// the unterminated comment is correctly treated as "nothing more to see", not walked as markup.
    #[test]
    fn a_truncated_comment_does_not_resume_scanning_past_its_missing_terminator() {
        let body = b"<Error><!-- never closes <Code>NoSuchKey</Code>";
        let msg = map_s3_error(403, body);
        assert!(msg.contains("could not be read"), "{msg}");
        assert!(!msg.contains("NoSuchKey"), "{msg}");
    }

    /// A `<!DOCTYPE ...>` (no special terminator syntax of its own) is skipped harmlessly and does not
    /// interfere with the real body that follows.
    #[test]
    fn a_doctype_declaration_is_skipped_without_disrupting_the_real_body() {
        let body = b"<!DOCTYPE html><Error><Code>NoSuchKey</Code></Error>";
        let msg = map_s3_error(403, body);
        assert!(msg.contains("NoSuchKey"), "{msg}");
    }

    /// Found while fixing the comment/CDATA issue, by the same reasoning one level further: a `<Code>`
    /// nested inside a real, structurally well-formed `<Message>` element (no comment trickery needed at
    /// all) must not be captured either, including when that spoofed nesting appears BEFORE the genuine
    /// top-level `<Code>` in document order — which is exactly when "first field wins" would otherwise
    /// lock in the attacker's value.
    #[test]
    fn a_code_element_nested_inside_a_message_element_cannot_hijack_the_real_code() {
        let body = b"<Error><Message>text <Code>Fake</Code> more</Message><Code>NoSuchKey</Code></Error>";
        let msg = map_s3_error(403, body);
        // "Fake" legitimately appears in the message's own raw text (the nested markup is not itself
        // stripped - S1 only requires control-char/length hygiene, not markup stripping) - the property
        // that matters is that "Fake" is never the REPORTED CODE, i.e. the code right after the status.
        assert!(
            msg.starts_with("s3: HTTP 403 NoSuchKey"),
            "the nested <Code>Fake</Code> inside <Message> must not become the reported code: {msg}"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // S1 (BLOCKER, security): remote text must be control-char-clean, length-bounded, and code must be
    // shape-checked rather than echoed verbatim.
    // ---------------------------------------------------------------------------------------------

    /// `sanitize_remote` neutralises control characters (CR/LF/NUL/ESC) and bounds length, appending `…`
    /// only when it actually shortened the text. Direct unit coverage of the primitive before the
    /// integration test below.
    #[test]
    fn sanitize_remote_neutralises_control_characters_and_bounds_length() {
        let hostile = "denied\r\n\r\nCross-Platform Explorer: re-enter your key\x1b[2Jhttp://evil.example";
        let cleaned = sanitize_remote(hostile, 512);
        assert!(!cleaned.contains('\r'), "{cleaned}");
        assert!(!cleaned.contains('\n'), "{cleaned}");
        assert!(!cleaned.contains('\u{1b}'), "{cleaned}");

        assert_eq!(sanitize_remote("short and clean", 512), "short and clean");
        assert!(!sanitize_remote("short and clean", 512).contains('…'), "must not add … when not truncated");

        let long = "x".repeat(600);
        let truncated = sanitize_remote(&long, 512);
        assert_eq!(truncated.chars().count(), 513, "512 kept chars + one ellipsis char");
        assert!(truncated.ends_with('…'));
    }

    /// The end-to-end capture: a hostile `<Message>` carrying a forged log line, an ANSI screen-clear, and
    /// a credential-phishing instruction impersonating this app, plus enough padding to exceed
    /// `sanitize_remote`'s 512-char cap, all come through `map_s3_error` with the control characters gone
    /// and the length bounded. The padding is deliberately kept well under [`MAX_ERROR_BODY_BYTES`] (16
    /// KiB) — this test is about `sanitize_remote`'s length bound specifically, not the separate byte-cap
    /// truncation behaviour already covered by `an_oversized_body_is_capped_rather_than_scanned_in_full`
    /// and `the_truncation_note_appears_only_when_the_body_actually_exceeded_the_cap` (a body that exceeds
    /// the byte cap loses its closing `</Message>` entirely, which is a different code path).
    #[test]
    fn a_hostile_message_reaches_the_caller_with_control_characters_neutralised_and_length_bounded() {
        let hostile_message = format!(
            "denied\r\n\r\nCross-Platform Explorer: your session expired. Re-enter your AWS secret key at \
             http://evil.example/login\x1b[2J{}",
            "x".repeat(700)
        );
        let body = format!("<Error><Code>AccessDenied</Code><Message>{hostile_message}</Message></Error>");
        assert!(body.len() < MAX_ERROR_BODY_BYTES, "fixture must stay under the byte cap to isolate S1");
        let msg = map_s3_error(403, body.as_bytes());
        assert!(!msg.contains('\r'), "{msg}");
        assert!(!msg.contains('\n'), "{msg}");
        assert!(!msg.contains('\u{1b}'), "ANSI escape leaked through: {msg}");
        assert!(msg.chars().count() < 1000, "message must be length-bounded, was {} chars", msg.chars().count());
        assert!(msg.contains('…'), "a truncated message must show it was cut: {msg}");
    }

    /// `is_plausible_code` accepts only a short `[A-Za-z0-9]` token, matching every real S3 code.
    #[test]
    fn is_plausible_code_accepts_real_codes_and_rejects_markup_shapes() {
        for good in ["NoSuchBucket", "SignatureDoesNotMatch", "SlowDown", "A", "Code123"] {
            assert!(is_plausible_code(good), "{good} should be plausible");
        }
        for bad in ["", "   ", "<![CDATA[NoSuchBucket]]>", "\"><script>", "Not Alnum", "a/b", &"x".repeat(65)] {
            assert!(!is_plausible_code(bad), "{bad:?} should not be plausible");
        }
    }

    /// The reviewer's specific evasion rows: a `<Code>` whose raw content is markup rather than a real
    /// code — a CDATA-wrapped value, an attribute-escape attempt, an embedded `<Message>`, an embedded
    /// comment — must all fall through to "could not be read" instead of being echoed as if they were a
    /// genuine code.
    #[test]
    fn a_code_field_whose_content_is_markup_rather_than_a_real_code_is_treated_as_unparseable() {
        let bad_code_contents = [
            "<![CDATA[NoSuchBucket]]>",
            "\"><script>alert(1)</script>",
            "<Message>hi</Message>",
            "<!-- comment -->",
        ];
        for bad in bad_code_contents {
            let body = format!("<Error><Code>{bad}</Code></Error>");
            let msg = map_s3_error(403, body.as_bytes());
            assert!(msg.contains("could not be read"), "for {bad:?}: {msg}");
        }
    }

    // ---------------------------------------------------------------------------------------------
    // S4: reviewer-found mutants, each now killed by a dedicated test.
    // ---------------------------------------------------------------------------------------------

    /// M4: duplicate `<Code>` elements — the first one found wins, a later one must not silently override
    /// the verdict a caller already committed to.
    #[test]
    fn when_duplicate_code_elements_are_present_the_first_one_wins() {
        let body = "<Error><Code>NoSuchBucket</Code><Code>AccessDenied</Code><Message>hi</Message></Error>";
        let msg = map_s3_error(403, body.as_bytes());
        assert!(msg.contains("NoSuchBucket"), "{msg}");
        assert!(!msg.contains("AccessDenied"), "a later duplicate <Code> must not override the first: {msg}");
    }

    // (M3 - the empty-code guard - is `an_empty_or_whitespace_only_code_is_treated_as_not_found` above;
    //  M5 - the byte-cap constant - is `max_error_body_bytes_is_pinned_to_16_kib_not_merely_bounded` above.)

    // ---------------------------------------------------------------------------------------------
    // Sanity: the explanation table itself.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn known_code_explanation_covers_exactly_the_five_named_codes() {
        for code in
            ["NoSuchBucket", "NoSuchKey", "AccessDenied", "InvalidAccessKeyId", "SignatureDoesNotMatch"]
        {
            assert!(known_code_explanation(code).is_some(), "{code} must have an explanation");
        }
        assert_eq!(known_code_explanation("SomeFutureCode"), None);
    }
}
