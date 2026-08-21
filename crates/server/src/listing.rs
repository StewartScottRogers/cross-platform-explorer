//! Directory listing (CPE-663/662): the single directory walker behind both the synchronous `list_dir`
//! and the streaming `list_dir_stream` command, so their contents + skip behaviour stay identical. Pure
//! and Tauri-free (CPE-815): the walker takes a `flush` callback returning `ControlFlow`, so the app's
//! streaming command keeps its `ipc::Channel` (and cancel registry) in the adapter and feeds this walker.

use std::fs;
use std::io;

use crate::fsutil::to_epoch_ms;
use crate::model::{extension_of, is_hidden, DirEntry};

/// Number of entries per streamed batch — small enough that the first rows paint within a frame or two
/// on a big folder, large enough that a tiny folder is one flush (CPE-662).
pub const LIST_DIR_BATCH: usize = 256;

/// Map one directory entry to a [`DirEntry`], or `None` if it can't be read — the caller skips those
/// rather than failing the whole listing.
fn dir_entry_from(entry: &fs::DirEntry) -> Option<DirEntry> {
    // `fs::DirEntry::metadata()` does NOT follow symlinks (unlike `fs::metadata()`), so `meta.is_dir()`
    // is already false for a symlink pointing at a directory and `meta.file_type().is_symlink()` is free
    // — no extra syscall beyond the one this listing already makes per entry.
    let meta = entry.metadata().ok()?;
    let entry_path = entry.path();
    let is_dir = meta.is_dir();
    Some(DirEntry {
        hidden: is_hidden(&entry_path, &meta),
        name: entry.file_name().to_string_lossy().to_string(),
        path: entry_path.to_string_lossy().to_string(),
        is_dir,
        size: if is_dir { 0 } else { meta.len() },
        modified: meta.modified().ok().and_then(to_epoch_ms),
        extension: if is_dir { String::new() } else { extension_of(&entry_path) },
        is_symlink: meta.file_type().is_symlink(),
    })
}

/// Outcome of a [`stream_dir_entries`] walk (CPE-1780): how many entries were actually emitted
/// (`total`), and how many rows the walk had to skip because they couldn't be read (`unreadable`) — a
/// `read_dir` iteration error, or a `metadata()` failure on an otherwise-named entry. Deliberately a
/// DIFFERENT count from `ListDirResult::filtered` (CPE-1708): `filtered` is a REMOTE provider refusing to
/// show an entry at all because of its own keyspace rule (the name is never even seen); `unreadable` is a
/// row the LOCAL walk did see but could not stat. Different facts, so they're never added together — see
/// `ListDirResult`'s doc in `crates/server/src/model.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirWalkStats {
    pub total: usize,
    pub unreadable: usize,
}

/// Folds one `read_dir` iteration outcome into the walk's running state — pulled out of the loop below so
/// the counting rule (a row that couldn't be read increments `unreadable`, never `total`, and is never
/// pushed into the batch) is unit-testable on its own (CPE-1780), without needing to race the real OS
/// into producing an actual unreadable directory entry (`metadata()` failing on a `fs::DirEntry` is
/// inherently a TOCTOU race — the file vanishing between `read_dir` yielding it and the `metadata()` call
/// a moment later — not something a portable, deterministic test can force to order). `None` covers BOTH
/// failure shapes the caller collapses before calling this: the iteration step itself erroring, and a
/// successful iteration step whose `dir_entry_from` stat failed — both are "couldn't read this row",
/// counted identically.
fn fold_walk_entry(outcome: Option<DirEntry>, buf: &mut Vec<DirEntry>, unreadable: &mut usize) {
    match outcome {
        Some(de) => buf.push(de),
        None => *unreadable += 1,
    }
}

/// Walk `path`, invoking `flush` with each batch of up to `batch` readable entries as they're read.
/// Unreadable entries are skipped (never fail the listing) but counted in the returned stats'
/// `unreadable` field rather than silently dropped (CPE-1780) — see [`DirWalkStats`]. `flush` returns a
/// `ControlFlow` so a streaming caller can stop the walk early (cancellation, CPE-665) at a batch
/// boundary; `unreadable` only reflects rows actually seen before a break. A thin `fs::read_dir` wrapper
/// around [`stream_dir_entries_over`] — see that function's doc for why the walk loop itself lives there.
pub fn stream_dir_entries(
    path: &str,
    batch: usize,
    flush: impl FnMut(Vec<DirEntry>) -> std::ops::ControlFlow<()>,
) -> Result<DirWalkStats, String> {
    let read = fs::read_dir(path).map_err(|e| format!("{path}: {e}"))?;
    Ok(stream_dir_entries_over(read, batch, flush))
}

