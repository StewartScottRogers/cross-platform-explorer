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
    // CPE-1896 review: built up component by component rather than `root.join(candidate)`, so the
    // result carries the PLATFORM's separator throughout. `join` splices the plan string in verbatim,
    // and plan paths are `/`-separated (they come from the frontend), so on Windows every joined path
    // came out mixed — `...\dst\sub/victim.txt`. That string is not cosmetic: it is what
    // `OpResult::path` carries to the dashboard and what every refusal in this module interpolates,
    // including a landing-check refusal, which reaches a screen at the worst moment a backup has.
    // `Path::components()` already normalises the separators away (it is what the loop below inspects),
    // so pushing each component yields the same path with a consistent spelling — no new rule, nothing
    // newly rejected.
    let mut joined = root.to_path_buf();
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
                joined.push(part);
                named += 1;
            }
            _ => return Err(format!("unsafe path in plan: {rel}")),
        }
    }
    if named == 0 {
        return Err(format!("plan entry names the backup root itself, not a path inside it: {rel:?}"));
    }
    Ok(joined)
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

/// The CPE-1896 **landing check**: after the bytes are written, establish that they went into a file
/// inside the backup destination — and that it is *the file this call just wrote*. Returns the resolved
/// destination, which is then the only path the verify read-back is allowed to open.
///
/// # What this is for, stated exactly — it is a MITIGATION, not the atomic fix
///
/// [`copy_one_verified`]'s containment guard is not atomic with the destination open, and its doc
/// records that. CPE-1896's Security Auditor turned the recorded residual into a measured one: exactly
/// three syscalls separate check (1)'s `canonicalize(parent)` from the open, a racing thread that
/// renames the parent inside that window redirects the write outside the root, and the run reported
/// **`ok: true` with an empty error** — 73 escapes over 1200 trials.
///
/// The part this function exists for is the next sentence of that finding: **turning verification on
/// did not catch it** — 68 escapes over 1200 with `verify = true`, because the sha256 read-back
/// re-opened *the same redirected path* and agreed with itself. Verification was not merely failing to
/// help; it was **laundering** the escape, handing the user a checksum-confirmed "backed up" for a file
/// that had just overwritten something outside the folder they pointed the job at.
///
/// So this check runs **after** the write and asks the one question the pre-write checks cannot answer:
/// *where did the bytes actually go?* An escaped write is then reported as a per-file failure naming
/// the path it could not verify, and the verify leg reads back through the resolved, contained path
/// rather than the plan-relative name that a swapped junction is still redirecting.
///
/// # The check is a HANDLE question, not a second path question — and that distinction is the whole fix
///
/// The first version of this function asked containment about `canonicalize(dst)` and stopped there.
/// **The auditor defeated that in one thread, and the numbers are the reason this paragraph exists.**
/// The attack wins the same window *twice, in opposite directions*: phase 1 is the known race (swap a
/// junction in, the write escapes); then it watches the outside file grow past 4 KiB — proof the escaped
/// write is streaming — and renames the innocent directory **back** before the engine asks where the
/// bytes went. `canonicalize(dst)` then resolves to a real, ordinary file inside the root: on an
/// `update` plan entry, the stale previous backup that is naturally already sitting there. Containment
/// says yes. Measured, `verify = false`, 400 trials: **297 escaped, 38 reported `ok: true`** — the
/// escape *and* the backup silently not happening, both reported as success:
///
/// ```text
/// AUDIT A2 HARM trial 9: 2097152 bytes landed at "…\outside\victim.txt" (OUTSIDE the root) and the
/// engine reported OpResult { ok: true, error: "", outcome: Applied }; the file inside the root still
/// holds 21 bytes
/// ```
///
/// No path question can answer that, because after the swap-back every path *is* what it says it is.
/// The question with an answer is about the **object**:
/// [`crate::fsutil::copy_file_onto_no_follow_with_wording`] already calls
/// [`crate::batch_media::handle_facts`] on the destination handle it writes through — for the
/// reparse/directory guard it has always had — so the identity of the object the bytes went into is
/// available for **no additional syscall**. This function opens the contained path and compares. A
/// swapped-back name is a *different file object*, so the comparison refuses it. Renaming cannot forge
/// an identity, and the hard-link route to forging one is already refused upstream by that function's
/// `facts.links > 1` branch.
///
/// # What it does NOT do — read this before believing the write is safe
///
/// **It does not stop an escape; it reports one.** By the time this runs the bytes are already
/// wherever they went, and this function cannot un-write them — nor does it try. A backup engine
/// deleting a file outside the destination it was pointed at would be committing the same violation it
/// is reporting, with less excuse. What it converts is silent success into a loud, per-file failure
/// that names the entry, the outside path, and the source file whose bytes are now sitting there.
///
/// # Its role changed when the atomic walk landed — read this before deleting it
///
/// This function was written as the *mitigation* for a window that was still open. The window is now
/// closed at the source: [`copy_one_verified`] opens the destination through
/// [`crate::open_beneath::create_beneath`], one component at a time against the previous component's
/// handle, so no rename can redirect the write and the 73-escapes-per-1200-trials shape measures **0
/// escapes over 400 trials**. Prevention replaced detection, and detection is what this is.
///
/// It is deliberately **kept**, for two reasons that are not "belt and braces":
///
/// 1. **It covers the walk's one genuine residual, and on the Unix fallback walk it is the *only*
///    thing that does** — see the per-platform breakdown on [`copy_one_verified`]. An actor who
///    renames the directory object the walk is descending into out of the root mid-copy takes the
///    write with it. Windows refuses that rename outright while a descendant is open (measured, 248
///    attempts, all `Access is denied`) and `openat2(RESOLVE_BENEATH)` re-resolves from the root fd,
///    but POSIX `rename` has no such restriction, so on macOS — and on Linux whenever the `openat2`
///    fast path declines — this check is the whole defence.
///
///    **It catches it on every platform with a usable file identity, which is not every platform.**
///    When [`crate::batch_media::handle_facts`] returns `None`, or the identity is
///    [`crate::batch_media::FileIdentity::is_degenerate`] (network redirectors that answer
///    `GetFileInformationByHandle` with a zeroed file index), this function short-circuits to the path
///    answer alone. On such a destination the residual is **silent rather than loud** — the path
///    resolves to something inside the root and nothing objects. That is a real gap on exactly the
///    kind of volume a backup destination often is; CPE-1895 owns the network-destination work, and
///    CPE-1915's `GetFinalPathNameByHandleW` / `F_GETPATH` route would close it by asking the handle
///    for its own path instead of comparing identities.
/// 2. **It is the only cross-check on a new, unsafe, platform-specific walk.** Deleting the detector
///    in the same change that introduces the prevention would leave a `NtCreateFile`/`openat` bug with
///    nothing but tests standing between it and a user's files.
///
/// **It is also, measured, ~99% of this engine's guard cost** — see the cost section on
/// [`copy_one_verified`]. Its identity probe (the read-only second open, which existed to catch a
/// swap-back the walk now prevents) is the redundant half and the expensive half at once, ~850 µs/file
/// against the walk's unmeasurably-small delta, because a first read-open of a just-written file is
/// what triggers Windows Defender's synchronous scan. Removing it is the obvious follow-up and is
/// deliberately not done here; CPE-1915's `GetFinalPathNameByHandleW` route would remove both re-opens.
///
/// Do not let a later reader upgrade any of this to "backup is race-safe" without re-reading the
/// residual in point 1.
///
/// **It degrades to the path question on a volume with no usable identity, and there the swap-back is
/// NOT closed.** [`crate::batch_media::FileIdentity::is_degenerate`] documents the case: several network
/// redirectors let `GetFileInformationByHandle` *succeed* and hand back a zero index, so every object on
/// the volume carries the same identity. That crate-wide rule says a caller must refuse rather than
/// compare a degenerate identity — and this caller deliberately does not, because refusing here does not
/// mean "decline to act", it means **report every file of a backup as failed**, and a backup destination
/// is by design an external drive or a share. Reddening every entry of every backup to such a volume is
/// worse than the residual it would close. So an unusable identity (absent, or degenerate at either end)
/// falls back to the containment answer alone — still strictly better than before CPE-1896, and still
/// enough for the one-phase escape — and the two-phase swap-back remains open **on those volumes only**.
/// Closing it there needs the path *of the open handle itself* (`GetFinalPathNameByHandleW` on Windows,
/// `F_GETPATH` / `/proc/self/fd` on macOS/Linux), which is a different mechanism from identity and a
/// separate change.
///
/// **A failure to open what was just written REFUSES**, unlike the two cases above, and the split is
/// principled rather than squeamish: a degenerate identity means *this volume cannot answer identity
/// questions at all*, a platform property that must not break ordinary backups, whereas an open that
/// fails now means *this particular attempt could not tell*, which is exactly the state that must never
/// be reported as "backed up". The file itself is fine and a re-run copies it again, so the cost of
/// being wrong here is one recoverable per-file failure, not lost data.
///
/// **It reads the destination once more, so it can touch the destination's access time.** The copy's
/// `carry_file_times` has already run by then, so on a volume with access-time updates enabled the
/// destination's atime will not match the source's. Nothing in the plan comparison looks at atime, so
/// this is recorded rather than defended against.
///
/// # Cost
///
/// One `canonicalize` plus one open-and-describe per file, on top of the two path resolutions
/// CPE-1889's guard already adds. See the cost section on [`copy_one_verified`] — the number is stated
/// there rather than measured again here, and CPE-1895 owns measuring it on a network destination where
/// each resolution is a round trip.
fn landed_inside(
    src: &Path,
    dst: &Path,
    real_dst_root: &Path,
    written: Option<crate::batch_media::FileIdentity>,
) -> Result<PathBuf, String> {
    let real = std::fs::canonicalize(dst).map_err(|e| {
        format!(
            "refusing to report {dst:?} as backed up: the bytes of {src:?} were written, but the system \
             would not say where that name actually leads ({e}), so nothing can confirm they landed \
             inside the backup destination {real_dst_root:?}. Treat this entry as NOT backed up, and \
             check the destination for a link or junction along that path."
        )
    })?;
    if !real.starts_with(real_dst_root) {
        return Err(format!(
            "refusing to report {dst:?} as backed up: it resolves to {real:?}, which is OUTSIDE the \
             backup destination {real_dst_root:?}. The bytes of {src:?} have ALREADY been written to \
             {real:?}, replacing whatever was there — a link or junction along the path was swapped in \
             after this entry's containment check and before the file was opened, and that gap is not \
             closed. So {real:?} now holds this backup's copy of {src:?}; treat it as overwritten, and \
             treat this entry as not backed up."
        ));
    }

    // Identity unusable at the writing end — see the doc's "degrades to the path question" section for
    // why this is a fallback and not a refusal. The containment answer above still stands.
    let Some(written) = written.filter(|id| !id.is_degenerate()) else {
        return Ok(real);
    };

    // The handle question. Read-only and never creating: the object must already be there, and this is
    // an identity probe, not a write.
    let probe = crate::batch_media::open_existing_no_follow_read(&real).map_err(|e| {
        format!(
            "refusing to report {dst:?} as backed up: the bytes of {src:?} were written, and {real:?} \
             is inside the backup destination, but it could not be opened to confirm the bytes went \
             into it ({e}). Nothing can say where they landed, so this entry is reported as not backed \
             up rather than as a success — run the job again."
        )
    })?;
    let Some(here) = crate::batch_media::handle_facts(&probe).map(|f| f.id) else {
        // The platform has no identity model at all (`handle_facts` is `None` only off Unix and
        // Windows). Same fallback as a degenerate identity, and for the same reason.
        return Ok(real);
    };
    if here.is_degenerate() {
        return Ok(real);
    }
    if here != written {
        return Err(format!(
            "refusing to report {dst:?} as backed up: it does now resolve to {real:?}, which IS inside \
             the backup destination — but that is not the file this entry wrote. The bytes of {src:?} \
             went into a different file object, so a link or junction along the path was swapped in \
             during the copy and then swapped back before this check: the copied bytes are outside the \
             destination and this check can no longer say where, while {real:?} still holds whatever it \
             held before. Treat this entry as NOT backed up, and treat the destination as being \
             modified by something else while the job runs."
        ));
    }
    Ok(real)
}

