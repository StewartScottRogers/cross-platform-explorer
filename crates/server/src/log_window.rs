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
//! **UTF-16 is refused, not silently garbled (CPE-1637 review).** A `String::from_utf8` check alone
//! cannot catch it: every UTF-16 ASCII byte (NUL bytes included) is individually valid single-byte
//! UTF-8, so a UTF-16 log would otherwise "succeed" into NUL-interleaved garbage. [`align_window`] runs
//! the crate's existing [`text_encoding::detect_encoding`] sniffer over the window first and returns a
//! clean, encoding-naming error for `Utf16Le`/`Utf16Be` instead.

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
    // The old whole-file `read_file_text` path happened to mask this by refusing such files outright on
    // size before ever decoding them; this windowed path exposed it. Reuse the crate's existing sniffer
    // — bounded (it only ever samples a small leading prefix of whatever slice it's given, so this is no
    // extra I/O beyond the window already in hand) — and refuse cleanly, naming the encoding, rather than
    // decode it wrong. Checked on the raw (pre-trim) window so the NUL-lane signature is intact.
    match text_encoding::detect_encoding(buf) {
        text_encoding::EncodingGuess::Utf16Le => {
            return Err(
                "This file looks like UTF-16 (little-endian) text, which the log preview doesn't decode \
                 yet. Open it in a text editor instead."
                    .to_string(),
            );
        }
        text_encoding::EncodingGuess::Utf16Be => {
            return Err(
                "This file looks like UTF-16 (big-endian) text, which the log preview doesn't decode \
                 yet. Open it in a text editor instead."
                    .to_string(),
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-log-window-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
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
        fs::write(&f, [0xFF, 0xFE, 0x00, 0x01]).unwrap();
        let r = read_window(&f, 1024, None);
        assert!(r.is_err());
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

    // --- UTF-16 refusal (CPE-1637 review) ---

    #[test]
    fn bom_less_utf16le_content_is_refused_cleanly_not_silently_garbled() {
        // Regression: a real UTF-16LE log (e.g. MicrosoftEdgeUpdate.log) previously "succeeded" through
        // `String::from_utf8` into NUL-interleaved garbage, because every UTF-16LE ASCII byte — NULs
        // included — is individually valid single-byte UTF-8. Build a synthetic UTF-16LE fixture (no
        // machine-specific path) matching that shape: plain ASCII log lines encoded as UTF-16LE, no BOM.
        let d = scratch("utf16le");
        let f = d.join("edge.log");
        let text = "2026-08-11 00:00:00 INFO Starting update check\r\n2026-08-11 00:00:01 INFO Done\r\n";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        fs::write(&f, &utf16le).unwrap();

        let r = read_window(&f, 1024, None);
        let err = r.expect_err("UTF-16LE content must be refused, not decoded as garbage");
        assert!(err.contains("UTF-16"), "error should name the encoding: {err}");
        assert!(!err.contains("too large"), "must be a distinct message from the old size-refusal");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bom_less_utf16be_content_is_refused_cleanly() {
        let d = scratch("utf16be");
        let f = d.join("be.log");
        let text = "hello log line one\r\nhello log line two\r\n";
        let utf16be: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        fs::write(&f, &utf16be).unwrap();

        let r = read_window(&f, 1024, None);
        let err = r.expect_err("UTF-16BE content must be refused, not decoded as garbage");
        assert!(err.contains("UTF-16"), "error should name the encoding: {err}");
        let _ = fs::remove_dir_all(&d);
    }
}
