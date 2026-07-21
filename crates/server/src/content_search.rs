//! Content search (CPE-416, epic CPE-662): recursively search text files under a folder for lines
//! containing a query. Bounded (match/file/byte caps, snippet cap), skips dot-dirs, symlinked dirs
//! (cycle-safe), and binary/oversized files. Pure and Tauri-free (CPE-815); the Tauri
//! `search_file_contents` command dispatches here.

use std::path::Path;

use serde::Serialize;

use crate::fsutil::entry_is_symlink;

/// One content-search hit: the file, the 1-based line number, and the (trimmed, truncated) line.
#[derive(Serialize)]
pub struct ContentMatch {
    pub path: String,
    pub line_number: u64,
    pub line: String,
}

/// The result of a content search: the hits, how many files were scanned, and whether a cap was hit.
#[derive(Serialize)]
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

/// Search text files under `root` for lines containing `query`. Skips dot-dirs, symlinked dirs, and
/// binary/oversized files; stops at match/file caps (reporting `truncated`); unreadable entries are
/// skipped. Empty/whitespace `query` returns nothing; a non-folder root is an `Err`.
pub fn search_file_contents(
    root: &str,
    query: &str,
    case_sensitive: bool,
) -> Result<ContentSearchResult, String> {
    let needle = if case_sensitive { query.to_string() } else { query.to_lowercase() };
    let mut result = ContentSearchResult { matches: Vec::new(), files_scanned: 0, truncated: false };
    if needle.trim().is_empty() {
        return Ok(result);
    }
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    // Explicit stack, not recursion — bounded memory, and matches `list_dir`'s skip-on-error ethos.
    let mut stack = vec![root_path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if result.matches.len() >= SEARCH_MAX_MATCHES || result.files_scanned >= SEARCH_MAX_FILES {
                result.truncated = true;
                return Ok(result);
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
            result.files_scanned += 1;
            let text = String::from_utf8_lossy(&bytes);
            let path_str = path.to_string_lossy().into_owned();
            for (i, line) in text.lines().enumerate() {
                let hay = if case_sensitive { line.to_string() } else { line.to_lowercase() };
                if hay.contains(&needle) {
                    let mut snippet = line.trim().to_string();
                    if snippet.chars().count() > SEARCH_SNIPPET_MAX {
                        snippet = snippet.chars().take(SEARCH_SNIPPET_MAX).collect::<String>() + "…";
                    }
                    result.matches.push(ContentMatch {
                        path: path_str.clone(),
                        line_number: (i + 1) as u64,
                        line: snippet,
                    });
                    if result.matches.len() >= SEARCH_MAX_MATCHES {
                        result.truncated = true;
                        return Ok(result);
                    }
                }
            }
        }
    }
    Ok(result)
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
}
