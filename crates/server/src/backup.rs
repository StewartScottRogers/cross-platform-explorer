//! Backup copy engine (CPE-797, epic CPE-736): execute a plan produced by the frontend `planBackup` —
//! copy new files, overwrite changed ones, and (mirror mode) delete extraneous files under the dest root,
//! verifying each written file by SHA-256. Plan lists are **relative paths** under the source/dest roots,
//! so the engine never widens the blast radius beyond `dest_root`. Per-file [`OpResult`] (never
//! all-or-nothing) so a single locked file doesn't sink the whole run. Pure and Tauri-free (CPE-815/821);
//! reuses `cpe_server::model::OpResult` + `cpe_server::fsutil::sha256_file`. Follows the streaming split:
//! the walker takes a `flush(OpResult)` callback so the collect command and the streaming command
//! (`ipc::Channel`, in the app) both drive it.

use std::path::{Path, PathBuf};

use crate::fsutil::{sha256_file, win32_name_is_unstable};
use crate::model::OpResult;

/// Join a `dest_root` with a plan-relative path, rejecting anything that would escape the root (`..`,
/// absolute, or a Windows drive prefix) or address the root itself.
///
/// **This is the cheap first filter, NOT the guarantee.** The PR #855 security audit established why:
/// classifying components cannot work, because `Path::components()` special-cases exactly `.` and `..`
/// and reports **every** other string as [`std::path::Component::Normal`]. It drove 34 candidate
/// spellings through a fully consented `apply_backup_plan` and five wiped the root — `" "`, `"  "`,
/// `"..."`, `". "`, `" ."` — because Win32 strips trailing spaces and dots from a path's final
/// component, so `root.join(" ")` **is** `root`. A rule of "at least one `Normal` component" waved all
/// five through to `remove_dir_all(dest_root)` reporting `ok: true`. The guarantee is
/// [`contained_under`], asserted on the *resolved* path immediately before the destructive call.
///
/// What this function still does, cheaply and IO-free: reject non-`Normal` components (`..`, absolute
/// paths, drive prefixes, `.`), reject an entry with no components at all (`""`, since `root.join("")`
/// is `root`), and reject any component [`win32_name_is_unstable`] flags — which also catches the
/// milder aliasing variant where `"report. "` would delete `root/report`, the wrong file, silently.
///
/// **Not an attacker-only concern.** `deletePaths` is derived from a real listing —
/// `scan_tree(job.dest)` → `planBackup` → `p.delete` — and a directory named `" "` is creatable and
/// enumerates verbatim. It arrives from a Samba/NAS share (the QNAP is a live test target here), a
/// WSL-created name, or an extracted archive. The user presses **Run** on a mirror job, entirely
/// legitimately, and without these checks the whole backup destination is deleted.
///
/// Applied uniformly rather than under `#[cfg(windows)]` so all three CI legs exercise the same
/// behaviour — see [`win32_name_is_unstable`] for that trade-off.
fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(rel);
    let mut named = 0usize;
    for comp in candidate.components() {
        use std::path::Component;
        match comp {
            Component::Normal(part) => {
                if win32_name_is_unstable(&part.to_string_lossy()) {
                    return Err(format!(
                        "plan entry component {part:?} has trailing dots/spaces, which Windows strips — \
                         it would address a different path (often the backup root itself) than it names: \
                         {rel:?}"
                    ));
                }
                named += 1;
            }
            _ => return Err(format!("unsafe path in plan: {rel}")),
        }
    }
    if named == 0 {
        return Err(format!("plan entry names the backup root itself, not a path inside it: {rel:?}"));
    }
    Ok(root.join(candidate))
}

