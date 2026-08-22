//! Revert-plan execution engine (CPE-1081, epic CPE-732 "checkpoint & rollback"): apply a minimal
//! [`RestoreAction`] plan (as computed by [`crate::restore_plan::plan_restore`]) to a REAL directory —
//! surgical, current-state-aware execution, distinct from [`crate::snapshot_capture::restore`] (CPE-735),
//! which only replays a whole manifest onto a (possibly fresh) directory and never deletes or considers
//! what's already there.
//!
//! Reuses [`crate::restore_plan::RestoreAction`]/[`crate::restore_plan::RestoreOp`]/
//! [`crate::restore_plan::Snapshot`] and [`crate::snapshot_capture`]'s on-disk blob layout
//! (`store_dir/blobs/<hash>`, one file per content hash) without modifying either module.
//!
//! Design notes (deliberate choices):
//! - **Skip-on-error, never fatal.** Mirrors `list_dir`/`snapshot_capture`'s guardrail: any op that fails
//!   (missing blob, permission denied, path-safety refusal) is recorded in
//!   [`RestoreReport::skipped`] with a reason and the rest of the plan still runs.
//! - **Deletes apply deepest-first** (descending `/`-segment depth) so a directory empties out before
//!   anything tries to touch it — matches `restore_plan`'s "execution note" doc comment.
//! - **Path safety.** Every action's `path` is a `/`-joined relative path (same convention as
//!   `snapshot_capture::scan_dir`). Before touching disk we split it on `/` and reject (skip, don't
//!   panic or write) any segment that is empty, `.`, `..`, or that makes the path absolute/drive-rooted —
//!   this is a pure string/segment guard, not `Path::starts_with` (which is defeated by `..` components
//!   that never get resolved against the filesystem). The real target is then rebuilt with
//!   `Path::join` over the validated segments (portable — no manual separator concatenation).
//! - **No recursion.** The plan is iterated; deletes are sorted once up front.

use std::fs;
use std::path::{Path, PathBuf};

use crate::restore_plan::{RestoreAction, RestoreOp, Snapshot};

/// Outcome of [`execute_restore`]: how many ops applied cleanly, and which were skipped (with why).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestoreReport {
    /// Number of actions that applied successfully.
    pub applied: usize,
    /// Actions that could not be applied: `(path, reason)`. Never fatal — the rest of the plan still runs.
    pub skipped: Vec<(String, String)>,
}