/// Copy one file from `src` to `dst`, creating parent dirs, then optionally verify by sha256.
///
/// `real_dst_root` is the **already-canonicalised** backup destination root — see
/// [`crate::fsutil::confined_to_resolved_root`]'s precondition and the containment section below.
/// [`apply_backup_plan_walk`] resolves it once per run and hands the same value to every entry.
///
/// # The link guard (CPE-1879) — the final path component
///
/// Scoped to `dst` itself. The **path leading to it** is a separate guard with a separate mechanism,
/// added by CPE-1889 and documented in the next section; neither one covers the other's case, and the
/// write is only safe because both run.
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
/// # The parent-directory containment guard (CPE-1889) — the other half, and the cheap route
///
/// The link guard above reads facts off `dst`'s **own** handle, so it sees only the final path
/// component. It could not see a directory **junction** at `dst.parent()` or at any ancestor of it:
/// from the guard's point of view the final component is then a perfectly ordinary file sitting in an
/// entirely real (if redirected) directory. That was the **cheap** route, not the exotic one — a
/// junction (`mklink /J`) needs no privilege at all on Windows, unlike the symlink leg
/// (`SeCreateSymbolicLinkPrivilege`) or the hard-link leg (a pre-existing second name at one exact
/// filename) — and one junction redirects an entire subtree rather than a single name. Worse, it
/// reported **`ok: true` with an empty error**: the silent-success shape, not a loud skip. Measured by
/// the CPE-1879 Security Auditor on PR #1022 and reproduced here as
/// `cpe_1889_a_junction_at_the_parent_never_redirects_the_write_outside_the_root` (overwrites an
/// existing file outside the root) and `…_never_creates_a_new_file_outside_the_root` (creates a new
/// one).
///
/// It also left the write leg **asymmetric with the mirror-delete leg of this same engine**, which has
/// asserted [`crate::fsutil::contained_under`] on the *resolved* path since PR #855. The two legs now
/// agree: both resolve before they act, and both refuse what resolves out.
///
/// `contained_under` itself is **not** the mechanism here, and its own doc says why — it returns `Ok`
/// for a target that does not canonicalise, which is sound for a path about to be *removed* and fails
/// open every time for a path about to be *created*. The crate's answer for the create/write direction
/// is [`crate::fsutil::confined_to`], which walks up to the deepest existing ancestor, follows a
/// dangling link by hand, and refuses every case it cannot resolve. This site asks it through
/// [`crate::fsutil::confined_to_resolved_root`] — the same walk with the root's `canonicalize` hoisted
/// out of the per-file loop by [`apply_backup_plan_walk`].
///
/// **The guard runs twice, and each call earns its place:**
///
/// 1. **Before `create_dir_all`.** Otherwise a refusal still leaves *directory debris* outside the
///    root: `create_dir_all` walks a junction like any other directory, so an entry two levels below
///    one materialised `<outside>/deeper/` before the write was refused. Pinned by
///    `cpe_1889_a_refused_write_creates_no_directory_debris_outside_the_root`.
/// 2. **After it, immediately before the write** — but only when this call actually created the parent.
///    The fresh check confirms *what was just created* rather than what a probe predicted, and it is
///    skipped in the common case (the parent already existed, `create_dir_all` was never called, and
///    the pre-check is already the last thing that happened before the write).
///
/// **State plainly what check (2) is worth, because the sabotage probe measured it.** Deleting check
/// (1) reddens all four CPE-1889 harm tests. Deleting check (2) alone reddens **nothing**, and that is
/// not a gap in the tests — it is what check (2) *is*. A junction that is already sitting at the parent
/// is caught by check (1); check (2) can only ever differ from check (1) if the tree changed **between
/// them**, which is a race no deterministic test can stage. It is kept, rather than deleted as untested
/// code, because `create_dir_all` is the part of this function that does real work while the window is
/// open — potentially many `mkdir` round trips over a network share — and because the cost is amortised
/// (it fires once per newly-created directory, not once per file). It is not kept on a claim that it
/// closes anything, and a reader must not upgrade it to one.
///
/// **TOCTOU, stated honestly: the window is narrowed, not closed.** Between the last containment check
/// and the destination `open` inside `copy_file_onto_no_follow_with_wording`, an attacker with write
/// access to the destination tree can still swap a parent component for a junction, and the write
/// follows it. Closing that needs `openat2(RESOLVE_BENEATH)` on Linux or an `O_NOFOLLOW` directory
/// walk on Windows, neither of which `std` offers — the same residual `confined_to` records for every
/// other caller, and the same one CPE-1765 could only close for the *final* component (by claiming the
/// name with `create_new`, which an overwriting backup cannot do). What *is* atomic is the final
/// component: the link/hard-link guard above reads the handle the bytes go through, so a link planted
/// at `dst` itself after the check is still caught. Do not read this guard as making the write
/// path-safe against an attacker racing it; read it as making the write refuse a junction that is
/// *sitting there*, which is the shape that was measured.
///
/// # The landing check (CPE-1896) — the window is still open; the SILENT SUCCESS is not
///
/// The paragraph above was written as an honest disclosure. CPE-1896's Security Auditor measured it:
/// exactly three syscalls separate check (1)'s `canonicalize(parent)` from the destination open
/// (`metadata(parent)` → `File::open(src)` → `r.metadata()` → `open_no_follow(dst)`), and a racing
/// thread doing two renames — `dst/sub` → `dst/sub_old`, then a pre-parked `dst/junc` → `dst/sub`, both
/// of which work on a **non-empty** directory — landed **73 escapes over 1200 trials**, each one
/// overwriting a pre-existing file outside the root and reported as `ok: true` with an empty error.
///
/// **And `verify = true` did not catch it: 68 escapes over 1200.** The sha256 read-back re-opened the
/// same redirected path, so it agreed with itself. Verification was not a neutral bystander there — it
/// was *laundering* the escape, upgrading a silent success into a checksum-confirmed one.
///
/// Check (3), [`landed_inside`], closes that half and only that half. After the write it establishes
/// where the bytes went and refuses to report success unless they went into a file inside
/// `real_dst_root` **that is the very object this call wrote** — an identity comparison against the
/// destination handle, not a second path resolution, because the auditor defeated the path-only form by
/// winning the window twice in opposite directions (junction in, write escapes, junction back out
/// before the check: 38 of 400 trials still reported `ok: true`). The verify leg then reads back
/// through that resolved path instead of the plan name. An escaped write is now a per-file failure
/// naming the entry, the outside path, and the source file whose bytes are sitting there.
///
/// # The atomic open (CPE-1896, the other half) — the race is CLOSED, not narrowed
///
/// Every paragraph above describes guards that ask a question about a **path** and then, some syscalls
/// later, perform an **open** by that same path. That gap is the whole bug, and no number of re-checks
/// removes it, because each re-check is another path question.
///
/// The destination is now opened by [`crate::open_beneath::create_beneath`]: the run canonicalises the
/// root once and **holds it open**, and each component of the plan-relative path is opened relative to
/// the handle of the component before it, refusing a link at every step —
/// `NtCreateFile(RootDirectory=…)` with `FILE_OPEN_REPARSE_POINT` on Windows,
/// `openat2(RESOLVE_BENEATH)` with an `openat`/`O_NOFOLLOW` walk behind it on Unix. Missing directories
/// are created the same way, *inside the handle we hold*, so a refusal still cannot leave directory
/// debris outside the root. The handle the bytes go through is therefore beneath the root **by
/// construction**, not by a check that could be stale, and the racing rename that produced 73 escapes
/// over 1200 trials has nothing left to redirect. Measured on this branch: **400 trials, 0 escapes**
/// (`cpe_1896_a_parent_swapped_under_the_copy_is_never_reported_as_a_success`, which now asserts
/// `escaped == 0`).
///
/// **Checks (1) and (2) are consequently DELETED, not disabled** — along with `parent_contained`
/// itself, which had no other caller. That is where two `canonicalize` calls per file went; see the
/// cost section. An intermediate revision of this PR kept them behind an `open_beneath::ATOMIC`
/// `const bool` "for a target with no handle-relative open"; the reviewer compiled that target's arm
/// and found it had never built, so the branch was unreachable code that `dead_code` could not see.
/// See the "no fourth row" note on [`crate::open_beneath`].
///
/// **The residual, and the reason it is safe is DIFFERENT ON EACH PLATFORM — an earlier draft of this
/// paragraph gave one reason for both and it was wrong on Unix.** The shape: the walk holds an open
/// handle on the directory it is currently descending into, and an actor who *renames that directory
/// object out of the root* while the copy is in flight moves the write with it, because the bytes go
/// into the object we hold, wherever it now lives.
///
/// - **Windows: unreachable, measured.** Windows refuses to rename a directory that has an open
///   descendant, whatever the share mode — `FILE_SHARE_DELETE` does not buy the attacker this. PR
///   #1043's Security Auditor instrumented the ordering (timestamp of the successful rename against
///   the instant `apply_backup_plan` returned) and moved both the leaf's parent and its grandparent
///   during 192 MiB and 768 MiB copies: **218 and 30 attempts, every one `Access is denied
///   (os error 5)`, zero mid-flight escapes.** *If you re-test this, instrument the ordering.* The
///   auditor's first un-instrumented run reported "100,663,296 bytes outside the root, ok: true" —
///   that was its racer winning *after* the plan returned, which is not an escape.
/// - **Linux, for an entry whose parent chain already exists: immune** — and the qualifier is not
///   pedantry. `RESOLVE_BENEATH` re-resolves from the root fd on every call and the kernel enforces the
///   beneath property, so a moved directory is simply not found. But `openat2` returns `ENOENT`
///   whenever any parent in the chain is missing, and `openat2_beneath` falls through to the walk on
///   **any** failure — so **every first-entry-into-a-new-directory takes the walk**, which on a first
///   full backup is every directory in the tree. Measured on 6.6.87: chain present → 1 walk syscall for
///   a create, 2 for an overwrite (the fast path served it); new chain → 6 (the walk).
///
///   **And an actor can force the fall-through on demand.** With a thread churning `rename(p ↔ q)` in
///   the destination, **184,854 of 400,000 `openat2` calls (46%) returned `ENOENT`**. That is not an
///   escape — the walk still refuses links and [`landed_inside`] still catches the move — but "immune"
///   would read as a property of the platform when it is one an actor with write access can revoke,
///   per entry, at will. (`ENOENT` is correctly outside the `SUPPORTED` latch set for exactly this
///   reason; no `EAGAIN` was observed on 6.6.87, so the latch is not implicated.)
/// - **The Unix fallback walk — macOS always, and Linux for every entry the fast path declines:
///   genuinely open, and only [`landed_inside`] catches it.** Demonstrated live by PR #1043's Security
///   Auditor: renaming a directory with an open descendant **succeeds** on POSIX where Windows refuses,
///   and bytes written through the held fd landed in a pre-existing `dot-ssh/authorized_keys`.
///   POSIX `rename` has no open-descendant restriction. The earlier
///   claim that "the attacker can only relocate a directory the backup itself owns, so the bytes
///   cannot be aimed at a pre-existing `.ssh`" is **false** here: `rename` into a *new name inside* a
///   pre-existing sensitive directory succeeds, and since both the directory name and the filename
///   come from the source tree, an actor who controls the source picks the whole landing path. What
///   actually saves this case is the post-write check, not the shape of the attack.
///
/// [`landed_inside`] also records the one configuration where even that honesty degrades (a volume
/// whose `GetFileInformationByHandle` returns a degenerate identity), and it is stated there rather
/// than here so it cannot drift from the code that decides it.
///
/// # Cost, since this is the backup engine's inner loop (CPE-1889 AC4)
///
/// **Syscall count — the durable number.** The root is canonicalised **once per run**, not per file.
/// Per file the common case (parent directory already exists — every entry after the first in a given
/// directory) adds one `metadata` and one `canonicalize` of the parent: two path resolutions on top of
/// the three-to-five syscalls the link guard already costs. The first entry in a not-yet-existing
/// directory adds the walk-up instead — one failed `canonicalize` plus one `symlink_metadata` per
/// absent level — then one confirming `canonicalize` after `create_dir_all`. So for a 100,000-file
/// backup: ~200,000 extra path resolutions, plus roughly two or three more per new directory.
///
/// **CPE-1896's landing check adds one `canonicalize` and one open-and-describe, per file, in both
/// verify modes.** The identity of the *written* side costs nothing at all: it is read off the write
/// handle by a `handle_facts` call `copy_file_onto_destination_handle` was already making for its
/// reparse/directory guard. Not gated on `verify`, deliberately: the escape it makes loud was measured
/// at 73/1200 with verification **off**, so gating it would leave the default configuration silent.
///
/// **CPE-1896's atomic walk REMOVES the two `canonicalize` calls checks (1) and (2) made** and replaces
/// them with handle-relative opens. Counted exactly by
/// `crate::open_beneath::tests::cpe_1896_report_the_walk_syscall_cost` (a thread-local counter around
/// every syscall the walk makes), for the ordinary `a/b/name.txt` shape with the directory chain
/// already present:
///
/// ```text
/// creating a new name    5 syscalls/file   2 dirs x (open + GetFileInformationByHandle) + 1 create
/// overwriting an existing name    6        the same, + the exclusive create that loses the race to
///                                          the file already being there, then the plain open
/// ```
///
/// Those are **handle-relative** opens: one name resolved against one open directory object, not a
/// path walk from a drive letter. So the *count* went up (5–6 against the previous 2 `canonicalize` +
/// 1 `metadata` + 1 open) while the per-operation work went down, and the A/B below measures the
/// combined effect as being below the copy's own noise floor — **+100.9, −51.1 and +19.6 µs/file over
/// three runs of 2,000 files**, both signs, exactly as CPE-1889's own measurement behaved.
///
/// # The number that actually matters, and it is not the walk (CPE-1896 AC5)
///
/// The same A/B with the landing check **left in** — i.e. the engine exactly as it ships — measures
/// **+920.5, +1093.2, +945.7 and +960.8 µs/file**, roughly a thousand times the walk's cost. That is
/// ~95 s on a 100,000-file backup, and PURPOSE.md's fast/small/predictable tiebreaker deserves it
/// stated in those terms rather than in syscalls.
///
/// **Bisected, because a number nobody has decomposed invites the wrong fix.** Disabling only
/// [`landed_inside`]'s identity probe (its second open, the read-only one) drops the delta to +71.2,
/// +101.9, +92.5 µs/file. Disabling the whole landing check drops it into the noise. So:
///
/// ```text
/// the per-component atomic walk        below the noise floor (both signs)
/// landed_inside's canonicalize         ~80 us/file
/// landed_inside's identity probe       ~850 us/file          <- the whole cost, near enough
/// ```
///
/// **And the ~850 µs is not syscall time — it is antivirus.** `canonicalize` opens the destination for
/// *attributes*; the identity probe opens it for *read data*, and that first read-open of a
/// just-written file is what makes Windows Defender's real-time scanner scan it synchronously. The
/// giveaway is the ratio: two opens of the same file, one ~80 µs and one ~850 µs. Recorded as measured
/// rather than filed as a syscall cost, because tuning syscalls would not move it.
///
/// **This PR does not add that cost — it inherited it** (PR #1037) and is the first thing to measure
/// it. Net, this change is cost-negative: two `canonicalize` calls per file removed, a walk that
/// measures as free added. The follow-up worth doing is deleting the identity probe, which the atomic
/// walk makes redundant (it existed to catch a swap-back that can no longer redirect the write) —
/// deliberately **not** done here, because removing an auditor-mandated guard belongs in its own
/// reviewed change and not in the same PR as ~800 lines of new platform FFI. CPE-1915's
/// `GetFinalPathNameByHandleW` route would remove both re-opens at once.
///
/// **Host and tooling for every number above**, since none of them travel: Windows 11 Pro 10.0.26200,
/// x86_64, local NTFS (`%TEMP%`), `rustc`/`cargo` **1.98.0** (`x86_64-pc-windows-msvc`, from
/// `rustc -Vv`), Defender real-time protection **on**, engine 4.18.26070.9 (`Get-MpComputerStatus`),
/// `cargo test` **debug** profile. CPE-1895 owns re-measuring against a network destination, where each
/// resolution is a round trip and the balance between these rows changes completely.
///
/// **Wall clock — measured, and the measurement's honest answer is "too small to see here".**
/// `cpe_1889_measure_the_guard_cost` A/Bs the guarded engine against the pre-fix shape in one process
/// over 2,000 files. Four runs on a local NTFS volume gave deltas of +11.3, −67.0, −21.2 and
/// +29.2 µs/file — **both signs, swamped by run-to-run variance** in the copy itself, which is an open,
/// a read-loop, a write, a permissions set and a times set per file. The guard is not free; it is
/// simply far below the noise floor of the thing it sits next to on local storage, and quoting the
/// +11.3 µs figure alone (the first run, and the only positive one at the time) would have been picking
/// the number that suited the argument.
///
/// **Where it will be visible: a network destination.** Each of those extra resolutions is a round trip
/// to a SMB/NFS server, where the copy's own cost no longer hides them. That is the case to re-measure
/// against a real share (the QNAP on the LAN is the standing test target) before anyone claims this is
/// free everywhere. What is not in question is the trade: the alternative is an engine that writes
/// outside the folder the user pointed it at and calls it a success.
///
/// **The rest of the create-then-write class, swept (CPE-1889 item 4).** This was the last unguarded
/// site of the shape, not the only one — every sibling already resolved the path before writing, which
/// is why the asymmetry was worth closing rather than accepting:
///
/// - `archive::entry_sink_action` / `entry_dir_action` — resolve **every intermediate component** via
///   [`crate::fsutil::confined_to`], and deliberately *before* their `create_dir_all(parent)` for the
///   same no-debris reason as check (1) above (CPE-1744/CPE-1759).
/// - `transfer::download_tree` — **no longer in this list** (CPE-1913). It walked the ancestors with
///   its own `classify_ancestor_probe` before `create_dir_all`, which is where its deliberately
///   different `NotADirectory` stance came from (CPE-1742); it now opens the download folder once and
///   resolves every component against that handle, exactly as this function does, so it has no
///   ancestor walk and no `create_dir_all` left to run one before.
/// - `revert_engine::apply_write` — **also no longer in this list** (CPE-1913), for the same reason and
///   in the same way. It asked [`crate::fsutil::confined_to`] via `safe_target` before `create_dir_all`
///   and before touching the blob source (CPE-1750); it now asks `open_beneath`. Its sibling
///   `revert_engine::apply_delete` still does resolve by path and **is** still in this list — see
///   CPE-1937, which measured that leg destroying bystander files outside the root.
/// - `copilot::apply_op` — asks it on every path field of every op, final component included.
/// - `batch_media` — resolves the computed `out_dir` against the input's directory rather than
///   comparing text, after PR #828 measured the text fast path being bypassed three ways (CPE-1623).
///
/// No site is knowingly left with the create-then-write hole. That is a claim about this class only —
/// it says nothing about the *other* residuals recorded on this function.
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
fn copy_one_verified(
    src: &Path,
    dst: &Path,
    rel: &Path,
    root: &crate::open_beneath::RootDir,
    real_dst_root: &Path,
    verify: bool,
) -> Result<(), String> {
    // CPE-1889's checks (1) and (2) used to run here — a `canonicalize` of the parent before
    // `create_dir_all` and another after it. Both are **gone**, not disabled: the walk below opens
    // every component against the previous component's handle, so there is no path left for them to
    // resolve and nothing they could add. An intermediate revision of this PR kept them behind an
    // `open_beneath::ATOMIC` `const bool` "for a target with no handle-relative open"; the reviewer
    // then compiled that target's arm and found it had never built (two `E0308`s and an `E0507`), which
    // made this block dead code that `dead_code` could not see and left `parent_contained` with no
    // reachable caller. Deleted together. See the "no fourth row" note on [`crate::open_beneath`].
    let copied = crate::fsutil::copy_file_onto_destination_handle(
        src,
        dst,
        crate::fsutil::LinkGuardWording::BACKUP,
        // THE atomic half of CPE-1896. The destination is opened one component at a time, each
        // relative to the handle of the one before it, starting from the root handle this run opened
        // once — so there is no moment at which a rename can put a junction between the check and the
        // write, because there is no second lookup of any parent to race. Missing directories are
        // created the same way, inside the handle we hold, which is why no refusal can leave directory
        // debris outside the root either.
        // CPE-1961: the closure became a `DestinationSite` so the same root handle also creates and
        // commits the staging sibling the bytes actually go into. `copied.written` below is now the
        // identity of a file THIS call created, not of one it found at the destination — which makes
        // `landed_inside`'s comparison strictly stronger.
        crate::fsutil::DestinationSite::Beneath { root, rel },
    )?;

    // (3) AFTER the write (CPE-1896): where did the bytes actually go? This is the only check in the
    // function that can answer that, because it is the only one that runs when the answer exists. It
    // runs in BOTH verify modes on purpose — the measured silent success was 73/1200 with `verify =
    // false` and 68/1200 with it on, so gating the honesty on a setting the user may not have ticked
    // would leave the default configuration reporting `ok: true` on an escaped write. `copied.written`
    // is the identity of the object the bytes actually went into, read off the write handle for no
    // additional syscall, and it is what makes this a HANDLE question rather than a second path
    // question that a swap-back can answer wrongly. See [`landed_inside`] for what it does and, more
    // importantly, what it does not.
    let landed = landed_inside(src, dst, real_dst_root, copied.written)?;

    if verify {
        let a = sha256_file(src).map_err(|e| e.to_string())?;
        // Read back through `landed` — the RESOLVED, containment-confirmed path — never through `dst`,
        // the plan-relative name. That single substitution is what stops verification from laundering
        // an escape: re-opening `dst` re-traverses whatever junction redirected the write, so the
        // read-back agreed with itself and produced a checksum-confirmed "backed up" for a file
        // outside the root (CPE-1896, measured 68/1200). `landed` cannot lead out of the root, because
        // this call would have returned an error above if it did.
        let b = sha256_file(&landed).map_err(|e| e.to_string())?;
        if a != b {
            return Err(format!(
                "checksum mismatch after copy: {landed:?} does not match the source it was copied \
                 from, so this entry is NOT verified — the file may have been changed underneath the \
                 copy, or the bytes did not all land"
            ));
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
/// **The write/delete asymmetry is closed (CPE-1889).** For two tickets this comment recorded, as a
/// known hole, that `contained_under` guarded the *delete* loop only while the copy/update loop relied
/// on `safe_join` alone — so a **junction or symlink inside `dest_root`** let a copy/update entry write
/// through it to a location outside the root, reported as `ok: true`. Both legs now resolve before they
/// act. They ask **different** functions and that is deliberate, not an oversight: the delete leg asks
/// [`cpe_server::fsutil::contained_under`], which fails *open* on a target that will not canonicalise
/// (nothing that does not exist can be destroyed); the write leg asks
/// [`cpe_server::fsutil::confined_to_resolved_root`], which fails *closed* on everything it cannot
/// resolve, because a write target not existing yet is the ordinary case and a guard that returns "fine"
/// for exactly its own subject is worse than none. The write side's own reasoning, cost and residual
/// TOCTOU window live on [`copy_one_verified`].
///
/// **The destination root is canonicalised once, here, before the copy loop** — not per file, on what
/// may be a 100,000-entry inner loop against a network share. If it will not resolve even after an
/// attempt to create it, the **whole plan** is refused up front: with no resolvable root there is no
/// containment question that can be answered, and every write in the run would be unguarded. That is a
/// loud, whole-run [`Err`] rather than a per-file skip, for the same reason the consent gate is.
///
/// # `create_dirs` — the directory entry kind (CPE-1925)
///
/// Until this ticket the plan carried **only files**, so a directory existed in a backup destination
/// only as a side effect of writing a file into it. A source directory with no files under it — a
/// scaffolded `logs/`, an output folder, a mount point, anything whose contents are gitignored —
/// therefore had no entry of any kind, was never created, and the run still reported a clean `ok` for
/// every file it did carry. Measured on `main` before the fix: a tree with five such directories
/// backed up as `ok=3 fail=0` and **5 of 5 missing on disk** afterwards, in both the backup and the
/// restore direction.
///
/// `create_dirs` is that missing entry kind. It holds plan-relative directory paths and is applied
/// **first**, before the copy loop, so the run materialises the tree's *shape* and then its contents.
/// Each entry emits its own [`OpResult`] exactly like a file does, so a directory that could not be
/// created is a reported per-entry failure rather than a shape the user finds altered later.
///
/// **It is the minimal set, not every directory.** `planBackup` emits an entry only for a directory
/// that no copy/update entry would create as a side effect, and only the deepest such path (creating
/// `a/b/c` creates `a` and `a/b` on the way). A first full backup of a large tree therefore gains a
/// handful of entries, not one per folder.
///
/// **It goes through [`crate::open_beneath::create_dir_beneath`], never `create_dir_all`** — the
/// CPE-1925 acceptance criterion, and for CPE-1896's reason: `create_dir_all` resolves a path, walks a
/// junction like any other directory, and would re-open exactly the race the handle-relative walk
/// closed. `create_dir_beneath` opens each level relative to the level above it, inside the root
/// handle this run already holds, and refuses a link at every one. It is the same primitive
/// `archive::extract_zip_archive_stream` and `transfer::download_tree` use for the identical job, so
/// the containment guarantee and the refusal wording are shared rather than re-derived.
///
/// **What a directory entry carries, and what it does not.** It carries *existence* and nothing else:
/// the directory is created with the platform default mode (`0o777 & !umask` on Unix, the parent's
/// inherited ACL on Windows). Its mode bits, owner, timestamps, and Windows attributes (hidden,
/// system, compressed, encrypted) are **not** copied from the source, and neither are its extended
/// attributes or alternate data streams. That is deliberately the same contract the file leg already
/// has — [`copy_one_verified`] carries bytes and nothing else, not even the modification time — so
/// this ticket does not invent a second, richer answer for directories than the one files get. Saying
/// it here rather than implying it: a restored directory can have different permissions from the
/// original, and for a directory whose whole purpose is a restrictive mode that is a real loss. It is
/// a named gap, not a silent one, and it is the same gap as the file leg's; fixing both together is
/// the honest shape for it.
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
    create_dirs: &[String],
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

    // CPE-1889: resolve the destination root ONCE, before the per-file loop. A first run legitimately
    // points at a folder that does not exist yet (nothing has created it — `copy_one_verified`'s
    // `create_dir_all` used to), so the root is created on demand before the containment question is
    // asked; only a root that still will not resolve after that is refused.
    let real_dst_root = match std::fs::canonicalize(&dst_root) {
        Ok(p) => p,
        Err(_) => {
            let _ = std::fs::create_dir_all(&dst_root);
            std::fs::canonicalize(&dst_root).map_err(|e| {
                format!(
                    "refusing to run the backup plan: the destination folder {dst_root:?} could not be \
                     resolved ({e}), so nothing can check that the files land inside it. Reconnect the \
                     drive or share and run the job again."
                )
            })?
        }
    };

    // CPE-1896: hold the resolved destination root **open** for the whole run, once. Every write below
    // is resolved component-by-component against this handle rather than by re-parsing a path, which is
    // what makes each entry's containment atomic with its own open. Opened here, next to the
    // `canonicalize` it pairs with, so the inner loop pays neither.
    //
    // A root that resolves but will not open is a whole-plan refusal, for the same reason an
    // unresolvable one is: with no anchor there is no containment question that can be answered, and
    // every write in the run would be back to trusting a path.
    let root_handle = crate::open_beneath::open_root(&real_dst_root, "backup destination").map_err(|e| {
        format!(
            "refusing to run the backup plan: the destination folder {real_dst_root:?} could not be \
             opened ({e}), so the files cannot be written into it in a way that can be checked. \
             Reconnect the drive or share and run the job again."
        )
    })?;

    // CPE-1925: the tree's SHAPE first, then its contents. Applied before the copy loop so that a
    // directory a later copy also needs is already there (a second `create_dir_beneath` for it would
    // be a harmless re-open either way), and so the entries a user sees stream in the order the plan
    // preview lists them. `safe_join` runs for the same two reasons it runs on the copy leg: the cheap
    // textual filter, and the resolved path that `OpResult` reports — the containment guarantee itself
    // is `create_dir_beneath`'s per-component walk against `root_handle`, never this join.
    for rel in create_dirs {
        let dst = match safe_join(&dst_root, rel, PlanEntry::Write) {
            Ok(d) => d,
            Err(e) => {
                emit(OpResult::err(Path::new(rel), e));
                continue;
            }
        };
        match crate::open_beneath::create_dir_beneath(&root_handle, Path::new(rel)) {
            Ok(()) => emit(OpResult::ok(&dst)),
            Err(r) => emit(OpResult::err(&dst, r.why)),
        }
    }

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
        match copy_one_verified(&src, &dst, Path::new(rel), &root_handle, &real_dst_root, verify) {
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
#[allow(clippy::too_many_arguments)] // mirrors `apply_backup_plan_walk`, one over the threshold.
pub fn apply_backup_plan(
    source_root: &str,
    dest_root: &str,
    copy: &[String],
    update: &[String],
    delete: &[String],
    create_dirs: &[String],
    verify: bool,
    confirmed: bool,
) -> Result<Vec<OpResult>, String> {
    let mut out = Vec::with_capacity(copy.len() + update.len() + delete.len() + create_dirs.len());
    apply_backup_plan_walk(
        source_root,
        dest_root,
        copy,
        update,
        delete,
        create_dirs,
        verify,
        confirmed,
        |r| out.push(r),
    )?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-backup-{tag}"))
    }

    /// Drive one entry through [`copy_one_verified`] the way `apply_backup_plan_walk` does — resolving
    /// the destination root, opening it once, and naming the entry by its plan-relative path (CPE-1896).
    /// A test that hand-built the arguments differently from the engine would be exercising a shape
    /// production never runs.
    fn copy_one(
        src: &std::path::Path,
        dst_root: &std::path::Path,
        rel: &str,
        verify: bool,
    ) -> Result<(), String> {
        let real = fs::canonicalize(dst_root).unwrap();
        let root = crate::open_beneath::open_root(&real, "backup destination").unwrap();
        copy_one_verified(src, &dst_root.join(rel), std::path::Path::new(rel), &root, &real, verify)
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
                &[],
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
            &[],
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
                &[],
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

    /// CPE-1925: the plan's `create_dirs` list reaches the disk, at every depth, including the case
    /// the ticket singles out — a directory whose ONLY content is another empty directory.
    ///
    /// **The assertion is `is_dir()` on the destination, never the returned `OpResult`s.** That is the
    /// whole point of the ticket: `main` reported `ok=3 fail=0` for this exact tree while five
    /// directories were missing afterwards, so a verdict is precisely the thing that cannot be trusted
    /// to answer "did the shape survive?".
    ///
    /// Red-proof, run by hand: emptying the `create_dirs` loop in `apply_backup_plan_walk` (the
    /// pre-CPE-1925 behaviour) fails this test on the first `assert!` with the directory missing on
    /// disk — measured, not assumed. **It reds FOUR tests, not three** (round-1's note undercounted):
    /// this one, `a_restore_run_reproduces_the_directory_structure_not_just_the_files`,
    /// `a_directory_entry_that_cannot_be_created_is_reported_per_entry`, and
    /// `a_directory_entry_cannot_be_redirected_out_of_the_destination_by_a_planted_link` — the last on
    /// its `results.len()` assertion, not on its harm assertion.
    ///
    /// The one test that survived that sabotage **vacuously** was
    /// `a_directory_entry_naming_the_root_or_walking_up_is_refused_by_the_textual_filter`, whose
    /// `results.iter().all(|r| !r.ok)` is trivially true over an empty vec. It now asserts its length
    /// first; see the note there.
    #[test]
    fn apply_backup_plan_creates_the_planned_empty_directories_on_disk() {
        let d = scratch("createdirs");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&dst).unwrap();
        fs::create_dir_all(src.join("a")).unwrap();
        fs::write(src.join("a/keep.txt"), b"content").unwrap();

        // The plan a `planBackup` over this tree produces: one file, and the directories no file
        // creates on its way in. Only the DEEPEST path of a chain is listed, exactly as the planner
        // emits it — `b/only-an-empty-dir/leaf-empty` is expected to bring its two ancestors with it.
        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["a/keep.txt".into()],
            &[],
            &[],
            &[
                "empty-at-depth-1".into(),
                "a/empty-at-depth-2".into(),
                "b/only-an-empty-dir/leaf-empty".into(),
                "c/d/e/deep-empty".into(),
            ],
            true, // verify
            true, // confirmed (CPE-1664)
        )
        .expect("a confirmed plan runs");

        for dir in [
            "empty-at-depth-1",
            "a/empty-at-depth-2",
            "b/only-an-empty-dir/leaf-empty",
            "b/only-an-empty-dir", // the ancestor, created on the way to the leaf
            "c/d/e/deep-empty",
            "c/d/e",
        ] {
            assert!(
                dst.join(dir).is_dir(),
                "CPE-1925: {dir:?} is not a directory under the backup destination — the tree's shape \
                 did not survive the run. Engine said: {results:?}"
            );
        }
        // Every directory entry is reported like any other entry, so the count the dashboard shows is
        // the count of things actually attempted: 1 file + 4 directory entries.
        assert_eq!(results.len(), 5, "one OpResult per plan entry, directories included: {results:?}");
        assert!(results.iter().all(|r| r.ok), "nothing should have failed: {results:?}");
        assert_eq!(fs::read(dst.join("a/keep.txt")).unwrap(), b"content");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1925, the other half of the round trip: **restore** is the same engine with the roots
    /// swapped (that is literally what `BackupDashboard`'s Restore button does), so a directory can be
    /// lost at either end and the test has to check both. Measured on `main`, with the directories
    /// planted in the backup destination first so the restore leg was a fair independent test: 5 of 5
    /// missing after restore too, reported as `ok=3 fail=0`.
    #[test]
    fn a_restore_run_reproduces_the_directory_structure_not_just_the_files() {
        let d = scratch("restore-dirs");
        let (backup, live) = (d.join("backup"), d.join("live"));
        fs::create_dir_all(&live).unwrap();
        fs::create_dir_all(backup.join("logs")).unwrap();
        fs::create_dir_all(backup.join("out/nested/leaf")).unwrap();
        fs::write(backup.join("notes.txt"), b"n").unwrap();

        let results = apply_backup_plan(
            &backup.to_string_lossy(),
            &live.to_string_lossy(),
            &["notes.txt".into()],
            &[],
            &[],
            &["logs".into(), "out/nested/leaf".into()],
            true,
            true,
        )
        .expect("a confirmed plan runs");

        assert!(live.join("logs").is_dir(), "restore dropped an empty folder: {results:?}");
        assert!(live.join("out/nested/leaf").is_dir(), "restore dropped a nested empty folder: {results:?}");
        assert!(live.join("out/nested").is_dir(), "restore dropped an intermediate folder: {results:?}");
        assert_eq!(fs::read(live.join("notes.txt")).unwrap(), b"n");
        let _ = fs::remove_dir_all(&d);
    }

    /// A directory entry that cannot be created is a **reported per-entry failure**, not a silent skip
    /// and not a whole-run abort — the same contract every file entry has (CPE-1925). Driven with a
    /// plain file standing where the directory should go, which is also what a file-to-directory type
    /// change in the source produces.
    #[test]
    fn a_directory_entry_that_cannot_be_created_is_reported_per_entry() {
        let d = scratch("createdirs-blocked");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(dst.join("blocked"), b"a file is standing here").unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &[],
            &[],
            &[],
            &["blocked".into(), "fine".into()],
            false,
            true,
        )
        .expect("the run as a whole still succeeds");

        assert_eq!(results.len(), 2, "one OpResult per directory entry: {results:?}");
        assert!(!results[0].ok, "the blocked entry must be reported as a failure: {results:?}");
        assert!(!results[0].error.is_empty(), "and it must say why: {results:?}");
        assert!(results[1].ok, "the rest of the run continues: {results:?}");
        assert!(dst.join("fine").is_dir());
        assert!(dst.join("blocked").is_file(), "the file in the way is left alone");
        let _ = fs::remove_dir_all(&d);
    }

    /// A `create_dirs` entry goes through the same handle-relative walk every write does — CPE-1925's
    /// acceptance criterion, and CPE-1896's reason for it: `create_dir_all` resolves a path and walks
    /// a directory link like any other directory, so a new directory-creation path built on it would
    /// re-open exactly the hole the atomic walk closed.
    ///
    /// **The shape is a link planted INSIDE the destination**, deliberately, and not `..`. A `..`
    /// entry is refused by [`safe_join`]'s textual filter before `create_dir_beneath` is ever reached,
    /// so a test built on one is **shadowed** (CPE-1929): it passes with the containment walk swapped
    /// out for `create_dir_all`, and therefore reads as coverage while proving nothing. An earlier
    /// revision of this test was exactly that, and the sabotage below is what caught it.
    ///
    /// Red-proof, run by hand and recorded here rather than asserted: replacing the
    /// `create_dir_beneath` call with `std::fs::create_dir_all(&dst)` fails this test on the HARM
    /// assertion — the directory appears outside the destination root. With the `..` version of this
    /// test, that same sabotage stayed green.
    #[test]
    fn a_directory_entry_cannot_be_redirected_out_of_the_destination_by_a_planted_link() {
        let d = scratch("createdirs-link");
        let (src, dst, outside) = (d.join("src"), d.join("dst"), d.join("outside"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::create_dir_all(&outside).unwrap();

        if !crate::fsutil::make_dir_link(&outside, &dst.join("escape")) {
            crate::skip_notice!(
                "SKIPPING a_directory_entry_cannot_be_redirected_out_of_the_destination_by_a_planted_link: \
                 could not stage a directory link. NOTHING on this run covered a directory entry meeting \
                 a link at an interior component"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        // Liveness: the fixture must really redirect, or the test certifies nothing.
        fs::write(dst.join("escape/liveness.txt"), b"through the link").unwrap();
        assert_eq!(
            fs::read(outside.join("liveness.txt")).ok().as_deref(),
            Some(&b"through the link"[..]),
            "fixture is inert: the planted link does not redirect out of the destination"
        );
        fs::remove_file(outside.join("liveness.txt")).unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &[],
            &[],
            &[],
            &["escape/planted".into()],
            false,
            true,
        )
        .expect("the run as a whole still succeeds");

        // HARM FIRST, off the filesystem — never off the verdict.
        assert!(
            !outside.join("planted").exists(),
            "HARM: a directory entry was created OUTSIDE the backup destination because a link at an \
             interior component redirected it (CPE-1896/CPE-1925). Engine said: {results:?}"
        );
        assert_eq!(results.len(), 1, "one OpResult per directory entry: {results:?}");
        assert!(!results[0].ok, "and the refusal is reported, not silent: {results:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// The cheap textual filter still refuses the obvious spellings, and — as [`safe_join`]'s own doc
    /// insists — it is a filter, not the guarantee. Kept separate from the test above precisely so
    /// that neither is mistaken for the other's coverage.
    ///
    /// **The length assertion comes first, and it is load-bearing.** `all(|r| !r.ok)` is `true` over an
    /// empty vec, so without it this test passes **vacuously** the moment nothing runs at all — which
    /// is precisely what the round-1 sabotage (emptying the `create_dirs` loop) produces. It was the
    /// one test in this file that stayed green under a sabotage that removed the entire feature, and it
    /// stayed green for a reason that had nothing to do with the refusal it claims to cover.
    #[test]
    fn a_directory_entry_naming_the_root_or_walking_up_is_refused_by_the_textual_filter() {
        let d = scratch("createdirs-textual");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &[],
            &[],
            &[],
            &["../outside".into(), "".into()],
            false,
            true,
        )
        .expect("the run as a whole still succeeds");

        assert_eq!(results.len(), 2, "both entries must have been ATTEMPTED — `all(!ok)` is true over an empty vec, so this assertion is what stops the test passing vacuously: {results:?}");
        assert!(results.iter().all(|r| !r.ok), "both spellings must be refused: {results:?}");
        assert!(!d.join("outside").exists(), "nothing may be created outside the destination root");
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
            &[],
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

        let outcome = copy_one(&src, &backup_root, "h.txt", false);

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

        let outcome = copy_one(&src, &backup_root, "link.txt", false);

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

    /// CPE-1889 / A3, the Security Auditor's finding on PR #1022, reproduced through the **public**
    /// `apply_backup_plan`: a directory junction one level up inside the backup root redirects the
    /// write to an existing file outside the root, and the run reports `ok: true` with an empty error.
    ///
    /// Staged with [`crate::fsutil::make_dir_link`], whose Windows fallback is an NTFS **junction** —
    /// which needs no privilege at all, unlike the symlink and hard-link legs CPE-1857/CPE-1879 close.
    /// That is the whole point of this ticket: the cheap route, not the exotic one.
    #[test]
    fn cpe_1889_a_junction_at_the_parent_never_redirects_the_write_outside_the_root() {
        let d = scratch("cpe1889-a3");
        let (src, dst, outside) = (d.join("src"), d.join("dst"), d.join("outside"));
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("authorized_keys");
        fs::write(&victim, b"USER KEY").unwrap();
        if !crate::fsutil::make_dir_link(&outside, &dst.join("sub")) {
            crate::skip_notice!(
                "SKIPPING cpe_1889_a_junction_at_the_parent_never_redirects_the_write_outside_the_root: \
                 could not stage a directory link. NOTHING on this run covered the parent-junction hole \
                 in the backup writer"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        // Liveness: the fixture must really lead out of the root, or the test certifies nothing.
        assert_eq!(
            fs::read(dst.join("sub/authorized_keys")).ok().as_deref(),
            Some(&b"USER KEY"[..]),
            "fixture is inert: the planted junction does not lead to the outside file"
        );

        fs::write(src.join("sub/authorized_keys"), b"ATTACKER PAYLOAD").unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["sub/authorized_keys".into()],
            &[],
            &[],
            &[],
            false,
            true, // confirmed
        )
        .expect("a confirmed plan runs");

        // HARM FIRST, off the filesystem, before any verdict is looked at.
        assert_eq!(
            fs::read(&victim).ok().as_deref(),
            Some(&b"USER KEY"[..]),
            "HARM: the backup wrote through a directory junction one level up, overwriting a file \
             OUTSIDE the backup root that nothing in the plan ever named"
        );
        assert_eq!(results.len(), 1, "one OpResult per plan entry: {results:?}");
        assert!(
            !results[0].ok,
            "the redirected write must be REFUSED and reported, never reported as a success: {:?}",
            results[0]
        );
        assert!(
            results[0].error.contains("\"sub\"") && results[0].error.contains("is a link"),
            "the refusal must name the offending component AND say a link is what stopped it. \
             CPE-1889's wording said the destination 'resolves outside the backup root', which was the \
             only thing a path resolution could establish; CPE-1896 replaced that resolution with a \
             per-component walk that never resolves the whole path at all, so the refusal now names \
             the exact component — strictly more, not less, than the old assertion: {:?}",
            results[0]
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1912's fixture, staged verbatim, as a deterministic test.** A directory junction
    /// `dst/Photos -> dst/Trash` planted *inside* the destination root. No race, no thread, no timing
    /// window — write access to the destination tree is the whole precondition.
    ///
    /// Before CPE-1896's per-component walk, both containment guards admitted it and were right to by
    /// their own contracts: the pre-write check (`confined_to_resolved_root`) and the post-write
    /// landing check (`landed_inside`) each compare the resolved destination against the **root**, and
    /// `dst/Trash` is inside the root. Neither asked whether the bytes landed at the path the plan
    /// actually *named*. CPE-1912 measured the result: `ok: true`, the photo in `dst/Trash`, and
    /// `dst/Photos` keeping whatever stale content it had.
    ///
    /// CPE-1896 closed it as a side effect, and this test is what establishes that rather than
    /// inferring it: the walk opens `Photos` relative to the root handle with
    /// `FILE_OPEN_REPARSE_POINT` / `O_NOFOLLOW` and refuses **any** name surrogate at **any**
    /// component, whether it points inside the root or outside it. "Inside the root" was never the
    /// question the walk asks; "is this component a real directory" is.
    ///
    /// Kept as a standing regression rather than deleted as redundant with the outside-the-root
    /// junction tests above, because it is the one fixture that distinguishes the two guards: a
    /// containment-only guard passes it, and the per-component walk does not.
    #[test]
    fn cpe_1912_a_junction_inside_the_destination_never_silently_redirects_a_subtree() {
        let d = scratch("cpe1912-inside");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(src.join("Photos")).unwrap();
        fs::create_dir_all(dst.join("Trash")).unwrap();
        fs::write(src.join("Photos/holiday.jpg"), b"THE PHOTO THE USER IS BACKING UP").unwrap();

        if !crate::fsutil::make_dir_link(&dst.join("Trash"), &dst.join("Photos")) {
            crate::skip_notice!(
                "SKIPPING cpe_1912_a_junction_inside_the_destination_never_silently_redirects_a_subtree: \
                 could not stage a directory link. NOTHING on this run covered the inside-the-root \
                 junction shape"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        // Liveness: the fixture must really redirect, or the test certifies nothing. A write through
        // `dst/Photos` has to come out in `dst/Trash`.
        fs::write(dst.join("Photos/liveness.txt"), b"through the junction").unwrap();
        assert_eq!(
            fs::read(dst.join("Trash/liveness.txt")).ok().as_deref(),
            Some(&b"through the junction"[..]),
            "fixture is inert: the planted junction does not redirect into dst/Trash"
        );
        fs::remove_file(dst.join("Trash/liveness.txt")).unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["Photos/holiday.jpg".into()],
            &[],
            &[],
            &[],
            false,
            true, // confirmed
        )
        .expect("a confirmed plan runs");

        // HARM FIRST, off the filesystem. The photo must not be sitting in Trash.
        assert!(
            !dst.join("Trash/holiday.jpg").exists(),
            "HARM: the backup wrote the user's photo into dst/Trash because a junction at dst/Photos \
             redirected the whole subtree — both paths are inside the root, so no containment check \
             can see it (CPE-1912)"
        );
        assert_eq!(results.len(), 1, "one OpResult per plan entry: {results:?}");
        assert!(
            !results[0].ok,
            "a redirected subtree must be REFUSED and reported, never reported as a success: {:?}",
            results[0]
        );
        assert!(
            results[0].error.contains("\"Photos\"") && results[0].error.contains("is a link"),
            "the refusal must name the redirecting component and say a link stopped it: {:?}",
            results[0]
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1889 / A4: the same junction, but the plan entry names a file that does **not** exist
    /// outside — so the redirected write *creates* a brand-new file in the attacker's directory. The
    /// harm assertion is therefore "nothing was created", which no return-value check can make.
    #[test]
    fn cpe_1889_a_junction_at_the_parent_never_creates_a_new_file_outside_the_root() {
        let d = scratch("cpe1889-a4");
        let (src, dst, outside) = (d.join("src"), d.join("dst"), d.join("outside"));
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::create_dir_all(&outside).unwrap();

        if !crate::fsutil::make_dir_link(&outside, &dst.join("sub")) {
            crate::skip_notice!(
                "SKIPPING cpe_1889_a_junction_at_the_parent_never_creates_a_new_file_outside_the_root: \
                 could not stage a directory link. NOTHING on this run covered the parent-junction \
                 create hole in the backup writer"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        fs::write(src.join("sub/planted.txt"), b"ATTACKER PAYLOAD").unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["sub/planted.txt".into()],
            &[],
            &[],
            &[],
            false,
            true, // confirmed
        )
        .expect("a confirmed plan runs");

        assert!(
            !outside.join("planted.txt").exists(),
            "HARM: the backup CREATED a new file outside the backup root, through a directory \
             junction one level up"
        );
        assert!(!results[0].ok, "the redirected create must be refused and reported per file: {:?}", results[0]);
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1889, the `create_dir_all` half specifically: a refusal must not leave **directory debris**
    /// outside the root either. `copy_one_verified` used to `create_dir_all(dst.parent())` before any
    /// guard ran, so a plan entry two levels below the junction materialised `outside/deeper/` even
    /// when the file write was then refused. The containment check therefore has to run BEFORE the
    /// `create_dir_all`, not merely before the write.
    #[test]
    fn cpe_1889_a_refused_write_creates_no_directory_debris_outside_the_root() {
        let d = scratch("cpe1889-debris");
        let (src, dst, outside) = (d.join("src"), d.join("dst"), d.join("outside"));
        fs::create_dir_all(src.join("sub/deeper")).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::create_dir_all(&outside).unwrap();

        if !crate::fsutil::make_dir_link(&outside, &dst.join("sub")) {
            crate::skip_notice!(
                "SKIPPING cpe_1889_a_refused_write_creates_no_directory_debris_outside_the_root: could \
                 not stage a directory link. NOTHING on this run covered the create_dir_all debris leg"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        fs::write(src.join("sub/deeper/x.txt"), b"ATTACKER PAYLOAD").unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["sub/deeper/x.txt".into()],
            &[],
            &[],
            &[],
            false,
            true, // confirmed
        )
        .expect("a confirmed plan runs");

        assert!(
            !outside.join("deeper").exists(),
            "HARM: a refused backup entry still created a directory OUTSIDE the backup root, because \
             create_dir_all ran through the junction before any guard did"
        );
        assert!(!results[0].ok, "the entry must be reported as refused: {:?}", results[0]);
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1889 AC2: the refusal is reported **per file** and the rest of the batch still applies —
    /// the same shape CPE-1879's `apply_backup_plan_reports_a_hard_link_refusal_per_file_not_silently`
    /// pins for the link guard. A whole-run abort would be a different (and worse) regression than the
    /// silent success this ticket closes, so it is asserted rather than assumed.
    #[test]
    fn cpe_1889_a_junction_refusal_is_per_file_and_the_rest_of_the_batch_still_applies() {
        let d = scratch("cpe1889-per-file");
        let (src, dst, outside) = (d.join("src"), d.join("dst"), d.join("outside"));
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::create_dir_all(&outside).unwrap();

        if !crate::fsutil::make_dir_link(&outside, &dst.join("sub")) {
            crate::skip_notice!(
                "SKIPPING cpe_1889_a_junction_refusal_is_per_file_and_the_rest_of_the_batch_still_applies: \
                 could not stage a directory link. NOTHING on this run covered the per-file reporting \
                 shape of the parent-containment refusal"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        fs::write(src.join("ok.txt"), b"ordinary file").unwrap();
        fs::write(src.join("sub/redirected.txt"), b"ATTACKER PAYLOAD").unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["sub/redirected.txt".into(), "ok.txt".into()],
            &[],
            &[],
            &[],
            false,
            true, // confirmed
        )
        .expect("a confirmed plan runs — a refused entry must not abort the whole plan");

        assert!(
            !outside.join("redirected.txt").exists(),
            "HARM: the redirected entry still landed outside the backup root"
        );
        assert_eq!(results.len(), 2, "one OpResult per plan entry — nothing silently dropped: {results:?}");
        let bad = results
            .iter()
            .find(|r| r.path.ends_with("redirected.txt"))
            .expect("the refused entry must still be REPORTED, not dropped from the results");
        assert!(!bad.ok, "the refused entry must be reported as failed: {bad:?}");
        assert!(
            !bad.error.is_empty(),
            "a refusal with an empty error is the silent-success shape this ticket exists to close: {bad:?}"
        );
        // The rest of the batch, off disk — not off the return value.
        assert_eq!(
            fs::read(dst.join("ok.txt")).ok().as_deref(),
            Some(&b"ordinary file"[..]),
            "the unrelated entry must still have been copied: {results:?}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// The guard must not cost the ordinary case anything: a plan entry whose parent directories do
    /// **not exist yet** is the normal shape of a first backup run, and containment has to answer
    /// "contained" for it (every absent component provably cannot be a link) rather than failing open
    /// or failing closed. Without this, the fix for CPE-1889 would break first-run backups outright.
    #[test]
    fn cpe_1889_a_deep_new_directory_chain_still_backs_up_normally() {
        let d = scratch("cpe1889-newdirs");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(src.join("a/b/c")).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(src.join("a/b/c/deep.txt"), b"ordinary content").unwrap();

        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            &["a/b/c/deep.txt".into()],
            &[],
            &[],
            &[],
            true, // verify
            true, // confirmed
        )
        .expect("a confirmed plan runs");

        assert_eq!(
            fs::read(dst.join("a/b/c/deep.txt")).ok().as_deref(),
            Some(&b"ordinary content"[..]),
            "an ordinary first-run backup into directories that do not exist yet must still copy: \
             {results:?}"
        );
        assert!(results[0].ok, "…and must be reported as a success: {:?}", results[0]);
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1889 AC4 — **cost, measured rather than asserted**, because this guard sits on the backup
    /// engine's inner loop and the ticket asks what it means for a 100,000-file run.
    ///
    /// `#[ignore]` on purpose: it is a measurement, not a property. Timings vary by machine,
    /// filesystem and cache state, so making CI assert a number would produce a flaky red that says
    /// nothing about correctness. Run it deliberately:
    ///
    /// ```text
    /// cargo test --lib cpe_1889_measure_the_guard_cost -- --ignored --nocapture
    /// ```
    ///
    /// The A/B is inside one binary and one process, so the two legs share every other cost: leg A is
    /// the real guarded `apply_backup_plan`; leg B replays the **pre-fix** shape byte for byte
    /// (`create_dir_all` then the same `copy_file_onto_no_follow_with_wording`) with the containment
    /// calls removed. The difference is the guard and nothing else.
    ///
    /// **Expect a NEGATIVE delta about as often as a positive one, and do not report either as the
    /// answer.** Four runs on a local NTFS volume gave +11.3, −67.0, −21.2 and +29.2 µs/file: the copy
    /// itself (open, read-loop, write, permissions, times, per file) varies by far more than the two
    /// extra path resolutions the guard adds, so on local storage this measures noise. It is still
    /// worth running — a delta that ever came out *consistently* large would mean something changed —
    /// but the number to quote for the guard's cost is the syscall count on [`copy_one_verified`], and
    /// the machine to re-measure wall clock on is a **network** destination, where each resolution is a
    /// round trip and no longer hides behind the copy.
    #[test]
    #[ignore = "measurement, not a property — see the doc comment; run with --ignored"]
    fn cpe_1889_measure_the_guard_cost() {
        const DIRS: usize = 20;
        const PER_DIR: usize = 100;

        let d = scratch("cpe1889-cost");
        let src = d.join("src");
        let mut rels: Vec<String> = Vec::with_capacity(DIRS * PER_DIR);
        for dir in 0..DIRS {
            fs::create_dir_all(src.join(format!("d{dir}"))).unwrap();
            for f in 0..PER_DIR {
                let rel = format!("d{dir}/f{f}.bin");
                fs::write(src.join(&rel), vec![b'x'; 4096]).unwrap();
                rels.push(rel);
            }
        }

        // Leg B first, so leg A cannot be flattered by leg B having warmed the metadata cache.
        let unguarded_dst = d.join("dst-unguarded");
        fs::create_dir_all(&unguarded_dst).unwrap();
        let t_unguarded = std::time::Instant::now();
        for rel in &rels {
            let (s, t) = (src.join(rel), unguarded_dst.join(rel));
            if let Some(parent) = t.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            crate::fsutil::copy_file_onto_no_follow_with_wording(
                &s,
                &t,
                crate::fsutil::LinkGuardWording::BACKUP,
            )
            .unwrap();
        }
        let unguarded = t_unguarded.elapsed();

        let guarded_dst = d.join("dst-guarded");
        fs::create_dir_all(&guarded_dst).unwrap();
        let t_guarded = std::time::Instant::now();
        let results = apply_backup_plan(
            &src.to_string_lossy(),
            &guarded_dst.to_string_lossy(),
            &rels,
            &[],
            &[],
            &[],
            false,
            true, // confirmed
        )
        .expect("a confirmed plan runs");
        let guarded = t_guarded.elapsed();

        assert!(results.iter().all(|r| r.ok), "the measurement is only meaningful if every entry copied");
        let n = rels.len() as f64;
        let delta = guarded.as_secs_f64() - unguarded.as_secs_f64();
        // stderr, not `println!`: libtest swallows the macros (see `require_staged`'s doc comment).
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "CPE-1889 guard cost over {} files ({DIRS} dirs x {PER_DIR}): guarded {:?}, pre-fix shape \
             {:?}, delta {:.3} ms total = {:.1} us/file. Extrapolated to 100k files: {:.1} s.",
            rels.len(),
            guarded,
            unguarded,
            delta * 1000.0,
            delta / n * 1_000_000.0,
            delta / n * 100_000.0,
        );
        let _ = fs::remove_dir_all(&d);
    }

    // ─────────────────────────────────────────────────────────────────────────────────────────────
    // CPE-1896 — the landing check. Read the doc comment on `landed_inside` first: these tests cover
    // the MITIGATION (an escaped write is never reported as a success), NOT the race, which is still
    // open. Nothing below asserts that the write is prevented, because it is not.
    // ─────────────────────────────────────────────────────────────────────────────────────────────

    /// Identity of whatever is at `path`, by the same route [`landed_inside`] uses. Test-side helper so
    /// the deterministic legs can hand the check a *real* `written` identity — the whole point of those
    /// legs after the CPE-1896 round-1 finding is that a fabricated one would prove nothing.
    fn identity_of(path: &std::path::Path) -> Option<crate::batch_media::FileIdentity> {
        crate::batch_media::open_existing_no_follow_read(path)
            .ok()
            .and_then(|f| crate::batch_media::handle_facts(&f))
            .map(|f| f.id)
    }

    /// The mitigation's own red-proof for the **one-phase** escape, staged **deterministically** — no
    /// thread, no timing.
    ///
    /// It stages the exact on-disk state an escaped write leaves behind: a directory junction at
    /// `dst/sub` pointing outside the backup root, with the victim file sitting at the far end of it,
    /// and hands the check the identity of the file the bytes really went into. That is what check (3)
    /// meets when the race has just fired and the attacker has *not* swapped back (the swap-back case is
    /// the next test).
    ///
    /// Three assertions, all load-bearing: the call **refuses**; the refusal **names the outside path**;
    /// and it **names the source file whose bytes are now sitting there**. The last two matter as much
    /// as the first, because the bytes are already out there — a refusal that says neither where they
    /// landed nor what now overwrites the user's file leaves them nothing to act on.
    #[test]
    fn cpe_1896_the_landing_check_refuses_a_destination_that_resolves_outside_the_root() {
        let d = scratch("cpe1896-landing-out");
        let (src, dst, outside) = (d.join("src"), d.join("dst"), d.join("outside"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(src.join("victim.txt"), b"BACKUP SOURCE BYTES").unwrap();
        fs::write(outside.join("victim.txt"), b"USER DATA").unwrap();

        if !crate::fsutil::make_dir_link(&outside, &dst.join("sub")) {
            crate::skip_notice!(
                "SKIPPING cpe_1896_the_landing_check_refuses_a_destination_that_resolves_outside_the_root: \
                 could not stage a directory link. NOTHING on this run covered the CPE-1896 landing check's \
                 refusal leg"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let real_root = fs::canonicalize(&dst).unwrap();
        let escaped_to = outside.join("victim.txt");
        let err = landed_inside(
            &src.join("victim.txt"),
            &dst.join("sub").join("victim.txt"),
            &real_root,
            identity_of(&escaped_to),
        )
        .expect_err(
            "a destination that resolves OUTSIDE the backup root must be refused — reporting it as \
             backed up is the silent-success shape CPE-1896 measured 73 times in 1200 trials",
        );

        let real_outside = fs::canonicalize(&escaped_to).unwrap();
        assert!(
            err.contains(&format!("{real_outside:?}")),
            "the refusal must NAME the outside path the bytes actually reached, or the user has no way \
             to find the file this run overwrote: {err}"
        );
        assert!(
            err.contains(&format!("{:?}", src.join("victim.txt"))),
            "the refusal must name the SOURCE file whose bytes are now sitting at that outside path — \
             telling a user their file was destroyed without saying what replaced it is half an answer \
             (CPE-1896 round-1 review): {err}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **The CPE-1896 round-1 blocking finding, as a deterministic test: the two-phase swap-back.**
    ///
    /// Round 1's landing check asked containment about `canonicalize(dst)` and stopped there. The
    /// auditor beat it by winning the window **twice in opposite directions** — junction in, the write
    /// escapes and streams past 4 KiB, then the innocent directory renamed *back* before the engine asks
    /// where the bytes went. `canonicalize(dst)` then resolves to a real, ordinary file inside the root:
    /// on an `update` entry, the stale previous backup that is naturally already sitting there. Path
    /// containment says yes, and the run reported `ok: true` on an escaped write — 38 of 400 trials,
    /// with `verify = false`, *both* harms at once (the escape, and the backup silently not happening).
    ///
    /// The post-swap-back state is fully stageable without a thread — that is the auditor's own
    /// demonstration — because after the swap-back every path genuinely *is* what it says it is. What
    /// distinguishes the two worlds is not any path but the **object**: the identity the write handle
    /// reported is not the identity of the file now sitting at the contained path. This test stages
    /// exactly that and asserts the refusal, so the round-2 fix has a gate that goes red without it on
    /// **every** `cargo test`, not only when a race happens to fire.
    #[test]
    fn cpe_1896_the_landing_check_refuses_a_swapped_back_path_that_is_not_the_file_it_wrote() {
        let d = scratch("cpe1896-swapback");
        let (src, dst, outside) = (d.join("src"), d.join("dst"), d.join("outside"));
        fs::create_dir_all(&src).unwrap();
        // The post-swap-back world: `dst/sub` is a perfectly ordinary directory again, and the file the
        // plan named is really there — the stale previous backup an `update` entry always has.
        fs::create_dir_all(dst.join("sub")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(src.join("victim.txt"), b"BACKUP SOURCE BYTES").unwrap();
        fs::write(dst.join("sub").join("victim.txt"), b"the stale previous backup").unwrap();
        // Where the bytes actually went while the junction was in place.
        fs::write(outside.join("victim.txt"), b"BACKUP SOURCE BYTES").unwrap();

        let real_root = fs::canonicalize(&dst).unwrap();
        let written = identity_of(&outside.join("victim.txt"));
        if written.is_none_or(|id| id.is_degenerate()) {
            crate::skip_notice!(
                "SKIPPING cpe_1896_the_landing_check_refuses_a_swapped_back_path_that_is_not_the_file_it_wrote: \
                 this volume reports no usable file identity, which is the documented fallback case. \
                 NOTHING on this run covered the two-phase swap-back refusal"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let err = landed_inside(
            &src.join("victim.txt"),
            &dst.join("sub").join("victim.txt"),
            &real_root,
            written,
        )
        .expect_err(
            "the swapped-back path is inside the root and resolves cleanly, so PATH containment admits \
             it — only the identity comparison can tell that it is not the object the bytes went into. \
             Admitting it is the 38/400 `ok: true` the round-1 review blocked on",
        );
        assert!(
            err.contains("not the file this entry wrote"),
            "the refusal must say what is actually wrong — the path is fine, the object is not: {err}"
        );
        assert!(
            err.contains(&format!("{:?}", src.join("victim.txt"))),
            "…and must name the source file whose bytes are now somewhere else: {err}"
        );
        // The other half of the harm, asserted so it cannot be forgotten: the backup did NOT happen.
        assert_eq!(
            fs::read(dst.join("sub").join("victim.txt")).unwrap(),
            b"the stale previous backup",
            "the file inside the root is untouched — reporting this entry as a success would claim a \
             backup that never took place"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// The other half of the red-proof, and the one that matters more in practice: the landing check
    /// must **admit** an ordinary destination and hand back the resolved path the verify leg then reads.
    /// A landing check that reds on healthy backups would be worse than the bug it closes — every run
    /// would report failures nobody could act on.
    ///
    /// Covers both identity worlds, because they are different code paths: a real `written` identity
    /// that matches, and `None` — the documented fallback for a volume whose
    /// `GetFileInformationByHandle` cannot supply a usable one (several network redirectors; see
    /// [`landed_inside`]'s "degrades to the path question" section). The fallback must **admit**, not
    /// refuse: refusing there would report every file of every backup to such a volume as failed.
    #[test]
    fn cpe_1896_the_landing_check_admits_an_ordinary_destination() {
        let d = scratch("cpe1896-landing-in");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(dst.join("sub")).unwrap();
        fs::write(src.join("ok.txt"), b"ordinary content").unwrap();
        fs::write(dst.join("sub").join("ok.txt"), b"ordinary content").unwrap();

        let real_root = fs::canonicalize(&dst).unwrap();
        let here = dst.join("sub").join("ok.txt");
        for (label, written) in [("the real identity", identity_of(&here)), ("no identity at all", None)] {
            let landed = landed_inside(&src.join("ok.txt"), &here, &real_root, written)
                .unwrap_or_else(|e| panic!("an ordinary file inside the backup root must be admitted with {label}: {e}"));
            assert!(
                landed.starts_with(&real_root),
                "the resolved path must be inside the root ({label}): {landed:?}"
            );
            assert_eq!(
                fs::read(&landed).unwrap(),
                b"ordinary content",
                "the resolved path is what the verify leg reads back, so it has to address the real file"
            );
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1896 regression guard on the ordinary path, at the **engine** level and in **both** verify
    /// modes: a healthy backup still copies, still verifies, and still reports success. Check (3) runs
    /// unconditionally now, so it is on the inner loop of every backup anyone runs — if it were wrong
    /// about a normal destination, every entry of every job would report a failure.
    ///
    /// Deliberately covers the two shapes that differ inside `copy_one_verified`: a file whose parent
    /// chain does not exist yet (`create_dir_all` runs, check (2) fires) and an overwrite of a file that
    /// is already there (neither runs). Both must land as successes with **empty** error text.
    #[test]
    fn cpe_1896_an_ordinary_backup_still_verifies_and_still_reports_success() {
        for verify in [false, true] {
            let d = scratch("cpe1896-healthy");
            let (src, dst) = (d.join("src"), d.join("dst"));
            fs::create_dir_all(src.join("a/b")).unwrap();
            fs::create_dir_all(&dst).unwrap();
            fs::write(src.join("a/b/fresh.txt"), b"brand new content").unwrap();
            fs::write(src.join("existing.txt"), b"updated content").unwrap();
            fs::write(dst.join("existing.txt"), b"stale content").unwrap();

            let results = apply_backup_plan(
                &src.to_string_lossy(),
                &dst.to_string_lossy(),
                &["a/b/fresh.txt".into()],
                &["existing.txt".into()],
                &[],
                &[],
                verify,
                true, // confirmed
            )
            .expect("a confirmed plan runs");

            assert_eq!(results.len(), 2, "one OpResult per plan entry (verify={verify}): {results:?}");
            for r in &results {
                assert!(
                    r.ok && r.error.is_empty(),
                    "a healthy backup entry must still report success with no error text \
                     (verify={verify}) — a landing check that reds on ordinary backups is worse than \
                     the bug it closes: {r:?}"
                );
            }
            // Off disk, not off the return value.
            assert_eq!(fs::read(dst.join("a/b/fresh.txt")).unwrap(), b"brand new content");
            assert_eq!(fs::read(dst.join("existing.txt")).unwrap(), b"updated content");
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **The CPE-1896 race probe — an OBSERVATION INSTRUMENT for a race that is still open. It is NOT
    /// the regression gate, and it must never be turned into one.**
    ///
    /// Read that first, because the obvious "improvement" to this test is the wrong move and the round-1
    /// review measured why.
    ///
    /// # Its role, and the trap
    ///
    /// The regression gate for the landing check is the three **deterministic** tests above
    /// (`…_refuses_a_destination_that_resolves_outside_the_root`,
    /// `…_refuses_a_swapped_back_path_that_is_not_the_file_it_wrote`,
    /// `…_admits_an_ordinary_destination`). Sabotage `landed_inside` and they go red **100% of the
    /// time, on every `cargo test`, on every runner** — no thread, no timing, no volume-dependence.
    /// They are what stops this fix being deleted by accident.
    ///
    /// This test is the opposite kind of thing: it races the real engine to *observe* whether the window
    /// is still reachable and what the engine says when it fires. Its escape rate is wildly
    /// volume-dependent — the reviewer measured **1 per 600 trials on the machine `TEMP` volume against
    /// 4, 4, 5 and 3 per 600 on the worktree volume**, same binary, same machine, same afternoon. At
    /// 1/600 a zero-escape run is entirely plausible, so a run of this test can go **green against a
    /// removed fix**. That is why it is not the gate, and why nobody should later "harden" it by
    /// asserting a rate (`escaped > 0`, or any threshold): that would buy nothing the deterministic
    /// tests do not already give, in exchange for a test that reds at random. It is markedly more
    /// sensitive with `TMP`/`TEMP` redirected to a fast local volume — the reviewer got 4-of-4 sabotage
    /// detection that way, against 2-of-3 at the default — so run it that way when you want it to bite.
    ///
    /// # The recipe, unchanged from the finding
    ///
    /// A junction is parked at `dst/junc` pointing outside the backup root, with a victim file at the
    /// far end. A racing thread does **two renames**: `dst/sub` → `dst/sub_old`, then `dst/junc` →
    /// `dst/sub`. `rename` moves a **non-empty** directory, so `dst/sub` is deliberately populated (with
    /// the stale previous backup an `update` entry always has) — the attacker never needs the slot free,
    /// which is what makes this reachable against a backup destination already in use. If the swap lands
    /// inside the three-syscall window between `copy_one_verified`'s check (1) and the destination open,
    /// the copy writes through the junction and the file outside the root is overwritten.
    ///
    /// **Half the trials run the two-phase form** that beat round 1's path-only landing check: after the
    /// swap the racer watches the outside file grow past 4 KiB — proof the escaped write is streaming —
    /// and renames the innocent directory **back**, so that by the time the engine asks where the bytes
    /// went, every path is once again exactly what it says it is. The source file is 1 MiB so that
    /// window exists at all. Both forms assert the same property.
    ///
    /// # The two-phase leg DOES win on Windows — this test's racer is simply not patient enough
    ///
    /// An earlier revision of this comment claimed the swap-back could not land on Windows, on the
    /// theory that the engine holds the escaped file open through the junction while it streams and
    /// Windows refuses a directory rename until that handle closes. **The mechanism is real; the
    /// conclusion was wrong, and it is corrected here rather than quietly dropped.**
    ///
    /// This test's racer retries the rename-back 2,000 times at 10 µs — a **20 ms** budget — and sees
    /// zero landings. An independent Security Auditor's racer waits for the outside file to pass 4 KiB
    /// and then retries against a **400 ms** deadline, and on this same Windows machine measured, with
    /// the identity comparison neutralised: **206 escapes, 9 reported `ok: true`, swap-back completing
    /// 214 times in 400 trials.** With the comparison live and the same racer: **214 escapes, 0 reported
    /// `ok: true`**, swap-back completing the same 214 times. The attack lands equally in both; only the
    /// engine's ability to be lied to changed.
    ///
    /// The open-handle argument does not protect anyone: `copy_file_onto_no_follow_with_wording` drops
    /// the write handle when it returns, and `landed_inside`'s `canonicalize` runs *after* that. There is
    /// a genuine handle-free window on Windows and a patient racer finds it. 20 ms was simply shorter
    /// than a 1 MiB copy plus `set_permissions` and `carry_file_times` on a loaded machine.
    ///
    /// **Do not read this as Windows being structurally safer than POSIX here. It is not.**
    ///
    /// **As written — with its 20 ms budget — this test still does not red-proof the identity
    /// comparison, and must not be cited as doing so.** That is a property of this racer's impatience,
    /// not of the platform, and CPE-1915 tracks widening the deadline so it bites here too (~2% of
    /// trials at a few hundred ms, per the auditor's measurement).
    ///
    /// The thing that actually red-proofs the identity comparison, today, is
    /// `cpe_1896_the_landing_check_refuses_a_swapped_back_path_that_is_not_the_file_it_wrote` — on every
    /// run, on every platform, with no thread and no timing. Neutralise the comparison and it fails
    /// while the other three stay green; the auditor confirmed that across three consecutive runs.
    ///
    /// The leg is kept because it is the auditor's actual attack shape rather than a reconstruction of
    /// its end state. Do not delete it, and do not read a quiet run — on any platform — as evidence the
    /// swap-back is closed.
    ///
    /// # What it asserts — the SAFETY PROPERTY, not the race outcome
    ///
    /// > **If a write escaped, it was never reported as a success.**
    ///
    /// A conditional: vacuously satisfied on a run where nothing escapes, violated the instant an escape
    /// comes back `ok: true`. Nothing in it depends on how often the window is hit — which is exactly
    /// the property that lets an unreliable instrument still be worth keeping in the tree.
    ///
    /// # `#[ignore]`, deliberately
    ///
    /// A thread and a junction per trial, ~1 MiB copied per trial, seconds of wall clock. Run it
    /// deliberately:
    ///
    /// ```text
    /// cargo test --lib cpe_1896_a_parent_swapped_under_the_copy -- --ignored --nocapture
    /// ```
    ///
    /// # It now asserts `escaped == 0` — the atomic half landed, and the evidence is directional
    ///
    /// Every revision of this comment before CPE-1896's atomic half said, correctly, that the test
    /// could not show the race being fixed because the fix was not written. It is written now:
    /// [`crate::open_beneath`] opens the destination one component at a time against the previous
    /// component's **open handle**, so the racer's rename has no second parent lookup left to
    /// redirect. `escaped == 0` is the assertion the ticket always named as the one to add here, and
    /// it is added.
    ///
    /// **Measured, this branch, this machine (Windows 11, NTFS, `%TEMP%`):** 400 trials, **0 escapes**
    /// (0 one-phase, 0 two-phase), 397 refused, 3 wrote-inside-normally. Neutralised — `create_beneath`
    /// swapped for the pre-fix path open, everything else identical — the same 400 trials gave
    /// escapes back. The red/green pair is recorded in the CPE-1896 Work Log with both raw counts.
    ///
    /// **Read the direction of the evidence.** A zero count on the *pre*-fix code proved nothing: the
    /// window was hit a few times per 600 and the rate swung by an order of magnitude between two
    /// volumes on one machine, which is why this test was previously labelled an observation
    /// instrument rather than a gate. A *nonzero* count on the post-fix code is the opposite kind of
    /// evidence — the property no longer depends on winning a race, so an escape is a defect in the
    /// walk. That asymmetry is what makes the assertion sound while the instrument stays unreliable.
    ///
    /// It stays `#[ignore]`d anyway, on cost alone: 400 trials × 1 MiB × a thread and a junction each
    /// is **~64 s** of wall clock here, which does not belong on every `cargo test`. The per-run
    /// deterministic cover for the same mechanism is
    /// `crate::open_beneath::tests::refuses_a_link_at_an_intermediate_component_and_writes_nothing_through_it`,
    /// which needs no thread and no timing.
    #[test]
    #[ignore = "race probe: spawns a racing thread per trial and sweeps a sub-millisecond window — see the doc comment; run with --ignored"]
    fn cpe_1896_a_parent_swapped_under_the_copy_is_never_reported_as_a_success() {
        const TRIALS: usize = 400;
        const USER_BYTES: &[u8] = b"USER DATA THIS BACKUP WAS NEVER POINTED AT";
        const STALE: &[u8] = b"the stale previous backup";
        // Big enough that the escaped write STREAMS, which is what the two-phase leg needs to observe
        // before it swaps back.
        const SOURCE_LEN: usize = 1 << 20;

        let d = scratch("cpe1896-race");
        let (mut escaped, mut refused, mut normal) = (0usize, 0usize, 0usize);
        let (mut escaped_one_phase, mut escaped_two_phase) = (0usize, 0usize);

        for trial in 0..TRIALS {
            let two_phase = trial % 4 < 2;
            let t = d.join(format!("t{trial}"));
            let (src, dst, outside) = (t.join("src"), t.join("dst"), t.join("outside"));
            fs::create_dir_all(src.join("sub")).unwrap();
            fs::write(src.join("sub/victim.txt"), vec![b'S'; SOURCE_LEN]).unwrap();
            fs::create_dir_all(dst.join("sub")).unwrap();
            // Populated on purpose, twice over: `rename` moves a non-empty directory (so the attacker
            // never waits for the slot to be free), and the entry's own stale previous copy is what the
            // two-phase swap-back leaves the engine looking at.
            fs::write(dst.join("sub/victim.txt"), STALE).unwrap();
            fs::write(dst.join("sub/already-backed-up.txt"), b"an existing backed-up file").unwrap();
            fs::create_dir_all(&outside).unwrap();
            fs::write(outside.join("victim.txt"), USER_BYTES).unwrap();

            if !crate::fsutil::make_dir_link(&outside, &dst.join("junc")) {
                crate::skip_notice!(
                    "SKIPPING cpe_1896_a_parent_swapped_under_the_copy_is_never_reported_as_a_success: \
                     could not stage a directory link. NOTHING on this run raced the backup engine"
                );
                let _ = fs::remove_dir_all(&d);
                return;
            }

            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let racer = {
                let gate = std::sync::Arc::clone(&barrier);
                let (junc, sub, old) = (dst.join("junc"), dst.join("sub"), dst.join("sub_old"));
                let watch = outside.join("victim.txt");
                // Sweep the offset instead of sampling one point of it: the whole copy is tens of
                // microseconds at the front, so a fixed delay would test the same instant 400 times.
                let spins = (trial * 37) % 2048;
                // `fs::rename` raw, and the `disallowed_methods` allow is the POINT of the test rather
                // than an exemption from it. The lint exists because `rename` replaces its destination
                // silently and destroys a link at it — which is precisely the primitive the attacker
                // uses here. Routing this through `fsutil::rename_into_slot` would make the racer
                // refuse to do the thing being measured, and the probe would go quiet without the
                // engine getting any safer.
                #[allow(clippy::disallowed_methods)]
                std::thread::spawn(move || {
                    gate.wait();
                    for _ in 0..spins {
                        std::hint::spin_loop();
                    }
                    let _ = fs::rename(&sub, &old);
                    let _ = fs::rename(&junc, &sub);
                    if !two_phase {
                        return;
                    }
                    // Phase 2: wait for proof the escaped write is streaming, then put the innocent
                    // directory back so every path is honest again by the time the engine looks.
                    for _ in 0..20_000 {
                        if fs::metadata(&watch).is_ok_and(|m| m.len() > 4096) {
                            break;
                        }
                        std::thread::yield_now();
                    }
                    // One attempt, measured rather than assumed — see the "the two-phase leg does not
                    // win on Windows" section of this test's doc comment. Retrying it (2,000 attempts,
                    // 10 µs apart) was tried and changed nothing except the runtime, 20 s to 256 s.
                    let _ = fs::rename(&sub, &junc);
                    let _ = fs::rename(&old, &sub);
                })
            };

            barrier.wait();
            let results = apply_backup_plan(
                &src.to_string_lossy(),
                &dst.to_string_lossy(),
                &[],
                // The `update` list, not `copy`: an entry whose destination already exists is what makes
                // the two-phase swap-back land on a real, ordinary file.
                &["sub/victim.txt".into()],
                &[],
                &[],
                // Sweep BOTH verify modes: the finding measured the escape at 73/1200 with verification
                // off and 68/1200 with it ON, and the second number is what this branch exists for.
                trial % 2 == 1,
                true, // confirmed
            )
            .expect("a confirmed plan runs");
            let _ = racer.join();

            assert_eq!(results.len(), 1, "one OpResult per plan entry: {results:?}");
            let r = &results[0];
            // Length, not content: the source is 1 MiB and reading it back 400 times would dominate the
            // run. Any length other than the victim's own means the backup's bytes reached it.
            let landed_outside = fs::metadata(outside.join("victim.txt"))
                .is_ok_and(|m| m.len() != USER_BYTES.len() as u64);

            if landed_outside {
                escaped += 1;
                if two_phase {
                    escaped_two_phase += 1;
                } else {
                    escaped_one_phase += 1;
                }
                assert!(
                    !r.ok && !r.error.is_empty(),
                    "HARM (CPE-1896): trial {trial} (two_phase={two_phase}) wrote the backup's bytes to \
                     {:?}, OUTSIDE the backup destination, and reported it as a SUCCESS with an empty \
                     error — the silent-success shape this branch exists to close: {r:?}",
                    outside.join("victim.txt")
                );
            } else if r.ok {
                normal += 1;
            } else {
                refused += 1;
            }
            let _ = fs::remove_dir_all(&t);
        }

        // stderr, not `println!`: libtest swallows the macros (see `require_staged`'s doc comment).
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "CPE-1896 race probe over {TRIALS} trials: ESCAPED (bytes written outside the root) \
             {escaped} ({escaped_one_phase} one-phase, {escaped_two_phase} two-phase swap-back), \
             refused {refused}, wrote-inside-normally {normal}. Two properties are asserted, in this \
             order: any escape that DID happen was reported as a per-file failure (the CPE-1896 \
             mitigation half), and no escape happened at all (the CPE-1896 atomic half, added once \
             the per-component walk landed — the expected escape count is now ZERO). A high 'refused' \
             count is the healthy shape here: the racer parks a junction at the parent, and refusing \
             to write through it is the whole point."
        );

        // THE atomic half's assertion, and the one rate-shaped assertion this test was ever supposed
        // to grow (the ticket names it explicitly). Before the per-component walk it would simply
        // have been red: the escapes were real, and the branch that shipped only stopped the engine
        // calling them successes. Now an escape is not narrowed, it is prevented — the destination is
        // opened one component at a time against the previous component's open handle, so there is no
        // second lookup of any parent left for the racer's rename to redirect.
        //
        // **Read the direction of the evidence correctly.** A zero count on a run of the PRE-fix code
        // proves nothing (the window is hit a few times per 600 and the rate swings by an order of
        // magnitude between volumes on one machine), which is why this test was called an observation
        // instrument. A NONZERO count on the post-fix code is the opposite: it is a defect in the
        // walk, never a scheduling accident, because the property no longer depends on winning a race.
        assert_eq!(
            escaped, 0,
            "HARM (CPE-1896): {escaped} of {TRIALS} trials wrote the backup's bytes OUTSIDE the \
             destination root ({escaped_one_phase} one-phase, {escaped_two_phase} two-phase \
             swap-back). The per-component walk is supposed to make that impossible, not merely \
             loud. Refused {refused}, wrote-inside-normally {normal}."
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
