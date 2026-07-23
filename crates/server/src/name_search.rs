//! Filename search (CPE-603/697/666): find files and folders whose *name* matches a query — a plain
//! substring, or an anchored glob with `*`/`?` and bash-style `{a,b}` brace groups. Pure and Tauri-free
//! (CPE-815): the shared [`walk_name_matches`] walker takes a `flush` callback returning `ControlFlow`,
//! so the collect command ([`find_files_by_name`]) and the streaming command both drive it — the app's
//! streaming command supplies a callback that sends batches over its `ipc::Channel`, keeping the
//! transport in the adapter while the walk lives here.

use std::path::Path;

use serde::Serialize;

use crate::fsutil::entry_is_symlink;

/// One filename-search hit: the full path, the bare name, and whether it's a folder.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct NameMatch {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

/// The result of a filename search: the hits, how many directories were walked, and whether a cap was
/// hit (so the UI can say "showing the first results").
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct NameSearchResult {
    pub matches: Vec<NameMatch>,
    pub dirs_scanned: u64,
    pub truncated: bool,
}

const NAME_SEARCH_MAX_MATCHES: usize = 2000;
const NAME_SEARCH_MAX_DIRS: u64 = 50_000;

/// How many matches to buffer before flushing a batch over the channel — small so hits appear live.
pub const NAME_SEARCH_BATCH: usize = 32;

/// Anchored wildcard match: `*` matches any run of characters, `?` exactly one. Both `name` and
/// `pattern` are assumed already lowercased. Iterative two-pointer backtracking — no regex dependency.
fn glob_is_match(name: &str, pattern: &str) -> bool {
    let n: Vec<char> = name.chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while i < n.len() {
        if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == '*' {
            star = Some(j);
            mark = i;
            j += 1;
        } else if let Some(s) = star {
            j = s + 1;
            mark += 1;
            i = mark;
        } else {
            return false;
        }
    }
    while j < p.len() && p[j] == '*' {
        j += 1;
    }
    j == p.len()
}

/// Safety cap on the number of patterns one query's brace expansion may produce.
const BRACE_EXPANSION_CAP: usize = 1024;

/// Expand bash-style brace groups `{a,b}` in a glob into the flat list of concrete globs they denote
/// (CPE-697): `*.{jpg,png}` → `["*.jpg","*.png"]`; multiple/nested groups expand as a cartesian product.
fn expand_braces(pattern: &str) -> Vec<String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    expand_braces_into(&chars, &mut out);
    out
}

fn expand_braces_into(chars: &[char], out: &mut Vec<String>) {
    if out.len() > BRACE_EXPANSION_CAP {
        return;
    }
    match first_brace_group(chars) {
        Some((open, close, alts)) => {
            for alt in alts {
                let mut combined: Vec<char> = Vec::with_capacity(chars.len());
                combined.extend_from_slice(&chars[..open]);
                combined.extend(alt);
                combined.extend_from_slice(&chars[close + 1..]);
                expand_braces_into(&combined, out);
                if out.len() > BRACE_EXPANSION_CAP {
                    return;
                }
            }
        }
        None => out.push(chars.iter().collect()),
    }
}

/// Find the first real brace group in `chars`: a `{` with a matching `}` and at least one top-level
/// comma. Returns `(open_index, close_index, alternatives)`. A `{` that is unmatched or has no top-level
/// comma is skipped (treated as a literal).
fn first_brace_group(chars: &[char]) -> Option<(usize, usize, Vec<Vec<char>>)> {
    for open in 0..chars.len() {
        if chars[open] != '{' {
            continue;
        }
        let mut depth = 0usize;
        let mut start = open + 1;
        let mut alts: Vec<Vec<char>> = Vec::new();
        let mut saw_top_comma = false;
        for i in open..chars.len() {
            match chars[i] {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        alts.push(chars[start..i].to_vec());
                        if saw_top_comma {
                            return Some((open, i, alts));
                        }
                        break; // comma-less group → literal; resume scanning after this `{`
                    }
                }
                ',' if depth == 1 => {
                    alts.push(chars[start..i].to_vec());
                    start = i + 1;
                    saw_top_comma = true;
                }
                _ => {}
            }
        }
    }
    None
}