/// Execute `plan` (as produced by [`crate::restore_plan::plan_restore`]) against the real directory
/// `dest_root`, pulling restored content for `Create`/`Overwrite` from `store_dir`'s blob store using each
/// path's hash recorded in `checkpoint`. `Delete` actions are applied deepest-first so a directory is
/// empty by the time anything might look at it (nothing in this engine removes directories itself —
/// only the files the plan names).
///
/// Every action is independent: a failing op is skipped and recorded rather than aborting the rest of the
/// plan (mirrors the `list_dir`/`snapshot_capture` skip-on-error guardrail). A plan path that escapes
/// `dest_root` (`..`, an absolute path, or a drive-rooted path) is refused the same way — skipped with a
/// reason, nothing is ever written outside `dest_root`.
pub fn execute_restore(
    plan: &[RestoreAction],
    dest_root: &str,
    store_dir: &str,
    checkpoint: &Snapshot,
) -> RestoreReport {
    let dest_root_path = Path::new(dest_root);
    let blobs_dir = Path::new(store_dir).join("blobs");
    let mut report = RestoreReport::default();

    // Partition into writes (Create/Overwrite) and deletes; order deletes deepest-first (most `/`
    // segments first) so nested files are removed before anything above them is touched.
    let mut writes: Vec<&RestoreAction> = Vec::new();
    let mut deletes: Vec<&RestoreAction> = Vec::new();
    for action in plan {
        match action.op {
            RestoreOp::Create | RestoreOp::Overwrite => writes.push(action),
            RestoreOp::Delete => deletes.push(action),
        }
    }
    deletes.sort_by_key(|a| std::cmp::Reverse(segment_depth(&a.path)));

    for action in &writes {
        match apply_write(action, dest_root_path, &blobs_dir, checkpoint) {
            Ok(()) => report.applied += 1,
            Err(reason) => report.skipped.push((action.path.clone(), reason)),
        }
    }

    // **A delete is only safe once the checkpoint state has actually been established (CPE-1823 round 3).**
    //
    // Refusing the write alone did not save the user's file. The reported shape: a planted entry
    // `a.txt ` against a live `a.txt`. Refusing the Create is right — but `plan_restore` had also
    // emitted `Delete a.txt`, because on its reading `a.txt` is a file added since the checkpoint. So
    // the guard skipped the write, the delete ran, and the file was gone anyway; the report just said
    // `applied: 1, skipped: 1` instead of `applied: 2, skipped: []`. A visible refusal next to the same
    // destroyed file is not a fix.
    //
    // The rule is deliberately about the *class*, not that one alias. A delete's whole justification is
    // "this path is not in the checkpoint" — a judgement that depends on having read the checkpoint
    // correctly and applied it. Any skipped write means we did not, so no delete's premise can be
    // trusted, and the destructive half of the revert stands down. That also covers aliasing shapes this
    // engine cannot see coming (the case-insensitive `A.txt`/`a.txt` collapse recorded on
    // `win32_addresses_a_different_path`, for one).
    //
    // Conservative in the safe direction and reported per path, never silent: the user is told exactly
    // which cleanups were held back and why, and re-running after fixing the manifest performs them.
    if report.skipped.is_empty() {
        for action in &deletes {
            match apply_delete(action, dest_root_path) {
                Ok(()) => report.applied += 1,
                Err(reason) => report.skipped.push((action.path.clone(), reason)),
            }
        }
    } else {
        let held = report.skipped.len();
        for action in &deletes {
            report.skipped.push((
                action.path.clone(),
                format!(
                    "not deleted: {held} checkpoint entr{} could not be restored, so \"this file is not \
                     in the checkpoint\" cannot be trusted — deleting it might destroy a file the \
                     checkpoint actually holds under a name this platform spells differently",
                    if held == 1 { "y" } else { "ies" }
                ),
            ));
        }
    }

    report
}

/// Split `rel` on `/` and validate every segment is a plain path component: non-empty, not `.`/`..`, and
/// not itself absolute/drive-rooted (guards a Windows `C:\...`-style segment slipping in as the first
/// piece). Returns the validated segments, or an error naming why the path was refused. This is a pure
/// string check over the `/`-joined convention `scan_dir`/`plan_restore` use — deliberately not
/// `Path::starts_with`, which only compares an already-joined path and can be fooled by `..` components
/// that are never resolved against the real filesystem.
pub(crate) fn safe_segments(rel: &str) -> Result<Vec<&str>, String> {
    if rel.is_empty() {
        return Err("empty path".to_string());
    }
    if Path::new(rel).is_absolute() {
        return Err("escapes dest_root: absolute path".to_string());
    }
    let mut segments = Vec::new();
    for seg in rel.split('/') {
        if seg.is_empty() {
            return Err("escapes dest_root: empty path segment".to_string());
        }
        if seg == "." || seg == ".." {
            return Err("escapes dest_root: `.`/`..` segment".to_string());
        }
        if let Some(why) = win32_addresses_a_different_path(seg) {
            return Err(why);
        }
        // A segment containing a drive letter (`C:`) or a backslash would, on Windows, be
        // reinterpreted as rooting/escaping once joined — reject it the same way.
        //
        // **`cfg!(windows)`-gated (CPE-1823 round 2), and the gate is load-bearing.** On Linux and macOS
        // neither character roots anything: `:` and `\` are ordinary filename bytes, `a\b` is one real
        // directory entry, and `2026-08-21 10:30 notes.txt` is a name users type on purpose. macOS makes
        // it routine rather than exotic — a Finder name containing `/` is stored on disk as `:`, so any
        // "Q1/Q2 report" becomes a colon on the volume.
        //
        // Refusing those everywhere was survivable while this rule only served `apply_write`, which
        // *skips one file and continues* with the reason in its report. `snapshot_capture::restore` now
        // shares the rule and **aborts the whole manifest** at the first refusal, so an unqualified
        // colon check would turn one ordinary Unix filename into a half-restored tree — a file that
        // restored fine before CPE-1823 touched anything. Same reasoning, and the same `cfg!(windows)`
        // shape, as `crate::backup`'s `refuses_unstable_names` and `fsutil::win32_name_is_unstable`'s
        // doc: apply the Windows rule where Windows is, and nowhere else.
        //
        // Containment on Unix is unaffected — it never rested on this line. `..`, `.`, empty segments and
        // absolute paths are all rejected above, and every surviving segment is pushed as one component.
        if cfg!(windows) && (seg.contains(':') || seg.contains('\\')) {
            return Err("escapes dest_root: drive-rooted or backslash segment".to_string());
        }
        // CPE-1823 round 2: a segment Win32 resolves to something OTHER than what it spells.
        // Lives HERE, not at a call site, so every `safe_target` caller inherits it — see below.
        segments.push(seg);
    }
    Ok(segments)
}

