//! Backup copy engine (CPE-797, epic CPE-736): execute a plan produced by the frontend `planBackup` —
//! copy new files, overwrite changed ones, and (mirror mode) delete extraneous files under the dest root,
//! verifying each written file by SHA-256. Plan lists are **relative paths** under the source/dest roots,
//! so the engine never widens the blast radius beyond `dest_root`. Per-file [`OpResult`] (never
//! all-or-nothing) so a single locked file doesn't sink the whole run. Pure and Tauri-free (CPE-815/821);
//! reuses `cpe_server::model::OpResult` + `cpe_server::fsutil::sha256_file`. Follows the streaming split:
//! the walker takes a `flush(OpResult)` callback so the collect command and the streaming command
//! (`ipc::Channel`, in the app) both drive it.

use std::path::{Path, PathBuf};

use crate::fsutil::sha256_file;
use crate::model::OpResult;

/// Join a `dest_root` with a plan-relative path, rejecting anything that would escape the root (`..`,
/// absolute, or a Windows drive prefix) **or resolve to the root itself**, so a malformed plan can't
/// reach outside — or swallow the whole of — the backup target.
///
/// **CPE-1664:** `Component::CurDir` used to be accepted here, which made `"."` join to `dest_root`
/// itself; the mirror-delete loop below then saw a directory and called `remove_dir_all(dest_root)`,
/// recursively destroying the entire destination tree from one plan entry (verified by
/// `safe_join_refuses_a_plan_entry_that_names_the_root_itself`). The empty string is the same hole by a
/// different spelling — `root.join("")` is `root` — so a plan entry must now contain at least one
/// `Normal` component and nothing else. This is a **correctness fix independent of any consent gate**: a
/// backup-plan entry naming its own root is malformed however the call arrived. Real plans from
/// `planBackup` (src/lib/backup.ts) are always `name` / `sub/name` relative paths, never `.`, `""`, or a
/// `./`-prefixed path, so nothing legitimate is turned away.
fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(rel);
    let mut named = 0usize;
    for comp in candidate.components() {
        use std::path::Component;
        match comp {
            Component::Normal(_) => named += 1,
            _ => return Err(format!("unsafe path in plan: {rel}")),
        }
    }
    if named == 0 {
        return Err(format!("plan entry names the backup root itself, not a path inside it: {rel:?}"));
    }
    Ok(root.join(candidate))
}

/// The refusal message [`apply_backup_plan_walk`] returns when `confirmed` is `false` (CPE-1664).
/// Shared with the tests so the assertion can't drift from the text the user actually sees.
pub const BACKUP_NOT_CONFIRMED: &str =
    "refusing to run the backup plan: `confirmed` was not set on this apply_backup_plan call — a \
     mirror plan deletes files under the destination root outright (no Recycle Bin copy, no undo), so \
     it must be re-invoked with an explicit confirmation (only BackupDashboard's Run/Restore buttons, \
     or the drive-connect scheduler acting on a job the user ticked auto-run for, should ever set it)";

/// Copy one file from `src` to `dst`, creating parent dirs, then optionally verify by sha256.
fn copy_one_verified(src: &Path, dst: &Path, verify: bool) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(src, dst).map_err(|e| e.to_string())?;
    if verify {
        let a = sha256_file(src).map_err(|e| e.to_string())?;
        let b = sha256_file(dst).map_err(|e| e.to_string())?;
        if a != b {
            return Err("checksum mismatch after copy".into());
        }
    }
    Ok(())
}

