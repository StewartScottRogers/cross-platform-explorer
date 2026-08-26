//! Text-encoding + line-ending detection (CPE-1003, epic CPE-1002 "File inspection & safety
//! utilities"). A pure byte/str sniffer — no filesystem I/O, no new dependencies — that guesses a
//! text file's character encoding from a leading byte-order mark or UTF-8 validity, and separately
//! reports which line-ending convention(s) a decoded string uses. The caller supplies the bytes (or
//! the already-decoded string for line endings); this module never reads a file itself.

/// A guessed text encoding for an in-memory byte slice.
///
/// This is a *guess*, not a certainty: outside of a BOM, byte-level sniffing cannot reliably
/// distinguish every 8-bit encoding, and — see [`detect_encoding`]'s docs — UTF-16 without a BOM is
/// not detected at all (documented limitation, not a bug).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingGuess {
    /// Zero-byte input — nothing to guess.
    Empty,
    /// Valid UTF-8 with no byte-order mark.
    Utf8,
    /// UTF-8 with a leading `EF BB BF` byte-order mark.
    Utf8Bom,
    /// UTF-16, little-endian, identified by a leading `FF FE` byte-order mark.
    Utf16Le,
    /// UTF-16, big-endian, identified by a leading `FE FF` byte-order mark.
    Utf16Be,
    /// Not valid UTF-8 and not flagged as binary — probably an 8-bit legacy encoding (Latin-1 /
    /// Windows-1252 / similar). A guess of last resort, not a positive identification.
    Latin1,
    /// Looks like non-text data (a NUL byte, or a high proportion of control bytes) rather than a
    /// legacy text encoding.
    Binary,
}

impl EncodingGuess {
    /// A short, human-readable label for display.
    pub fn label(self) -> &'static str {
        match self {
            EncodingGuess::Empty => "Empty file",
            EncodingGuess::Utf8 => "UTF-8",
            EncodingGuess::Utf8Bom => "UTF-8 (with BOM)",
            EncodingGuess::Utf16Le => "UTF-16 LE (with BOM)",
            EncodingGuess::Utf16Be => "UTF-16 BE (with BOM)",
            EncodingGuess::Latin1 => "Latin-1 / 8-bit (guessed)",
            EncodingGuess::Binary => "Binary",
        }
    }
}

/// How many leading bytes [`detect_encoding`]'s binary heuristic samples when deciding whether
/// non-UTF-8 bytes look like binary data. Capped so the check stays O(1)-ish on large input; a NUL
/// byte anywhere in `bytes` (not just this prefix) is checked separately and always wins.
const BINARY_SNIFF_LEN: usize = 512;

/// Above this fraction of sniffed bytes being non-text control bytes, the input is judged
/// [`EncodingGuess::Binary`] rather than an 8-bit legacy text encoding.
const BINARY_CONTROL_RATIO: f64 = 0.3;

/// Above this fraction of the BOM-less UTF-16 NUL-lane heuristic's *minority* lane being NUL, the input
/// stops looking like a clean UTF-16 lane pattern — see [`classify_nul_bytes`]'s doc comment for why a
/// strict "exactly zero" requirement here (the pre-CPE-1636-followups behavior) misfires on any UTF-16
/// content that isn't pure ASCII, and for the measurements behind this specific number: real mixed-ASCII
/// UTF-16 log text (English log lines with an occasional emoji/accented name/CJK username) puts at most
/// ~1-2% of the minority lane's positions at NUL — an emoji's low surrogate is the only realistic case
/// that pollutes the "wrong" lane at all, since accented Latin-1 and BMP CJK/RTL characters all land with
/// a zero *high* byte, i.e. on the SAME lane as ASCII, contributing no minority-lane noise. Real binary
/// data that clears the *majority*-lane bar at all (the existing `nul * 2 >= lane` check, unchanged by
/// this constant) pollutes both lanes roughly together rather than concentrating on one — the worst case
/// found scanning real files (a Windows PE executable's zero-padded header) had a ~46% minority lane
/// alongside its ~53% majority lane.
///
/// **Limitation (CPE-1656, softened from an earlier "wide margin both ways" claim that oversold this
/// single number's confidence):** this ratio alone cannot separate genuine UTF-16 ASCII text from a
/// small-`u16` sequential/incrementing index table (font glyph/loca tables, resource or index
/// directories) — measured at ~0.4% minority-lane NUL for a representative synthetic table, which sits
/// *below* the ~1-2% real-text noise floor above, not near the 25% bar at all. No threshold on this ratio
/// could reject that shape without also rejecting genuine near-pure-ASCII UTF-16 text (which can measure
/// exactly as low). [`classify_nul_bytes`] therefore no longer relies on this ratio alone — it also
/// requires the content lane to look like actual printable text ([`UTF16_CONTENT_LANE_PRINTABLE_RATIO`]),
/// a signal an index table's roughly-uniform byte distribution fails but real text passes easily.
const UTF16_MINORITY_NUL_RATIO: f64 = 0.25;