/// Names that Win32 resolves to **something other than what they spell**, so acting on one neither
/// refuses nor does what it says (CPE-1823 round 2). Two shapes, both measured:
///
/// - a **reserved DOS device name** (`sub/NUL`, `CON.txt`) — the write "succeeds" into the device and
///   leaves nothing on disk, so a revert reported `applied: 1, skipped: []` against an empty tree;
/// - a **trailing dot or space** (`evil.txt `) — Win32 strips it on any non-verbatim path, so the entry
///   addresses `evil.txt`. In this engine that is **destructive**, not merely wrong: `plan_restore` sees
///   `a.txt ` and `a.txt` as two distinct keys and plans Create `a.txt ` + Delete `a.txt`. Writes run
///   first, so the Create lands *on* `a.txt`; the Delete then removes it. The user's real file is gone,
///   the tree is empty, and the report says `applied: 2, skipped: []` — complete success, from a planted
///   manifest, through a registered command.
///
/// **Why it lives in [`safe_segments`] rather than at a call site.** CPE-1823 twice put a guard on
/// `snapshot_capture::restore` — which has no production caller — while `apply_write` and `apply_delete`,
/// reached from `checkpoint_revert`/`checkpoint_revert_one`, went unguarded. All four `safe_target`
/// callers now inherit this by construction, and a fifth cannot forget it.
///
/// **`cfg!(windows)`-gated**, like the `:`/`\` rule above it and for the same reason: on Linux and macOS
/// `NUL` and `notes. ` are ordinary distinct filenames that a capture will store and a revert must
/// restore. Reuses [`crate::fsutil::win32_name_is_unstable`] and
/// [`crate::transfer::is_windows_device_name`] — the crate's existing predicates, not a third spelling.
///
/// **A host property, not a manifest property — recorded rather than assumed away.** The gate asks about
/// the machine doing the restoring, so a manifest captured on Linux carrying `a\b` or `notes. ` restores
/// as a literal name there and is refused here. That is the correct direction — refuse where the hazard
/// is — but it does mean the same manifest is legal on one machine and not another: the round-trip
/// asymmetry is *reduced* by these gates, not eliminated.
///
/// **The member of this class that is still open, so nobody reads the above as "the class is shut":**
/// a case-sensitive capture holding both `A.txt` and `a.txt` still collapses onto one file when restored
/// to a case-insensitive volume — two distinct manifest entries, one surviving file, no error. Same
/// shape as the trailing-space case and not closed here: it is not a *name* this predicate can look at,
/// since either name alone is perfectly legal and only the pair is a problem, so catching it means a
/// collision check across the whole manifest rather than a per-segment rule. Pre-existing, out of scope
/// for CPE-1823, and written down rather than left for the next reader to rediscover.
fn win32_addresses_a_different_path(seg: &str) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    if crate::fsutil::win32_name_is_unstable(seg) {
        return Some(format!(
            "the component {seg:?} ends in a dot or space, which Windows strips — it would address a \
             different path than it names, silently colliding with whatever answers to the stripped one"
        ));
    }
    if crate::transfer::is_windows_device_name(seg) {
        return Some(format!(
            "the component {seg:?} is a reserved DOS device name — the write would succeed into the \
             device and leave nothing on disk, reporting work that did not happen"
        ));
    }
    None
}