/// The shared plan executor: run the copy/update/mirror-delete plan, invoking `emit` with each per-file
/// [`OpResult`] as it completes. The collect helper and the streaming command both drive this — one
/// walker, two surfaces (per docs/design/STREAMING.md).
///
/// **Refuses the whole plan up front** ([`Err`], nothing copied, nothing deleted, `emit` never called)
/// when `confirmed` is `false` — CPE-1664, the same shape CPE-1611 gave `secure_shred::shred_paths`,
/// CPE-1630 gave `vault_manager::create_vault`, and CPE-1651 gave `delete_permanent`/`empty_trash`. The
/// gate lives in the engine rather than in the Tauri command so that **both** dispatchers (the
/// collect-to-vec one and the streaming twin) are covered by construction, and so a test exercises the
/// same entry point a forged IPC call reaches.
///
/// The gate deliberately covers the **whole** call, not just the `delete` list, unlike CPE-1662's
/// narrower Overwrite-only gate on `start_transfer`: a backup run is a single deliberate act behind one
/// button (Run/Restore in `BackupDashboard.svelte`, or a job the user ticked auto-run for), so there is
/// no routine non-destructive traffic here that a blanket prompt would train the user to click through.
///
/// **Be precise about what this defends — it is UI discipline enforced in Rust, NOT an authorization
/// boundary.** `confirmed` rides on the same IPC message as `dest_root` and `delete`, so a caller able
/// to forge the call can set the flag too. What it genuinely stops: a call site that runs a plan without
/// the user ever pressing anything; a replayed pre-CPE-1664 payload (serde gives `confirmed` no default,
/// so the old argument shape now fails to deserialize outright); and a mechanical enumerator working
/// from `bindings.gen.ts` that doesn't know the field exists. What it does **not** do is stop a
/// deliberate attacker already on the IPC surface — the fix that holds regardless of the caller is
/// `safe_join`'s rejection of a plan entry naming the root, above.
// One over clippy's threshold since CPE-1664 added `confirmed`. The list mirrors the plan the frontend
// sends; bundling it into a struct would only move the same fields somewhere less readable, and the
// consent flag in particular is deliberately a positional argument the caller cannot forget.
#[allow(clippy::too_many_arguments)]
pub fn apply_backup_plan_walk(
    source_root: &str,
    dest_root: &str,
    copy: &[String],
    update: &[String],
    delete: &[String],
    verify: bool,
    confirmed: bool,
    mut emit: impl FnMut(OpResult),
) -> Result<(), String> {
    // CONFIRM GATE (CPE-1664): checked first, before a single plan entry is joined, inspected, or
    // stat'd. Refuses cleanly — never a panic, never a partial run.
    if !confirmed {
        return Err(BACKUP_NOT_CONFIRMED.to_string());
    }

    let src_root = PathBuf::from(source_root);
    let dst_root = PathBuf::from(dest_root);

    for rel in copy.iter().chain(update.iter()) {
        let (src, dst) = match (safe_join(&src_root, rel), safe_join(&dst_root, rel)) {
            (Ok(s), Ok(d)) => (s, d),
            (Err(e), _) | (_, Err(e)) => {
                emit(OpResult::err(Path::new(rel), e));
                continue;
            }
        };
        match copy_one_verified(&src, &dst, verify) {
            Ok(()) => emit(OpResult::ok(&dst)),
            Err(e) => emit(OpResult::err(&dst, e)),
        }
    }

    for rel in delete {
        let dst = match safe_join(&dst_root, rel) {
            Ok(d) => d,
            Err(e) => {
                emit(OpResult::err(Path::new(rel), e));
                continue;
            }
        };
        let result = if dst.is_dir() {
            std::fs::remove_dir_all(&dst)
        } else {
            std::fs::remove_file(&dst)
        };
        match result {
            Ok(()) => emit(OpResult::ok(&dst)),
            Err(e) => emit(OpResult::err(&dst, e)),
        }
    }

    Ok(())
}