/// The walk loop itself, generalised over any `io::Result<fs::DirEntry>` iterator rather than hard-wired
/// to `fs::read_dir`'s `ReadDir` (CPE-1780 Reviewer round 2). [`stream_dir_entries`] above is a thin
/// wrapper that supplies the real one; a test below instead supplies a SYNTHETIC iterator — a real
/// `fs::DirEntry` (borrowed from an incidental `read_dir` call; the type has no public constructor,
/// so it can't be fabricated outright) spliced with a synthetic `Err` — to drive an iteration failure
/// through this EXACT production loop and prove `DirWalkStats.unreadable` increments (and `total` does
/// not) without racing the OS for a genuine one. UAT round (2026-08-20) confirmed there is no
/// deterministic way to stage a real one: the repo's only ACL-staging helper, `fsutil::deny_stat_of`,
/// denies list-directory on the PARENT too (so `fs::read_dir` itself fails for the whole directory, a
/// different failure shape than one bad row among good ones), and denying only the target file's own read
/// permission doesn't reproduce the failure at all on Windows — `DirEntry::metadata()` reuses data already
/// cached from the `FindNextFileW` enumeration rather than re-opening the file, so a target with its read
/// permission revoked AFTER being enumerated still reports `unreadable: 0`.
fn stream_dir_entries_over(
    iter: impl Iterator<Item = io::Result<fs::DirEntry>>,
    batch: usize,
    mut flush: impl FnMut(Vec<DirEntry>) -> std::ops::ControlFlow<()>,
) -> DirWalkStats {
    let cap = batch.min(1024);
    let mut buf: Vec<DirEntry> = Vec::with_capacity(cap);
    let mut total = 0usize;
    let mut unreadable = 0usize;
    for entry in iter {
        let outcome = entry.ok().and_then(|e| dir_entry_from(&e));
        fold_walk_entry(outcome, &mut buf, &mut unreadable);
        if buf.len() >= batch {
            total += buf.len();
            if flush(std::mem::replace(&mut buf, Vec::with_capacity(cap))).is_break() {
                return DirWalkStats { total, unreadable };
            }
        }
    }
    if !buf.is_empty() {
        total += buf.len();
        let _ = flush(buf);
    }
    DirWalkStats { total, unreadable }
}

/// Collect-to-vec directory listing: every readable entry of `path`. A missing/unreadable `path` is an
/// `Err`. Discards the walk's `unreadable` count (see [`list_dir_with_unreadable`] for the caller that
/// needs it) — `list_dir`'s several non-UI callers (search, organize, replay-baseline, copilot) never
/// asked for it, so it stays out of this signature rather than touching every one of them.
pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, String> {
    let mut out = Vec::new();
    stream_dir_entries(path, LIST_DIR_BATCH, |batch| {
        out.extend(batch);
        std::ops::ControlFlow::Continue(())
    })?;
    Ok(out)
}

/// The `DirWalkStats` → `(Vec<DirEntry>, unreadable)` link [`list_dir_with_unreadable`] wraps its walk
/// result in (CPE-1780 Reviewer round 2) — pulled out so "carry `unreadable` through, never zero it" is
/// independently unit testable with a non-zero count, without needing a real unreadable directory entry
/// (see [`stream_dir_entries_over`]'s doc for why the WALK's own counting is proven that way instead).
fn walk_result_tuple(out: Vec<DirEntry>, stats: DirWalkStats) -> (Vec<DirEntry>, usize) {
    (out, stats.unreadable)
}