/// Rebuild the real, validated target path under `root` from `rel`'s `/`-segments via [`Path::join`]
/// (portable — never string concatenation), or an error if `rel` escapes `root`. `pub(crate)` so sibling
/// command-layer modules needing the same "resolve a caller-supplied relative path safely under a root"
/// guard (e.g. [`crate::checkpoint_store`]'s per-file diff, CPE-1197) reuse this rather than duplicating
/// the segment validation.
pub(crate) fn safe_target(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let segments = safe_segments(rel)?;
    let mut p = root.to_path_buf();
    for seg in segments {
        p.push(seg);
    }
    // **Resolved containment, for every caller (CPE-1823 round 3).** [`safe_segments`] is textual, and a
    // textual check cannot see a symlink or junction planted at an *interior* component: `sub/x.txt` is
    // four innocent characters and a slash, and if `root/sub` leads out of the tree then the write, or
    // the `remove_file`, lands outside it. Nothing about the spelling gives that away.
    //
    // This was closed in `snapshot_capture::restore` — which has no production caller — and left open in
    // `apply_write` and `apply_delete`, which the registered `checkpoint_revert` commands reach. That is
    // the third time in this ticket a guard landed on the function with no callers, and it was found by
    // walking every manifest-derived value to its sink rather than by another review. Putting it here
    // rather than at the call sites is the same conclusion as the Win32 rule above: the callers cannot
    // drift if they do not each own a copy.
    //
    // `confined_to` canonicalises and fails closed, and it handles a target that does not exist yet by
    // walking up to the nearest ancestor that does — which is why `restore` can still be handed a
    // destination it is about to create, provided the destination itself exists by the time this runs.
    if !crate::fsutil::confined_to(&p, root) {
        return Err(format!("escapes dest_root: {rel:?} resolves outside {}", root.display()));
    }
    Ok(p)
}

/// Number of `/`-separated segments in `rel` — used only to order deletes deepest-first. A malformed
/// path (caught later by [`safe_segments`]) simply sorts by its raw segment count; it will be refused
/// before anything is written regardless of where it lands in delete order.
fn segment_depth(rel: &str) -> usize {
    rel.split('/').count()
}

fn apply_write(
    action: &RestoreAction,
    dest_root: &Path,
    blobs_dir: &Path,
    checkpoint: &Snapshot,
) -> Result<(), String> {
    let target = safe_target(dest_root, &action.path)?;
    let Some(state) = checkpoint.get(&action.path) else {
        return Err("no checkpoint entry for this path".to_string());
    };
    // CPE-1823: `state` comes from `snapshot_capture::manifest_snapshot` — the same planted JSON — and
    // `hash` was joined onto `blobs_dir` with nothing checking it, so `../../<anywhere>/secret` copied
    // any readable file into the reverted tree and the report counted it **applied**. This is the sink
    // the app actually ships: `checkpoint_revert` / `checkpoint_revert_one` reach it from registered
    // Tauri commands, where `snapshot_capture::restore` has no production caller at all.
    //
    // One shared validator (`snapshot_capture::blob_source`), never a second spelling of the hex rule.
    // The refusal lands in this action's `Err`, which `apply_restore` records as a per-file
    // skip-with-reason in the report — this engine's existing loud channel, not a silent drop.
    let blob = crate::snapshot_capture::blob_source(blobs_dir, &state.hash)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::copy(&blob, &target).map_err(|e| format!("{}: {e}", blob.display()))?;
    Ok(())
}

