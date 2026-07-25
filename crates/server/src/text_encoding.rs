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

/// True for a byte that reads as non-text control data: the C0 control range excluding tab/LF/CR
/// (the three control characters that legitimately appear in text), plus DEL (`0x7F`).
fn is_binary_control_byte(b: u8) -> bool {
    (b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r') || b == 0x7F
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
    if bytes.is_empty() {
        return EncodingGuess::Empty;
    }
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return EncodingGuess::Utf8Bom;
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        return EncodingGuess::Utf16Le;
    }
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        return EncodingGuess::Utf16Be;
    }
    // A NUL byte means this isn't plain UTF-8/Latin-1 *text* (neither ever contains NUL); classify it as
    // BOM-less UTF-16 or binary BEFORE the UTF-8 check — a NUL is itself valid UTF-8, so a BOM-less
    // UTF-16 ASCII file like `h\0i\0` would otherwise round-trip through `from_utf8` and be mis-reported
    // as plain text (CPE-1003 QA finding).
    if bytes.contains(&0) {
        return classify_nul_bytes(bytes);
    }
    if std::str::from_utf8(bytes).is_ok() {
        return EncodingGuess::Utf8;
    }
    if looks_binary(bytes) {
        return EncodingGuess::Binary;
    }
    EncodingGuess::Latin1
}

/// Classify NUL-containing, BOM-less input (called only from [`detect_encoding`]). BOM-less UTF-16 of
/// ASCII-range text fills one whole byte-lane with NULs — odd offsets for little-endian, even for
/// big-endian — so a lane that is at least half NUL while the other lane is clear is reported as UTF-16;
/// anything else (NULs on both lanes, or just a stray NUL) is [`Binary`](EncodingGuess::Binary).
fn classify_nul_bytes(bytes: &[u8]) -> EncodingGuess {
    let sniff_len = bytes.len().min(BINARY_SNIFF_LEN);
    if sniff_len < 2 {
        return EncodingGuess::Binary;
    }
    let lane = (sniff_len / 2).max(1); // approx positions per byte-lane in the sniffed prefix
    let mut even_nul = 0usize;
    let mut odd_nul = 0usize;
    for (i, &b) in bytes[..sniff_len].iter().enumerate() {
        if b == 0 {
            if i % 2 == 0 {
                even_nul += 1;
            } else {
                odd_nul += 1;
            }
        }
    }
    if odd_nul * 2 >= lane && even_nul == 0 {
        EncodingGuess::Utf16Le
    } else if even_nul * 2 >= lane && odd_nul == 0 {
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