/// Above this fraction of the UTF-16 NUL-lane heuristic's *content* lane (the minority-NUL lane — the one
/// carrying the actual low/ASCII-range byte of each code unit) needing to look like ordinary printable
/// text, for a NUL-lane match to be trusted as genuine UTF-16 (CPE-1656's second signal — see
/// [`classify_nul_bytes`] and [`UTF16_MINORITY_NUL_RATIO`]'s doc comment for why the NUL ratio alone
/// cannot make this call). A small-uint16 index/glyph table's content-lane byte cycles through
/// essentially the full 0-255 range as its values increase, so at most ~37% of it (95 of 256 byte values)
/// can even land in the printable-ASCII band by chance — measured ~35-39% on synthetic sequential and
/// near-sequential index tables. Real UTF-16 log text (English prose, source paths, JSON-ish structured
/// fields) measured >90% printable on every real UTF-16 log excerpt used to verify this (a real
/// `MicrosoftEdgeUpdate.log` windowed read, plus the existing emoji/accented/CJK/RTL fixtures below). 60%
/// sits with wide margin below that real-text floor and well above the index-table ceiling.
const UTF16_CONTENT_LANE_PRINTABLE_RATIO: f64 = 0.60;

/// True for a byte that reads as non-text control data: the C0 control range excluding tab/LF/CR
/// (the three control characters that legitimately appear in text), plus DEL (`0x7F`).
fn is_binary_control_byte(b: u8) -> bool {
    (b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r') || b == 0x7F
}

/// True for a byte that looks like ordinary printable text when read as a UTF-16 code unit's
/// low/content byte in isolation: printable ASCII (`0x20`-`0x7E`) or common text whitespace
/// (tab/LF/CR). Used by [`classify_nul_bytes`]'s content-lane plausibility check
/// ([`UTF16_CONTENT_LANE_PRINTABLE_RATIO`], CPE-1656) — deliberately narrow (printable ASCII only, not
/// the Latin-1 supplement `0xA0`-`0xFF`): widening it to include that range would let a byte-uniform
/// index table's content lane score ~75% "printable" purely from covering the fuller range, defeating
/// the whole point of the check (measured on a synthetic sequential-index fixture during this ticket).
fn looks_like_printable_text_byte(b: u8) -> bool {
    (0x20..=0x7E).contains(&b) || b == b'\t' || b == b'\n' || b == b'\r'
}

/// Heuristic binary check for bytes that are *not* valid UTF-8 (this is only ever consulted after
/// that check has already failed — see [`detect_encoding`]).
///
/// True if `bytes` contains a NUL byte anywhere, or if more than [`BINARY_CONTROL_RATIO`] of a
/// leading [`BINARY_SNIFF_LEN`]-byte prefix are non-text control bytes ([`is_binary_control_byte`]).
/// Bounds-safe on any length, including empty (returns `false`).
fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return true;
    }
    let sniff_len = bytes.len().min(BINARY_SNIFF_LEN);
    if sniff_len == 0 {
        return false;
    }
    let control_count = bytes[..sniff_len].iter().filter(|&&b| is_binary_control_byte(b)).count();
    (control_count as f64 / sniff_len as f64) > BINARY_CONTROL_RATIO
}

/// Guess the text encoding of `bytes`. Bounds-safe, never panics, regardless of length.
///
/// Order of checks:
/// 1. Empty input → [`Empty`](EncodingGuess::Empty).
/// 2. A byte-order mark → [`Utf8Bom`](EncodingGuess::Utf8Bom) (`EF BB BF`),
///    [`Utf16Le`](EncodingGuess::Utf16Le) (`FF FE`), or [`Utf16Be`](EncodingGuess::Utf16Be)
///    (`FE FF`).
/// 3. A NUL byte (no BOM) → not plain text: BOM-less UTF-16 or [`Binary`](EncodingGuess::Binary). Runs
///    **before** the UTF-8 check because a NUL is itself valid UTF-8, so BOM-less UTF-16 ASCII (`h\0i\0`)
///    would otherwise be mis-reported as plain [`Utf8`](EncodingGuess::Utf8). If the NULs sit cleanly in
///    one byte-lane it is [`Utf16Le`](EncodingGuess::Utf16Le) (odd offsets) / [`Utf16Be`] (even offsets);
///    otherwise [`Binary`](EncodingGuess::Binary). See [`classify_nul_bytes`].
/// 4. Valid UTF-8 (`std::str::from_utf8`) → [`Utf8`](EncodingGuess::Utf8).
/// 5. Otherwise, [`looks_binary`] (a high ratio of control bytes) → [`Binary`](EncodingGuess::Binary).
/// 6. Otherwise → [`Latin1`](EncodingGuess::Latin1), a guess of last resort for the remaining
///    not-valid-UTF-8, not-binary-looking bytes.
///
/// **Limitation:** BOM-less UTF-16 detection is heuristic (NUL-lane pattern over a sniffed prefix); a
/// UTF-16 file dominated by non-ASCII (few NUL high-bytes) may fall through to `Binary`. UTF-32 is not
/// distinguished (a `FF FE 00 00` BOM reads as UTF-16LE).
pub fn detect_encoding(bytes: &[u8]) -> EncodingGuess {
    detect_encoding_at(bytes, 0)
}

