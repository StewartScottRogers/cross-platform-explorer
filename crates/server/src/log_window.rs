//! Bounded windowed reads for the log preview (CPE-1637, epic CPE-1568 slice 8).
//!
//! The log viewer (CPE-1618) reused the generic `read_file_text` command, which checks the file's size
//! *before reading any bytes* and refuses outright above `PREVIEW_MAX_BYTES` (256 KiB). That's honest —
//! no silent truncation — but it means the viewer refuses every real incident log a person actually
//! reaches for: `CBS.log` (15.4 MB), `dism.log` (19.2 MB), … A log-viewing feature that declines the logs
//! worth viewing isn't one.
//!
//! [`read_window`] reads a single bounded **window** of up to `max_bytes` instead of the whole file,
//! defaulting to the **tail** (`end: None`) — for a log, the end is almost always what you want — and
//! pages backward from there by passing a previous window's `window_start` back in as `end`.
//!
//! **Bounds the WORK, not just the output.** This crew learned the hard way (CPE-1616: a crafted font
//! froze the app for 8.8s because a cap counted items *emitted* rather than *examined*) that a cap on
//! output alone isn't a real cap. [`read_window`] seeks straight to the window's start and reads exactly
//! the window's byte span — it never measures, scans, or reads anything else in the file, so opening a
//! 19 MB log costs exactly `max_bytes` of I/O, the same as opening a 19 GB one. **Do not** change this to
//! read-then-truncate.
//!
//! **Line-boundary alignment, never a silent truncation.** An arbitrary byte offset can land mid-line, so
//! whenever a window's start isn't the true start of the file, [`align_window`] discards the partial
//! leading line rather than showing (or silently eating) a fragment of it, and reports exactly which byte
//! range survived (`window_start`/`window_end`/`file_len` on [`LogWindow`]) so the caller can render an
//! accurate "showing bytes X–Y of Z" note instead of guessing. Aligning to `\n` (`0x0A`) is also always a
//! valid UTF-8 boundary — `0x0A` can never appear as a continuation byte (those are all `0x80..=0xBF`) —
//! so line alignment and UTF-8-safety are the same fix. [`LogWindow::line_aligned`] is `false` only in the
//! degenerate case where the window contains no newline at all (one pathologically long line), where
//! alignment instead falls back to the next valid UTF-8 character boundary so decoding still succeeds.
//!
//! Deliberately **not** `read_file_text` with a raised cap: raising the shared `PREVIEW_MAX_BYTES` would
//! push a multi-megabyte read into every other preview provider that reuses it. This is a separate,
//! log-specific path.
//!
//! **UTF-16 is decoded, not silently garbled (CPE-1637 review; decoding landed in CPE-1644 A′).** A
//! `String::from_utf8` check alone cannot catch it: every UTF-16 ASCII byte (NUL bytes included) is
//! individually valid single-byte UTF-8, so a UTF-16 log would otherwise "succeed" into NUL-interleaved
//! garbage. [`align_window`] runs the crate's existing [`text_encoding::detect_encoding_at`] sniffer over
//! the window first (offset-aware — see [`text_encoding::detect_encoding_at`]'s docs on why the sniff
//! needs the window's absolute file offset, not just its own bytes, to tell LE from BE correctly) and
//! routes `Utf16Le`/`Utf16Be` windows to [`decode_utf16_window`] — a parallel, code-unit-pair-aligned
//! version of the byte-aligned UTF-8 path below it, using only `std::char::decode_utf16` (no new
//! dependency). UTF-16 was refused outright for one CPE-1637 review cycle before this landed; that
//! interim answer is gone now that the byte-pair-aligned seeking it needed is implemented.

use crate::text_encoding;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// One bounded window of a text file, decoded to a `String` — see the module docs for the alignment and
/// work-bounding guarantees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LogWindow {
    /// The window's decoded text.
    pub text: String,
    /// Byte offset into the file where `text` starts, after line-boundary alignment.
    pub window_start: u64,
    /// Byte offset into the file where `text` ends (exclusive).
    pub window_end: u64,
    /// The file's total size in bytes, at the moment of this read.
    pub file_len: u64,
    /// `true` when `window_start == 0` — there is nothing further back to page to.
    pub at_start: bool,
    /// `true` when `window_end == file_len` — this window is the tail of the file.
    pub at_end: bool,
    /// `true` unless the window held no newline at all, forcing the start to fall back to a raw UTF-8
    /// character boundary instead of a true line boundary (one pathologically long line).
    pub line_aligned: bool,
}

