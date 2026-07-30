//! Live index watch: pure event → mutation mapping (CPE-1138, epic CPE-703).
//!
//! CPE-1137's [`crate::index_service::IndexService`] holds the resident per-volume indices; CPE-833 gave
//! [`crate::index::Index`] the incremental primitives (`apply_create`/`apply_remove`/`apply_rename`). This
//! module is the **pure** half of the wiring between a live OS filesystem watcher and those primitives: it
//! owns ONLY the translation from a normalized [`WatchEvent`] into the [`IndexMutation`]s that
//! [`crate::index_service::IndexService::apply_mutations`] applies (that method lives on `IndexService`
//! itself, in `index_service.rs`, since it needs the resident-map field this module doesn't have access
//! to — but it's the same batch/lock/save story this module's doc block describes). This module has
//! **zero dependency on `notify`** — the real OS subscription (starting/stopping a
//! `notify::RecommendedWatcher`, correlating rename-cookie pairs, debouncing a burst into one flush) is
//! the thin app adapter that lives in `src-tauri` (mirrors `FolderWatchState`/`AgentWatchState`). Keeping
//! the mapping notify-free is what makes it unit-testable headlessly with synthetic events, per the
//! ticket's Design.
//!
//! ## Rename fidelity (mirrors the Agent Watch ceiling, CPE-1117)
//! A `notify` rename sometimes arrives as one paired event (`from`+`to` known) and sometimes as two
//! separate, unpaired events (a lone source or a lone destination — see `notify`'s `RenameMode`). The app
//! adapter resolves cookie-correlated pairs into [`WatchEvent::Renamed`]; anything it can't pair degrades
//! to [`WatchEvent::Created`]/[`WatchEvent::Removed`] at the one path it does know (the ticket's
//! "rename-as-remove+create" fallback) — no crash, no fabricated pair.
//!
//! Feature-gated behind `index` (same gate as [`crate::index`] / [`crate::index_service`]): the plain
//! `cpe-server` build compiles zero watch code.

/// One coalesced filesystem change, already resolved to *what happened* — cookie correlation and
/// from/to pairing happen in the app adapter before [`plan_from_event`] is called, so this type carries
/// no `notify` types and this module has no `notify` dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// A file or directory came into existence at `path`.
    Created { path: String, is_dir: bool },
    /// A path (file or directory subtree) stopped existing.
    Removed { path: String },
    /// A path's *contents* changed but its identity (name/location) didn't. The filename index has
    /// nothing to update for this — [`plan_from_event`] maps it to no mutations. Kept as its own variant
    /// (rather than being filtered out before reaching this module) so the adapter can hand over every
    /// `notify` event uniformly and let the pure mapping decide what matters.
    Modified { path: String },
    /// A resolved rename/move: `from` → `to`.
    Renamed { from: String, to: String },
}

/// One primitive mutation to apply to a resident [`crate::index::Index`] — the pure output of
/// [`plan_from_event`], mapping 1:1 onto `Index::apply_create`/`apply_remove`/`apply_rename`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexMutation {
    Create { path: String, is_dir: bool },
    Remove { path: String },
    Rename { from: String, to: String },
}

/// Translate one normalized [`WatchEvent`] into zero or more [`IndexMutation`]s. `Modified` always maps
/// to nothing (the filename index doesn't track content); every other variant maps 1:1 to its
/// `Index::apply_*` primitive.
pub fn plan_from_event(event: &WatchEvent) -> Vec<IndexMutation> {
    match event {
        WatchEvent::Created { path, is_dir } => vec![IndexMutation::Create {
            path: path.clone(),
            is_dir: *is_dir,
        }],
        WatchEvent::Removed { path } => vec![IndexMutation::Remove { path: path.clone() }],
        WatchEvent::Modified { .. } => Vec::new(),
        WatchEvent::Renamed { from, to } => vec![IndexMutation::Rename {
            from: from.clone(),
            to: to.clone(),
        }],
    }
}

/// Translate a whole batch (e.g. one debounce-window flush) in event order, concatenating each event's
/// mutations. The app adapter's flush hands the resulting `Vec` to
/// [`crate::index_service::IndexService::apply_mutations`] as a single batch, so a burst of OS events
/// becomes one lock acquisition + one debounced save instead of one of each per event.
pub fn plan_from_events(events: &[WatchEvent]) -> Vec<IndexMutation> {
    events.iter().flat_map(plan_from_event).collect()
}