/// Case-insensitive name match: a query containing `*`/`?` or a brace group `{a,b}` is an anchored glob
/// over the whole name; otherwise a plain substring. `query_lower` must already be lowercased. Shared
/// with the instant-search query core ([`crate::index_query`], CPE-831) so folder search and instant
/// search use one matching semantics.
pub fn name_matches(name: &str, query_lower: &str) -> bool {
    if query_lower.is_empty() {
        return false;
    }
    let n = name.to_lowercase();
    let has_group = query_lower.contains('{')
        && first_brace_group(&query_lower.chars().collect::<Vec<_>>()).is_some();
    if query_lower.contains('*') || query_lower.contains('?') || has_group {
        expand_braces(query_lower).iter().any(|p| glob_is_match(&n, p))
    } else {
        n.contains(query_lower)
    }
}

/// Directories scanned + whether a cap truncated the walk — the non-match part of a name search.
pub struct NameWalkStats {
    pub dirs_scanned: u64,
    pub truncated: bool,
}

/// The shared filename-search walk behind both the collect and streaming commands. Invokes `flush` with
/// each batch of ≤`batch` matches as they're found; `flush` returns `ControlFlow` so a streaming caller
/// can stop early. Skips dot-dirs + symlinks, caps dirs/matches (reporting `truncated`), skips unreadable
/// dirs. Empty query yields nothing; a non-folder root is an `Err`.
pub fn walk_name_matches(
    root: &str,
    query: &str,
    batch: usize,
    mut flush: impl FnMut(Vec<NameMatch>) -> std::ops::ControlFlow<()>,
) -> Result<NameWalkStats, String> {
    let q = query.trim().to_lowercase();
    let mut stats = NameWalkStats { dirs_scanned: 0, truncated: false };
    if q.is_empty() {
        return Ok(stats);
    }
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    let mut buf: Vec<NameMatch> = Vec::new();
    let mut total_matches = 0usize;
    // Explicit stack, not recursion — bounded memory, and matches `list_dir`'s skip-on-error ethos.
    let mut stack = vec![root_path.to_path_buf()];
    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        stats.dirs_scanned += 1;
        if stats.dirs_scanned > NAME_SEARCH_MAX_DIRS {
            stats.truncated = true;
            break;
        }
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(meta) = entry.metadata() else { continue };
            let is_dir = meta.is_dir();
            if name_matches(&name, &q) {
                buf.push(NameMatch {
                    path: path.to_string_lossy().into_owned(),
                    name: name.to_string(),
                    is_dir,
                });
                total_matches += 1;
                if buf.len() >= batch && flush(std::mem::take(&mut buf)).is_break() {
                    break 'walk;
                }
                if total_matches >= NAME_SEARCH_MAX_MATCHES {
                    stats.truncated = true;
                    break 'walk;
                }
            }
            // Descend into real sub-directories, skipping dot-dirs and symlinks (avoid cycles).
            if is_dir && !name.starts_with('.') && !entry_is_symlink(&entry) {
                stack.push(path);
            }
        }
    }
    if !buf.is_empty() {
        let _ = flush(buf);
    }
    Ok(stats)
}