/// Read a single bounded window of the file at `path`, ending at `end` (or the file's current length —
/// i.e. the tail — when `end` is `None`), at most `max_bytes` long.
///
/// Bounded I/O: seeks directly to the window's start and reads exactly `end - start` bytes. Never reads,
/// scans, or hashes the rest of the file, regardless of how large it is — see the module docs.
pub fn read_window(path: &Path, max_bytes: u64, end: Option<u64>) -> Result<LogWindow, String> {
    let file_len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    let end = end.unwrap_or(file_len).min(file_len);
    let raw_start = end.saturating_sub(max_bytes);

    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;

    // Bounded 1-byte peek (CPE-1637 review): `raw_start` can land exactly on a genuine line boundary —
    // not rare for uniform-length logs, and GUARANTEED whenever the tail line's own length is close to
    // `max_bytes` (the one-pathologically-long-line case this module already special-cases below). If
    // the byte immediately before `raw_start` is already `\n`, `raw_start` needs no trim at all; blindly
    // trimming through the next `\n` regardless (the old behavior) discarded an entire legitimate line
    // it never needed to drop — see `landing_exactly_on_a_line_boundary_does_not_discard_the_next_line`.
    let already_aligned = if raw_start == 0 {
        true
    } else {
        let mut prev = [0u8; 1];
        file.seek(SeekFrom::Start(raw_start - 1)).map_err(|e| e.to_string())?;
        file.read_exact(&mut prev).map_err(|e| e.to_string())?;
        prev[0] == b'\n'
    };

    file.seek(SeekFrom::Start(raw_start)).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; (end - raw_start) as usize];
    file.read_exact(&mut buf).map_err(|e| e.to_string())?;

    align_window(&buf, raw_start, end, file_len, already_aligned)
}

/// Pure alignment/decode step, split out from [`read_window`] so it's unit-testable without touching a
/// real file: given the raw bytes already read for `[raw_start, end)`, trims a partial leading line (or,
/// failing that, a partial leading UTF-8 sequence) and decodes what remains. `already_aligned` — see
/// [`read_window`]'s 1-byte peek — must be `true` whenever `raw_start` is already known to sit right
/// after a `\n` (or at true byte 0), so this never trims through a `\n` it didn't need to.
fn align_window(
    buf: &[u8],
    raw_start: u64,
    end: u64,
    file_len: u64,
    already_aligned: bool,
) -> Result<LogWindow, String> {
    // UTF-16 (with or without a BOM) is NOT caught by the `String::from_utf8` check below — every
    // UTF-16LE/BE ASCII byte, NUL bytes included, is individually valid single-byte UTF-8, so a UTF-16
    // log would otherwise "succeed" straight into NUL-interleaved garbage on screen (CPE-1637 review).
    // Sniff first — bounded (it only ever samples a small leading prefix of whatever slice it's given, so
    // this is no extra I/O beyond the window already in hand), and offset-aware (`raw_start` is this
    // window's true absolute file offset, needed to tell LE from BE correctly — see
    // `text_encoding::detect_encoding_at`'s docs, CPE-1644 A″) — and route a UTF-16 window through its own
    // code-unit-aligned decode path (CPE-1644 A′) rather than the byte-aligned UTF-8 path below, whose
    // line-alignment relies on `0x0A` never being a UTF-8 continuation byte — a guarantee that doesn't
    // extend to UTF-16's 2-byte, endianness-dependent code units. Checked on the raw (pre-trim) window so
    // the NUL-lane signature is intact.
    match text_encoding::detect_encoding_at(buf, raw_start) {
        text_encoding::EncodingGuess::Utf16Le => {
            return decode_utf16_window(buf, raw_start, end, file_len, Utf16Endianness::Le);
        }
        text_encoding::EncodingGuess::Utf16Be => {
            return decode_utf16_window(buf, raw_start, end, file_len, Utf16Endianness::Be);
        }
        _ => {}
    }

    let (skip, line_aligned) = if raw_start == 0 || already_aligned {
        // True start of the file, or `raw_start` already sits right after a '\n' — nothing to trim.
        (0, true)
    } else {
        match buf.iter().position(|&b| b == b'\n') {
            Some(i) => (i + 1, true),
            // No newline anywhere in this window (one pathologically long line): fall back to the next
            // valid UTF-8 character boundary — a continuation byte is always 0x80..=0xBF — so decoding
            // still succeeds, but flag that the start isn't a clean line boundary.
            None => (
                buf.iter().take_while(|&&b| (b & 0b1100_0000) == 0b1000_0000).count(),
                false,
            ),
        }
    };

    let text = String::from_utf8(buf[skip..].to_vec())
        .map_err(|_| "File is not valid UTF-8 text.".to_string())?;
    let window_start = raw_start + skip as u64;
    Ok(LogWindow {
        text,
        window_start,
        window_end: end,
        file_len,
        at_start: window_start == 0,
        at_end: end == file_len,
        line_aligned,
    })
}