/// Collect-to-vec directory listing PLUS how many rows the walk had to skip because they couldn't be read
/// (CPE-1780) — the entry point the UI-facing `list_dir` Tauri command uses so the status bar can say so,
/// distinct from [`list_dir`] above (whose other callers don't want the count).
pub fn list_dir_with_unreadable(path: &str) -> Result<(Vec<DirEntry>, usize), String> {
    let mut out = Vec::new();
    let stats = stream_dir_entries(path, LIST_DIR_BATCH, |batch| {
        out.extend(batch);
        std::ops::ControlFlow::Continue(())
    })?;
    Ok(walk_result_tuple(out, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-listing-{tag}"))
    }

    #[test]
    fn list_dir_errors_on_a_missing_path() {
        assert!(list_dir("/definitely/not/a/real/path/xyz").is_err());
    }

    #[test]
    fn list_dir_lists_a_real_directory() {
        assert!(list_dir(&std::env::temp_dir().to_string_lossy()).is_ok());
    }

    #[test]
    fn stream_dir_entries_batches_and_flushes_all() {
        let d = scratch("streamdir");
        for i in 0..500 {
            fs::write(d.join(format!("f{i:03}.txt")), b"x").unwrap();
        }
        let mut batch_sizes = Vec::new();
        let stats = stream_dir_entries(d.to_str().unwrap(), 256, |b| {
            batch_sizes.push(b.len());
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(stats.total, 500);
        assert_eq!(stats.unreadable, 0, "a scratch dir of plain files has nothing unreadable");
        assert_eq!(batch_sizes.iter().sum::<usize>(), 500);
        assert!(batch_sizes.len() >= 2, "500 entries at batch 256 should flush more than once");
        assert!(batch_sizes.iter().all(|&s| s <= 256), "no batch exceeds the cap");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stream_dir_entries_stops_on_break() {
        let d = scratch("streambreak");
        for i in 0..1000 {
            fs::write(d.join(format!("f{i:04}.txt")), b"x").unwrap();
        }
        let mut seen = 0usize;
        // Break after the first flush — the walk must stop rather than read all 1000.
        let stats = stream_dir_entries(d.to_str().unwrap(), 100, |b| {
            seen += b.len();
            std::ops::ControlFlow::Break(())
        })
        .unwrap();
        assert_eq!(seen, 100, "break after the first batch stops the walk");
        assert_eq!(stats.total, 100);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn list_dir_flags_symlinks_and_leaves_plain_files_unflagged() {
        let d = scratch("symlink");
        let target = d.join("target.txt");
        fs::write(&target, b"data").unwrap();
        let plain = d.join("plain.txt");
        fs::write(&plain, b"data").unwrap();
        let link = d.join("link.txt");

        // Symlink creation is unprivileged on Windows only with Developer Mode / admin — skip the
        // is_symlink assertion there rather than failing; POSIX always permits it (matches links.rs).
        #[cfg(unix)]
        let created = std::os::unix::fs::symlink(&target, &link).is_ok();
        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_file(&target, &link).is_ok();

        let entries = list_dir(&d.to_string_lossy()).unwrap();
        let plain_entry = entries.iter().find(|e| e.name == "plain.txt").unwrap();
        assert!(!plain_entry.is_symlink, "a plain file is never a symlink");

        if created {
            let link_entry = entries.iter().find(|e| e.name == "link.txt").unwrap();
            assert!(link_entry.is_symlink, "a listed symlink entry reports is_symlink=true");
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stream_dir_entries_matches_list_dir_contents() {
        let d = scratch("streameq");
        for n in ["a.txt", "b.rs", "c.png"] {
            fs::write(d.join(n), b"x").unwrap();
        }
        fs::create_dir(d.join("sub")).unwrap();
        // A tiny batch of 2 exercises the mid-walk flush path.
        let mut streamed = Vec::new();
        stream_dir_entries(d.to_str().unwrap(), 2, |b| {
            streamed.extend(b);
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        let listed = list_dir(&d.to_string_lossy()).unwrap();
        let mut a: Vec<_> = streamed.iter().map(|e| e.name.clone()).collect();
        let mut b: Vec<_> = listed.iter().map(|e| e.name.clone()).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        assert_eq!(a, vec!["a.txt", "b.rs", "c.png", "sub"]);
        let _ = fs::remove_dir_all(&d);
    }

    // ---- Unreadable-entry counting (CPE-1780) --------------------------------------------------------
    // `stream_dir_entries` used to `continue` past both a `read_dir` iteration error and a
    // `dir_entry_from` stat failure with nothing counting either — a name-safety refusal (`filtered`,
    // CPE-1708) and an unreadable row are different facts, so this must never fold into `filtered` either.
    // Forcing a REAL unreadable `fs::DirEntry` in a portable, deterministic test would mean racing the OS
    // (the file vanishing between `read_dir` yielding it and the follow-up `metadata()` call) — not
    // reproducible across the 3-OS CI matrix — so `fold_walk_entry` was pulled out specifically to make
    // the counting RULE itself testable without that race.

    #[test]
    fn fold_walk_entry_counts_an_unreadable_row_without_adding_it_to_the_batch() {
        let mut buf = Vec::new();
        let mut unreadable = 0usize;
        fold_walk_entry(None, &mut buf, &mut unreadable);
        assert_eq!(unreadable, 1, "an unreadable row must be counted");
        assert!(buf.is_empty(), "an unreadable row must never be pushed into the batch as a fake entry");
    }

    #[test]
    fn fold_walk_entry_pushes_a_readable_row_without_counting_it_unreadable() {
        let de = DirEntry {
            name: "ok.txt".into(),
            path: "/tmp/ok.txt".into(),
            is_dir: false,
            size: 1,
            modified: None,
            extension: "txt".into(),
            hidden: false,
            is_symlink: false,
        };
        let mut buf = Vec::new();
        let mut unreadable = 0usize;
        fold_walk_entry(Some(de), &mut buf, &mut unreadable);
        assert_eq!(unreadable, 0, "a readable row must not bump the unreadable count");
        assert_eq!(buf.len(), 1, "a readable row must land in the batch");
    }

    #[test]
    fn list_dir_with_unreadable_reports_zero_for_an_ordinary_directory() {
        let d = scratch("unreadable-ordinary");
        fs::write(d.join("a.txt"), b"x").unwrap();
        let (entries, unreadable) = list_dir_with_unreadable(&d.to_string_lossy()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(unreadable, 0, "an ordinary directory has nothing unreadable");
        let _ = fs::remove_dir_all(&d);
    }

    /// Reviewer round 2: `fold_walk_entry`'s two tests above prove the COUNTING RULE in isolation, but
    /// mutating the WIRING around it (`crates/server/src/listing.rs:118`, `Ok((out, stats.unreadable))` →
    /// `Ok((out, 0))`) left the whole suite green, because nothing drove a real iteration failure through
    /// the ACTUAL production loop end to end. This does: a real `fs::DirEntry` (borrowed from an
    /// incidental `read_dir` — the type has no public constructor) spliced with a synthetic `io::Error`,
    /// fed straight into `stream_dir_entries_over` — the exact function `stream_dir_entries` (and
    /// therefore `list_dir`/`list_dir_with_unreadable`) delegates to.
    #[test]
    fn stream_dir_entries_over_counts_a_synthetic_iteration_error_as_unreadable_through_the_real_walk_loop() {
        let d = scratch("unreadable-synthetic");
        fs::write(d.join("ok.txt"), b"x").unwrap();
        let real_entry = fs::read_dir(&d)
            .unwrap()
            .next()
            .expect("scratch dir has exactly one real entry")
            .expect("the real read_dir entry itself must be Ok");

        let synthetic: Vec<io::Result<fs::DirEntry>> =
            vec![Ok(real_entry), Err(io::Error::other("synthetic unreadable row"))];

        let mut collected: Vec<DirEntry> = Vec::new();
        let stats = stream_dir_entries_over(synthetic.into_iter(), 256, |batch| {
            collected.extend(batch);
            std::ops::ControlFlow::Continue(())
        });

        assert_eq!(stats.total, 1, "only the real, readable row counts toward total");
        assert_eq!(stats.unreadable, 1, "the synthetic iteration error must be counted as unreadable");
        assert_eq!(collected.len(), 1, "the synthetic error must never be pushed into the batch as a fake row");
        assert_eq!(collected[0].name, "ok.txt");
        let _ = fs::remove_dir_all(&d);
    }

    /// Reviewer round 2's other proven mutation: `src-tauri/src/lib.rs:699`'s `unreadable: stats.unreadable`
    /// could be mutated to `unreadable: 0` with zero test failures — this crate's own equivalent risk is
    /// `walk_result_tuple` above, so it gets the same direct, non-zero-count mapping test (mirrors the
    /// `local_stream_result_for`/`local_list_dir_result_from` tests added in `src-tauri/src/lib.rs`).
    #[test]
    fn walk_result_tuple_carries_the_real_unreadable_count_through() {
        let stats = DirWalkStats { total: 4, unreadable: 3 };
        let (entries, unreadable) = walk_result_tuple(vec![], stats);
        assert_eq!(entries.len(), 0);
        assert_eq!(unreadable, 3, "list_dir_with_unreadable must carry the real unreadable count through, not 0");
    }
}