/// Collect-to-vec filename search: walk `root` for names matching `query` and return every hit.
pub fn find_files_by_name(root: &str, query: &str) -> Result<NameSearchResult, String> {
    let mut matches = Vec::new();
    let stats = walk_name_matches(root, query, usize::MAX, |b| {
        matches.extend(b);
        std::ops::ControlFlow::Continue(())
    })?;
    Ok(NameSearchResult { matches, dirs_scanned: stats.dirs_scanned, truncated: stats.truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-namesearch-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn name_matches_does_substring_and_glob() {
        // Plain query = case-insensitive substring.
        assert!(name_matches("Report.pdf", "report"));
        assert!(name_matches("Report.pdf", "port"));
        assert!(!name_matches("Report.pdf", "xls"));
        // Glob = anchored over the whole name.
        assert!(name_matches("photo.png", "*.png"));
        assert!(!name_matches("photo.pngx", "*.png")); // anchored: trailing chars fail
        assert!(name_matches("a1b2.txt", "a?b?.txt"));
        assert!(!name_matches("ab.txt", "a?b?.txt")); // '?' needs exactly one char
        assert!(name_matches("anything", "*")); // lone star matches all
        assert!(name_matches("IMG_2024.jpg", "img_*.jpg")); // case-insensitive glob
        assert!(!name_matches("x", "")); // empty query matches nothing
    }

    #[test]
    fn name_matches_expands_brace_groups() {
        // A {a,b} group matches any of its alternatives (CPE-697). Queries are pre-lowercased.
        assert!(name_matches("photo.jpg", "*.{jpg,png}"));
        assert!(name_matches("photo.png", "*.{jpg,png}"));
        assert!(!name_matches("photo.gif", "*.{jpg,png}"));
        assert!(name_matches("photo.gif", "*.{jpg,png,gif}"));
        assert!(name_matches("PHOTO.JPG", "*.{jpg,png}")); // case-insensitive
        // `*`/`?` work inside a group.
        assert!(name_matches("archive.tar.gz", "*.{tar.*,zip}"));
        assert!(name_matches("data.zip", "*.{tar.*,zip}"));
        assert!(name_matches("report1.md", "report{?,10}.md"));
        assert!(name_matches("report10.md", "report{?,10}.md"));
        assert!(!name_matches("reportxx.md", "report{?,10}.md"));
        // Multiple + nested groups expand as a cartesian product.
        assert!(name_matches("img.jpg", "{img,pic}.{jpg,png}"));
        assert!(name_matches("pic.png", "{img,pic}.{jpg,png}"));
        assert!(!name_matches("doc.jpg", "{img,pic}.{jpg,png}"));
        assert!(name_matches("a1.txt", "{a{1,2},b}.txt"));
        assert!(name_matches("b.txt", "{a{1,2},b}.txt"));
        assert!(!name_matches("a3.txt", "{a{1,2},b}.txt"));
    }

    #[test]
    fn name_matches_treats_lone_braces_and_commas_literally() {
        assert!(name_matches("{x}.txt", "{x}.txt")); // no top-level comma → literal
        assert!(!name_matches("x.txt", "{x}.txt"));
        assert!(name_matches("a{b.txt", "a{b")); // unmatched brace → literal
        assert!(!name_matches("axb.txt", "a{b"));
        assert!(name_matches("a,b.txt", "a,b*")); // comma outside a group is literal
        assert!(!name_matches("ab.txt", "a,b*"));
        assert!(glob_is_match("a.jpg", "?.jpg")); // direct glob smoke-check
    }

    #[test]
    fn expand_braces_produces_expected_pattern_lists() {
        assert_eq!(expand_braces("*.{jpg,png}"), vec!["*.jpg", "*.png"]);
        assert_eq!(expand_braces("{a,b}.{x,y}"), vec!["a.x", "a.y", "b.x", "b.y"]);
        assert_eq!(expand_braces("*.txt"), vec!["*.txt"]);
        assert_eq!(expand_braces("{x}.txt"), vec!["{x}.txt"]); // comma-less → literal
    }

    #[test]
    fn find_files_by_name_walks_recursively_and_skips_dot_dirs() {
        let d = scratch("name");
        fs::create_dir_all(d.join(".git")).unwrap();
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("report.txt"), b"x").unwrap();
        fs::write(d.join("sub/report_final.txt"), b"x").unwrap();
        fs::write(d.join(".git/report_ignored.txt"), b"x").unwrap();
        let r = find_files_by_name(&d.to_string_lossy(), "report").unwrap();
        assert!(r.matches.iter().any(|m| m.name == "report.txt"));
        assert!(r.matches.iter().any(|m| m.name == "report_final.txt"));
        assert!(!r.matches.iter().any(|m| m.name == "report_ignored.txt"), "dot-dir skipped");
        // Empty query yields nothing; a file root errors.
        assert!(find_files_by_name(&d.to_string_lossy(), "  ").unwrap().matches.is_empty());
        assert!(find_files_by_name(&d.join("report.txt").to_string_lossy(), "x").is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn walker_flushes_in_batches() {
        let d = scratch("batch");
        for i in 0..5 {
            fs::write(d.join(format!("report{i}.txt")), b"x").unwrap();
        }
        let mut batches = 0;
        let stats = walk_name_matches(&d.to_string_lossy(), "report", 1, |b| {
            assert!(!b.is_empty());
            batches += 1;
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        assert!(batches >= 5); // one per match at batch size 1
        assert!(!stats.truncated);
        let _ = fs::remove_dir_all(&d);
    }
}