fn apply_delete(action: &RestoreAction, dest_root: &Path) -> Result<(), String> {
    let target = safe_target(dest_root, &action.path)?;
    fs::remove_file(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore_plan::plan_restore;
    use crate::snapshot_capture::scan_dir;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-revert-{tag}"))
    }

    /// Write every file in `snapshot` (paths relative, `/`-joined) as a blob in `store_dir/blobs/<hash>`,
    /// content taken from `contents` (path -> bytes). Minimal hand-rolled store setup for these tests —
    /// deliberately not reusing `snapshot_capture::capture` so the test doesn't depend on its internals.
    fn write_blobs(store_dir: &Path, checkpoint: &Snapshot, contents: &[(&str, &[u8])]) {
        let blobs = store_dir.join("blobs");
        fs::create_dir_all(&blobs).unwrap();
        for (path, bytes) in contents {
            let hash = &checkpoint.get(*path).unwrap().hash;
            fs::write(blobs.join(hash), bytes).unwrap();
        }
    }

    #[test]
    fn round_trips_a_mutated_tree_back_to_the_checkpoint() {
        let root = scratch("rt-root");
        let store = scratch("rt-store");

        // Build the checkpoint state on disk.
        fs::write(root.join("a.txt"), b"checkpoint a").unwrap();
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested").join("b.txt"), b"checkpoint b").unwrap();
        fs::write(root.join("gone_later.txt"), b"will be deleted after checkpoint").unwrap();

        let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
        write_blobs(
            &store,
            &checkpoint,
            &[
                ("a.txt", b"checkpoint a"),
                ("nested/b.txt", b"checkpoint b"),
                ("gone_later.txt", b"will be deleted after checkpoint"),
            ],
        );

        // Mutate: edit a.txt, delete gone_later.txt, add a new file.
        fs::write(root.join("a.txt"), b"edited a").unwrap();
        fs::remove_file(root.join("gone_later.txt")).unwrap();
        fs::write(root.join("added.txt"), b"new since checkpoint").unwrap();

        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 3, "overwrite a.txt, create gone_later.txt, delete added.txt");

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);
        assert!(report.skipped.is_empty(), "skipped: {:?}", report.skipped);
        assert_eq!(report.applied, 3);

        let restored = scan_dir(&root.to_string_lossy()).unwrap();
        assert_eq!(restored, checkpoint, "tree matches the checkpoint content-for-content");
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"checkpoint a");
        assert_eq!(fs::read(root.join("gone_later.txt")).unwrap(), b"will be deleted after checkpoint");
        assert!(!root.join("added.txt").exists());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn a_missing_blob_is_skipped_while_the_rest_of_the_plan_applies() {
        let root = scratch("missing-blob-root");
        let store = scratch("missing-blob-store");

        fs::write(root.join("ok.txt"), b"checkpoint ok").unwrap();
        fs::write(root.join("broken.txt"), b"checkpoint broken").unwrap();
        let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
        // Only store the blob for ok.txt — broken.txt's hash points at a blob that never gets written,
        // a portable stand-in for "the blob store is missing/corrupt for one file" that doesn't require
        // OS-specific permission tricks.
        write_blobs(&store, &checkpoint, &[("ok.txt", b"checkpoint ok")]);

        fs::write(root.join("ok.txt"), b"edited ok").unwrap();
        fs::write(root.join("broken.txt"), b"edited broken").unwrap();
        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 2);

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);
        assert_eq!(report.applied, 1, "ok.txt restores");
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, "broken.txt");
        assert_eq!(fs::read(root.join("ok.txt")).unwrap(), b"checkpoint ok");
        assert_eq!(
            fs::read(root.join("broken.txt")).unwrap(),
            b"edited broken",
            "the failed restore leaves the current content untouched"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn an_escaping_plan_path_is_refused_and_writes_nothing_outside_dest_root() {
        let root = scratch("escape-root");
        let store = scratch("escape-store");
        fs::create_dir_all(store.join("blobs")).unwrap();

        let mut checkpoint = Snapshot::new();
        checkpoint.insert(
            "../escape.txt".to_string(),
            crate::restore_plan::FileState::new("deadbeef", 4),
        );
        fs::write(store.join("blobs").join("deadbeef"), b"evil").unwrap();

        let plan = vec![RestoreAction { path: "../escape.txt".to_string(), op: RestoreOp::Create }];
        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(report.applied, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, "../escape.txt");
        // The escape target, one level above root, must not have been written.
        assert!(!root.parent().unwrap().join("escape.txt").exists());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1823 round 2 — the sink the shipped app actually reaches.** `snapshot_capture::restore` has
    /// no production caller; *this* function does, via `checkpoint_revert` / `checkpoint_revert_one`.
    /// `state` comes from `manifest_snapshot` — the same hand-editable JSON — and its `hash` was joined
    /// onto `blobs/` unvalidated, so a climbing path pulled any readable file into the reverted tree and
    /// the report counted it **applied**.
    ///
    /// The harm assertion is on the file's *content*: the target path is one the plan legitimately names,
    /// so its existence proves nothing — only whether it holds the victim's bytes does.
    #[test]
    fn cpe_1823_an_escaping_hash_never_reads_a_file_outside_the_blob_store() {
        const SECRET: &[u8] = b"THE VICTIM PRIVATE KEY FROM OUTSIDE THE STORE";
        const LIVE: &[u8] = b"the user's real current content";
        let root = scratch("cpe1823-hash-root");
        let store = scratch("cpe1823-hash-store");
        let secrets = scratch("cpe1823-hash-secrets");
        fs::write(secrets.join("id_rsa"), SECRET).unwrap();
        fs::create_dir_all(store.join("blobs")).unwrap();

        // `blobs/` is `<store>/blobs`, so `../../<sibling>/id_rsa` climbs store → temp and back down.
        let hash = format!("../../{}/id_rsa", secrets.file_name().unwrap().to_string_lossy());
        let mut checkpoint = Snapshot::new();
        checkpoint
            .insert("a.txt".to_string(), crate::restore_plan::FileState::new(hash.as_str(), SECRET.len() as u64));

        fs::write(root.join("a.txt"), LIVE).unwrap();
        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        let landed = fs::read(root.join("a.txt")).unwrap();
        assert_ne!(
            landed, SECRET,
            "HARM: the revert pulled {} bytes from outside the blob store into the user's tree",
            SECRET.len()
        );
        assert_eq!(landed, LIVE, "a refused entry must leave the live file exactly as it was");
        assert_eq!(report.applied, 0, "a refused entry must never be counted applied");
        assert_eq!(report.skipped.len(), 1, "and it must be reported, not dropped: {report:?}");
        assert_eq!(report.skipped[0].0, "a.txt");
        assert!(
            report.skipped[0].1.contains("hex"),
            "the skip reason must say what was wrong, got: {}",
            report.skipped[0].1
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1823 round 2.** `safe_segments` refused `:` and `\` on every platform. That was survivable
    /// while this engine was the only caller — it skips one file and continues — but
    /// `snapshot_capture::restore` now shares the rule and aborts the whole manifest, so an ordinary
    /// Unix filename would have turned into a half-restored tree. On macOS this is routine, not exotic:
    /// a Finder name containing `/` is stored on disk as `:`.
    ///
    /// Nothing covered this before, which is why the ubuntu CI leg was green while the regression was
    /// live. Containment on Unix never rested on the colon rule — `..`, `.`, empty and absolute segments
    /// are all still refused above it, and `an_escaping_plan_path_is_refused_and_writes_nothing_outside_dest_root`
    /// still proves that on the same platform.
    #[cfg(unix)]
    #[test]
    fn cpe_1823_a_colon_or_backslash_is_an_ordinary_unix_filename_and_still_reverts() {
        for name in ["2026-08-21 10:30 notes.txt", r"Q1\Q2 report.txt"] {
            let root = scratch("cpe1823-unix-name-root");
            let store = scratch("cpe1823-unix-name-store");

            fs::write(root.join(name), b"checkpoint content").unwrap();
            let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
            assert!(checkpoint.contains_key(name), "the scan must key it verbatim: {checkpoint:?}");
            write_blobs(&store, &checkpoint, &[(name, b"checkpoint content")]);

            fs::write(root.join(name), b"edited since the checkpoint").unwrap();
            let current = scan_dir(&root.to_string_lossy()).unwrap();
            let plan = plan_restore(&checkpoint, &current);
            let report =
                execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

            assert!(report.skipped.is_empty(), "{name:?} is a legal Unix filename: {:?}", report.skipped);
            assert_eq!(report.applied, 1);
            assert_eq!(fs::read(root.join(name)).unwrap(), b"checkpoint content");

            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&store);
        }
    }

    /// **CPE-1823 round 3 — the destructive one, and the reason this guard is not in `restore`.** A
    /// manifest entry `a.txt ` (one trailing space) against a live `a.txt` holding the user's real
    /// content. `plan_restore` sees two distinct keys, so it plans Create `a.txt ` + Delete `a.txt`.
    /// Writes run before deletes, Win32 strips the space so the Create lands **on** `a.txt`, and the
    /// Delete then removes it. Measured before the fix:
    ///
    /// ```text
    /// report = RestoreReport { applied: 2, skipped: [] }
    /// tree after revert = []
    /// a.txt = Err(NotFound)
    /// ```
    ///
    /// The user's file is gone and the command reports complete success. The harm assertion is that the
    /// file still holds its bytes — asserting on the report alone would pass on the unfixed code, which
    /// reports success precisely while destroying the file.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_a_trailing_space_entry_never_destroys_the_file_it_aliases() {
        const LIVE: &[u8] = b"the user's only copy of their real content";
        let root = scratch("cpe1823-alias-root");
        let store = scratch("cpe1823-alias-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::write(store.join("blobs").join("deadbeef"), b"attacker bytes").unwrap();

        fs::write(root.join("a.txt"), LIVE).unwrap();
        let mut checkpoint = Snapshot::new();
        checkpoint.insert("a.txt ".to_string(), crate::restore_plan::FileState::new("deadbeef", 14));

        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 2, "the premise: a Create for `a.txt ` and a Delete for `a.txt`");

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(
            fs::read(root.join("a.txt")).ok().as_deref(),
            Some(LIVE),
            "HARM: the revert destroyed the user's file via a trailing-space alias — report was {report:?}"
        );
        assert!(
            report.skipped.iter().any(|(p, _)| p == "a.txt "),
            "the aliasing entry must be reported as skipped, not applied: {report:?}"
        );
        // Refusing the write alone did NOT save the file — the paired Delete finished the job. The
        // stand-down has to be visible in the report too, or the next reader will "simplify" it away.
        assert!(
            report.skipped.iter().any(|(p, why)| p == "a.txt" && why.contains("not deleted")),
            "the paired delete must be held back and said so: {report:?}"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1823 round 3.** A reserved device name through the *shipping* path. Before the fix:
    /// `RestoreReport { applied: 1, skipped: [] }` with `tree=[]` — the copy "succeeded" into the null
    /// device, so the engine reported restoring a file that is not on disk. `restore()` refused the same
    /// entry, which is exactly the asymmetry: the guard was on the function with no callers.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_a_device_name_entry_is_never_reported_applied_with_nothing_on_disk() {
        let root = scratch("cpe1823-dev-root");
        let store = scratch("cpe1823-dev-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::write(store.join("blobs").join("deadbeef"), b"evil").unwrap();

        let mut checkpoint = Snapshot::new();
        checkpoint.insert("sub/NUL".to_string(), crate::restore_plan::FileState::new("deadbeef", 4));
        let plan = vec![RestoreAction { path: "sub/NUL".to_string(), op: RestoreOp::Create }];

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(
            report.applied, 0,
            "a write into the null device leaves nothing on disk, so counting it applied reports work \
             that did not happen: {report:?}"
        );
        assert_eq!(report.skipped.len(), 1, "and it must be reported: {report:?}");
        assert_eq!(report.skipped[0].0, "sub/NUL");
        assert!(!root.join("sub").exists(), "and it must refuse before creating the parent directory");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// The stand-down rule itself, on **every** platform — the Windows aliasing leg above is the reason
    /// it exists, but the rule is not about aliasing and must not be pinned only where the alias is.
    /// A delete's justification is "this path is not in the checkpoint"; a skipped write means the
    /// checkpoint was not established, so that justification is unavailable for every delete in the plan.
    ///
    /// Uses a missing blob as the portable stand-in for a skipped write (the same device
    /// `a_missing_blob_is_skipped_while_the_rest_of_the_plan_applies` uses), so this runs on all three
    /// CI legs rather than only where a device name or trailing space means something.
    #[test]
    fn cpe_1823_a_skipped_write_holds_back_every_delete_in_the_plan() {
        const KEEP: &[u8] = b"a file the user added after the checkpoint";
        let root = scratch("cpe1823-standdown-root");
        let store = scratch("cpe1823-standdown-store");

        fs::write(root.join("restored.txt"), b"checkpoint content").unwrap();
        let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
        fs::create_dir_all(store.join("blobs")).unwrap(); // …but never write the blob: the write will skip

        fs::write(root.join("restored.txt"), b"edited since").unwrap();
        fs::write(root.join("added.txt"), KEEP).unwrap();
        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 2, "the premise: one Overwrite that will skip, and one Delete");

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(
            fs::read(root.join("added.txt")).ok().as_deref(),
            Some(KEEP),
            "a delete must not run while any checkpoint entry could not be restored: {report:?}"
        );
        assert_eq!(report.applied, 0, "nothing was applied: {report:?}");
        assert!(
            report.skipped.iter().any(|(p, why)| p == "added.txt" && why.contains("not deleted")),
            "and the held-back delete must be reported with its reason, never silently dropped: {report:?}"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1823 round 3 — found by walking every manifest-derived value to its sink, not by review.**
    /// `safe_segments` is textual, and `sub/x.txt` is textually spotless; if `root/sub` is a junction
    /// leading out of the tree then both the write and the delete land outside it. `restore` had the
    /// resolved-containment check and these two — the ones the registered `checkpoint_revert` commands
    /// actually reach — did not. Third instance of the same asymmetry in this ticket.
    ///
    /// Both directions are staged, because `apply_delete` is the one that destroys something that was
    /// never ours: a `remove_file` through the link deletes the victim's file outright.
    #[test]
    fn cpe_1823_a_link_at_an_interior_component_never_redirects_a_revert_out_of_the_tree() {
        const VICTIM: &[u8] = b"a file that has nothing to do with this checkpoint";
        let root = scratch("cpe1823-revlink-root");
        let store = scratch("cpe1823-revlink-store");
        let outside = scratch("cpe1823-revlink-outside");
        fs::write(outside.join("victim.txt"), VICTIM).unwrap();
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::write(store.join("blobs").join("deadbeef"), b"attacker bytes").unwrap();

        let link = root.join("sub");
        if !crate::fsutil::make_dir_link(&outside, &link) {
            crate::skip_notice!(
                "[CPE-1823] SKIPPED the revert interior-link leg: this machine could not create a \
                 directory link at {} (no symlink privilege and no junction). NOTHING on this run \
                 covered a revert reaching THROUGH a link out of the reverted tree.",
                link.display()
            );
            return;
        }

        let mut checkpoint = Snapshot::new();
        checkpoint.insert("sub/planted.txt".to_string(), crate::restore_plan::FileState::new("deadbeef", 14));
        let plan = vec![
            RestoreAction { path: "sub/planted.txt".to_string(), op: RestoreOp::Create },
            RestoreAction { path: "sub/victim.txt".to_string(), op: RestoreOp::Delete },
        ];

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert!(
            !outside.join("planted.txt").exists(),
            "HARM: the revert wrote through the planted link, outside the reverted tree: {report:?}"
        );
        assert_eq!(
            fs::read(outside.join("victim.txt")).ok().as_deref(),
            Some(VICTIM),
            "HARM: the revert DELETED a file outside the reverted tree through the planted link: {report:?}"
        );
        assert_eq!(report.applied, 0, "neither action may be counted applied: {report:?}");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn an_absolute_plan_path_is_refused() {
        let root = scratch("absolute-root");
        let store = scratch("absolute-store");
        fs::create_dir_all(store.join("blobs")).unwrap();

        #[cfg(unix)]
        let abs_path = "/etc/passwd";
        #[cfg(windows)]
        let abs_path = "C:/evil.txt";

        let mut checkpoint = Snapshot::new();
        checkpoint.insert(abs_path.to_string(), crate::restore_plan::FileState::new("deadbeef", 4));

        let plan = vec![RestoreAction { path: abs_path.to_string(), op: RestoreOp::Create }];
        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(report.applied, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, abs_path);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn deletes_apply_deepest_first() {
        let root = scratch("deep-delete-root");
        let store = scratch("deep-delete-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/c.txt"), b"deep").unwrap();
        fs::write(root.join("top.txt"), b"shallow").unwrap();

        let checkpoint = Snapshot::new(); // both files are new-since-checkpoint -> deletes
        let plan = vec![
            RestoreAction { path: "top.txt".to_string(), op: RestoreOp::Delete },
            RestoreAction { path: "a/b/c.txt".to_string(), op: RestoreOp::Delete },
        ];

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);
        assert_eq!(report.applied, 2);
        assert!(report.skipped.is_empty());
        assert!(!root.join("a/b/c.txt").exists());
        assert!(!root.join("top.txt").exists());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }
}
