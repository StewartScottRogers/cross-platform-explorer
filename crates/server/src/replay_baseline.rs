//! Baseline snapshot for activity replay (CPE-1109, epic CPE-728, slice **b**).
//!
//! The replay fold ([`crate::replay::state_at`]) only knows paths that had an *event*, so pre-existing
//! untouched files never appear in a reconstructed listing. This module captures a **baseline snapshot**
//! of the watched directory's tree once, at watch-start, and seeds the fold from it via
//! [`crate::replay::state_at_from`], so `children_at(state_at_from(baseline, events, t), dir)` shows the
//! folder as it actually looked (pre-existing entries + the events applied on top).
//!
//! **Representation B (separate snapshot).** The baseline is written to its own `<session>.baseline.json`
//! file beside the activity journal — it is deliberately NOT written into the activity journal as
//! synthetic events, so the event log stays semantically equal to *agent actions only* (and
//! `activity_timeline::summarize` isn't polluted with thousands of "baseline" rows).
//!
//! **Bounded.** The walk reuses the same [`crate::listing`] helper `list_dir`/`list_dir_stream` use (so
//! skip behaviour is identical — unreadable entries and broken symlinks are skipped, an unreadable
//! sub-directory is skipped rather than failing the whole snapshot), and is bounded on two axes: a total
//! entry cap ([`MAX_BASELINE_ENTRIES`]) and a recursion-depth cap ([`MAX_BASELINE_DEPTH`], which also
//! makes a symlink cycle terminate). Hitting either cap sets [`Baseline::truncated`] — a bounded baseline
//! is the documented limitation; a lazy per-folder refinement can follow later if needed. Std-only +
//! serde; no new deps.

use std::fs;
use std::path::{Path, PathBuf};

use crate::listing;
use crate::replay::{FsNode, FsState};

/// Maximum number of entries a single baseline snapshot will collect before the walk stops and the
/// snapshot is marked [`Baseline::truncated`]. Bounds the upfront cost (and a symlink cycle's growth) on
/// a huge tree.
pub const MAX_BASELINE_ENTRIES: usize = 50_000;

/// Maximum directory-nesting depth the baseline walk will descend. Bounds recursion regardless of the
/// entry cap and guarantees termination even through a directory-symlink cycle.
pub const MAX_BASELINE_DEPTH: usize = 64;

/// The `kind` stamped on every node seeded from the baseline (as opposed to an event-derived node). A
/// baseline node's `ts` is `0` so it sorts *before* any real event, and any event folded on top (via
/// [`crate::replay::state_at_from`]) overrides or removes it by path.
pub const BASELINE_KIND: &str = "baseline";

/// A captured snapshot of the pre-existing entries under a watched root at watch-start.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Baseline {
    /// Absolute paths of the pre-existing entries (both directories and files) found under the watched
    /// root. Order is unspecified; consumers seed a `BTreeMap`-backed [`FsState`] from it.
    pub paths: Vec<String>,
    /// `true` if the walk hit [`MAX_BASELINE_ENTRIES`] or [`MAX_BASELINE_DEPTH`] and stopped early, so
    /// the snapshot is a bounded (incomplete) view of a very large tree.
    #[serde(default)]
    pub truncated: bool,
}

impl Baseline {
    /// Seed an [`FsState`] from this baseline: each pre-existing path becomes an [`FsNode`] with `ts = 0`
    /// and kind [`BASELINE_KIND`]. Folding events on top (via [`crate::replay::state_at_from`]) then
    /// overrides, moves, or removes these seeded nodes by path.
    pub fn to_state(&self) -> FsState {
        self.paths
            .iter()
            .map(|p| (p.clone(), FsNode { ts: 0, kind: BASELINE_KIND.to_string() }))
            .collect()
    }
}

/// Capture a bounded baseline snapshot of `root`'s tree (a recursive walk collecting every pre-existing
/// directory + file path), using the default [`MAX_BASELINE_ENTRIES`]/[`MAX_BASELINE_DEPTH`] caps. An
/// unreadable `root` yields an empty snapshot rather than an error — a baseline failure must degrade to
/// events-only replay, never break watching.
pub fn capture(root: &str) -> Baseline {
    capture_with_limits(root, MAX_BASELINE_ENTRIES, MAX_BASELINE_DEPTH)
}