/// Which byte order a UTF-16 window is being decoded as — see [`decode_utf16_window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Utf16Endianness {
    Le,
    Be,
}

/// Decode a UTF-16 window (CPE-1644 A′) — a code-unit-pair-aligned parallel to [`align_window`]'s UTF-8
/// path, since a UTF-16 `\n` is the 2-byte code unit `0x0A 0x00` (LE) / `0x00 0x0A` (BE), not the single
/// always-boundary-safe `0x0A` byte the UTF-8 path relies on. Uses only `std::char::decode_utf16` — no
/// new dependency.
///
/// **Alignment model:** a UTF-16 stream's code-unit boundaries fall on absolute byte offsets of the same
/// *parity* as the stream's own start — and a leading 2-byte BOM never changes that parity, since 2 is
/// even. So `raw_start` only ever needs its own parity fixed up (trim one leading byte if `raw_start` is
/// odd) — never a peek at byte 0 to learn whether a BOM is present.
///
/// **Simplification vs. the UTF-8 path:** unlike [`align_window`], this never trusts a precomputed
/// "already on a boundary" signal (the 1-byte peek in [`read_window`] only checks for a single `\n` byte,
/// which doesn't mean anything for a 2-byte UTF-16 code unit) — it always searches for the code-unit
/// `\n` when `raw_start != 0`. That can occasionally trim one line that didn't strictly need trimming
/// (the exact-boundary edge case [`landing_exactly_on_a_line_boundary_does_not_discard_the_next_line`]
/// fixed for the UTF-8 path), which is an accepted, documented trade-off for this new path rather than a
/// regression of that fix (which remains intact for UTF-8/most real logs).
///
/// **Never fails on malformed input:** an unpaired surrogate decodes to U+FFFD (`REPLACEMENT_CHARACTER`)
/// rather than erroring, matching this crate's "a hostile/malformed file degrades gracefully, never a
/// hard failure" convention elsewhere (`parseLog`'s frontend counterpart never throws either).
fn decode_utf16_window(
    buf: &[u8],
    raw_start: u64,
    end: u64,
    file_len: u64,
    endianness: Utf16Endianness,
) -> Result<LogWindow, String> {
    // A BOM only ever appears in the window that starts at true byte 0 — strip it before pairing so it
    // isn't decoded as a literal (harmless but ugly) U+FEFF character in the rendered text.
    let bom_skip = if raw_start == 0 && buf.len() >= 2 {
        match (endianness, buf[0], buf[1]) {
            (Utf16Endianness::Le, 0xFF, 0xFE) => 2,
            (Utf16Endianness::Be, 0xFE, 0xFF) => 2,
            _ => 0,
        }
    } else {
        0
    };

    // Snap to a code-unit boundary: an odd number of bytes remaining before pairing means `raw_start`
    // landed mid-code-unit. That single stray byte is an unrecoverable fragment of the previous window's
    // last (unseen) code unit — the same "loss of the fragment is expected" trade-off `align_window`'s
    // UTF-8 path documents for a split multibyte character.
    let parity_skip = if raw_start == 0 { 0 } else { (raw_start % 2) as usize };
    let body_start = (bom_skip + parity_skip).min(buf.len());
    let body = &buf[body_start..];

    let code_units: Vec<u16> = body
        .chunks_exact(2)
        .map(|pair| match endianness {
            Utf16Endianness::Le => u16::from_le_bytes([pair[0], pair[1]]),
            Utf16Endianness::Be => u16::from_be_bytes([pair[0], pair[1]]),
        })
        .collect();

    // Line-boundary alignment in code-unit space — see the doc comment above on why this always searches
    // rather than trusting `already_aligned` the way the UTF-8 path does.
    let (skip_units, line_aligned) = if raw_start == 0 {
        (0, true)
    } else {
        match code_units.iter().position(|&u| u == 0x000A) {
            Some(i) => (i + 1, true),
            // No newline code unit anywhere in this window (one pathologically long line): keep
            // everything and flag the start as not a clean line boundary, mirroring the UTF-8 path.
            None => (0, false),
        }
    };

    let text: String = char::decode_utf16(code_units[skip_units..].iter().copied())
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();

    let window_start = raw_start + body_start as u64 + (skip_units as u64 * 2);
    Ok(LogWindow {
        text,
        window_start,
        window_end: end,
        file_len,
        // `raw_start == 0`, not `window_start == 0`: a BOM'd file's very first window trims 2 bytes of
        // BOM off `window_start` even though `raw_start` (and this read) genuinely started at true byte
        // 0 — there is nothing further back to page to, so "Load earlier" must still be disabled. (The
        // UTF-8 path's `window_start == 0` check is equivalent to this for it, since its skip is always 0
        // whenever `raw_start == 0`; the BOM-trim here is what breaks that equivalence for UTF-16.)
        at_start: raw_start == 0,
        at_end: end == file_len,
        line_aligned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-log-window-{tag}"))
    }

    /// Build a synthetic multi-megabyte log file (never checked in, never a real machine file) so the
    /// "opens a huge file cheaply" behavior is exercised without depending on what happens to be on disk.
    fn write_big_log(path: &Path, lines: u32) {
        let mut f = fs::File::create(path).unwrap();
        for i in 0..lines {
            writeln!(f, "2026-08-11T00:00:{:02}Z [INFO] line number {i} of the synthetic log", i % 60).unwrap();
        }
    }

    #[test]
    fn small_file_fits_in_one_window_untouched() {
        let d = scratch("small");
        let f = d.join("a.log");
        fs::write(&f, b"line one\nline two\nline three\n").unwrap();
        let w = read_window(&f, 1024, None).unwrap();
        assert_eq!(w.text, "line one\nline two\nline three\n");
        assert!(w.at_start);
        assert!(w.at_end);
        assert!(w.line_aligned);
        assert_eq!(w.window_start, 0);
        assert_eq!(w.window_end, w.file_len);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_file_is_an_empty_window_not_an_error() {
        let d = scratch("empty");
        let f = d.join("empty.log");
        fs::write(&f, b"").unwrap();
        let w = read_window(&f, 1024, None).unwrap();
        assert_eq!(w.text, "");
        assert!(w.at_start);
        assert!(w.at_end);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_big_file_yields_a_tail_window_far_smaller_than_the_file() {
        let d = scratch("big");
        let f = d.join("big.log");
        // ~5MB of synthetic content — big enough to be far over the old 256 KiB refuse threshold and to
        // prove the read genuinely stayed windowed, without needing a real machine log.
        write_big_log(&f, 80_000);
        let file_len = fs::metadata(&f).unwrap().len();
        assert!(file_len > 2 * 1024 * 1024, "fixture should be multi-megabyte");

        let w = read_window(&f, 64 * 1024, None).unwrap();
        assert!(w.at_end);
        assert!(!w.at_start);
        assert!(w.line_aligned);
        assert_eq!(w.window_end, file_len);
        // The window is bounded near max_bytes, nowhere near the full file size.
        assert!(w.text.len() as u64 <= 64 * 1024);
        assert!((w.window_end - w.window_start) <= 64 * 1024);
        // Tail text really is the tail: the file's last line appears in it.
        assert!(w.text.contains("line number 79999"));
        // And the (trimmed) window never starts mid-line: it always starts right after a '\n', except
        // possibly at true byte 0, which this window isn't.
        assert!(w.window_start > 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn paging_backward_from_a_windows_start_lands_on_the_immediately_preceding_bytes() {
        let d = scratch("page");
        let f = d.join("page.log");
        write_big_log(&f, 20_000);

        let tail = read_window(&f, 32 * 1024, None).unwrap();
        assert!(!tail.at_start);
        let older = read_window(&f, 32 * 1024, Some(tail.window_start)).unwrap();
        // The older window ends exactly where the tail window began — no gap, no overlap.
        assert_eq!(older.window_end, tail.window_start);
        assert!(older.line_aligned);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn reaching_byte_zero_reports_at_start() {
        let d = scratch("start");
        let f = d.join("start.log");
        write_big_log(&f, 50); // small enough that a couple of backward pages reach byte 0
        let file_len = fs::metadata(&f).unwrap().len();

        let mut end = None;
        let mut w = read_window(&f, 200, end).unwrap();
        let mut guard = 0;
        while !w.at_start {
            end = Some(w.window_start);
            w = read_window(&f, 200, end).unwrap();
            guard += 1;
            assert!(guard < 1000, "paging backward should terminate");
        }
        assert_eq!(w.window_start, 0);
        assert!(file_len > 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn invalid_utf8_is_reported_not_panicked() {
        let d = scratch("badutf8");
        let f = d.join("bad.log");
        // Bare UTF-8 continuation bytes: invalid as a UTF-8 lead, no NUL byte (so it never takes the
        // UTF-16 path either) — this exercises the plain "not valid UTF-8" refusal, distinct from
        // `[0xFF, 0xFE, ...]`, which is now a genuine (if tiny) valid UTF-16LE-with-BOM file since
        // CPE-1644 A′ added real UTF-16 decoding.
        fs::write(&f, [0x80, 0x81, 0x82, 0x83]).unwrap();
        let r = read_window(&f, 1024, None);
        let err = r.expect_err("bare continuation bytes are not valid UTF-8 or a UTF-16 lane");
        assert!(err.contains("UTF-8"), "expected the plain UTF-8 refusal, got: {err}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn landing_exactly_on_a_line_boundary_does_not_discard_the_next_line() {
        // Regression (CPE-1637 review). `raw_start` can coincide with a genuine '\n' boundary — not
        // rare for uniform-length logs, and GUARANTEED when the tail line's length is close to
        // max_bytes (the one-long-line case this ticket exists for). The old alignment unconditionally
        // trimmed through the first '\n' it found regardless of whether raw_start already sat on a
        // boundary, discarding an entire legitimate line it never needed to drop.
        //
        // Exact reproduction from review: 15-byte file "AAAA\nBBBB\nCCCC\n", max_bytes=5, tail read.
        // raw_start = 15 - 5 = 10, which is exactly one past the '\n' at offset 9 — already aligned.
        let d = scratch("boundary");
        let f = d.join("boundary.log");
        fs::write(&f, b"AAAA\nBBBB\nCCCC\n").unwrap();
        let w = read_window(&f, 5, None).unwrap();
        assert_eq!(w.text, "CCCC\n", "must not discard the legitimate last line");
        assert_eq!(w.window_start, 10);
        assert_eq!(w.window_end, 15);
        assert!(w.at_end);
        assert!(!w.at_start);
        assert!(w.line_aligned);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_file_is_a_clean_error_not_a_panic() {
        let f = std::env::temp_dir().join("cpe-log-window-does-not-exist-12345.log");
        let r = read_window(&f, 1024, None);
        assert!(r.is_err());
    }

    // --- align_window unit tests: exercise the pure alignment logic directly with synthetic byte
    // buffers, independent of any file I/O, per the "generate what you need" testing rule. ---

    #[test]
    fn align_window_trims_a_partial_leading_line() {
        // Simulates a window whose raw start landed mid-line: "rtial\nfull line\n" — the "rtial" prefix
        // (a fragment of the previous, unseen line) must be dropped. `already_aligned=false` since this
        // window is deliberately simulating a raw_start that did NOT land on a boundary.
        let buf = b"rtial\nfull line\n";
        let w = align_window(buf, 100, 117, 500, false).unwrap();
        assert_eq!(w.text, "full line\n");
        assert_eq!(w.window_start, 106); // 100 + len("rtial\n")
        assert!(w.line_aligned);
        assert!(!w.at_start);
        assert!(!w.at_end);
    }

    #[test]
    fn align_window_at_true_start_trims_nothing() {
        let buf = b"first line\nsecond\n";
        let w = align_window(buf, 0, buf.len() as u64, buf.len() as u64, true).unwrap();
        assert_eq!(w.text, "first line\nsecond\n");
        assert!(w.at_start);
        assert!(w.at_end);
        assert!(w.line_aligned);
    }

    #[test]
    fn align_window_already_aligned_trims_nothing_even_though_the_buffer_starts_with_a_full_line() {
        // Regression (CPE-1637 review): raw_start sits right after a genuine '\n' (already_aligned=true)
        // even though it isn't byte 0. The buffer's own first line ("keep me\n") must NOT be trimmed away
        // just because a '\n' exists somewhere in it — the old code trimmed unconditionally whenever
        // raw_start != 0, discarding this legitimate content.
        let buf = b"keep me\nand this too\n";
        let w = align_window(buf, 100, 100 + buf.len() as u64, 10_000, true).unwrap();
        assert_eq!(w.text, "keep me\nand this too\n");
        assert_eq!(w.window_start, 100);
        assert!(w.line_aligned);
        assert!(!w.at_start);
    }

    #[test]
    fn align_window_with_no_newline_falls_back_to_utf8_boundary_and_flags_unaligned() {
        // A window entirely inside one giant line: no '\n' anywhere. Falls back to keeping the whole
        // (already UTF-8-boundary-clean, since these are all ASCII) buffer and flags line_aligned=false.
        let buf = b"middleofaverylongsingleline";
        let w = align_window(buf, 100, 100 + buf.len() as u64, 10_000, false).unwrap();
        assert_eq!(w.text, "middleofaverylongsingleline");
        assert!(!w.line_aligned);
        assert!(!w.at_start);
    }

    #[test]
    fn align_window_never_splits_a_multibyte_utf8_character() {
        // "é" is 2 bytes (0xC3 0xA9). Build a window whose raw start lands exactly between those two
        // bytes with no newline before the safe part, followed by a real line boundary — the loss of the
        // fragment is expected (this crew is reconstructing a window we already know starts mid-content),
        // the important guarantee is that from_utf8 never fails/panics on the retained tail.
        let mut buf = Vec::new();
        buf.push(0xA9); // stray continuation byte (second half of a split "é")
        buf.extend_from_slice("rest\nnext line\n".as_bytes());
        let w = align_window(&buf, 50, 50 + buf.len() as u64, 1000, false).unwrap();
        assert_eq!(w.text, "next line\n");
        assert!(w.line_aligned);
    }

    // --- UTF-16 decoding (CPE-1644 A′) ---
    //
    // Every fixture's EXPECTED text is the plain original `text` string — never something derived by
    // calling this crate's own decoder — encoded to raw UTF-16 bytes via Rust's own `encode_utf16` (a
    // trusted, independent reference encoder), matching the ticket's "assert on bytes, don't read your
    // own decoder" instruction.

    #[test]
    fn bom_less_utf16le_content_decodes_correctly_not_silently_garbled() {
        // Regression: a real UTF-16LE log (e.g. MicrosoftEdgeUpdate.log) previously "succeeded" through
        // `String::from_utf8` into NUL-interleaved garbage, because every UTF-16LE ASCII byte — NULs
        // included — is individually valid single-byte UTF-8. For one CPE-1637 review cycle this was
        // instead refused outright (an honest but incomplete interim answer); CPE-1644 A′ now decodes it.
        let d = scratch("utf16le");
        let f = d.join("edge.log");
        let text = "2026-08-11 00:00:00 INFO Starting update check\r\n2026-08-11 00:00:01 INFO Done\r\n";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        fs::write(&f, &utf16le).unwrap();

        let w = read_window(&f, 1024, None).expect("UTF-16LE content must decode, not be refused or garbled");
        assert_eq!(w.text, text);
        assert!(w.at_start);
        assert!(w.at_end);
        assert!(w.line_aligned);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bom_less_utf16be_content_decodes_correctly() {
        let d = scratch("utf16be");
        let f = d.join("be.log");
        let text = "hello log line one\r\nhello log line two\r\n";
        let utf16be: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        fs::write(&f, &utf16be).unwrap();

        let w = read_window(&f, 1024, None).expect("UTF-16BE content must decode, not be refused or garbled");
        assert_eq!(w.text, text);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn utf16le_with_a_bom_decodes_correctly_and_the_bom_itself_is_not_rendered() {
        let d = scratch("utf16le-bom");
        let f = d.join("bommed.log");
        let text = "line one\r\nline two\r\n";
        let mut utf16le_bom: Vec<u8> = vec![0xFF, 0xFE]; // UTF-16LE BOM
        utf16le_bom.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
        fs::write(&f, &utf16le_bom).unwrap();

        let w = read_window(&f, 1024, None).expect("BOM'd UTF-16LE must decode cleanly");
        assert_eq!(w.text, text, "the BOM itself must not appear as a literal U+FEFF in the rendered text");
        assert!(w.at_start);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn utf16be_with_a_bom_decodes_correctly_and_the_bom_itself_is_not_rendered() {
        let d = scratch("utf16be-bom");
        let f = d.join("bommed.log");
        let text = "line one\r\nline two\r\n";
        let mut utf16be_bom: Vec<u8> = vec![0xFE, 0xFF]; // UTF-16BE BOM
        utf16be_bom.extend(text.encode_utf16().flat_map(|u| u.to_be_bytes()));
        fs::write(&f, &utf16be_bom).unwrap();

        let w = read_window(&f, 1024, None).expect("BOM'd UTF-16BE must decode cleanly");
        assert_eq!(w.text, text);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bom_less_utf16le_with_an_emoji_decodes_not_refused() {
        // Regression (reviewer's PR #842 repro, F3): `classify_nul_bytes` used to require the minority
        // NUL lane to be EXACTLY zero, so any UTF-16 content that wasn't pure ASCII flipped the whole
        // window to `Binary` and fell through to the byte-aligned UTF-8 path, which then failed outright
        // — a regression from the honest "looks like UTF-16" refusal this ticket exists to replace. A
        // realistic mixed-ASCII log line with one emoji (its low surrogate's low byte, 0x00, lands on the
        // "wrong" lane) must still decode.
        let d = scratch("utf16le-emoji");
        let f = d.join("emoji.log");
        let text = "2026-08-11 00:00:00 INFO user reacted with \u{1F600} to the message\r\nnext line\r\n";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        fs::write(&f, &utf16le).unwrap();

        let w = read_window(&f, 4096, None).expect("UTF-16LE content with an emoji must decode, not be refused");
        assert_eq!(w.text, text);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bom_less_utf16le_with_accented_latin_decodes() {
        let d = scratch("utf16le-accented");
        let f = d.join("accented.log");
        let text = "2026-08-11 00:00:00 INFO utilisateur a r\u{e9}agi avec \u{e9}motion \u{e0} la commande caf\u{e9}\r\nligne suivante\r\n";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        fs::write(&f, &utf16le).unwrap();

        let w = read_window(&f, 4096, None).expect("UTF-16LE content with accented Latin must decode");
        assert_eq!(w.text, text);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bom_less_utf16le_with_cjk_names_mixed_into_mostly_ascii_decodes() {
        let d = scratch("utf16le-cjk");
        let f = d.join("cjk.log");
        let text = "2026-08-11 00:00:00 INFO \u{7528}\u{6237}\u{540d} \u{5f20}\u{4f1f} logged in successfully\r\nnext line\r\n";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        fs::write(&f, &utf16le).unwrap();

        let w = read_window(&f, 4096, None).expect("UTF-16LE content with CJK names must decode");
        assert_eq!(w.text, text);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bom_less_utf16be_no_bom_with_rtl_content_decodes() {
        // A BOM-less BE fixture (F4 coverage gap: every prior UTF-16 fixture had a BOM or was LE) mixed
        // with RTL (Arabic) text.
        let d = scratch("utf16be-rtl");
        let f = d.join("rtl.log");
        let text = "2026-08-11 00:00:00 INFO \u{62a}\u{645} \u{62a}\u{633}\u{62c}\u{64a}\u{644} \u{627}\u{644}\u{62f}\u{62e}\u{648}\u{644}\r\nnext line\r\n";
        let utf16be: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        fs::write(&f, &utf16be).unwrap();

        let w = read_window(&f, 4096, None).expect("BOM-less UTF-16BE content with RTL text must decode");
        assert_eq!(w.text, text);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn utf16le_surrogate_pair_bisected_by_the_window_boundary_degrades_to_replacement_character_not_a_panic() {
        // F4: commit the reviewer's ad-hoc probe as a permanent test. A window boundary that lands
        // between the two code units of a surrogate pair (a non-BMP character split across a page) loses
        // that one character to U+FFFD — a documented, accepted trade-off (see `decode_utf16_window`'s doc
        // comment) — but must never panic and must decode everything else cleanly.
        //
        // Deliberately ONE long line with no newline at all: if a `\n` code unit followed the bisection
        // point, line-boundary alignment would simply discard the orphaned fragment (the same "loss of a
        // partial leading line is expected" behavior `align_window`'s UTF-8 path already documents) and
        // the replacement character would never make it into the rendered text. The no-newline fallback
        // path is what actually keeps (and thus decodes) the orphaned low surrogate — see
        // `decode_utf16_window`'s "no newline code unit anywhere in this window" branch.
        let d = scratch("utf16le-bisected");
        let f = d.join("bisected.log");
        // "aaaa" (8 bytes) + emoji (4 bytes: 2 code units at byte offsets 8..12) + "bbbb" (8 bytes), no
        // newline anywhere. A max_bytes chosen so raw_start lands at byte 10 (mid-emoji) bisects it.
        let text = "aaaa\u{1F600}bbbb";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        fs::write(&f, &utf16le).unwrap();
        let file_len = utf16le.len() as u64;
        let max_bytes = file_len - 10; // raw_start = file_len - max_bytes = 10, mid-emoji

        let w = read_window(&f, max_bytes, None).expect("a bisected surrogate pair must not panic or error");
        assert!(!w.line_aligned, "no newline anywhere in this window — must be flagged unaligned");
        // The emoji's second code unit (the low surrogate, now orphaned at the window's start) decodes to
        // U+FFFD; the rest of the content after it is intact.
        assert_eq!(w.text, "\u{FFFD}bbbb", "expected a replacement character for the orphaned low surrogate");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn utf16le_tail_window_of_a_larger_file_decodes_correctly_even_when_the_window_lands_on_an_odd_byte_offset() {
        // The real "byte-pair-aligned seeks" concern the ticket calls out: a window into a UTF-16 file
        // whose start doesn't happen to land on an even absolute offset must still decode cleanly (no
        // split code units, no replacement characters, no garbage) — this is what A″'s offset-aware
        // sniffing and A′'s parity-skip logic exist to guarantee together.
        let d = scratch("utf16le-tail");
        let f = d.join("big.log");
        let mut text = String::new();
        for i in 0..2000 {
            text.push_str(&format!("2026-08-11T00:00:{:02}Z [INFO] line number {i} of the synthetic log\r\n", i % 60));
        }
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        fs::write(&f, &utf16le).unwrap();
        let file_len = utf16le.len() as u64;

        // Choose max_bytes so `file_len - max_bytes` (the raw window start before alignment) is ODD —
        // forcing the parity-skip path this test exists to exercise.
        let mut max_bytes = 20_000u64;
        if (file_len - max_bytes) % 2 == 0 {
            max_bytes += 1;
        }
        assert_eq!((file_len - max_bytes) % 2, 1, "test setup: need an odd raw window start");

        let w = read_window(&f, max_bytes, None).unwrap();
        assert!(w.at_end);
        assert!(!w.at_start);
        assert!(w.line_aligned);
        // No replacement characters — a split/misaligned code unit would decode to one.
        assert!(!w.text.contains('\u{FFFD}'), "must not contain replacement characters: {:?}", &w.text[..80.min(w.text.len())]);
        // The tail's last line is intact and readable.
        assert!(w.text.contains("line number 1999 of the synthetic log"));
        // The window starts cleanly at a real line boundary — not a fragment of a line.
        assert!(
            w.text.starts_with("2026-08-11T00:00:"),
            "window should start at a clean line boundary, got: {:?}",
            &w.text[..40.min(w.text.len())]
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn utf16_windowed_paging_never_reintroduces_a_split_code_unit_across_several_backward_pages() {
        let d = scratch("utf16le-paging");
        let f = d.join("paging.log");
        let mut text = String::new();
        for i in 0..500 {
            text.push_str(&format!("line {i:04}\r\n"));
        }
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        fs::write(&f, &utf16le).unwrap();

        let mut end = None;
        for _ in 0..5 {
            let w = read_window(&f, 777, end).unwrap(); // an odd, deliberately awkward page size
            assert!(!w.text.contains('\u{FFFD}'), "page must not contain replacement characters");
            if w.at_start {
                break;
            }
            end = Some(w.window_start);
        }
        let _ = fs::remove_dir_all(&d);
    }
}