/// Same as [`detect_encoding`], but for a slice that is not itself the true start of the file —
/// `base_offset` is the slice's absolute byte offset within that file (CPE-1644 A″).
///
/// The BOM-less NUL-lane heuristic ([`classify_nul_bytes`]) has to know which byte-lane (even/odd)
/// carries the NUL bytes to tell UTF-16LE from UTF-16BE apart, and that lane is a property of the
/// **absolute** file offset, not the slice's own index 0 — a window starting at an odd absolute byte
/// splits every UTF-16 code unit in it, flipping which lane looks NUL-heavy. `detect_encoding` (offset
/// 0) is unaffected by this; only a windowed reader (`log_window.rs`) sniffing a slice that starts
/// mid-file needs this variant.
pub fn detect_encoding_at(bytes: &[u8], base_offset: u64) -> EncodingGuess {
    if bytes.is_empty() {
        return EncodingGuess::Empty;
    }
    // A BOM is only ever meaningful at the true start of the file — a `FF FE` two bytes into a UTF-16
    // stream is just two ordinary code units, not a byte-order mark.
    if base_offset == 0 {
        if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
            return EncodingGuess::Utf8Bom;
        }
        if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
            return EncodingGuess::Utf16Le;
        }
        if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
            return EncodingGuess::Utf16Be;
        }
    }
    // A NUL byte means this isn't plain UTF-8/Latin-1 *text* (neither ever contains NUL); classify it as
    // BOM-less UTF-16 or binary BEFORE the UTF-8 check — a NUL is itself valid UTF-8, so a BOM-less
    // UTF-16 ASCII file like `h\0i\0` would otherwise round-trip through `from_utf8` and be mis-reported
    // as plain text (CPE-1003 QA finding).
    if bytes.contains(&0) {
        return classify_nul_bytes(bytes, base_offset);
    }
    if std::str::from_utf8(bytes).is_ok() {
        return EncodingGuess::Utf8;
    }
    if looks_binary(bytes) {
        return EncodingGuess::Binary;
    }
    EncodingGuess::Latin1
}

/// Classify NUL-containing, BOM-less input (called only from [`detect_encoding_at`]). BOM-less UTF-16 of
/// ASCII-range text fills one whole byte-lane with NULs — the lane at an *odd absolute file offset* for
/// little-endian, *even absolute file offset* for big-endian — so a lane that is at least half NUL while
/// the OTHER ("minority") lane stays under [`UTF16_MINORITY_NUL_RATIO`] is reported as UTF-16; anything
/// else (NULs spread across both lanes, or just a stray NUL) is [`Binary`](EncodingGuess::Binary).
/// `base_offset` is `bytes`'s own absolute offset in the file (CPE-1644 A″) — parity is computed as
/// `(base_offset + i) % 2`, not `i % 2`, so a window that starts mid-file at an odd byte doesn't flip
/// LE/BE relative to a window starting at byte 0 of the same stream.
///
/// **Why the minority lane tolerates a nonzero ratio, not just exactly zero (F3, CPE-1636 followups):**
/// real UTF-16 log content that isn't pure ASCII puts an occasional NUL on the "wrong" lane — most
/// visibly, a non-BMP character's low surrogate can have a zero low byte (e.g. U+1F600 😀 → LE code units
/// `3D D8` `00 DE`; that `00` sits on the lane that's otherwise all-non-NUL ASCII bytes). A strict
/// exactly-zero requirement flipped the *entire window* to [`Binary`](EncodingGuess::Binary) the moment
/// one such character appeared, which is worse than the state this heuristic exists to detect: CPE-1644's
/// whole point is decoding UTF-16 instead of refusing it, and a window wrongly called `Binary` falls
/// through to the plain UTF-8 path and fails outright rather than getting even an honest refusal. See
/// [`UTF16_MINORITY_NUL_RATIO`]'s doc comment for the measurements behind the specific threshold.
///
/// **Second signal: content-lane printability (CPE-1656).** The NUL-lane ratio alone cannot separate
/// genuine UTF-16 ASCII text from a small-`u16` sequential/incrementing index table (font glyph/loca
/// tables, resource or index directories) — both produce the same "one lane ~0% NUL, the other ~100%"
/// shape, because a small index value's high byte is usually zero too, same as ASCII. What differs is the
/// CONTENT of the non-NUL ("content") lane: real text's bytes are overwhelmingly printable ASCII; an
/// index table's content byte cycles through close to the full 0-255 range as its values increase, so at
/// most ~37% of it can even land in the printable range by chance. Once the NUL-lane shape matches, this
/// function ALSO requires the content lane to clear [`UTF16_CONTENT_LANE_PRINTABLE_RATIO`] before
/// trusting the UTF-16 call — seeing constants for the measurements behind both thresholds.
fn classify_nul_bytes(bytes: &[u8], base_offset: u64) -> EncodingGuess {
    let sniff_len = bytes.len().min(BINARY_SNIFF_LEN);
    if sniff_len < 2 {
        return EncodingGuess::Binary;
    }
    let lane = (sniff_len / 2).max(1); // approx positions per byte-lane in the sniffed prefix
    let mut even_nul = 0usize;
    let mut odd_nul = 0usize;
    let mut even_printable = 0usize;
    let mut odd_printable = 0usize;
    let mut even_count = 0usize;
    let mut odd_count = 0usize;
    for (i, &b) in bytes[..sniff_len].iter().enumerate() {
        if (base_offset + i as u64).is_multiple_of(2) {
            even_count += 1;
            if b == 0 {
                even_nul += 1;
            } else if looks_like_printable_text_byte(b) {
                even_printable += 1;
            }
        } else {
            odd_count += 1;
            if b == 0 {
                odd_nul += 1;
            } else if looks_like_printable_text_byte(b) {
                odd_printable += 1;
            }
        }
    }
    let minority_tolerance = (lane as f64 * UTF16_MINORITY_NUL_RATIO) as usize;
    // For LE the content (minority-NUL) lane is EVEN (the low/ASCII byte of each code unit); for BE it's
    // ODD. Guard the division: an empty lane (possible only for a 2-3 byte sniff) has no content to judge,
    // so it can't clear the printability bar either.
    let even_printable_ratio = if even_count > 0 { even_printable as f64 / even_count as f64 } else { 0.0 };
    let odd_printable_ratio = if odd_count > 0 { odd_printable as f64 / odd_count as f64 } else { 0.0 };

    if odd_nul * 2 >= lane && even_nul <= minority_tolerance && even_printable_ratio >= UTF16_CONTENT_LANE_PRINTABLE_RATIO {
        EncodingGuess::Utf16Le
    } else if even_nul * 2 >= lane && odd_nul <= minority_tolerance && odd_printable_ratio >= UTF16_CONTENT_LANE_PRINTABLE_RATIO {
        EncodingGuess::Utf16Be
    } else {
        EncodingGuess::Binary
    }
}