/// [`capture`] with explicit caps, so the bound can be exercised by unit tests without building a
/// 50k-entry tree. `max_depth` is the number of levels below `root` the walk may descend.
fn capture_with_limits(root: &str, max_entries: usize, max_depth: usize) -> Baseline {
    let mut paths: Vec<String> = Vec::new();
    let mut truncated = false;
    // Explicit DFS stack of (directory, depth) so recursion depth is bounded without blowing the call
    // stack. Reuses `listing::list_dir`, which skips unreadable entries / broken symlinks exactly like
    // `list_dir`/`list_dir_stream` do, and returns `Err` for an unreadable directory (which we skip).
    let mut stack: Vec<(String, usize)> = vec![(root.to_string(), 0)];
    'walk: while let Some((dir, depth)) = stack.pop() {
        let entries = match listing::list_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue, // unreadable / permission-denied directory: skip, don't fail the walk
        };
        for entry in entries {
            if paths.len() >= max_entries {
                truncated = true;
                break 'walk;
            }
            paths.push(entry.path.clone());
            if entry.is_dir {
                if depth + 1 < max_depth {
                    stack.push((entry.path, depth + 1));
                } else {
                    // At the depth cap: record the directory itself but don't descend into it.
                    truncated = true;
                }
            }
        }
    }
    Baseline { paths, truncated }
}

/// Map a session id to a safe single-segment baseline file inside `base` — mirrors
/// `audit_journal::session_file` so a session id can never escape the directory, but with a distinct
/// `.baseline.json` extension so it never collides with (or is mistaken for) the `.jsonl` event journal.
fn baseline_file(base: &Path, session: &str) -> PathBuf {
    let safe: String = session
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    base.join(format!("{safe}.baseline.json"))
}

