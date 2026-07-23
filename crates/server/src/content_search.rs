//! Content search (CPE-416, epic CPE-662): recursively search text files under a folder for lines
//! containing a query. Bounded (match/file/byte caps, snippet cap), skips dot-dirs, symlinked dirs
//! (cycle-safe), and binary/oversized files. Pure and Tauri-free (CPE-815); the Tauri
//! `search_file_contents` command dispatches here.

use std::path::Path;

use serde::Serialize;

use crate::fsutil::entry_is_symlink;

/// One content-search hit: the file, the 1-based line number, and the (trimmed, truncated) line.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContentMatch {
    pub path: String,
    pub line_number: u64,
    pub line: String,
}

/// The result of a content search: the hits, how many files were scanned, and whether a cap was hit.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContentSearchResult {
    pub matches: Vec<ContentMatch>,
    pub files_scanned: u64,
    pub truncated: bool,
}

// Bounds that keep a content search fast + predictable regardless of the tree it's pointed at.
const SEARCH_MAX_MATCHES: usize = 1000;
const SEARCH_MAX_FILES: u64 = 20_000;
const SEARCH_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const SEARCH_SNIPPET_MAX: usize = 400;

/// True if a byte slice looks binary (contains a NUL in the sniffed prefix) — skip such files.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// How many matches to buffer before flushing a batch to a streaming caller — small so hits appear live.
pub const CONTENT_SEARCH_BATCH: usize = 32;

/// The non-match part of a content search: files scanned + whether a cap truncated the walk.
pub struct ContentWalkStats {
    pub files_scanned: u64,
    pub truncated: bool,
}

/// The shared content-search walk (CPE-662): invoke `flush` with each batch of ≤`batch` [`ContentMatch`]es
/// as they're found; `flush` returns a `ControlFlow` so a streaming caller can stop early. Skips dot-dirs,
/// symlinked dirs (cycle-safe), and binary/oversized files; caps matches + files (reporting `truncated`);
/// unreadable entries are skipped. Empty/whitespace `query` yields nothing; a non-folder root is an `Err`.
/// Backs both [`search_file_contents`] (collect) and the streaming command.
pub fn stream_file_contents(
    root: &str,
    query: &str,
    case_sensitive: bool,
    batch: usize,
    mut flush: impl FnMut(Vec<ContentMatch>) -> std::ops::ControlFlow<()>,
) -> Result<ContentWalkStats, String> {
    let needle = if case_sensitive { query.to_string() } else { query.to_lowercase() };
    let mut stats = ContentWalkStats { files_scanned: 0, truncated: false };
    if needle.trim().is_empty() {
        return Ok(stats);
    }
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    let mut buf: Vec<ContentMatch> = Vec::new();
    let mut total_matches = 0usize;
    // Explicit stack, not recursion — bounded memory, and matches `list_dir`'s skip-on-error ethos.
    let mut stack = vec![root_path.to_path_buf()];
    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if total_matches >= SEARCH_MAX_MATCHES || stats.files_scanned >= SEARCH_MAX_FILES {
                stats.truncated = true;
                break 'walk;
            }
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip dot-dirs and symlinked dirs (avoid cycles, CPE-609).
                if !name.starts_with('.') && !entry_is_symlink(&entry) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() || meta.len() > SEARCH_MAX_FILE_BYTES {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            if looks_binary(&bytes) {
                continue;
            }
            stats.files_scanned += 1;
            let text = String::from_utf8_lossy(&bytes);
            let path_str = path.to_string_lossy().into_owned();
            for (i, line) in text.lines().enumerate() {
                let hay = if case_sensitive { line.to_string() } else { line.to_lowercase() };
                if hay.contains(&needle) {
                    let mut snippet = line.trim().to_string();
                    if snippet.chars().count() > SEARCH_SNIPPET_MAX {
                        snippet = snippet.chars().take(SEARCH_SNIPPET_MAX).collect::<String>() + "…";
                    }
                    buf.push(ContentMatch {
                        path: path_str.clone(),
                        line_number: (i + 1) as u64,
                        line: snippet,
                    });
                    total_matches += 1;
                    if buf.len() >= batch && flush(std::mem::take(&mut buf)).is_break() {
                        break 'walk;
                    }
                    if total_matches >= SEARCH_MAX_MATCHES {
                        stats.truncated = true;
                        break 'walk;
                    }
                }
            }
        }
    }
    if !buf.is_empty() {
        let _ = flush(buf);
    }
    Ok(stats)
}

