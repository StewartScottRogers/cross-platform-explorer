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
//! Two guards run before any text is read out of the body:
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

/// Upper bound on how many bytes of an error response body [`map_s3_error`] will look at. A real S3
/// `<Error>` document is a few hundred bytes; this is generous headroom above that while keeping a huge or
/// hostile body from costing more than a bounded, constant amount of memory and scan time to handle.
const MAX_ERROR_BODY_BYTES: usize = 16 * 1024;

/// Upper bound on how many levels of XML element nesting [`scan_elements`] will recurse into before
/// refusing to go deeper. A real S3 error body nests two levels deep (`Error` -> `Code`/`Message`); this
/// is wide headroom above that — enough for a gateway that wraps the body in an extra envelope element —
/// while still refusing a document nested far beyond anything real. See this module's top doc for why
/// going deeper is refused at all rather than merely made expensive.
const MAX_ELEMENT_DEPTH: usize = 32;

/// The two fields this module ever extracts from an S3 error body. `None` means "not found" — including
/// "found, but the closing tag never matched", which a truncated read produces (see [`scan_elements`]).
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
/// all rather than surfacing the partial text as if it had been read in full.
///
/// A code this module does not name explicitly (this crate talks to gateways beyond AWS proper, which
/// have their own extension codes) still comes through with its code and message, just without the
/// explanatory suffix [`known_code_explanation`] adds for the five named ones — "unknown codes pass
/// through verbatim rather than being flattened", per the ticket's scope.
pub fn map_s3_error(status: u16, body: &[u8]) -> String {
    let capped = &body[..body.len().min(MAX_ERROR_BODY_BYTES)];
    let mut fields = ParsedFields::default();
    if let Err(reason) = scan_elements(capped, 0, 0, &mut fields) {
        return format!("s3: HTTP {status} — {reason}");
    }
    match fields.code {
        Some(code) if !code.is_empty() => {
            let message = fields.message.unwrap_or_default();
            let explanation = known_code_explanation(&code).map(|e| format!(" — {e}")).unwrap_or_default();
            if message.is_empty() {
                format!("s3: HTTP {status} {code}{explanation}")
            } else {
                format!("s3: HTTP {status} {code}: {message}{explanation}")
            }
        }
        _ => format!(
            "s3: HTTP {status} and the response body could not be read as an S3 error (no <Code> element \
             was found in it); refusing to guess which cause applies"
        ),
    }
}

/// What actually went wrong for the five S3 error codes CPE-1682 names explicitly, in plain language that
/// points at the fix instead of leaving three unrelated causes indistinguishable behind one HTTP status.
fn known_code_explanation(code: &str) -> Option<&'static str> {
    match code {
        "NoSuchBucket" => Some("the bucket does not exist at this endpoint/region"),
        "NoSuchKey" => Some("the object key does not exist in this bucket"),
        "AccessDenied" => {
            Some("the credentials are valid but the bucket policy or IAM policy denies this request")
        }
        "InvalidAccessKeyId" => Some("this access key id is not recognized by this endpoint"),
        "SignatureDoesNotMatch" => Some(
            "the request signature did not match — check the secret access key and whether the local \
             clock is skewed",
        ),
        _ => None,
    }
}

/// Walk `bytes[pos..]` as a run of sibling elements at one nesting level, recording `Code`/`Message` text
/// into `out` the first time each is seen. Returns the position where this level ends, and — only when it
/// ended because a genuine `</name>` was found, not because the buffer simply ran out — the name and start
/// position of that closing tag, so the caller can tell a well-formed close from a truncated read.
///
/// See this module's top doc for why depth-checking and recursion are the same function rather than a
/// separate pre-scan, and why no self-closing-tag detection is attempted at all.
fn scan_elements(
    bytes: &[u8],
    mut pos: usize,
    depth: usize,
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
            // `<!...>` (comment/CDATA/doctype) or `<?...?>` (processing instruction): not an element,
            // skip its syntax and keep scanning this level.
            Some(b'!') | Some(b'?') => {
                pos = tag_end(bytes, lt);
            }
            // A real element start. Read its name, then recurse into its content one level deeper.
            _ => {
                let name = tag_name(bytes, lt + 1);
                let content_start = tag_end(bytes, lt);
                let (content_end, closed) = scan_elements(bytes, content_start, depth + 1, out)?;
                if let Some(ClosedAt { name: closed_name, tag_start: close_tag_start }) = &closed {
                    if *closed_name == name {
                        let text =
                            String::from_utf8_lossy(&bytes[content_start..*close_tag_start]).trim().to_string();
                        if name == b"Code" && out.code.is_none() {
                            out.code = Some(text);
                        } else if name == b"Message" && out.message.is_none() {
                            out.message = Some(text);
                        }
                    }
                    // A mismatched close (this element's own tag never matched) leaves the field
                    // unset — exactly the truncated-read case AC2 calls out by name.
                }
                pos = content_end;
            }
        }
    }
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
    /// well before recursion goes anywhere near that deep. See the PR body for the neutralisation probe
    /// that removed the depth check here and observed the real stack overflow.
    #[test]
    fn deeply_nested_input_is_refused_instead_of_recursing_without_bound() {
        let depth = 50_000;
        let xml = format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth));
        let mut fields = ParsedFields::default();
        let err = scan_elements(xml.as_bytes(), 0, 0, &mut fields).unwrap_err();
        assert!(err.contains("nests more than"), "{err}");
        assert!(err.contains(&MAX_ELEMENT_DEPTH.to_string()), "{err}");
    }

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