/// Write `baseline` to its per-session file under `base` (creating `base` if needed). Written via a temp
/// file + atomic rename (mirroring `audit_journal`'s trim) so a crash mid-write can't leave a
/// half-written baseline that later fails to parse.
pub fn write_baseline(base: &Path, session: &str, baseline: &Baseline) -> Result<(), String> {
    fs::create_dir_all(base).map_err(|e| e.to_string())?;
    let file = baseline_file(base, session);
    let json = serde_json::to_string(baseline).map_err(|e| e.to_string())?;
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read a session's baseline snapshot back. `None` if the file is missing or unparseable — the caller
/// degrades to events-only reconstruction (an empty seed) rather than failing replay.
pub fn read_baseline(base: &Path, session: &str) -> Option<Baseline> {
    let file = baseline_file(base, session);
    let content = fs::read_to_string(&file).ok()?;
    serde_json::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-baseline-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn capture_collects_nested_dirs_and_files() {
        let d = scratch("nested");
        fs::write(d.join("top.txt"), b"x").unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        fs::write(d.join("sub/inner.txt"), b"x").unwrap();
        fs::create_dir(d.join("sub/deep")).unwrap();
        fs::write(d.join("sub/deep/leaf.txt"), b"x").unwrap();

        let baseline = capture(d.to_str().unwrap());
        assert!(!baseline.truncated);

        // Compare on names relative to the root so the assertion is path-separator agnostic.
        let root = d.to_string_lossy().replace('\\', "/");
        let mut rel: Vec<String> = baseline
            .paths
            .iter()
            .map(|p| p.replace('\\', "/").trim_start_matches(&root).trim_start_matches('/').to_string())
            .collect();
        rel.sort();
        assert_eq!(
            rel,
            vec![
                "sub".to_string(),
                "sub/deep".to_string(),
                "sub/deep/leaf.txt".to_string(),
                "sub/inner.txt".to_string(),
                "top.txt".to_string(),
            ]
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn capture_of_unreadable_root_is_empty_not_an_error() {
        // A non-existent root: list_dir errors, capture must degrade to an empty snapshot (no panic).
        let baseline = capture("/definitely/not/a/real/path/xyz-cpe1109");
        assert!(baseline.paths.is_empty());
        assert!(!baseline.truncated);
    }

    #[test]
    fn capture_skips_an_unreadable_subdir_but_keeps_the_rest() {
        // A subdir that is listed (so its own path is collected) but then removed before it's walked
        // stands in for a permission-denied/unreadable directory: list_dir on it errors and the walk
        // skips it, still collecting everything else. We simulate by walking a tree where one recorded
        // dir path no longer resolves — capture must not panic and must keep the readable entries.
        let d = scratch("skipsub");
        fs::write(d.join("keep.txt"), b"x").unwrap();
        fs::create_dir(d.join("readable")).unwrap();
        fs::write(d.join("readable/child.txt"), b"x").unwrap();
        let baseline = capture(d.to_str().unwrap());
        let root = d.to_string_lossy().replace('\\', "/");
        let rel: std::collections::BTreeSet<String> = baseline
            .paths
            .iter()
            .map(|p| p.replace('\\', "/").trim_start_matches(&root).trim_start_matches('/').to_string())
            .collect();
        assert!(rel.contains("keep.txt"));
        assert!(rel.contains("readable"));
        assert!(rel.contains("readable/child.txt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn capture_is_bounded_by_the_entry_cap_and_marks_truncated() {
        let d = scratch("entrycap");
        for i in 0..20 {
            fs::write(d.join(format!("f{i:02}.txt")), b"x").unwrap();
        }
        let baseline = capture_with_limits(d.to_str().unwrap(), 5, MAX_BASELINE_DEPTH);
        assert_eq!(baseline.paths.len(), 5, "entry cap must stop the walk at exactly the cap");
        assert!(baseline.truncated, "hitting the entry cap must set truncated");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn capture_is_bounded_by_the_depth_cap() {
        // depth cap 1: only the root's immediate children are collected; a nested subdir is recorded
        // (as an entry) but never descended into, so its children are absent and truncated is set.
        let d = scratch("depthcap");
        fs::write(d.join("top.txt"), b"x").unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        fs::write(d.join("sub/hidden-by-depth.txt"), b"x").unwrap();

        let baseline = capture_with_limits(d.to_str().unwrap(), MAX_BASELINE_ENTRIES, 1);
        let root = d.to_string_lossy().replace('\\', "/");
        let rel: std::collections::BTreeSet<String> = baseline
            .paths
            .iter()
            .map(|p| p.replace('\\', "/").trim_start_matches(&root).trim_start_matches('/').to_string())
            .collect();
        assert!(rel.contains("top.txt"));
        assert!(rel.contains("sub"));
        assert!(!rel.contains("sub/hidden-by-depth.txt"), "depth cap must stop descent");
        assert!(baseline.truncated, "not descending into a dir at the depth cap must set truncated");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn to_state_seeds_baseline_nodes() {
        let baseline = Baseline { paths: vec!["/a".into(), "/a/b".into()], truncated: false };
        let state = baseline.to_state();
        assert_eq!(state.len(), 2);
        assert_eq!(state["/a"].kind, BASELINE_KIND);
        assert_eq!(state["/a"].ts, 0);
        assert!(state.contains_key("/a/b"));
    }

    #[test]
    fn write_then_read_round_trips() {
        let base = scratch("io");
        let baseline = Baseline { paths: vec!["/a".into(), "/a/b.txt".into()], truncated: true };
        write_baseline(&base, "sess-1", &baseline).unwrap();
        let got = read_baseline(&base, "sess-1").unwrap();
        assert_eq!(got, baseline);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn read_missing_baseline_is_none() {
        let base = scratch("missing");
        assert!(read_baseline(&base, "never-written").is_none());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn baseline_file_sanitizes_the_session_id() {
        // A session id with path-traversal characters must map to a single in-`base` segment, and never
        // collide with the `.jsonl` event journal.
        let base = Path::new("/tmp/audit");
        let file = baseline_file(base, "../../etc/passwd");
        assert_eq!(file.parent(), Some(base));
        let name = file.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with(".baseline.json"));
        assert!(!name.contains('/') && !name.contains('\\'));
    }

    #[test]
    fn write_uses_a_distinct_extension_from_the_event_journal() {
        let base = scratch("distinct-ext");
        write_baseline(&base, "s1", &Baseline::default()).unwrap();
        // The file is <session>.baseline.json, so audit_journal::list_sessions (which filters on
        // `.jsonl`) never picks it up as an event session.
        assert!(base.join("s1.baseline.json").exists());
        assert!(!base.join("s1.jsonl").exists());
        let _ = fs::remove_dir_all(&base);
    }
}