/// Resolve a debounce window's re-stat set (paths whose create/remove identity is decided by their
/// *current* existence, not the individual event kind) into ordered [`WatchEvent`]s.
///
/// **Ordering matters (CPE-1138 review):** the returned `Created` events are sorted so that an **ancestor
/// path always precedes its descendants** — a parent directory before any file inside it. This is required
/// because [`crate::index::Index::apply_create`] resolves a path's parent and *silently drops* the entry if
/// the parent isn't indexed yet; when a directory and files within it are created inside the same window
/// (archive extraction, `git checkout`, `cp -r`), applying a child before its parent would lose the child
/// until a manual rebuild. A lexicographic sort gives this ordering for free: an ancestor path is a prefix
/// of its descendants and so always sorts first (on both `/` and `\` separators).
///
/// `stat(path)` reports the path's current state: `Some(is_dir)` if it exists (→ `Created`), `None` if it's
/// gone (→ `Removed`). Removes are order-independent (`apply_remove` tombstones the whole subtree), so they
/// ride along in the same sorted pass.
pub fn resolve_touched(touched: &[String], stat: impl Fn(&str) -> Option<bool>) -> Vec<WatchEvent> {
    let mut paths: Vec<&str> = touched.iter().map(String::as_str).collect();
    paths.sort_unstable();
    paths
        .into_iter()
        .map(|p| match stat(p) {
            Some(is_dir) => WatchEvent::Created { path: p.to_string(), is_dir },
            None => WatchEvent::Removed { path: p.to_string() },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_service::IndexService;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-idxwatch-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// A small tree to crawl: report.rs / report.md across two folders + a README.
    fn sample_tree(root: &Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("README.md"), b"x").unwrap();
        fs::write(root.join("src/main.rs"), b"x").unwrap();
        fs::write(root.join("src/report.rs"), b"x").unwrap();
        fs::write(root.join("docs/report.md"), b"x").unwrap();
    }

    fn names(hits: &[crate::index::IndexHit]) -> Vec<String> {
        let mut v: Vec<String> = hits.iter().map(|h| h.name.clone()).collect();
        v.sort();
        v
    }

    fn abs(root: &Path, rel: &[&str]) -> String {
        let mut p = root.to_path_buf();
        for c in rel {
            p.push(c);
        }
        p.to_string_lossy().into_owned()
    }

    // ---- plan_from_event: pure mapping ----

    #[test]
    fn created_maps_to_a_single_create_mutation() {
        let muts = plan_from_event(&WatchEvent::Created { path: "/a/b.txt".into(), is_dir: false });
        assert_eq!(muts, vec![IndexMutation::Create { path: "/a/b.txt".into(), is_dir: false }]);
    }

    #[test]
    fn removed_maps_to_a_single_remove_mutation() {
        let muts = plan_from_event(&WatchEvent::Removed { path: "/a/b.txt".into() });
        assert_eq!(muts, vec![IndexMutation::Remove { path: "/a/b.txt".into() }]);
    }

    #[test]
    fn modified_maps_to_no_mutation() {
        assert!(plan_from_event(&WatchEvent::Modified { path: "/a/b.txt".into() }).is_empty());
    }

    #[test]
    fn a_paired_rename_maps_to_a_single_rename_mutation() {
        let muts = plan_from_event(&WatchEvent::Renamed { from: "/a/old.txt".into(), to: "/a/new.txt".into() });
        assert_eq!(muts, vec![IndexMutation::Rename { from: "/a/old.txt".into(), to: "/a/new.txt".into() }]);
    }

    /// The adapter's fallback for an unpaired rename half (see module docs): representing it as a plain
    /// `Removed` (source, destination unknown) + `Created` (destination, origin unknown) pair of
    /// `WatchEvent`s — rather than a fabricated `Renamed` — still maps to the correct primitives.
    #[test]
    fn rename_as_remove_plus_create_maps_via_the_plain_variants() {
        let events = vec![
            WatchEvent::Removed { path: "/a/gone-from-here.txt".into() },
            WatchEvent::Created { path: "/b/appeared-here.txt".into(), is_dir: false },
        ];
        let muts = plan_from_events(&events);
        assert_eq!(
            muts,
            vec![
                IndexMutation::Remove { path: "/a/gone-from-here.txt".into() },
                IndexMutation::Create { path: "/b/appeared-here.txt".into(), is_dir: false },
            ]
        );
    }

    #[test]
    fn plan_from_events_concatenates_in_order_and_skips_modified() {
        let events = vec![
            WatchEvent::Created { path: "/a".into(), is_dir: true },
            WatchEvent::Modified { path: "/a/x".into() },
            WatchEvent::Removed { path: "/a/y".into() },
            WatchEvent::Renamed { from: "/a/z1".into(), to: "/a/z2".into() },
        ];
        let muts = plan_from_events(&events);
        assert_eq!(
            muts,
            vec![
                IndexMutation::Create { path: "/a".into(), is_dir: true },
                IndexMutation::Remove { path: "/a/y".into() },
                IndexMutation::Rename { from: "/a/z1".into(), to: "/a/z2".into() },
            ]
        );
    }

    // ---- IndexService::apply_mutations + end-to-end search reflects the change (AC 3) ----

    #[test]
    fn apply_mutations_is_a_noop_on_an_empty_batch_or_a_non_resident_volume() {
        let dir = scratch("noop");
        let svc = IndexService::new();
        assert!(!svc.apply_mutations(&dir, 1, &[]).unwrap());
        assert!(
            !svc.apply_mutations(&dir, 1, &[IndexMutation::Remove { path: "/nope".into() }]).unwrap(),
            "no volume 1 is resident"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn watched_create_is_findable_without_a_rebuild() {
        let tree = scratch("create-tree");
        sample_tree(&tree);
        let idxdir = scratch("create-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 1, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert!(svc.search_all(&idxdir, "newfile", 10).is_empty());

        // A create event lands on disk + is planned into a mutation + applied — no rebuild.
        let new_path = abs(&tree, &["src", "newfile.rs"]);
        fs::write(&new_path, b"x").unwrap();
        let events = [WatchEvent::Created { path: new_path, is_dir: false }];
        let mutations = plan_from_events(&events);
        assert!(svc.apply_mutations(&idxdir, 1, &mutations).unwrap());

        assert_eq!(names(&svc.search_all(&idxdir, "newfile", 10)), vec!["newfile.rs"]);
        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    #[test]
    fn watched_remove_drops_from_search_without_a_rebuild() {
        let tree = scratch("remove-tree");
        sample_tree(&tree);
        let idxdir = scratch("remove-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 2, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert_eq!(names(&svc.search_all(&idxdir, "report", 10)), vec!["report.md", "report.rs"]);

        let gone = abs(&tree, &["src", "report.rs"]);
        fs::remove_file(&gone).unwrap();
        let mutations = plan_from_events(&[WatchEvent::Removed { path: gone }]);
        assert!(svc.apply_mutations(&idxdir, 2, &mutations).unwrap());

        assert_eq!(names(&svc.search_all(&idxdir, "report", 10)), vec!["report.md"]);
        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    #[test]
    fn watched_rename_pair_updates_search_without_a_rebuild() {
        let tree = scratch("rename-tree");
        sample_tree(&tree);
        let idxdir = scratch("rename-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 3, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();

        let from = abs(&tree, &["README.md"]);
        let to = abs(&tree, &["CHANGES.md"]);
        fs::rename(&from, &to).unwrap();
        let mutations = plan_from_events(&[WatchEvent::Renamed { from, to }]);
        assert!(svc.apply_mutations(&idxdir, 3, &mutations).unwrap());

        assert!(svc.search_all(&idxdir, "readme", 10).is_empty());
        assert_eq!(names(&svc.search_all(&idxdir, "changes", 10)), vec!["CHANGES.md"]);
        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    /// A burst of events (e.g. a `git checkout` touching many files) is applied as ONE batch — one lock
    /// acquisition, at most one save — not one per event. Asserted indirectly: `apply_mutations` is a
    /// single call over the whole batch (so by construction there is exactly one lock/save per call),
    /// and every mutation in the burst still lands.
    #[test]
    fn event_bursts_apply_as_one_batch() {
        let tree = scratch("burst-tree");
        sample_tree(&tree);
        let idxdir = scratch("burst-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 4, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();

        let mut events = Vec::new();
        for i in 0..20 {
            let p = abs(&tree, &["src", &format!("burst{i}.rs")]);
            fs::write(&p, b"x").unwrap();
            events.push(WatchEvent::Created { path: p, is_dir: false });
        }
        let mutations = plan_from_events(&events);
        assert_eq!(mutations.len(), 20);
        // One call = one lock acquisition + one save, regardless of batch size.
        assert!(svc.apply_mutations(&idxdir, 4, &mutations).unwrap());

        // `ext:rs` excludes the crawl root's own entry (its "name" is the whole scratch path, which
        // contains the substring "burst" too since the temp dir is tagged "burst-tree").
        let hits = svc.search_all(&idxdir, "burst ext:rs", 999);
        assert_eq!(hits.len(), 20, "every mutation in the burst must have landed");
        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    /// Off-means-off at the mutation layer: a volume that was never built (or was since dropped) simply
    /// ignores watch mutations rather than panicking or fabricating a volume.
    #[test]
    fn mutations_for_a_dropped_volume_are_ignored() {
        let tree = scratch("dropped-tree");
        sample_tree(&tree);
        let idxdir = scratch("dropped-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 5, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();
        assert!(svc.drop_volume(&idxdir, 5));

        let mutations = plan_from_events(&[WatchEvent::Created { path: abs(&tree, &["x.txt"]), is_dir: false }]);
        assert!(!svc.apply_mutations(&idxdir, 5, &mutations).unwrap());
        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    // ---- resolve_touched: ancestor-before-descendant ordering (CPE-1138 review) ----

    /// `resolve_touched` sorts so a parent dir's `Created` precedes any child's, regardless of the input
    /// order (the pump feeds it a `HashSet` drain, i.e. arbitrary order), and classifies gone paths as
    /// `Removed`.
    #[test]
    fn resolve_touched_orders_ancestors_before_descendants() {
        // Child listed BEFORE its parent dir; plus a gone path. `/`-separated so the assertion is
        // separator-agnostic (a real Windows pump would feed `\`, which sorts the same way).
        let touched = vec![
            "root/dir/child.rs".to_string(),
            "root/dir".to_string(),
            "root/gone.txt".to_string(),
        ];
        let events = resolve_touched(&touched, |p| match p {
            "root/gone.txt" => None,      // removed
            "root/dir" => Some(true),     // a directory
            _ => Some(false),             // a file
        });
        let created: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                WatchEvent::Created { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect();
        let dir_pos = created.iter().position(|p| *p == "root/dir").expect("dir created");
        let child_pos = created
            .iter()
            .position(|p| *p == "root/dir/child.rs")
            .expect("child created");
        assert!(dir_pos < child_pos, "parent dir's Create must precede the child's: {created:?}");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, WatchEvent::Removed { path } if path == "root/gone.txt")),
            "a gone path must resolve to Removed"
        );
    }

    /// End-to-end regression for the review defect: a directory AND a file inside it created in the SAME
    /// window, with the child's event seen FIRST, must still index the child (not silently drop it because
    /// its parent wasn't indexed yet). With the old arbitrary `HashSet` order this could lose the child.
    #[test]
    fn nested_create_in_one_window_is_indexed_regardless_of_event_order() {
        let tree = scratch("nested-tree");
        sample_tree(&tree);
        let idxdir = scratch("nested-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 8, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();

        // Create a brand-new subdir + a file inside it on disk (as an extraction/checkout would).
        let newdir = abs(&tree, &["freshdir"]);
        let child = abs(&tree, &["freshdir", "nested_hit.rs"]);
        fs::create_dir(&newdir).unwrap();
        fs::write(&child, b"x").unwrap();

        // Feed the re-stat set with the CHILD before the PARENT (the order the old HashSet could hand us).
        let touched = vec![child.clone(), newdir.clone()];
        let events = resolve_touched(&touched, |p| {
            let path = Path::new(p);
            path.exists().then(|| path.is_dir())
        });
        let mutations = plan_from_events(&events);
        assert!(svc.apply_mutations(&idxdir, 8, &mutations).unwrap());

        assert!(
            names(&svc.search_all(&idxdir, "nested_hit", 10)).contains(&"nested_hit.rs".to_string()),
            "a child created alongside its parent dir in one window must be searchable"
        );
        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }
}
