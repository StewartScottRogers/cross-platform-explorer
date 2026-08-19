//! Text statistics (CPE-414): line / word / character / byte counts for a text file. Pure and
//! Tauri-free (CPE-815); the Tauri `text_stats` command is a thin `spawn_blocking` dispatcher.

use std::fs;
use std::path::Path;

use serde::Serialize;

/// Counts for a text file. Serialized to match the frontend `TextStats`.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TextStats {
    lines: u64,
    words: u64,
    chars: u64,
    bytes: u64,
}

/// Largest file the text-stats command will read into memory (keeps it fast/predictable).
pub const TEXT_STATS_MAX_BYTES: u64 = 25 * 1024 * 1024;

/// Compute line/word/char/byte counts for `path`. Lines follow `str::lines`; words are
/// whitespace-separated; chars are Unicode scalar values. A directory, an over-cap file, or a
/// non-UTF-8 file is an `Err` (never a panic).
///
/// "I could not read it" and "I read it and it isn't text" are **different answers** (CPE-1678). The
/// read and the UTF-8 decode are therefore two separate steps: an I/O failure (permission denied, a
/// vanished file, a dead network mount) reports the OS's own cause, and only bytes that were actually
/// read and then failed to decode get the content-shaped "not a text file" verdict. Collapsing both
/// into the latter — what this used to do with `read_to_string(..).map_err(|_| "not a text file")` —
/// sent a user with a permissions problem to inspect their file's contents.
pub fn compute(path: &str) -> Result<TextStats, String> {
    let p = Path::new(path);
    let meta = fs::metadata(p).map_err(|e| format!("{path}: {e}"))?;
    if meta.is_dir() {
        return Err(format!("{path}: is a folder"));
    }
    if meta.len() > TEXT_STATS_MAX_BYTES {
        return Err("file is too large to analyze (25 MB limit)".into());
    }
    // Read first (I/O errors keep their real cause) ...
    let bytes = fs::read(p).map_err(|e| format!("{path}: could not be read: {e}"))?;
    // ... then decode (only a decode failure means "not text"). `from_utf8` takes the `Vec` by value,
    // so this is the same single allocation `read_to_string` made — no extra copy.
    let content = String::from_utf8(bytes).map_err(|_| format!("{path}: not a text file"))?;
    Ok(TextStats {
        lines: content.lines().count() as u64,
        words: content.split_whitespace().count() as u64,
        chars: content.chars().count() as u64,
        bytes: content.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir("cpe-textstats")
    }

    #[test]
    fn counts_lines_words_chars_bytes() {
        let d = scratch();
        // 2 lines, 3 words, 16 chars (incl. 2 newlines), 16 bytes (all ASCII).
        fs::write(d.join("t.txt"), b"hello world\nfoo\n").unwrap();
        let s = compute(&d.join("t.txt").to_string_lossy()).unwrap();
        assert_eq!((s.lines, s.words, s.chars, s.bytes), (2, 3, 16, 16));
        // A final unterminated line still counts (str::lines semantics).
        fs::write(d.join("u.txt"), b"a\nb").unwrap();
        assert_eq!(compute(&d.join("u.txt").to_string_lossy()).unwrap().lines, 2);
        // A multi-byte char makes chars < bytes.
        fs::write(d.join("m.txt"), "líne".as_bytes()).unwrap();
        let m = compute(&d.join("m.txt").to_string_lossy()).unwrap();
        assert!(m.chars < m.bytes);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn binary_and_directory_are_errors() {
        let d = scratch();
        // Non-UTF-8 (binary) and a folder are errors, not panics.
        fs::write(d.join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        let Err(e) = compute(&d.join("bin").to_string_lossy()) else {
            panic!("a non-UTF-8 file must be an error")
        };
        // CPE-1678: bytes that were read and failed to decode keep the content-shaped verdict — that
        // is the honest case, and splitting the read from the decode must not regress it.
        assert!(e.contains("not a text file"), "got {e}");
        assert!(compute(&d.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