/// **The actual containment guarantee** for the mirror-delete loop (CPE-1664, PR #855 audit): assert on
/// the *resolved* path, not on the spelling that produced it.
///
/// Canonicalise both sides and require `joined` to be strictly **inside** `root` — `starts_with(root)`
/// **and** `!= root`. That is the only formulation that holds without enumerating spellings, so it
/// covers the five the audit found, the two found before it, and whatever normalisation quirk, junction,
/// case-folding share or Unicode-folding filesystem produces the next one. [`safe_join`]'s textual rules
/// are a cheap filter in front of it, never a substitute for it.
///
/// Failure modes are chosen so the destructive call is the one that gets refused:
/// - `root` won't canonicalise → refuse. There is nothing legitimate to delete under a root that
///   doesn't resolve.
/// - `joined` won't canonicalise → allow. The path does not exist, so no `remove_*` can destroy
///   anything, and the loop's normal per-entry "not found" error reporting stays intact.
fn contained_under(joined: &Path, root: &Path) -> Result<(), String> {
    let Ok(real_root) = std::fs::canonicalize(root) else {
        return Err(format!(
            "refusing to delete: the backup destination root {root:?} could not be resolved"
        ));
    };
    let Ok(real) = std::fs::canonicalize(joined) else {
        return Ok(()); // doesn't exist — nothing to destroy; the delete itself will report it
    };
    if real == real_root {
        return Err(
            "refusing to delete the backup destination root itself — a mirror plan may only delete \
             paths inside it"
                .to_string(),
        );
    }
    if !real.starts_with(&real_root) {
        return Err(format!(
            "refusing to delete {real:?}: it resolves outside the backup destination root {real_root:?}"
        ));
    }
    Ok(())
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
/// deliberate attacker already on the IPC surface.
///
/// The half that holds **regardless of what the caller sets `confirmed` to** is [`contained_under`],
/// asserted on the *resolved* delete path immediately before the `remove_dir_all`: canonicalise, then
/// require the target to be strictly inside the root. [`safe_join`]'s textual rules sit in front of it
/// as a cheap filter, but they are **not** the guarantee — an earlier version of this comment claimed
/// they were, and the PR #855 audit then wiped the root through five spellings that satisfied them.
/// State the protection as the containment assertion, never as a list of rejected spellings.
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
        // THE containment check (CPE-1664, PR #855 audit), on the RESOLVED path, immediately before the
        // destructive call — never trusting that `safe_join` above has enumerated every spelling.
        if let Err(e) = contained_under(&dst, &dst_root) {
            emit(OpResult::err(&dst, e));
            continue;
        }
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

        // `taxes.docx` FIRST, deliberately (PR #855 review, nit 3). It is the only entry here that
        // `safe_join` would happily join, so it is the only one whose disk assertion has teeth: with the
        // consent gate neutralised, this iteration reaches the delete loop and the run fails with
        // "taxes.docx must survive…". Put a root-naming spelling first and `safe_join` blocks it, the
        // `expect_err` below trips on iteration 1, and the case that actually proves the gate is never
        // reached — the ordering defeats its own disk-first design.
        for delete in [vec!["taxes.docx".to_string()], vec![".".to_string()], vec![String::new()]] {
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

    /// Every spelling of "this entry addresses the destination root itself" that is known to reach
    /// `remove_dir_all(dest_root)`. `"."` is the one the ticket was filed for; `""` was found while
    /// fixing it; the **five Windows ones were found by the PR #855 security audit**, which drove 34
    /// candidates through a *fully consented* `apply_backup_plan` and watched these wipe the tree with
    /// `ok: true`.
    ///
    /// The table exists so a regression names its spelling, **not** as the specification — the
    /// specification is `contained_under`'s containment assertion on the resolved path, which is why
    /// `contained_under_is_the_guarantee_not_the_spelling_list` exercises it directly rather than
    /// through this list. Enumerating spellings is exactly the approach the audit showed cannot work.
    const ROOT_SPELLINGS: [&str; 7] = [
        ".",    // the spelling the ticket was filed for
        "",     // `root.join("")` is `root`
        " ",    // Win32: trailing space stripped → `root`
        "  ",   // ditto, two
        "...",  // Win32: trailing dots stripped → `root`
        ". ",   // ditto, mixed
        " .",   // ditto, mixed the other way
    ];

    /// Names that Win32 rewrites to address a *different* path than they spell — the milder variant of
    /// the same bug: `root\report. ` opens `root\report`, so a mirror plan meaning to delete a stale
    /// `report. ` silently deletes `report` instead. Wrong file, no error, no undo.
    const ALIASING_SPELLINGS: [&str; 4] = ["taxes.docx ", "taxes.docx.", "nested ", "nested."];

    /// CPE-1664's **second, independent** fix: `safe_join` itself rejects an entry that resolves to the
    /// root, so a *consented* mirror plan still can't delete its own destination root. Called directly
    /// per the ticket's acceptance criterion, then re-proved through the whole engine with
    /// `confirmed: true` so the disk is the witness for every spelling.
    #[test]
    fn safe_join_refuses_a_plan_entry_that_names_the_root_itself() {
        let root = Path::new("/backup/root");
        let all = ROOT_SPELLINGS.iter().copied().chain(ALIASING_SPELLINGS).chain(["./", "sub/.."]);
        for rel in all {
            let e = safe_join(root, rel)
                .expect_err("an entry addressing something other than what it names must be rejected");
            assert!(!e.is_empty(), "the rejection must carry a reason for {rel:?}");
        }
        // Ordinary entries — what `planBackup` actually emits — still join untouched.
        assert_eq!(safe_join(root, "sub/a.txt").unwrap(), root.join("sub/a.txt"));
        assert_eq!(safe_join(root, "a b/c.txt").unwrap(), root.join("a b/c.txt"));
        assert_eq!(safe_join(root, ".gitignore").unwrap(), root.join(".gitignore"));

        // …and end to end, with full consent, against a real tree: every spelling is a per-entry error
        // and the victim survives. `confirmed: true` throughout — the gate is NOT what is under test.
        //
        // WINDOWS-AWARE (PR #855 audit): on Linux/macOS these names address real, distinct, absent files,
        // so the plan would report "not found" and the tree would survive *even with the fix removed* —
        // a green Linux leg must not be read as coverage. The refusal is asserted everywhere (the rules
        // are IO-free and uniform); only the *wipe* is Windows-specific, so the survival assertion is
        // annotated as load-bearing only there.
        let d = scratch("root_spellings");
        let src = d.join("src");
        fs::create_dir_all(&src).unwrap();
        let dst = victim_dest(&d);
        for rel in ROOT_SPELLINGS.iter().copied().chain(ALIASING_SPELLINGS) {
            let results = apply_backup_plan(
                &src.to_string_lossy(),
                &dst.to_string_lossy(),
                &[],
                &[],
                &[rel.to_string()],
                false,
                true, // fully consented
            )
            .expect("a consented plan runs; such an entry is a per-entry error, not a refusal");
            assert_eq!(results.len(), 1);
            assert!(
                !results[0].ok,
                "the entry {rel:?} must be REFUSED (this assertion is the one with teeth on every OS \
                 leg — the wipe below only reproduces on Windows): {results:?}"
            );
            assert_victim_intact(&dst, &format!("a consented plan whose delete entry was {rel:?}"));
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// **The guarantee, tested as a guarantee.** `contained_under` is asserted on the resolved path, so
    /// it holds for spellings nobody has enumerated — which is the whole lesson of the PR #855 audit
    /// (`Path::components()` reports every non-`.`/`..` string as `Normal`, so classification cannot
    /// work). Driven directly with real paths rather than through the spelling table on purpose.
    #[test]
    fn contained_under_is_the_guarantee_not_the_spelling_list() {
        let d = scratch("contained");
        let dst = victim_dest(&d);

        // The root itself, however it is reached.
        assert!(contained_under(&dst, &dst).is_err(), "the root itself must be refused");
        assert!(contained_under(&dst.join("nested/.."), &dst).is_err(), "…and a traversal back to it");
        // Outside the root entirely.
        assert!(contained_under(&d, &dst).is_err(), "the root's PARENT must be refused");
        // Real children must pass — the check must not break ordinary mirror deletes.
        assert!(contained_under(&dst.join("nested"), &dst).is_ok(), "a real child must be allowed");
        assert!(contained_under(&dst.join("taxes.docx"), &dst).is_ok(), "…and a real file");
        assert!(contained_under(&dst.join("nested/deep.txt"), &dst).is_ok(), "…and a nested file");
        // A path that doesn't exist can't destroy anything: allowed, so the delete's own "not found"
        // error is what the user sees (pinned by `apply_backup_plan_mirror_deletes_and_reports_per_file`).
        assert!(contained_under(&dst.join("never-existed.txt"), &dst).is_ok());
        // A root that doesn't resolve refuses everything — there is nothing legitimate to delete.
        assert!(contained_under(&dst.join("nested"), &d.join("no-such-root")).is_err());
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
