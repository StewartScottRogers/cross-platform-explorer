//! Backup copy engine (CPE-797, epic CPE-736): execute a plan produced by the frontend `planBackup` —
//! copy new files, overwrite changed ones, and (mirror mode) delete extraneous files under the dest root,
//! verifying each written file by SHA-256. Plan lists are **relative paths** under the source/dest roots,
//! so the engine never widens the blast radius beyond `dest_root`. Per-file [`OpResult`] (never
//! all-or-nothing) so a single locked file doesn't sink the whole run. Pure and Tauri-free (CPE-815/821);
//! reuses `cpe_server::model::OpResult` + `cpe_server::fsutil::sha256_file`. Follows the streaming split:
//! the walker takes a `flush(OpResult)` callback so the collect command and the streaming command
//! (`ipc::Channel`, in the app) both drive it.

use std::path::{Path, PathBuf};

use crate::fsutil::{contained_under, sha256_file, win32_name_is_unstable};
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
/// paths, drive prefixes, `.`) and reject an entry with no components at all (`""`, since
/// `root.join("")` is `root`). Whether it *also* rejects a [`win32_name_is_unstable`] component depends
/// on `entry` — see [`PlanEntry`].
///
/// **Not an attacker-only concern.** `deletePaths` is derived from a real listing —
/// `scan_tree(job.dest)` → `planBackup` → `p.delete` — and a directory named `" "` is creatable and
/// enumerates verbatim. It arrives from a Samba/NAS share (the QNAP is a live test target here), a
/// WSL-created name, or an extracted archive. The user presses **Run** on a mirror job, entirely
/// legitimately, and on Windows the whole backup destination used to be deleted.
fn safe_join(root: &Path, rel: &str, entry: PlanEntry) -> Result<PathBuf, String> {
    let candidate = Path::new(rel);
    let mut named = 0usize;
    for comp in candidate.components() {
        use std::path::Component;
        match comp {
            Component::Normal(part) => {
                if entry.refuses_unstable_names() && win32_name_is_unstable(&part.to_string_lossy()) {
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

/// Which half of a backup plan an entry came from — both halves now get the SAME, Windows-only
/// treatment of a [`win32_name_is_unstable`] name (CPE-1675; previously asymmetric, PR #855 round-3
/// review).
///
/// `foo ` and `notes.` are legal, creatable, everyday filenames on Linux and macOS, where `root/notes.`
/// is a real distinct path and nothing is aliased. The first version of this change refused them on
/// every platform and for both halves, so **a Linux backup of a file named `notes.` was silently never
/// copied** — breaking a basic operation on two platforms to defend against a hazard that exists only
/// on the third. Round 3 fixed [`PlanEntry::Write`] but kept [`PlanEntry::Delete`] refusing everywhere,
/// on the reasoning that a refused delete destroys nothing and is a reported no-op — true, but the
/// consequence was that **a POSIX mirror-backup job whose destination held a stale `notes.` refused that
/// delete on every run, forever**, so the job could never report clean (CPE-1675).
///
/// - [`PlanEntry::Write`] (the `copy` + `update` lists) — refuse only on Windows. Elsewhere the name
///   addresses exactly what it spells, so refusing it loses the user's file for no safety gain. The
///   failure mode of being wrong here is a file that never gets backed up, which is bad.
/// - [`PlanEntry::Delete`] (the mirror-delete list) — refuse only on Windows too, now. On POSIX the name
///   addresses exactly what it spells, so refusing the delete buys no safety and costs convergence.
///   [`cpe_server::fsutil::contained_under`] — asserted on the *resolved* delete path, platform-
///   independently — is what makes the delete safe there; it already permits it.
///
/// Neither setting is what carries the safety — [`cpe_server::fsutil::contained_under`] does, on the
/// resolved path, platform-independently.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PlanEntry {
    /// A `copy`/`update` entry: something is about to be **written** at this path.
    Write,
    /// A `delete` entry: something is about to be **removed** at this path.
    Delete,
}

impl PlanEntry {
    fn refuses_unstable_names(self) -> bool {
        // CPE-1675: both variants behave identically now — one combined arm (rather than two arms with
        // the same body) so a future third variant must be visited explicitly, and so clippy's
        // same-arms lint has nothing to flag.
        match self {
            PlanEntry::Write | PlanEntry::Delete => cfg!(windows),
        }
    }
}

/// The refusal message [`apply_backup_plan_walk`] returns when `confirmed` is `false` (CPE-1664).
/// Shared with the tests so the assertion can't drift from the text the user actually sees.
pub const BACKUP_NOT_CONFIRMED: &str =
    "refusing to run the backup plan: `confirmed` was not set on this apply_backup_plan call — a \
     mirror plan deletes files under the destination root outright (no Recycle Bin copy, no undo), so \
     it must be re-invoked with an explicit confirmation (only BackupDashboard's Run/Restore buttons, \
     or the drive-connect scheduler acting on a job the user ticked auto-run for, should ever set it)";

/// Copy one file from `src` to `dst`, creating parent dirs, then optionally verify by sha256.
///
/// # The link guard (CPE-1879) — scoped to the final path component only
///
/// This used to be a bare `std::fs::copy(src, dst)` — **no** link guard of any kind, not even the
/// symlink refusal every sibling untrusted-name writer already has
/// ([`crate::fsutil::copy_file_onto_no_follow`] for restore, `archive::entry_sink_action` for
/// extraction, `transfer::download_tree`'s leaf). `fs::copy` writes **through** whatever inode `dst`
/// names. If that name is a symlink, or a second name (hard link) for a file that lives outside the
/// backup root, the write lands there instead — and no path-containment check can see it, because a
/// hard link has no target to resolve and the name genuinely *is* inside the root. Reproduced live
/// (worked through in the CPE-1879 ticket): `h.txt` hard-linked to `outside/victim.txt`, backed up, and
/// `victim.txt`'s content changed to the backup source's bytes — a file the backup was never pointed at.
///
/// Fixed, for the **final component** `dst` itself, by reusing CPE-1857's mechanism rather than
/// inventing a second one: [`crate::fsutil::copy_file_onto_no_follow_with_wording`] already does
/// exactly this, reading the reparse-point and link-count facts off the destination handle it opens for
/// the write anyway — the *check itself* costs no additional syscall over that open. It is not free
/// relative to the `fs::copy` it replaces, though: `fs::copy` is one OS-level call (`CopyFileExW` /
/// `copy_file_range`/`sendfile`), while this path is an explicit open, read-loop, write, permissions
/// set, and times set — roughly three to five syscalls per file, not zero. That cost was already
/// accepted for the restore path by CPE-1870's measurement; it is not re-measured here, only carried,
/// since correctness on a data-destroying path outweighs it.
///
/// **What this does NOT close (CPE-1879 review, finding 1): the parent-directory route.** The
/// `create_dir_all(parent)` line above walks through a directory **junction** exactly like any other
/// directory — Windows needs no privilege at all to create one, unlike the symlink leg below
/// (`SeCreateSymbolicLinkPrivilege`) or the hard-link leg (a pre-existing second name at one exact
/// filename). A junction at `dst.parent()`, or any ancestor of it, redirects the whole write **and every
/// write beneath it** outside the backup root, and this guard — reading facts off `dst`'s own handle —
/// cannot see it: the final component it opens is a perfectly ordinary file sitting in what is, from
/// there, an entirely real (if redirected) directory. This is the **cheap** attack, not the exotic one,
/// and a backup destination — a NAS share or external drive — is exactly the kind of directory tree
/// least likely to be locked down. **Asymmetric with the mirror-delete side of this same file**:
/// `apply_backup_plan_walk`'s delete loop asserts [`crate::fsutil::contained_under`] on the *resolved*
/// path before `remove_dir_all`, so a junction on the delete side is refused; nothing equivalent runs
/// before a write. Tracked in a follow-up ticket (parent-directory containment on the write side,
/// flagged by the CPE-1879 Security Auditor review); not fixed here, so as not to bury a `create_dir_all`
/// containment change inside a link-guard PR.
///
/// **Also not carried through this guard (CPE-1879 review, finding 2): Windows alternate data streams.**
/// `fs::copy` (`CopyFileExW`) carries ADS, including the `Zone.Identifier` "Mark of the Web" Windows
/// stamps on anything downloaded from the internet; this function's open → `set_len(0)` → byte-stream
/// path carries none, and if `dst` already existed with its own `Zone.Identifier`, that stale stream
/// **survives the overwrite** — measured on this branch: `Zone.Identifier present: false` after a
/// backup copy where `fs::copy` on `main` shows `true`, and a stale mark surviving an overwrite where
/// `fs::copy` clears it. So a backup copy of a file that carried a "downloaded, treat with caution" flag
/// silently drops it: SmartScreen does not prompt and Office does not open the restored copy in
/// Protected View. `crate::fsutil::copy_file_onto_no_follow`'s own doc comment excuses this ADS loss on
/// the reasoning that the bytes are "the user's own captured content from a local store" and the
/// direction is "toward keeping an existing warning" — **neither half of that reasoning holds here**:
/// this call site's source is the user's arbitrary source tree, not app-captured content, and the
/// direction is toward *dropping* a warning, not keeping one. Also unmeasured but likely: macOS
/// `std::fs::copy` uses `fcopyfile(COPYFILE_ALL)`, which very probably carries `com.apple.quarantine`
/// the same way; not asserted here because it was not measured. Tracked in a follow-up ticket by the
/// Foreman; not fixed in this PR.
///
/// **Symlink: refuse. No legitimate counter-case here** — a backup destination that is itself a link is
/// always a mistake or an attack, never a design the backup engine needs to honour.
///
/// **Hard link: refuse too, deliberately decided rather than copied across.** The ticket asks whether a
/// backup target being a legitimate deduplicating store (`rsync --link-dest`, Time Machine) changes the
/// answer — arguably it should matter *more* here than for CPE-1857's restore/archive/transfer sites,
/// since a backup's whole purpose can be dedup. But **this engine does not implement dedup**: it never
/// decides "link instead of copy" itself, it only ever executes `copy_one_verified` against a plan
/// (`copy`/`update` lists) computed by `planBackup` from a **flat comparison of two trees**. A real
/// link-based dedup tool creates its OWN hard links as a deliberate step and never subsequently `fs::copy`s
/// onto them; if THIS engine ever finds a multiply-linked name sitting at a plan-chosen destination, that
/// is not this tool's own dedup structure (it has none) — it is either an accident (the user pointed the
/// backup at a store some *other* tool manages) or a planted link, and writing through it is corruption
/// either way. So the CPE-1857 "refuse-per-entry-loudly, rest of the batch continues" answer transfers
/// unchanged, for the *same* reason it applied to restore: this writer, like that one, does not create
/// the link, so it has no business writing through one it finds. The cost is real and stated plainly, as
/// CPE-1857 states it for restore: a backup run over someone else's dedup store now refuses every entry
/// that already has a second name, per file, with a reason — never a silent skip, and the rest of the
/// run still applies (see `apply_backup_plan_walk`, which already treats a `copy_one_verified` error as
/// one more [`crate::model::OpResult::err`] and moves on to the next plan entry). Unlike
/// [`crate::fsutil::LinkGuardWording::RESTORE`]'s remedy text, this call site's wording
/// ([`crate::fsutil::LinkGuardWording::BACKUP`]) does **not** universally tell the user to break the
/// link — see that constant's doc comment.
///
/// **The Restore direction (CPE-1879 review, finding 3):** `BackupDashboard`'s Restore button calls the
/// same `apply_backup_plan_walk` with `source_root`/`dest_root` swapped, so `dst` here is then the
/// user's **live tree**, not a fresh backup destination — and a live tree is far more likely to hold a
/// pre-existing hard link (package manager stores, dedup sync clients) than an empty backup folder is.
/// Everything above still applies in that direction; it is called out because the "backup destination is
/// usually a fresh folder" severity argument in the ticket does **not** transfer to Restore.
fn copy_one_verified(src: &Path, dst: &Path, verify: bool) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::fsutil::copy_file_onto_no_follow_with_wording(src, dst, crate::fsutil::LinkGuardWording::BACKUP)?;
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
/// The half that holds **regardless of what the caller sets `confirmed` to** is
/// [`cpe_server::fsutil::contained_under`], asserted on the *resolved* delete path immediately before
/// the `remove_dir_all`: canonicalise, then require the target to be strictly inside the root.
/// [`safe_join`]'s textual rules sit in front of it as a cheap filter, but they are **not** the
/// guarantee — an earlier version of this comment claimed they were, and the PR #855 audit then wiped
/// the root through five spellings that satisfied them. State the protection as the containment
/// assertion, never as a list of rejected spellings.
///
/// **Known asymmetry, recorded rather than fixed (PR #855 audit).** `contained_under` guards the
/// *delete* loop only; the copy/update loop still relies on `safe_join` alone, because the containment
/// check's fail-open on an unresolvable path is sound only for a target that is about to be removed —
/// a write target is *expected* not to exist yet, so applying it there would fail open every time (see
/// that function's precondition). The consequence: a **junction or symlink inside `dest_root`** lets a
/// copy/update entry *write* through it to a location outside the root. That is writes, not deletes,
/// and no privilege gain for a caller who can already invoke this; guarding it properly needs a
/// parent-directory containment check on the write side, which is a separate change.
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
        let joined = (
            safe_join(&src_root, rel, PlanEntry::Write),
            safe_join(&dst_root, rel, PlanEntry::Write),
        );
        let (src, dst) = match joined {
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
        let dst = match safe_join(&dst_root, rel, PlanEntry::Delete) {
            Ok(d) => d,
            Err(e) => {
                emit(OpResult::err(Path::new(rel), e));
                continue;
            }
        };
        // THE containment check (CPE-1664, PR #855 audit), on the RESOLVED path, immediately before the
        // destructive call — never trusting that `safe_join` above has enumerated every spelling. Shared
        // with `resolve_conflict`'s Overwrite arm so both destructive sites have ONE failure policy.
        if let Err(e) = contained_under(&dst, &dst_root) {
            emit(OpResult::err(&dst, format!("refusing to delete: {e}")));
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

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-backup-{tag}"))
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
    /// specification is `contained_under`'s containment assertion on the resolved path, which is tested
    /// directly in `fsutil` rather than through this list. Enumerating spellings is exactly the approach
    /// the audit showed cannot work.
    ///
    /// These five resolve to the root **on Windows only**; on POSIX they are ordinary, distinct names.
    const WIN32_ROOT_SPELLINGS: [&str; 5] = [
        " ",    // Win32: trailing space stripped → `root`
        "  ",   // ditto, two
        "...",  // Win32: trailing dots stripped → `root`
        ". ",   // ditto, mixed
        " .",   // ditto, mixed the other way
    ];

    /// Every spelling the end-to-end sweep drives: the two that resolve to the root everywhere, plus the
    /// Windows-only ones.
    const ROOT_SPELLINGS: [&str; 7] = [
        ".", "", " ", "  ", "...", ". ", " .",
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
        // `.` / `""` / `./` / `sub/..` resolve to the root on EVERY platform — always refused, for both
        // halves of a plan.
        for rel in [".", "", "./", "sub/.."] {
            for entry in [PlanEntry::Write, PlanEntry::Delete] {
                let e = safe_join(root, rel, entry)
                    .expect_err("an entry resolving to the root must be rejected everywhere");
                assert!(!e.is_empty(), "the rejection must carry a reason for {rel:?}/{entry:?}");
            }
        }
        // The Win32-normalised spellings: refused on Windows, allowed elsewhere, for BOTH halves now
        // (CPE-1675 — the delete half used to refuse these everywhere; see `PlanEntry`'s doc for why that
        // bought no safety on POSIX and cost a mirror job its ability to ever report clean).
        for rel in WIN32_ROOT_SPELLINGS.iter().copied().chain(ALIASING_SPELLINGS) {
            for entry in [PlanEntry::Write, PlanEntry::Delete] {
                assert_eq!(
                    safe_join(root, rel, entry).is_err(),
                    cfg!(windows),
                    "{rel:?} as a {entry:?} entry must be refused on Windows and allowed elsewhere — \
                     refusing it on POSIX either silently drops a legitimate file from the backup (WRITE) \
                     or refuses to ever remove a legitimate stale one (DELETE)"
                );
            }
        }
        // Ordinary entries — what `planBackup` actually emits — still join untouched, both halves.
        for entry in [PlanEntry::Write, PlanEntry::Delete] {
            assert_eq!(safe_join(root, "sub/a.txt", entry).unwrap(), root.join("sub/a.txt"));
            assert_eq!(safe_join(root, "a b/c.txt", entry).unwrap(), root.join("a b/c.txt"));
            assert_eq!(safe_join(root, ".gitignore", entry).unwrap(), root.join(".gitignore"));
        }

        // …and end to end, with full consent, against a real tree: every spelling is a per-entry error
        // and the victim survives. `confirmed: true` throughout — the gate is NOT what is under test.
        //
        // WINDOWS-AWARE (PR #855 audit): on Linux/macOS these names address real, distinct, absent files,
        // so the plan would report "not found" and the tree would survive *even with the fix removed* —
        // a green Linux leg must not be read as coverage. Both legs assert the entry is refused and the
        // tree survives; only on Windows is that survival load-bearing.
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

    // `contained_under`'s own spec — the containment guarantee, the failure policy, and the junction
    // cases — lives with the function in `crate::fsutil`, since `resolve_conflict` shares it.

    /// **The POSIX regression this round exists to prevent** (PR #855 round-3 review, BLOCKING 1).
    ///
    /// `notes.` and `My Report ` are legal, creatable, everyday filenames on Linux and macOS. The first
    /// version of the CPE-1664 fix refused them on every platform and in both halves of a plan, so a
    /// Linux backup of such a file was **silently never copied** — a real user losing a real file from
    /// their backup, to defend against a hazard that only exists on Windows.
    ///
    /// There was no test in either direction, which is why it shipped. This is that test: on POSIX the
    /// file must be backed up normally; on Windows — where the name genuinely aliases — the entry is
    /// refused instead. Asserted per platform rather than skipped, so neither leg is silently uncovered.
    #[test]
    fn a_posix_legal_trailing_dot_name_is_backed_up_not_refused() {
        let d = scratch("posix_names");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();

        // Windows won't even let these be created, so build the plan from names rather than a scan.
        #[cfg(not(windows))]
        {
            fs::write(src.join("notes."), b"real contents").unwrap();
            fs::write(src.join("My Report "), b"also real").unwrap();
        }

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["notes.".to_string(), "My Report ".to_string()],
            &[],
            &[],
            false,
            true, // consented — this is about the NAME rule, not the gate
        )
        .expect("a consented plan runs");
        assert_eq!(results.len(), 2);

        #[cfg(not(windows))]
        {
            assert!(
                results.iter().all(|r| r.ok),
                "a POSIX-legal trailing-dot/space name must be COPIED, not refused: {results:?}"
            );
            assert_eq!(fs::read(dst.join("notes.")).unwrap(), b"real contents");
            assert_eq!(fs::read(dst.join("My Report ")).unwrap(), b"also real");
        }
        #[cfg(windows)]
        {
            assert!(
                results.iter().all(|r| !r.ok),
                "on Windows such a name aliases another path, so it must be refused: {results:?}"
            );
            assert!(
                results[0].error.contains("trailing dots/spaces"),
                "and the refusal must explain why: {results:?}"
            );
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1675 — the DELETE-side counterpart to the write-side test above.** Before this fix,
    /// `PlanEntry::Delete` refused a `win32_name_is_unstable` component on **every** platform, so a stale
    /// destination entry named `notes.` or `My Report ` — a legal, everyday POSIX filename — could never
    /// be removed by a mirror-delete plan: the same refused delete on every run, forever, so the job could
    /// never report clean. `contained_under`, asserted on the resolved path, already makes the delete safe
    /// on POSIX (the name addresses exactly what it spells, no aliasing), so the fix scopes the
    /// delete-side refusal to Windows too — same as the write side.
    ///
    /// Verified by **listing the destination directory back off disk**, not by trusting the `OpResult` —
    /// the return-value-only check is what let the CPE-1664 regression above ship silently; this repo's
    /// backup tests read the filesystem back deliberately (see `assert_victim_intact`). On Windows the
    /// refusal is unchanged and still asserted, matching the write-side test's shape.
    #[test]
    fn a_posix_legal_trailing_dot_name_is_removed_by_a_mirror_delete_not_refused() {
        let d = scratch("posix_delete_names");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();

        // Windows won't even let these be created — same guard as the write-side test.
        #[cfg(not(windows))]
        {
            fs::write(dst.join("notes."), b"stale - no longer in src").unwrap();
            fs::write(dst.join("My Report "), b"also stale").unwrap();
        }

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &[],
            &[],
            &["notes.".to_string(), "My Report ".to_string()],
            false,
            true, // consented — this is about the NAME rule, not the gate
        )
        .expect("a consented plan runs");
        assert_eq!(results.len(), 2);

        #[cfg(not(windows))]
        {
            assert!(
                results.iter().all(|r| r.ok),
                "a POSIX-legal trailing-dot/space stale entry must be REMOVED, not refused: {results:?}"
            );
            // Off disk, not just the return value (see this test's own doc).
            let names: Vec<String> = fs::read_dir(&dst)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                !names.iter().any(|n| n == "notes."),
                "notes. must actually be gone off disk, not just reported ok: {names:?}"
            );
            assert!(
                !names.iter().any(|n| n == "My Report "),
                "My Report  must actually be gone off disk, not just reported ok: {names:?}"
            );
        }
        #[cfg(windows)]
        {
            assert!(
                results.iter().all(|r| !r.ok),
                "on Windows such a name aliases another path, so the delete must still be refused: \
                 {results:?}"
            );
            assert!(
                results[0].error.contains("trailing dots/spaces"),
                "and the refusal must explain why: {results:?}"
            );
        }
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

    /// CPE-1879. `copy_one_verified` was a bare `std::fs::copy(src, dst)` with **no link guard of any
    /// kind** — not even the symlink refusal every sibling write path (`copy_file_onto_no_follow`,
    /// `archive::entry_sink_action`, `transfer::download_tree`) already has. `fs::copy` writes through
    /// whatever inode `dst` names; if that name is a **hard link** to a file outside the backup root, the
    /// backup silently rewrites the other file — no path-containment check can see this, because a hard
    /// link has no target to resolve and `dst` genuinely IS inside the backup root.
    ///
    /// **The fixture's liveness is proved before anything is asserted about the code under test**, the
    /// only way a hard link can be proved live: write through the OUTSIDE name and read it back through
    /// the INSIDE one. A fixture of two unrelated files would certify nothing.
    #[test]
    fn cpe_1879_a_hard_linked_backup_destination_is_never_written_through() {
        let d = scratch("cpe1879-hardlink-dest");
        let backup_root = d.join("dst");
        let outside = d.join("outside");
        fs::create_dir_all(&backup_root).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("victim.txt");
        fs::write(&victim, b"placeholder").unwrap();
        let dst = backup_root.join("h.txt");
        if fs::hard_link(&victim, &dst).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1879_a_hard_linked_backup_destination_is_never_written_through: no \
                 hard-link support on this filesystem - this run verified NOTHING about the hard-link \
                 hole in the backup writer"
            );
            return;
        }
        // Liveness: write through the OUTSIDE name and read it back through the INSIDE one.
        fs::write(&victim, b"VICTIM CONTENT").unwrap();
        assert_eq!(
            fs::read(&dst).ok().as_deref(),
            Some(&b"VICTIM CONTENT"[..]),
            "fixture is inert: the two names do not share one inode, so no write could have gone through"
        );

        let src = d.join("src.txt");
        fs::write(&src, b"BACKUP SOURCE").unwrap();

        let outcome = copy_one_verified(&src, &dst, false);

        // HARM FIRST, on the filesystem, before the verdict is looked at.
        assert_eq!(
            fs::read(&victim).ok().as_deref(),
            Some(&b"VICTIM CONTENT"[..]),
            "HARM: the backup wrote the source file's bytes through a hard link, into a file OUTSIDE \
             the backup root that nothing in the plan ever named"
        );
        assert!(
            outcome.is_err(),
            "a backup destination with a second name must be refused, not written: {outcome:?}"
        );
        let why = outcome.unwrap_err();
        assert!(
            why.contains("2 names"),
            "the refusal must say WHY, naming the link count the user has to act on: {why}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1879, the symlink half — cheaper to arrange than a hard link and with no legitimate
    /// counter-case here, unlike the hard-link one (see the doc comment on [`copy_one_verified`]).
    #[test]
    fn cpe_1879_a_symlinked_backup_destination_is_never_written_through() {
        let d = scratch("cpe1879-symlink-dest");
        let backup_root = d.join("dst");
        fs::create_dir_all(&backup_root).unwrap();

        let victim = d.join("victim.txt");
        fs::write(&victim, b"VICTIM CONTENT").unwrap();
        let dst = backup_root.join("link.txt");
        if !crate::fsutil::make_file_link(&victim, &dst) {
            crate::skip_notice!(
                "SKIPPING cpe_1879_a_symlinked_backup_destination_is_never_written_through: no file \
                 symlink privilege on this machine. NOTHING on this run covered the symlink hole in \
                 the backup writer"
            );
            return;
        }
        assert_eq!(
            fs::read(&dst).ok().as_deref(),
            Some(&b"VICTIM CONTENT"[..]),
            "fixture is inert: the planted link does not lead to the victim's bytes"
        );

        let src = d.join("src.txt");
        fs::write(&src, b"BACKUP SOURCE").unwrap();

        let outcome = copy_one_verified(&src, &dst, false);

        assert_eq!(
            fs::read(&victim).ok().as_deref(),
            Some(&b"VICTIM CONTENT"[..]),
            "HARM: the backup wrote the source file's bytes through a symlink, into a file outside \
             anything the backup plan ever named"
        );
        assert!(
            fs::symlink_metadata(&dst).is_ok_and(|m| m.file_type().is_symlink()),
            "the destination must still hold the link, not bytes written over it"
        );
        assert!(outcome.is_err(), "writing onto a link at the final component must be refused: {outcome:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1879 AC5: a refusal must reach the caller per-file through the normal [`OpResult`] channel,
    /// never a silent skip — proved at the `apply_backup_plan_walk` level, not just on the private
    /// helper, so a regression in the plumbing between them is caught too.
    #[test]
    fn apply_backup_plan_reports_a_hard_link_refusal_per_file_not_silently() {
        let d = scratch("cpe1879-plan-report");
        let src = d.join("src");
        let dst = d.join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();

        fs::write(src.join("ok.txt"), b"fine").unwrap();
        fs::write(src.join("linked.txt"), b"new content").unwrap();

        let outside = d.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        fs::write(&victim, b"placeholder").unwrap();
        if fs::hard_link(&victim, dst.join("linked.txt")).is_err() {
            crate::skip_notice!(
                "SKIPPING apply_backup_plan_reports_a_hard_link_refusal_per_file_not_silently: no \
                 hard-link support on this filesystem"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["ok.txt".into(), "linked.txt".into()],
            &[],
            &[],
            false,
            true, // confirmed
        )
        .expect("a confirmed plan runs");

        assert_eq!(results.len(), 2, "one OpResult per plan entry — nothing silently dropped: {results:?}");
        let ok = results.iter().find(|r| r.path.ends_with("ok.txt")).expect("ok.txt must be reported");
        assert!(ok.ok, "the unrelated file must still copy: {ok:?}");
        let linked =
            results.iter().find(|r| r.path.ends_with("linked.txt")).expect("linked.txt must be reported");
        assert!(!linked.ok, "the hard-linked entry must be reported as refused, not silently skipped: {linked:?}");
        assert!(
            linked.error.contains("hard-linked") || linked.error.contains("names"),
            "the reported error must say why: {linked:?}"
        );
        assert_eq!(
            fs::read(&victim).ok().as_deref(),
            Some(&b"placeholder"[..]),
            "HARM: the plan-level run wrote through the hard link into the file outside the backup root"
        );
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
