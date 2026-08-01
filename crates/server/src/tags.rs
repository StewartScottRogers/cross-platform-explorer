//! Tag store (CPE-635, epic CPE-614) — an app-side organisational layer: user tags + a colour
//! label per path, persisted as `tags.json` in the config dir (the filesystem is never touched).
//!
//! Extracted into the Server (CPE-815): the pure model helpers were always Tauri-free; the
//! command entry points ([`load`], [`set`], …) take a [`ServerCtx`] to resolve the config dir,
//! so the Tauri `#[tauri::command]` fns become one-line dispatchers. Path-keyed for v1 — a
//! move/rename outside the app orphans the entry (a re-link tool is a future follow-up).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ctx::ServerCtx;

/// The tags + colour label attached to one path.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TagEntry {
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    label: String,
}

impl TagEntry {
    /// The path's tags (normalized: trimmed, de-duped, sorted when set through the store).
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// The path's colour label (empty when none).
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// The whole store: path → entry. `BTreeMap` for a stable, diff-friendly on-disk order.
pub type TagStore = BTreeMap<String, TagEntry>;

/// Set (replace) a path's tags + label. Tags are trimmed, de-duplicated and sorted; an entry that
/// ends up with no tags and no label is removed entirely so the store stays tidy. Pure.
pub fn tag_store_set(store: &mut TagStore, path: &str, tags: Vec<String>, label: String) {
    let mut tags: Vec<String> = tags.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    tags.sort();
    tags.dedup();
    let label = label.trim().to_string();
    if tags.is_empty() && label.is_empty() {
        store.remove(path);
    } else {
        store.insert(path.to_string(), TagEntry { tags, label });
    }
}

/// Every tag with the number of paths carrying it, most-used first then alphabetical. Pure.
pub fn tag_store_counts(store: &TagStore) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in store.values() {
        for t in &e.tags {
            *counts.entry(t.clone()).or_default() += 1;
        }
    }
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

/// Rename tag `old` → `new` across every path (CPE-646). De-dupes if a path already has `new`; an
/// empty `new` just removes `old` (same as delete). Pure.
pub fn tag_store_rename_tag(store: &mut TagStore, old: &str, new: &str) {
    let new = new.trim();
    for e in store.values_mut() {
        if let Some(pos) = e.tags.iter().position(|t| t == old) {
            e.tags.remove(pos);
            if !new.is_empty() && !e.tags.iter().any(|t| t == new) {
                e.tags.push(new.to_string());
            }
            e.tags.sort();
        }
    }
}

/// Remove tag `tag` from every path (CPE-646). Pure.
pub fn tag_store_delete_tag(store: &mut TagStore, tag: &str) {
    for e in store.values_mut() {
        e.tags.retain(|t| t != tag);
    }
}

/// Drop entries left with no tags and no label, so the store stays tidy after bulk edits. Pure.
pub fn tag_store_prune_empty(store: &mut TagStore) {
    store.retain(|_, e| !e.tags.is_empty() || !e.label.is_empty());
}

/// Re-key an entry from `from` → `to` so its tags/label follow an in-app rename or move (CPE-650). A
/// no-op when `from` has no entry. Pure.
pub fn tag_store_rename(store: &mut TagStore, from: &str, to: &str) {
    if let Some(e) = store.remove(from) {
        store.insert(to.to_string(), e);
    }
}

/// Re-key an entry — AND, when `from` is a directory, every entry keyed somewhere under it — from
/// `from` → `to` (CPE-1222). `tag_store_rename` above only ever re-keyed the exact path, so a
/// tagged file living *inside* a renamed/moved folder was silently orphaned: its old-path entry
/// lingered in the store and the file itself showed no tags at its new path. This is the subtree-
/// aware superset: an exact match re-keys like `tag_store_rename`; any entry whose path starts with
/// `from` plus a path separator (`/` or `\`, checked both ways since the store may hold paths from
/// either convention) is re-keyed by swapping that `from` prefix for `to`, leaving the remainder —
/// and thus the whole subtree's internal structure — untouched. Returns whether the store changed
/// (so callers can skip a write when nothing needed to move). A no-op when nothing in the store is
/// `from` or under it. Pure.
pub fn tag_store_rename_subtree(store: &mut TagStore, from: &str, to: &str) -> bool {
    if from == to || from.is_empty() {
        return false;
    }
    let under_slash = format!("{from}/");
    let under_back = format!("{from}\\");
    let matches: Vec<String> = store
        .keys()
        .filter(|k| k.as_str() == from || k.starts_with(&under_slash) || k.starts_with(&under_back))
        .cloned()
        .collect();
    if matches.is_empty() {
        return false;
    }
    for old_path in matches {
        let new_path = if old_path == from {
            to.to_string()
        } else if let Some(rest) = old_path.strip_prefix(&under_slash) {
            format!("{to}/{rest}")
        } else if let Some(rest) = old_path.strip_prefix(&under_back) {
            format!("{to}\\{rest}")
        } else {
            continue; // unreachable given the filter above, but keep the match total honest
        };
        if let Some(e) = store.remove(&old_path) {
            store.insert(new_path, e);
        }
    }
    true
}

/// Merge `incoming` into `store` (CPE-640, import): union each path's tags; take a non-empty imported
/// label over an existing one. Non-destructive — existing tags are never dropped. Pure.
pub fn tag_store_merge(store: &mut TagStore, incoming: TagStore) {
    for (path, e) in incoming {
        let cur = store.entry(path).or_default();
        for t in e.tags {
            if !cur.tags.contains(&t) {
                cur.tags.push(t);
            }
        }
        cur.tags.sort();
        cur.tags.dedup();
        if !e.label.trim().is_empty() {
            cur.label = e.label;
        }
    }
}

/// Load the tag store from `tags.json` in `dir`, returning an empty store when absent or corrupt.
pub fn read_tags_from(dir: &Path) -> TagStore {
    fs::read_to_string(dir.join("tags.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the tag store to `tags.json` in `dir`, creating `dir` if needed.
pub fn write_tags_to(dir: &Path, store: &TagStore) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(dir.join("tags.json"), json.as_bytes()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Command entry points — resolve the config dir via ServerCtx and run the model.
// The Tauri `#[tauri::command]` fns are one-line dispatchers over these.
// ---------------------------------------------------------------------------

/// The whole tag store (path → {tags,label}); empty on a fresh install.
pub fn load(ctx: &dyn ServerCtx) -> Result<TagStore, String> {
    Ok(read_tags_from(&ctx.app_config_dir()?))
}

/// Replace one path's tags + label and persist. Returns the updated whole store.
pub fn set(
    ctx: &dyn ServerCtx,
    path: &str,
    tags: Vec<String>,
    label: String,
) -> Result<TagStore, String> {
    let dir = ctx.app_config_dir()?;
    let mut store = read_tags_from(&dir);
    tag_store_set(&mut store, path, tags, label);
    write_tags_to(&dir, &store)?;
    Ok(store)
}

/// Every tag with its usage count (most-used first).
pub fn counts(ctx: &dyn ServerCtx) -> Result<Vec<(String, usize)>, String> {
    Ok(tag_store_counts(&read_tags_from(&ctx.app_config_dir()?)))
}

/// Rename a tag across every path (CPE-646); an empty `new` deletes it. Returns the updated store.
pub fn rename_tag(ctx: &dyn ServerCtx, old: &str, new: &str) -> Result<TagStore, String> {
    let dir = ctx.app_config_dir()?;
    let mut store = read_tags_from(&dir);
    tag_store_rename_tag(&mut store, old, new);
    tag_store_prune_empty(&mut store);
    write_tags_to(&dir, &store)?;
    Ok(store)
}

/// Remove a tag from every path (CPE-646). Returns the updated store.
pub fn delete_tag(ctx: &dyn ServerCtx, tag: &str) -> Result<TagStore, String> {
    let dir = ctx.app_config_dir()?;
    let mut store = read_tags_from(&dir);
    tag_store_delete_tag(&mut store, tag);
    tag_store_prune_empty(&mut store);
    write_tags_to(&dir, &store)?;
    Ok(store)
}

/// Re-key a path's tags/label after an in-app rename or move (CPE-650) — including, when `from` is
/// a directory, every tagged entry somewhere inside it (CPE-1222, the subtree case: e.g. renaming a
/// tagged folder must carry a tagged file inside it too, not just the folder's own entry). A no-op
/// (no write) when nothing in the store is `from` or under it.
pub fn retag(ctx: &dyn ServerCtx, from: &str, to: &str) -> Result<TagStore, String> {
    let dir = ctx.app_config_dir()?;
    let mut store = read_tags_from(&dir);
    if tag_store_rename_subtree(&mut store, from, to) {
        write_tags_to(&dir, &store)?;
    }
    Ok(store)
}

/// Import a previously-exported tag store (JSON), merged into the current one (CPE-640). Non-
/// destructive: existing tags kept, imported tags unioned in. Returns the merged store.
pub fn import(ctx: &dyn ServerCtx, json: &str) -> Result<TagStore, String> {
    let incoming: TagStore =
        serde_json::from_str(json).map_err(|e| format!("invalid tags file: {e}"))?;
    let dir = ctx.app_config_dir()?;
    let mut store = read_tags_from(&dir);
    tag_store_merge(&mut store, incoming);
    write_tags_to(&dir, &store)?;
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::HeadlessCtx;

    /// A unique temp dir per call (parallel-test-safe).
    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-tags-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn tag_store_set_normalises_and_prunes_entries() {
        let mut s = TagStore::new();
        tag_store_set(&mut s, "/a", vec!["  Zeta ".into(), "alpha".into(), "alpha".into(), "  ".into()], "red".into());
        assert_eq!(s["/a"].tags, vec!["Zeta".to_string(), "alpha".to_string()]);
        assert_eq!(s["/a"].label, "red");
        tag_store_set(&mut s, "/a", vec![], "".into());
        assert!(!s.contains_key("/a"));
        tag_store_set(&mut s, "/b", vec![], "blue".into());
        assert_eq!(s["/b"].label, "blue");
    }

    #[test]
    fn tag_store_bulk_rename_and_delete() {
        let mut s = TagStore::new();
        tag_store_set(&mut s, "/a", vec!["wrk".into(), "urgent".into()], "".into());
        tag_store_set(&mut s, "/b", vec!["wrk".into(), "work".into()], "".into());
        tag_store_rename_tag(&mut s, "wrk", "work");
        assert_eq!(s["/a"].tags, vec!["urgent".to_string(), "work".to_string()]);
        assert_eq!(s["/b"].tags, vec!["work".to_string()]);
        tag_store_delete_tag(&mut s, "work");
        tag_store_prune_empty(&mut s);
        assert_eq!(s["/a"].tags, vec!["urgent".to_string()]);
        assert!(!s.contains_key("/b"), "an entry with no tags/label is pruned");
    }

    #[test]
    fn tag_store_rename_carries_tags_to_the_new_path() {
        let mut s = TagStore::new();
        tag_store_set(&mut s, "/old", vec!["keep".into()], "red".into());
        tag_store_rename(&mut s, "/old", "/new");
        assert!(!s.contains_key("/old") && s.contains_key("/new"));
        assert_eq!(s["/new"].tags, vec!["keep".to_string()]);
        assert_eq!(s["/new"].label, "red");
        tag_store_rename(&mut s, "/nothing", "/elsewhere");
        assert!(!s.contains_key("/elsewhere"));
    }

    #[test]
    fn tag_store_rename_subtree_carries_the_top_entry_and_every_descendant() {
        let mut s = TagStore::new();
        tag_store_set(&mut s, "/proj", vec!["work".into()], "".into());
        tag_store_set(&mut s, "/proj/a.txt", vec!["keep".into()], "red".into());
        tag_store_set(&mut s, "/proj/sub/b.txt", vec!["nested".into()], "".into());
        // A sibling that merely shares a prefix (NOT a real path separator boundary) must be left alone.
        tag_store_set(&mut s, "/projector.txt", vec!["unrelated".into()], "".into());

        let changed = tag_store_rename_subtree(&mut s, "/proj", "/archive");
        assert!(changed);

        assert!(!s.contains_key("/proj") && !s.contains_key("/proj/a.txt") && !s.contains_key("/proj/sub/b.txt"));
        assert_eq!(s["/archive"].tags, vec!["work".to_string()]);
        assert_eq!(s["/archive/a.txt"].tags, vec!["keep".to_string()]);
        assert_eq!(s["/archive/a.txt"].label, "red");
        assert_eq!(s["/archive/sub/b.txt"].tags, vec!["nested".to_string()]);
        // The false-prefix sibling is untouched, at its original path.
        assert_eq!(s["/projector.txt"].tags, vec!["unrelated".to_string()]);
    }

    #[test]
    fn tag_store_rename_subtree_handles_windows_backslash_paths_and_no_matches() {
        let mut s = TagStore::new();
        tag_store_set(&mut s, r"C:\Users\me\Docs", vec!["work".into()], "".into());
        tag_store_set(&mut s, r"C:\Users\me\Docs\notes.txt", vec!["keep".into()], "".into());

        assert!(tag_store_rename_subtree(&mut s, r"C:\Users\me\Docs", r"C:\Users\me\Archive"));
        assert!(s.contains_key(r"C:\Users\me\Archive"));
        assert!(s.contains_key(r"C:\Users\me\Archive\notes.txt"));
        assert!(!s.contains_key(r"C:\Users\me\Docs") && !s.contains_key(r"C:\Users\me\Docs\notes.txt"));

        // Nothing under an untagged path: a no-op that reports no change and touches nothing.
        assert!(!tag_store_rename_subtree(&mut s, "/nothing/here", "/elsewhere"));
        // Renaming a path onto itself is also a reported no-op.
        assert!(!tag_store_rename_subtree(&mut s, r"C:\Users\me\Archive", r"C:\Users\me\Archive"));
    }

    #[test]
    fn tag_store_merge_is_non_destructive() {
        let mut s = TagStore::new();
        tag_store_set(&mut s, "/a", vec!["keep".into()], "red".into());
        let mut incoming = TagStore::new();
        tag_store_set(&mut incoming, "/a", vec!["added".into()], "blue".into());
        tag_store_set(&mut incoming, "/b", vec!["new".into()], "".into());
        tag_store_merge(&mut s, incoming);
        assert_eq!(s["/a"].tags, vec!["added".to_string(), "keep".to_string()]);
        assert_eq!(s["/a"].label, "blue");
        assert_eq!(s["/b"].tags, vec!["new".to_string()]);
    }

    #[test]
    fn tag_store_rename_tag_self_rename_is_noop_and_merges_into_existing() {
        let mut s = TagStore::new();
        // Renaming a tag to itself must leave the path untouched (not drop or duplicate it).
        tag_store_set(&mut s, "/a", vec!["work".into(), "urgent".into()], "".into());
        tag_store_rename_tag(&mut s, "work", "work");
        assert_eq!(s["/a"].tags, vec!["urgent".to_string(), "work".to_string()], "self-rename is a no-op");
        // Renaming into a tag the path already carries collapses to one (no duplicate).
        tag_store_rename_tag(&mut s, "urgent", "work");
        assert_eq!(s["/a"].tags, vec!["work".to_string()], "merge into existing tag de-dupes");
    }

    #[test]
    fn tag_store_counts_most_used_first() {
        let mut s = TagStore::new();
        tag_store_set(&mut s, "/a", vec!["work".into(), "urgent".into()], "".into());
        tag_store_set(&mut s, "/b", vec!["work".into()], "".into());
        assert_eq!(tag_store_counts(&s), vec![("work".to_string(), 2), ("urgent".to_string(), 1)]);
    }

    #[test]
    fn tag_store_round_trips_through_disk() {
        let d = scratch("tags_io");
        let mut s = TagStore::new();
        tag_store_set(&mut s, "/x", vec!["keep".into()], "green".into());
        write_tags_to(&d, &s).unwrap();
        assert_eq!(read_tags_from(&d), s);
        assert!(read_tags_from(&scratch("tags_none")).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    // The ServerCtx entry points end-to-end, over a HeadlessCtx (no Tauri).
    #[test]
    fn ctx_entry_points_persist_and_reload() {
        let base = scratch("tags_ctx");
        let ctx = HeadlessCtx::new(&base);
        assert!(load(&ctx).unwrap().is_empty());
        set(&ctx, "/p", vec!["a".into(), "b".into()], "red".into()).unwrap();
        // Reloading through the ctx sees the persisted entry.
        let reloaded = load(&ctx).unwrap();
        assert_eq!(reloaded["/p"].tags, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(counts(&ctx).unwrap(), vec![("a".to_string(), 1), ("b".to_string(), 1)]);
        rename_tag(&ctx, "a", "z").unwrap();
        assert_eq!(load(&ctx).unwrap()["/p"].tags, vec!["b".to_string(), "z".to_string()]);
        retag(&ctx, "/p", "/q").unwrap();
        let s = load(&ctx).unwrap();
        assert!(s.contains_key("/q") && !s.contains_key("/p"));
        let _ = fs::remove_dir_all(&base);
    }

    // CPE-1222: `retag` (the command behind the rename/move backends' migration) must also carry a
    // tagged file living INSIDE a renamed/moved directory, not just the directory's own entry.
    #[test]
    fn retag_through_ctx_migrates_a_directorys_whole_tagged_subtree() {
        let base = scratch("tags_ctx_subtree");
        let ctx = HeadlessCtx::new(&base);
        set(&ctx, "/proj", vec!["work".into()], "".into()).unwrap();
        set(&ctx, "/proj/notes.txt", vec!["keep".into()], "blue".into()).unwrap();

        retag(&ctx, "/proj", "/archive").unwrap();
        let s = load(&ctx).unwrap();
        assert!(!s.contains_key("/proj") && !s.contains_key("/proj/notes.txt"), "old paths fully vacated");
        assert_eq!(s["/archive"].tags, vec!["work".to_string()]);
        assert_eq!(s["/archive/notes.txt"].tags, vec!["keep".to_string()]);
        assert_eq!(s["/archive/notes.txt"].label, "blue");

        // Retagging a path nothing is tagged under is a true no-op: no write, store unchanged.
        let before = load(&ctx).unwrap();
        retag(&ctx, "/nothing", "/elsewhere").unwrap();
        assert_eq!(load(&ctx).unwrap(), before);
        let _ = fs::remove_dir_all(&base);
    }

    // A malformed import must fail *without* touching the persisted store — the parse happens before
    // any read/write, so a bad file can never clobber the user's tags. Locks that ordering in.
    #[test]
    fn import_rejects_malformed_json_and_leaves_the_store_intact() {
        let base = scratch("tags_import_bad");
        let ctx = HeadlessCtx::new(&base);
        set(&ctx, "/keep", vec!["mine".into()], "red".into()).unwrap();

        let err = import(&ctx, "{ this is not json ]").unwrap_err();
        assert!(err.contains("invalid tags file"), "surfaces a clear parse error, got: {err}");
        // The pre-existing entry is still exactly as it was.
        let after = load(&ctx).unwrap();
        assert_eq!(after["/keep"].tags, vec!["mine".to_string()]);
        assert_eq!(after["/keep"].label, "red");
        let _ = fs::remove_dir_all(&base);
    }

    // A valid import unions into the current store (non-destructive) and persists the merge.
    #[test]
    fn import_merges_valid_json_into_the_current_store() {
        let base = scratch("tags_import_ok");
        let ctx = HeadlessCtx::new(&base);
        set(&ctx, "/a", vec!["keep".into()], "red".into()).unwrap();

        let json = r#"{"/a":{"tags":["added"],"label":"blue"},"/b":{"tags":["new"],"label":""}}"#;
        import(&ctx, json).unwrap();

        let s = load(&ctx).unwrap();
        assert_eq!(s["/a"].tags, vec!["added".to_string(), "keep".to_string()], "existing tag kept, import unioned");
        assert_eq!(s["/a"].label, "blue", "non-empty imported label wins");
        assert_eq!(s["/b"].tags, vec!["new".to_string()]);
        let _ = fs::remove_dir_all(&base);
    }
}