/// Collect-to-vec content search: every match under `root` for `query`. See [`stream_file_contents`].
pub fn search_file_contents(
    root: &str,
    query: &str,
    case_sensitive: bool,
) -> Result<ContentSearchResult, String> {
    let mut matches = Vec::new();
    let stats = stream_file_contents(root, query, case_sensitive, usize::MAX, |b| {
        matches.extend(b);
        std::ops::ControlFlow::Continue(())
    })?;
    Ok(ContentSearchResult { matches, files_scanned: stats.files_scanned, truncated: stats.truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-content-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn content_search_matches_lines_and_skips_binary_and_dot_dirs() {
        let d = scratch("search");
        fs::write(d.join("a.txt"), b"hello world\nsecond line\nNEEDLE here\n").unwrap();
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub").join("b.md"), b"nothing\nfound the needle deep\n").unwrap();
        // A binary file with the text — must be skipped.
        fs::write(d.join("c.bin"), b"needle\x00binary").unwrap();
        // A dot-dir — must be skipped.
        fs::create_dir_all(d.join(".git")).unwrap();
        fs::write(d.join(".git").join("x"), b"needle in git").unwrap();

        let r = search_file_contents(&d.to_string_lossy(), "needle", false).unwrap();
        let paths: Vec<String> = r.matches.iter().map(|m| m.path.replace('\\', "/")).collect();
        // Case-insensitive: matches "NEEDLE" (a.txt) and "needle" (sub/b.md); NOT the binary or .git.
        assert_eq!(r.matches.len(), 2, "got {paths:?}");
        assert!(paths.iter().any(|p| p.ends_with("a.txt")));
        assert!(paths.iter().any(|p| p.ends_with("sub/b.md")));
        assert!(!paths.iter().any(|p| p.contains(".git") || p.ends_with("c.bin")));
        // Line numbers are 1-based.
        let a = r.matches.iter().find(|m| m.path.ends_with("a.txt")).unwrap();
        assert_eq!(a.line_number, 3);
        assert_eq!(a.line, "NEEDLE here");

        // Case-sensitive excludes the uppercase hit.
        let cs = search_file_contents(&d.to_string_lossy(), "needle", true).unwrap();
        assert_eq!(cs.matches.len(), 1);
        assert!(cs.matches[0].path.replace('\\', "/").ends_with("sub/b.md"));

        // Empty query and a non-folder root behave sanely.
        assert_eq!(search_file_contents(&d.to_string_lossy(), "  ", false).unwrap().matches.len(), 0);
        assert!(search_file_contents(&d.join("a.txt").to_string_lossy(), "x", false).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stream_file_contents_flushes_batches_and_stops_early() {
        let d = scratch("stream");
        for i in 0..10 {
            fs::write(d.join(format!("f{i}.txt")), b"has NEEDLE line\n").unwrap();
        }
        // Batch size 1 → a flush per match; all ten stream through, no truncation.
        let (mut batches, mut total) = (0, 0);
        let stats = stream_file_contents(&d.to_string_lossy(), "needle", false, 1, |b| {
            batches += 1;
            total += b.len();
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(total, 10, "one match per file");
        assert!(batches >= 10);
        assert!(!stats.truncated);

        // A `Break` from `flush` stops the walk at the batch boundary (cancellation).
        let mut seen = 0;
        stream_file_contents(&d.to_string_lossy(), "needle", false, 1, |b| {
            seen += b.len();
            std::ops::ControlFlow::Break(())
        })
        .unwrap();
        assert_eq!(seen, 1, "Break after the first batch stops early");
        let _ = fs::remove_dir_all(&d);
    }
}