/// Collect-to-vec backup run: apply the plan and return one [`OpResult`] per attempted file. `confirmed`
/// is the CPE-1664 consent gate — see [`apply_backup_plan_walk`]; an unconfirmed call is [`Err`] with
/// nothing touched, not an empty result list.
pub fn apply_backup_plan(
    source_root: &str,
    dest_root: &str,
    copy: &[String],
    update: &[String],
    delete: &[String],
    verify: bool,
    confirmed: bool,
) -> Result<Vec<OpResult>, String> {
    let mut out = Vec::with_capacity(copy.len() + update.len() + delete.len());
    apply_backup_plan_walk(source_root, dest_root, copy, update, delete, verify, confirmed, |r| {
        out.push(r)
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-backup-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Build a destination tree worth losing: a top-level file, a nested directory, and a file inside
    /// it. Every CPE-1664 refusal test reads all three back **off disk** afterwards — asserting the
    /// return value alone is what let this class of bug survive three tickets.
    fn victim_dest(d: &std::path::Path) -> std::path::PathBuf {
        let dst = d.join("dst");
        fs::create_dir_all(dst.join("nested")).unwrap();
        fs::write(dst.join("taxes.docx"), b"irreplaceable").unwrap();
        fs::write(dst.join("nested/deep.txt"), b"also irreplaceable").unwrap();
        dst
    }

    /// Assert the whole victim tree is still on disk, by listing it and reading the nested file back.
    fn assert_victim_intact(dst: &std::path::Path, when: &str) {
        assert!(dst.is_dir(), "the destination root must still exist {when}");
        let names: Vec<String> = fs::read_dir(dst)
            .unwrap_or_else(|e| panic!("the destination root must still be listable {when}: {e}"))
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n == "taxes.docx"), "taxes.docx must survive {when}: {names:?}");
        assert!(names.iter().any(|n| n == "nested"), "nested/ must survive {when}: {names:?}");
        assert_eq!(
            fs::read(dst.join("nested/deep.txt")).unwrap_or_default(),
            b"also irreplaceable",
            "the nested file must survive {when} — the remove_dir_all arm is what this covers"
        );
    }

    /// CPE-1664, the exploit as filed: one IPC message with `deletePaths: ["."]` reached
    /// `remove_dir_all(dest_root)` and annihilated the whole destination tree. The consent gate refuses
    /// the call before a single entry is joined — proved by reading the tree back off disk.
    #[test]
    fn apply_backup_plan_refuses_the_whole_plan_when_not_confirmed() {
        let d = scratch("unconfirmed");
        let src = d.join("src");
        fs::create_dir_all(&src).unwrap();
        let dst = victim_dest(&d);

        for delete in [vec![".".to_string()], vec!["taxes.docx".to_string()], vec![String::new()]] {
            let outcome = apply_backup_plan(
                &src.to_string_lossy(),
                &dst.to_string_lossy(),
                &[],
                &[],
                &delete,
                false,
                false, // confirmed
            );
            // Disk first, deliberately: the off-disk check is the assertion carrying the claim, so it is
            // the one that trips on an ungated build — not a return-value check tripping ahead of it.
            assert_victim_intact(&dst, &format!("an unconfirmed plan with deletePaths {delete:?}"));
            let err = outcome.expect_err("an unconfirmed backup plan must be refused, not executed");
            assert!(err.contains("`confirmed` was not set"), "the refusal must name the flag: {err}");
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// The gate must also cover the streaming twin, and an unconfirmed call must be **inert** — not
    /// "inert but it emitted a pile of per-file errors first".
    #[test]
    fn apply_backup_plan_walk_emits_nothing_at_all_when_not_confirmed() {
        let d = scratch("unconfirmed_stream");
        let src = d.join("src");
        fs::create_dir_all(&src).unwrap();
        let dst = victim_dest(&d);

        let mut emitted = 0usize;
        let outcome = apply_backup_plan_walk(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &[],
            &[],
            &[".".to_string()],
            false,
            false, // confirmed
            |_| emitted += 1,
        );
        assert_victim_intact(&dst, "an unconfirmed streamed plan"); // disk first — see the test above
        let err = outcome.expect_err("an unconfirmed walk must be refused");
        assert!(err.contains("`confirmed` was not set"));
        assert_eq!(emitted, 0, "a refused plan must not emit a single result");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1664's **second, independent** fix: `safe_join` itself rejects an entry that resolves to the
    /// root, so a *consented* mirror plan still can't delete its own destination root. Called directly
    /// per the ticket's acceptance criterion, then re-proved through the whole engine with
    /// `confirmed: true` so the disk is the witness.
    #[test]
    fn safe_join_refuses_a_plan_entry_that_names_the_root_itself() {
        let root = Path::new("/backup/root");
        for rel in [".", "", "./", "sub/.."] {
            let e = safe_join(root, rel)
                .expect_err("an entry resolving to the root itself must be rejected");
            assert!(!e.is_empty(), "the rejection must carry a reason for {rel:?}");
        }
        // A normal relative entry — what `planBackup` actually emits — still joins.
        assert_eq!(safe_join(root, "sub/a.txt").unwrap(), root.join("sub/a.txt"));

        let d = scratch("curdir");
        let src = d.join("src");
        fs::create_dir_all(&src).unwrap();
        let dst = victim_dest(&d);
        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &[],
            &[],
            &[".".to_string(), String::new()],
            false,
            true, // fully consented — the gate is NOT what is being tested here
        )
        .expect("a consented plan runs; the bad entries are per-entry errors, not a refusal");
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| !r.ok), "both root-naming entries must be rejected: {results:?}");
        assert_victim_intact(&dst, "a consented plan whose delete entry named the root");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn apply_backup_plan_copies_updates_and_verifies() {
        let d = scratch("apply");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(src.join("new.txt"), b"brand new").unwrap();
        fs::write(src.join("sub/edited.txt"), b"fresh contents").unwrap();
        fs::write(dst.join("edited.txt.placeholder"), b"x").unwrap(); // unrelated, must survive

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["new.txt".into()],
            &["sub/edited.txt".into()],
            &[],
            true, // verify by checksum
            true, // confirmed (CPE-1664)
        )
        .expect("a confirmed plan runs");
        assert!(results.iter().all(|r| r.ok), "all files should copy+verify: {results:?}");
        assert_eq!(fs::read(dst.join("new.txt")).unwrap(), b"brand new");
        assert_eq!(fs::read(dst.join("sub/edited.txt")).unwrap(), b"fresh contents"); // parent dir created
        assert!(dst.join("edited.txt.placeholder").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn apply_backup_plan_mirror_deletes_and_reports_per_file() {
        let d = scratch("mirror");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(dst.join("stale.txt"), b"old").unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &[],
            &[],
            &["stale.txt".into(), "never-existed.txt".into()],
            false,
            true, // confirmed (CPE-1664)
        )
        .expect("a confirmed plan runs");
        assert!(!dst.join("stale.txt").exists()); // mirror-delete removed the extraneous file
        // Two results: the real delete succeeds, the missing one is reported (not a panic).
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.ok));
        assert!(results.iter().any(|r| !r.ok));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn apply_backup_plan_rejects_paths_escaping_the_root() {
        let d = scratch("escape");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        for esc in ["../evil.txt", "sub/../../evil.txt"] {
            let results = apply_backup_plan(
                &src.to_string_lossy(),
                &dst.to_string_lossy(),
                &[esc.to_string()],
                &[],
                &[],
                false,
                true, // confirmed (CPE-1664) — traversal must be rejected on its own merits
            )
            .expect("a confirmed plan runs");
            assert_eq!(results.len(), 1);
            assert!(!results[0].ok, "{esc} should be rejected");
        }
        assert!(!d.join("evil.txt").exists()); // nothing written outside dst
        let _ = fs::remove_dir_all(&d);
    }
}
