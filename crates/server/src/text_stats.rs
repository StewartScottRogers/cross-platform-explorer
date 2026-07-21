//! Text statistics (CPE-414): line / word / character / byte counts for a text file. Pure and
//! Tauri-free (CPE-815); the Tauri `text_stats` command is a thin `spawn_blocking` dispatcher.

use std::fs;
use std::path::Path;

use serde::Serialize;

/// Counts for a text file. Serialized to match the frontend `TextStats`.
#[derive(Serialize)]
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
pub fn compute(path: &str) -> Result<TextStats, String> {
    let p = Path::new(path);
    let meta = fs::metadata(p).map_err(|e| format!("{path}: {e}"))?;
    if meta.is_dir() {
        return Err(format!("{path}: is a folder"));
    }
    if meta.len() > TEXT_STATS_MAX_BYTES {
        return Err("file is too large to analyze (25 MB limit)".into());
    }
    let content = fs::read_to_string(p).map_err(|_| format!("{path}: not a text file"))?;
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

    fn scratch() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-textstats-{}-{}", std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
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
        assert!(compute(&d.join("bin").to_string_lossy()).is_err());
        assert!(compute(&d.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