/// A line-ending convention.
///
/// `Mixed` is part of this type for callers that want a single at-a-glance value covering both
/// "which convention" and "was it consistent" in one spot, but [`detect_line_endings`] itself never
/// returns it as `dominant` — `dominant` always names the most-common *individual* convention (see
/// its docs); consult [`LineEndingReport::mixed`] separately for consistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
    Cr,
    /// No line breaks at all (including empty input).
    None,
    Mixed,
}

/// Line-ending counts for a decoded text string, plus a summary of which convention dominates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineEndingReport {
    /// Count of `\r\n` pairs.
    pub crlf: usize,
    /// Count of lone `\n` (not immediately preceded by `\r`).
    pub lf: usize,
    /// Count of lone `\r` (not immediately followed by `\n`).
    pub cr: usize,
    /// True when more than one of `crlf`/`lf`/`cr` is non-zero — the file is inconsistent.
    pub mixed: bool,
    /// The most common individual convention. Ties are broken deterministically in the fixed
    /// priority order `Crlf` > `Lf` > `Cr` (arbitrary but stable — good enough for display; it is
    /// not intended to imply one convention is "more correct"). `None` when there are no line breaks
    /// at all.
    pub dominant: LineEnding,
}

/// Scan `text` and count each line-ending convention. Never panics on any input, including empty.
///
/// A `\r` immediately followed by `\n` counts once as `crlf` (the pair is consumed together, so it
/// is never also double-counted as a lone `cr`). Any other `\r` or `\n` counts as a lone `cr`/`lf`
/// respectively.
pub fn detect_line_endings(text: &str) -> LineEndingReport {
    let bytes = text.as_bytes();
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut cr = 0usize;

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if i + 1 < bytes.len() && bytes[i + 1] == b'\n' => {
                crlf += 1;
                i += 2;
                continue;
            }
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
        i += 1;
    }

    let nonzero_kinds = [crlf, lf, cr].iter().filter(|&&n| n > 0).count();
    let mixed = nonzero_kinds > 1;
    let dominant = if crlf == 0 && lf == 0 && cr == 0 {
        LineEnding::None
    } else if crlf >= lf && crlf >= cr {
        LineEnding::Crlf
    } else if lf >= cr {
        LineEnding::Lf
    } else {
        LineEnding::Cr
    };

    LineEndingReport { crlf, lf, cr, mixed, dominant }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- detect_encoding ----

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(detect_encoding(&[]), EncodingGuess::Empty);
    }

    #[test]
    fn plain_ascii_is_utf8() {
        assert_eq!(detect_encoding(b"hello world"), EncodingGuess::Utf8);
    }

    #[test]
    fn non_ascii_utf8_is_utf8() {
        assert_eq!(detect_encoding("café".as_bytes()), EncodingGuess::Utf8);
    }

    #[test]
    fn utf8_bom_is_detected() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"hello");
        assert_eq!(detect_encoding(&bytes), EncodingGuess::Utf8Bom);
    }

    #[test]
    fn utf16_le_bom_is_detected() {
        // "hi" as UTF-16LE code units after the BOM: 68 00 69 00.
        let bytes = [0xFF, 0xFE, b'h', 0x00, b'i', 0x00];
        assert_eq!(detect_encoding(&bytes), EncodingGuess::Utf16Le);
    }

    #[test]
    fn utf16_be_bom_is_detected() {
        // "hi" as UTF-16BE code units after the BOM: 00 68 00 69.
        let bytes = [0xFE, 0xFF, 0x00, b'h', 0x00, b'i'];
        assert_eq!(detect_encoding(&bytes), EncodingGuess::Utf16Be);
    }

    #[test]
    fn nul_byte_amid_invalid_utf8_is_binary() {
        // A NUL byte plus several other control/non-text bytes, and a 0xFF byte that is never a
        // valid UTF-8 lead byte — so this fails the UTF-8 check and lands on the binary heuristic.
        let bytes: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0x04, 0x05, 0xFE, b'a', b'b', b'c', b'd'];
        assert_eq!(detect_encoding(&bytes), EncodingGuess::Binary);
    }

    #[test]
    fn bom_less_utf16_and_embedded_nul_are_not_utf8() {
        // QA-finding regression: a NUL is valid UTF-8, so these used to mis-report as `Utf8`. The NUL
        // check now runs before the UTF-8 check.
        // BOM-less UTF-16LE ASCII "hi" — NUL high-bytes on odd offsets → Utf16Le.
        assert_eq!(detect_encoding(&[0x68, 0x00, 0x69, 0x00]), EncodingGuess::Utf16Le);
        // BOM-less UTF-16BE ASCII "hi" — NUL high-bytes on even offsets → Utf16Be.
        assert_eq!(detect_encoding(&[0x00, 0x68, 0x00, 0x69]), EncodingGuess::Utf16Be);
        // A longer BOM-less UTF-16LE run stays Utf16Le.
        let utf16le: Vec<u8> = "hello".bytes().flat_map(|b| [b, 0]).collect();
        assert_eq!(detect_encoding(&utf16le), EncodingGuess::Utf16Le);
        // The key point: none of these are reported as plain UTF-8 text anymore.
        assert_ne!(detect_encoding(&[0x68, 0x00, 0x69, 0x00]), EncodingGuess::Utf8);
        // A stray single embedded NUL in otherwise-ASCII text isn't a clean UTF-16 lane → Binary, not Utf8.
        assert_eq!(detect_encoding(b"hello\0world"), EncodingGuess::Binary);
    }

    // ---- F3/F4 (CPE-1636 followups PR #842 review): realistic non-ASCII BOM-less UTF-16 byte fixtures,
    // plus real-binary-file regression coverage for the widened minority-lane tolerance. Every UTF-16
    // fixture in the original PR was pure ASCII, which never exercised `UTF16_MINORITY_NUL_RATIO` at all
    // — these assert on bytes produced independently via `.encode_utf16()`, not on this module's own
    // decoder, matching this crate's "assert on bytes, don't read your own encoder" convention.

    #[test]
    fn bom_less_utf16le_non_bmp_surrogate_pair_mixed_with_ascii_is_detected_as_utf16_not_binary() {
        // Regression (F3): an emoji's low surrogate has a zero LOW byte (U+1F600 😀 → LE code units
        // `3D D8` `00 DE`), which lands a NUL on the lane that's otherwise all non-NUL ASCII — the exact
        // pollution the old exactly-zero minority check couldn't tolerate.
        let text = "user reacted with \u{1F600} to the message";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(detect_encoding(&utf16le), EncodingGuess::Utf16Le);
    }

    #[test]
    fn bom_less_utf16le_accented_latin_mixed_with_ascii_is_detected_as_utf16() {
        // Accented Latin-1-range characters (é, à, …) have a zero HIGH byte in LE, i.e. they land on the
        // SAME lane as ASCII's NULs — this already worked before F3, committed here as permanent coverage
        // since the PR's fixtures never included one.
        let text = "utilisateur a r\u{e9}agi avec \u{e9}motion \u{e0} la commande caf\u{e9}";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(detect_encoding(&utf16le), EncodingGuess::Utf16Le);
    }

    #[test]
    fn bom_less_utf16le_cjk_mixed_with_ascii_is_detected_as_utf16() {
        // CJK BMP characters have both bytes non-zero — they contribute to neither lane's NUL count, so a
        // CJK username embedded in an otherwise-ASCII line doesn't dilute the ASCII lane's dominance.
        let text = "user \u{7528}\u{6237}\u{540d} \u{5f20}\u{4f1f} logged in";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(detect_encoding(&utf16le), EncodingGuess::Utf16Le);
    }

    #[test]
    fn bom_less_utf16be_rtl_mixed_with_ascii_is_detected_as_utf16() {
        // F4 coverage gap: every UTF-16 fixture in the original PR was little-endian. A BOM-less
        // big-endian file mixed with RTL (Arabic) content.
        let text = "user \u{62a}\u{645} \u{62a}\u{633}\u{62c}\u{64a}\u{644} logged in";
        let utf16be: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        assert_eq!(detect_encoding(&utf16be), EncodingGuess::Utf16Be);
    }

    // ---- CPE-1656 A: content-lane printability gate on top of the NUL-lane ratio ----
    //
    // This gate is monotonic: it only ever ADDS a requirement to the Utf16Le/Utf16Be branches, so any
    // input that already classified as Binary before this change (every real binary format the CPE-1636
    // followups reviewer measured — PE, ELF, SQLite, WAV, BMP, TIFF, ICO, protobuf, all confirmed staying
    // Binary with wide NUL-ratio margin) is logically guaranteed to still classify as Binary now — there is
    // no path for a stricter UTF-16 condition to turn a rejection into an acceptance. The PE-header and PNG
    // fixtures already committed above (which exercise the *closest* real-world near-misses on the NUL-ratio
    // side) are re-asserted unchanged; these two new tests cover the NEW signal specifically.

    #[test]
    fn small_uint16_sequential_index_table_is_not_misread_as_utf16le() {
        // A synthetic small-u16 sequential index/glyph table — the shape of a font glyph/loca table or a
        // resource/index directory (CPE-1656). No real file format measured during the CPE-1636 followups
        // review triggered this; it's a synthetic-but-plausible shape the NUL-lane ratio ALONE (even
        // widened to UTF16_MINORITY_NUL_RATIO) cannot tell apart from genuine UTF-16 ASCII text, because
        // both produce the identical "one lane ~0% NUL, the other ~100%" pattern.
        let bytes: Vec<u8> = (0u16..256).flat_map(|v| v.to_le_bytes()).collect();
        // Fixture sanity: confirms this really does clear the pure NUL-lane bar the way the ticket
        // describes (measured ~0.4% minority-lane NUL there; this construction lands at ~0.39%).
        let content_lane_nul = bytes.iter().step_by(2).filter(|&&b| b == 0).count();
        assert!(
            (content_lane_nul as f64 / 256.0) < 0.01,
            "fixture sanity: content lane should be under 1% NUL, matching the ticket's ~0.4% measurement"
        );
        // Pre-fix (NUL-ratio alone) this reported Utf16Le — the exact bug CPE-1656 describes. Post-fix,
        // the content lane's printable ratio (~38%, see UTF16_CONTENT_LANE_PRINTABLE_RATIO's doc comment)
        // fails the new gate, so it falls through to Binary — an honest "not text", not a silent garbage
        // decode.
        assert_eq!(detect_encoding(&bytes), EncodingGuess::Binary);
    }

    #[test]
    fn real_edge_update_log_windowed_utf16_bytes_still_decode() {
        // CPE-1656 regression guard, real input: a genuine 120-byte excerpt captured mid-file (absolute
        // offset 2000, well past the file's own BOM) from a real `MicrosoftEdgeUpdate.log` on this
        // machine — decodes as UTF-16LE text `crosoftEdgeUpdate.exe" /ua /installsource scheduler]\r\n
        // [07/28`. Exercises the exact BOM-less windowed-read path (`detect_encoding_at` with a nonzero
        // `base_offset`) that `log_window.rs` uses for a page that doesn't start at byte 0 — this is
        // precisely the "genuine UTF-16 log" case the new content-lane printability gate must not
        // regress, alongside the emoji/accented/CJK/RTL fixtures above.
        let bytes: Vec<u8> = vec![
            0x63, 0x00, 0x72, 0x00, 0x6f, 0x00, 0x73, 0x00, 0x6f, 0x00, 0x66, 0x00, 0x74, 0x00, 0x45, 0x00,
            0x64, 0x00, 0x67, 0x00, 0x65, 0x00, 0x55, 0x00, 0x70, 0x00, 0x64, 0x00, 0x61, 0x00, 0x74, 0x00,
            0x65, 0x00, 0x2e, 0x00, 0x65, 0x00, 0x78, 0x00, 0x65, 0x00, 0x22, 0x00, 0x20, 0x00, 0x2f, 0x00,
            0x75, 0x00, 0x61, 0x00, 0x20, 0x00, 0x2f, 0x00, 0x69, 0x00, 0x6e, 0x00, 0x73, 0x00, 0x74, 0x00,
            0x61, 0x00, 0x6c, 0x00, 0x6c, 0x00, 0x73, 0x00, 0x6f, 0x00, 0x75, 0x00, 0x72, 0x00, 0x63, 0x00,
            0x65, 0x00, 0x20, 0x00, 0x73, 0x00, 0x63, 0x00, 0x68, 0x00, 0x65, 0x00, 0x64, 0x00, 0x75, 0x00,
            0x6c, 0x00, 0x65, 0x00, 0x72, 0x00, 0x5d, 0x00, 0x0d, 0x00, 0x0a, 0x00, 0x5b, 0x00, 0x30, 0x00,
            0x37, 0x00, 0x2f, 0x00, 0x32, 0x00, 0x38, 0x00,
        ];
        assert_eq!(detect_encoding_at(&bytes, 2000), EncodingGuess::Utf16Le);
    }

    #[test]
    fn real_png_bytes_stay_binary_under_the_widened_minority_tolerance() {
        // F3 verification (binary direction): the widened minority-lane tolerance must not turn real
        // binary data into a false-positive UTF-16 report. A real PNG signature + IHDR chunk (from this
        // repo's own `src-tauri/icons/32x32.png`) followed by a deterministic pseudo-random tail standing
        // in for compressed image data — PNG's majority-lane NUL ratio never approaches the 50% bar this
        // heuristic requires in the first place (measured ~3-5% on real icon files), so this is nowhere
        // close to the tolerance boundary; committed as a concrete regression fixture rather than relying
        // on manual measurement alone.
        let mut bytes: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', // IHDR chunk header
            0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x20, // 32x32 dimensions
            0x08, 0x06, 0x00, 0x00, 0x00, 0x73, 0x7A, 0x7A, // bit depth/color type/CRC-ish bytes
        ];
        // Deterministic xorshift PRNG (no new dependency) standing in for compressed pixel data.
        let mut state: u32 = 0x2026_0811;
        for _ in 0..480 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            bytes.push((state & 0xFF) as u8);
        }
        assert_eq!(detect_encoding(&bytes), EncodingGuess::Binary);
    }

    #[test]
    fn pe_header_shaped_bytes_with_zero_padding_on_both_lanes_stay_binary() {
        // F3 verification (binary direction, the closest real near-miss found by measurement): a Windows
        // PE executable's DOS/PE header is unusually NUL-heavy (zero-padded reserved fields) — measured on
        // a real `notepad.exe`, its first 512 bytes were ~46% NUL on the minority lane and ~53% on the
        // majority lane. That's the realistic worst case for "binary data that clears the majority-lane
        // bar at all" and it sits far above `UTF16_MINORITY_NUL_RATIO` (25%) on the minority side, because
        // real zero-padding pollutes both lanes together rather than concentrating on one — unlike genuine
        // UTF-16 ASCII text, which drives one lane to ~0 and the other to ~100%. Constructed here as an
        // alternating pattern (byte, 0x00, byte, 0x00, 0x00, 0x00, …) that lands NULs on both lanes at a
        // similar, well-above-25%-minority rate without depending on a real .exe being present on disk.
        let mut bytes: Vec<u8> = vec![b'M', b'Z']; // DOS header magic
        for i in 0..255u32 {
            if i % 2 == 0 {
                bytes.push(0x00);
                bytes.push((i % 200 + 1) as u8); // non-zero, keeps this lane from being ALL NUL
            } else {
                bytes.push((i % 200 + 1) as u8);
                bytes.push(0x00);
            }
        }
        assert_eq!(detect_encoding(&bytes), EncodingGuess::Binary);
    }

    #[test]
    fn mostly_control_bytes_without_nul_is_binary() {
        // No NUL byte, but well over 30% control bytes (excluding tab/lf/cr) plus an invalid UTF-8
        // lead byte so it doesn't get swallowed by the UTF-8 check first.
        let mut bytes: Vec<u8> = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0xFF];
        bytes.extend_from_slice(b"ab");
        assert_eq!(detect_encoding(&bytes), EncodingGuess::Binary);
    }

    #[test]
    fn isolated_high_byte_is_latin1() {
        // 0xE9 alone ('é' in Latin-1) is an incomplete/invalid UTF-8 sequence (it's a 3-byte lead
        // byte with no continuation bytes), but it's a single printable-ish byte, not binary-looking.
        assert_eq!(detect_encoding(&[0xE9]), EncodingGuess::Latin1);
    }

    #[test]
    fn detect_encoding_never_panics_on_short_or_bom_prefix_input() {
        for n in 0..4 {
            let bytes = vec![0xFFu8; n];
            let _ = detect_encoding(&bytes);
        }
    }

    #[test]
    fn single_nul_byte_is_binary_not_utf16() {
        // CPE-1014: a single [0x00] byte should be classified as Binary, not UTF-16.
        // A 1-byte file can never be valid UTF-16 (which requires at least 2 bytes).
        assert_eq!(detect_encoding(&[0x00]), EncodingGuess::Binary);
    }

    // ---- detect_encoding_at (CPE-1644 A″: offset-aware NUL-lane parity) ----

    #[test]
    fn detect_encoding_at_offset_zero_matches_plain_detect_encoding() {
        let utf16le: Vec<u8> = "hello".bytes().flat_map(|b| [b, 0]).collect();
        assert_eq!(detect_encoding_at(&utf16le, 0), detect_encoding(&utf16le));
        assert_eq!(detect_encoding_at(&utf16le, 0), EncodingGuess::Utf16Le);
    }

    #[test]
    fn detect_encoding_at_an_odd_absolute_offset_reports_the_correct_endianness_where_offset_naive_detection_would_flip_it() {
        // Regression (CPE-1644 A″): a UTF-16LE stream's NUL bytes sit on ODD absolute file offsets (the
        // high byte of each ASCII code unit). A window that starts at an EVEN absolute offset sees that
        // same lane pattern in its own slice-relative indices (0-based), so plain `detect_encoding` (which
        // implicitly assumes the slice IS byte 0) gets it right by luck. But a window starting at an ODD
        // absolute offset is itself missing the first byte of the first code unit, so the NULs that were on
        // odd absolute offsets now land on EVEN slice-relative indices — offset-naive detection reports the
        // opposite endianness (Utf16Be) even though the underlying stream is genuinely Utf16Le.
        let full_stream: Vec<u8> = "hello world".bytes().flat_map(|b| [b, 0]).collect(); // UTF-16LE, no BOM
        let odd_offset = 5u64; // itself odd — cuts a code unit in half, same as a real mid-file window would
        let window = &full_stream[odd_offset as usize..];

        // Offset-naive detection (what the pre-fix `align_window` effectively did by calling
        // `detect_encoding` on a mid-file slice) misreads this window as big-endian.
        assert_eq!(detect_encoding(window), EncodingGuess::Utf16Be, "sanity: this is the bug being fixed");

        // Offset-aware detection, told the window's true absolute offset, reports the correct endianness.
        assert_eq!(detect_encoding_at(window, odd_offset), EncodingGuess::Utf16Le);
    }

    #[test]
    fn detect_encoding_at_an_even_absolute_offset_also_reports_correct_endianness_for_utf16be() {
        let full_stream: Vec<u8> = "hello world".bytes().flat_map(|b| [0, b]).collect(); // UTF-16BE, no BOM
        let odd_offset = 5u64;
        let window = &full_stream[odd_offset as usize..];

        assert_eq!(detect_encoding(window), EncodingGuess::Utf16Le, "sanity: this is the bug being fixed");
        assert_eq!(detect_encoding_at(window, odd_offset), EncodingGuess::Utf16Be);
    }

    #[test]
    fn detect_encoding_at_ignores_a_bom_like_byte_pair_that_is_not_at_the_true_file_start() {
        // A `FF FE` byte pair mid-file is not a byte-order mark — only `base_offset == 0` may treat it as
        // one. Use content that, if BOM-sniffed, would misreport; the NUL-lane heuristic underneath should
        // instead do the classifying (or fall through if the bytes don't look like a clean UTF-16 lane).
        let bytes = [0xFF, 0xFE, 0x00, 0x01, 0x02];
        // At true offset 0 this IS a BOM.
        assert_eq!(detect_encoding_at(&bytes, 0), EncodingGuess::Utf16Le);
        // At a non-zero absolute offset, the same two leading bytes are just data, not a BOM — the result
        // must come from the NUL-lane/binary fallback path instead, not the BOM shortcut.
        assert_ne!(detect_encoding_at(&bytes, 10), EncodingGuess::Utf16Le);
    }

    #[test]
    fn label_covers_every_variant() {
        assert_eq!(EncodingGuess::Empty.label(), "Empty file");
        assert_eq!(EncodingGuess::Utf8.label(), "UTF-8");
        assert_eq!(EncodingGuess::Utf8Bom.label(), "UTF-8 (with BOM)");
        assert_eq!(EncodingGuess::Utf16Le.label(), "UTF-16 LE (with BOM)");
        assert_eq!(EncodingGuess::Utf16Be.label(), "UTF-16 BE (with BOM)");
        assert_eq!(EncodingGuess::Latin1.label(), "Latin-1 / 8-bit (guessed)");
        assert_eq!(EncodingGuess::Binary.label(), "Binary");
    }

    // ---- detect_line_endings ----

    #[test]
    fn pure_lf() {
        let r = detect_line_endings("a\nb\nc\n");
        assert_eq!((r.crlf, r.lf, r.cr, r.mixed), (0, 3, 0, false));
        assert_eq!(r.dominant, LineEnding::Lf);
    }

    #[test]
    fn pure_crlf() {
        let r = detect_line_endings("a\r\nb\r\nc\r\n");
        assert_eq!((r.crlf, r.lf, r.cr, r.mixed), (3, 0, 0, false));
        assert_eq!(r.dominant, LineEnding::Crlf);
    }

    #[test]
    fn mixed_crlf_and_lf_counts_and_flag() {
        let r = detect_line_endings("a\r\nb\nc\r\nd\n e\n");
        assert_eq!((r.crlf, r.lf, r.cr), (2, 3, 0));
        assert!(r.mixed);
        assert_eq!(r.dominant, LineEnding::Lf);
    }

    #[test]
    fn lone_cr_old_mac_style() {
        let r = detect_line_endings("a\rb\rc");
        assert_eq!((r.crlf, r.lf, r.cr, r.mixed), (0, 0, 2, false));
        assert_eq!(r.dominant, LineEnding::Cr);
    }

    #[test]
    fn no_line_breaks_is_none() {
        let r = detect_line_endings("hello world, no breaks here");
        assert_eq!((r.crlf, r.lf, r.cr, r.mixed), (0, 0, 0, false));
        assert_eq!(r.dominant, LineEnding::None);
    }

    #[test]
    fn empty_string_is_none() {
        let r = detect_line_endings("");
        assert_eq!((r.crlf, r.lf, r.cr, r.mixed), (0, 0, 0, false));
        assert_eq!(r.dominant, LineEnding::None);
    }

    #[test]
    fn crlf_pair_is_not_double_counted_as_lone_cr() {
        // A lone \r\n must count once as crlf, never as crlf=1 AND cr=1.
        let r = detect_line_endings("x\r\ny");
        assert_eq!((r.crlf, r.lf, r.cr), (1, 0, 0));
    }

    #[test]
    fn dominant_tie_break_prefers_crlf_over_lf_over_cr() {
        // Equal counts of crlf and lf: Crlf wins by the documented tie-break order.
        let r = detect_line_endings("a\r\nb\n");
        assert_eq!((r.crlf, r.lf), (1, 1));
        assert_eq!(r.dominant, LineEnding::Crlf);
    }
}
