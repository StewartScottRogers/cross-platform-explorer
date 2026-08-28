//! Open a file **beneath** a chosen root so that the containment is atomic with the open, one path
//! component at a time (CPE-1896, acceptance criterion 1).
//!
//! # The problem this exists to remove, not to narrow
//!
//! Every other containment guard in this crate — [`crate::fsutil::confined_to`],
//! [`crate::fsutil::contained_under`], `backup::landed_inside` — asks a
//! question about a **path** and then, some syscalls later, performs an **open** by that same path. The
//! two are separate operations, so an actor with write access to the tree can change what the path
//! means in between. CPE-1889 documented that window; CPE-1896's Security Auditor measured it, at
//! exactly three syscalls wide, and got **73 escaped writes over 1200 trials**, each one overwriting a
//! pre-existing file outside the backup root and each one reported as `ok: true`.
//!
//! No amount of re-checking closes that, because every re-check is another path question. The only
//! thing that closes it is never asking a path question at all: open the root **once**, then open each
//! subsequent component **relative to the handle of the component before it**, refusing to traverse a
//! link at every step. The kernel resolves one name against one already-open directory object; there is
//! no interval in which a rename can redirect it, because there is no second lookup of the parent. What
//! comes back is, by construction, an object reachable from the root by a chain of real directories.
//!
//! # What each platform uses, and why
//!
//! | platform | mechanism | atomic? |
//! |----------|-----------|---------|
//! | Windows  | `NtCreateFile` with `RootDirectory` = the parent handle, `FILE_OPEN_REPARSE_POINT` per component | yes |
//! | Linux    | `openat2(RESOLVE_BENEATH \| RESOLVE_NO_SYMLINKS)`, falling back to the walk below | yes |
//! | Unix     | `openat`/`mkdirat` with `O_NOFOLLOW`, one component at a time | yes |
//!
//! **There is deliberately no fourth row.** The module is `#[cfg(any(unix, windows))]` and
//! `backup::copy_one_verified` keeps no path-based fallback for a target without a handle-relative
//! open. An earlier revision shipped one — a `#[cfg(not(any(unix, windows)))]` arm plus an
//! `open_beneath::ATOMIC` `const bool` the caller branched on, so CPE-1889's two `canonicalize` checks
//! stayed in the source "for that target". PR #1043's reviewer extracted that arm and compiled it:
//! **two `E0308`s and an `E0507`**. It had never been built by anything, `const bool` (unlike `cfg`)
//! keeps `dead_code` silent, and the only two callers of `backup::parent_contained` were inside the
//! branch it guarded — so a security check with no test coverage sat behind a fallback that could not
//! exist. Deleted rather than repaired: a safety net nobody has ever compiled is worse than no net,
//! because it is read as one. A platform without `openat` or `NtCreateFile` now fails to build this
//! crate, loudly, which is the correct answer for a filesystem app.
//!
//! **Windows needs the NT layer and there is no way around it.** Win32 has no handle-relative open:
//! `CreateFileW` takes a path and re-parses it from a drive letter every time. `NtCreateFile` takes an
//! `OBJECT_ATTRIBUTES` whose `RootDirectory` is an open handle and whose `ObjectName` is a *relative*
//! name, which is precisely the primitive `openat` is on Unix. Passing `FILE_OPEN_REPARSE_POINT` on
//! every component means a junction or symlink in the path is opened **as itself** rather than
//! followed; this module then refuses it outright rather than descending into the reparse point's own
//! (usually empty) physical directory, which would be contained but silently surprising.
//!
//! **Linux's `openat2` is a fast path, not a second implementation.** `RESOLVE_BENEATH` gives the whole
//! multi-component resolution the same guarantee in **one** syscall, so the common case (the parent
//! directories already exist — every entry after the first in a given directory) costs one call instead
//! of one per component. It cannot replace the walk: creating the missing directory chain still needs a
//! `mkdirat` per level, and `openat2` is absent before Linux 5.6 and blocked by some seccomp policies.
//! So **any** failure of the fast path falls through to the walk, which then produces the authoritative
//! answer — including the authoritative *refusal*. That is what makes the fast path unable to weaken
//! anything: it can only ever succeed where the walk would also have succeeded.
//!
//! # What this module does NOT claim
//!
//! - **It is not a permission check.** It answers "is this object beneath that root", nothing else.
//! - **It does not defend the root itself.** The caller resolves the root once and passes it in; if the
//!   *root* was already a link to somewhere unexpected, every write goes there and this module agrees.
//!   That is the caller's question ([`crate::backup::apply_backup_plan_walk`] canonicalises it).
//! - **It says nothing about hard links at the final component.** A hard link has no target to follow
//!   and genuinely *is* an object inside the root; the write still comes out at its other name. That is
//!   [`crate::fsutil::copy_file_onto_no_follow_with_wording`]'s `facts.links > 1` refusal, on the same
//!   handle this module returns, and it is deliberately not duplicated here.
//! - **It does not exist on a platform with neither `openat` nor `NtCreateFile`** — the module is not
//!   compiled there and the crate does not build, deliberately. See the "no fourth row" note above.
//!
//! # How many handles are actually open, since the residual is reasoned from it
//!
//! **Exactly two: the root, and the deepest component reached so far.** The walk assigns
//! `held = Some(dir)` each time it descends, which drops the previous directory's handle immediately,
//! so nothing accumulates on a deep tree. An earlier revision of `backup::copy_one_verified`'s residual
//! paragraph said the walk "holds a handle on each intermediate directory" — both the Reviewer and the
//! Security Auditor of PR #1043 flagged it independently. The conclusion is unchanged and the blast
//! radius is smaller than described: an actor renaming a directory out from under the walk can affect
//! **one** directory, the one currently being descended into, not the chain.
//!
//! On Windows that rename is refused by the OS for as long as a descendant is open — 248 instrumented
//! attempts, every one `Access is denied` — and it is worth saying explicitly that this is **not**
//! because the held handle blocks it: [`sys::SHARE_ALL`] deliberately includes `FILE_SHARE_DELETE`, so
//! the handle grants no such veto and is still the right choice (without it, a run would stop the user
//! deleting their own backup folder). The protection is Windows' own open-descendant rule, which is a
//! different mechanism from the share mode. POSIX has no equivalent rule at all; see the per-platform
//! breakdown on `backup::copy_one_verified`.

use std::ffi::OsStr;
use std::fs::File;
use std::path::{Component, Path, PathBuf};

/// A destination root, resolved and **held open** for the life of a run.
///
/// Holding the handle is not an optimisation, it is the anchor: every write in the run is resolved
/// against *this object*, so renaming or replacing the root's name mid-run cannot redirect a single
/// entry. It also removes one path resolution per file from the inner loop, which is the cost half of
/// CPE-1896's acceptance criterion 5.
pub(crate) struct RootDir {
    /// The already-canonicalised root path. Kept for error messages only — never re-opened.
    path: PathBuf,
    /// The open directory handle every component is resolved against.
    dir: File,
    /// What to call this root in a refusal — "backup destination", "extraction folder", "download
    /// folder", "folder being restored" (CPE-1913). One sentence template, one owner, and the noun
    /// comes from the caller so a user who is extracting an archive is not told about a backup
    /// destination they never chose.
    noun: &'static str,
}

/// What [`create_beneath`] opened: the handle, and whether **this call** created it.
///
/// `created` carries exactly the meaning [`crate::batch_media::open_no_follow`]'s second return value
/// does, and is established the same way — an exclusive create attempted first, so the answer comes
/// from the kernel rather than from a preceding `exists()` that a race could invalidate.
#[derive(Debug)]
pub(crate) struct Opened {
    pub(crate) file: File,
    pub(crate) created: bool,
}

// Syscall counter for CPE-1896's cost measurement (acceptance criterion 5). Test builds only: the
// increment compiles to nothing in a shipped binary, so the instrument cannot itself become the cost.
//
// **Thread-local, and that is not a detail.** A process-wide counter was tried first and gave 5.16
// syscalls per file for a shape that can only cost a whole number — libtest runs tests in parallel, so
// the sibling tests in this module were adding their own walks to the total. The walk always runs on
// the calling thread, so a thread-local count is both exact and immune to whatever else the test
// binary is doing. (A plain `//` comment: rustdoc does not document a macro invocation, and
// `-D unused-doc-comments` is right to say so.)
#[cfg(test)]
thread_local! {
    pub(crate) static WALK_SYSCALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
#[inline]
fn tick() {
    WALK_SYSCALLS.with(|c| c.set(c.get() + 1));
}

#[cfg(not(test))]
#[inline]
fn tick() {}

// **A test-only seam between the descent and the leaf (CPE-1937 round 2), and why it had to exist.**
//
// PR #1059's Security Auditor ran the entire 2,406-test non-ignored suite with `unlinkat` swapped for a
// by-path `fs::remove_file` — the descent left completely intact — and got **2,406 passed, 0 failed**,
// while the `#[ignore]`d race harness caught 141 destroyed bystanders in 200 trials. So the leaf
// primitive, which is containment and not tidiness, had **zero CI coverage**: its only red-proof was a
// harness nobody runs by default, and the next ticket reuses this module for `copilot::apply_op`.
//
// A race harness cannot fix that — it is `#[ignore]`d precisely because it is slow and statistical. What
// closes it is making the window *deterministic*: fire once, exactly where the race lands, and let the
// test do the swap the racer was trying to hit by luck. `cpe_1937_the_leaf_and_not_only_the_descent…`
// then passes only if the leaf resolves against the handle rather than the path.
//
// **NOTE FOR THE NEXT AUTHOR: a new descent-then-leaf primitive in this module must call
// `between_descent_and_leaf` too.** Nothing structural enforces that — `renameat`, which
// `copilot::apply_op` is waiting on and which is this module's next consumer, would otherwise ship with
// exactly the CI gap this seam was added to close. There is no existing pattern to copy: `WALK_SYSCALLS`'s
// only consumer prints an unasserted number, so it has no such guard either.
//
// Compiled out entirely in a shipped binary — `#[cfg(test)]`, the same discipline as `WALK_SYSCALLS`
// above, so the seam cannot become a production hook. **Proven on the artifact rather than on the
// attribute** (PR #1059 round 2 audit): the linked, non-test `ticket-mcp` binary that depends on this
// crate contains **0 strings and 0 symbols** for the seam — `WALK_SYSCALLS` likewise 0 — against a
// test-binary control of 3/3, and zero occurrences across every codegen `.o` unit. The single surviving
// occurrence in the rlib is inside `lib.rmeta`, which the linker discards.
//
// The hook is **taken** rather than borrowed across the call, so it fires exactly once and a hook that
// re-enters this module cannot deadlock the `RefCell` (also verified by that audit).
#[cfg(test)]
thread_local! {
    pub(crate) static BETWEEN_DESCENT_AND_LEAF: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Disarms the seam when [`remove_file_beneath`] returns, **however it returns** (CPE-1937 round 2,
/// finding F-R2-1).
///
/// Taking the hook *at* the seam is not enough on its own, because a call can end **before** the seam:
/// arm the hook, run a delete whose **descent** refuses, and nothing consumes it — then the next,
/// unrelated delete fires someone else's hook. Measured identically on both platforms:
///
/// ```text
/// still_armed_after_refused_descent = true
/// fired_on_next_unrelated_delete    = 1
/// ```
///
/// Latent rather than live in the shipped tests (the one that arms it always reaches the seam, and
/// clears defensively in its skip path), but the cell is `pub(crate)`, any module's tests may arm it,
/// and `--test-threads=1` puts every test on one thread — so a leak crosses tests, and a cross-test
/// leak in a *containment* fixture is what makes a later green mean nothing.
///
/// **A guard rather than a clear-on-entry**, which was the other option offered: clearing at entry
/// would discard the hook the caller had just armed for that very call. This is a zero-sized value
/// bound at the top of [`remove_file_beneath`], so every early `Err` — a non-plain relative path, the
/// root-itself refusal, a refused descent — disarms it on the way out.
#[cfg(test)]
struct SeamGuard;

#[cfg(test)]
impl Drop for SeamGuard {
    fn drop(&mut self) {
        BETWEEN_DESCENT_AND_LEAF.with(|h| {
            let _ = h.borrow_mut().take();
        });
    }
}

#[cfg(test)]
fn between_descent_and_leaf() {
    let hook = BETWEEN_DESCENT_AND_LEAF.with(|h| h.borrow_mut().take());
    if let Some(f) = hook {
        f();
    }
}

#[cfg(not(test))]
#[inline]
fn between_descent_and_leaf() {}

/// Resolve and hold the root. `real_root` must already be canonical — this does not resolve it, and
/// deliberately **does** follow a link at the root itself, because the root is the location the user
/// chose and a link there is their own arrangement, not a redirect planted underneath them.
///
/// **It opens the root for READ, which is a new requirement on the destination.** A directory handle
/// is what every subsequent component is resolved against, and both `openat` and `NtCreateFile` need
/// one; there is no write-only equivalent. So a **write-only destination directory**, which the
/// pre-CPE-1896 engine could copy into perfectly well (it only ever created and wrote files by path),
/// now fails — and fails the *whole plan*, since `apply_backup_plan_walk` treats an unopenable root as
/// a refusal to run. Rare (an ordinary backup destination is readable by the user who chose it, and
/// mirror mode already lists it) and loud rather than silent, so it is recorded here rather than
/// worked around. If it ever turns up in the field, the fix is `FILE_TRAVERSE`-only on Windows and
/// `O_PATH` on Linux, neither of which macOS has an equivalent for.
pub(crate) fn open_root(real_root: &Path, noun: &'static str) -> std::io::Result<RootDir> {
    Ok(RootDir { path: real_root.to_path_buf(), dir: sys::open_root_dir(real_root)?, noun })
}

/// Open `root/rel` for writing — creating the file, and any missing directories along the way — without
/// ever following a link at any component, and without ever resolving `rel` as a path.
///
/// `rel` must be a plain relative path (only [`Component::Normal`] parts and at least one of them);
/// anything else is refused here rather than sanitised, because a caller handing this a `..` has a bug
/// that a silent fix would hide. `backup::safe_join` has already established that for the backup
/// engine — this re-establishes it, since the guarantee costs nothing and the module is the one place
/// the property has to hold.
///
/// # What it does NOT filter, and why a second caller must add it (CPE-1896 PR #1043, N2)
///
/// **Windows name normalisation.** Win32 strips trailing dots and spaces from a path's final
/// component and maps the DOS device names; the **NT** layer this module opens through does neither.
/// So `sub./f.txt`, `sp /g.txt`, `NUL`, `con/x.txt` and `a/b/c.txt:stream` all create real objects
/// here, correctly **contained** beneath the root (the Security Auditor fired 30 such shapes straight
/// into this function and got zero bytes outside it — `CON`/`NUL`/`COM1` become ordinary files, which
/// is strictly safer than the Win32 path, where they would have gone to a device). But several of them
/// are then **unaddressable by any Win32 caller**, including this app: the user cannot open or delete
/// what was written.
///
/// Today the only production caller, `backup::copy_one_verified`, is protected because
/// `backup::safe_join` refuses [`crate::fsutil::win32_name_is_unstable`] components *before* calling
/// here. That is a filter in the caller, not in this module, and this PR itself recommends wiring
/// `open_beneath` into the four other resolve-then-write legs. **A caller without that filter would
/// get a handle on one object while `backup::landed_inside`'s path-based half inspected a different
/// one** — the two would silently disagree. The rule is deliberately left in `safe_join` rather than
/// duplicated (one owner for the vocabulary), so the obligation is recorded here instead: a new caller
/// applies `win32_name_is_unstable` itself, or a future change moves the filter down into this
/// function for everyone.
///
/// # Errors
///
/// A refusal names the component it stopped at, relative to the root, and says whether the component
/// was a link (the attack shape) or simply could not be opened (a permission, sharing or vanished-name
/// problem). Both are refusals: this module never guesses. The two are distinguished by asking the
/// filesystem, never by the errno — see `sys::link_at` on Unix and `sys::name_surrogate_at` on Windows.
pub(crate) fn create_beneath(root: &RootDir, rel: &Path) -> Result<Opened, Refusal> {
    let parts = plain_components(root, rel)?;
    let Some((last, dirs)) = parts.split_last() else {
        return Err(Refusal {
            why: format!(
                "refusing to open {rel:?} inside {:?}: it names the destination root itself, not a \
                 file inside it",
                root.path
            ),
            policy: true,
        });
    };
    sys::walk(root, dirs, last)
}

/// Create — or open, if it is already there — a **directory** beneath `root` at `rel`, by the same
/// per-component handle-relative walk [`create_beneath`] uses for a file's missing parents (CPE-1913).
///
/// A file write does not need this: `create_beneath` materialises the whole parent chain on its way to
/// the leaf. It exists for the callers that have to materialise a directory **for its own sake** — an
/// archive's directory records and a remote tree's empty folders — which otherwise reach for
/// `fs::create_dir_all`, a by-path call that walks a junction like any other directory and is exactly
/// the escape route [`create_beneath`] removed from the file leg. `archive::extract_zip_archive_stream`
/// and `transfer::download_tree` both had one.
///
/// Nothing is returned: the handle is closed immediately, because the directory's *existence* is the
/// whole product. Every containment guarantee and every refusal wording is [`create_beneath`]'s, shared
/// rather than re-derived — the walk is one function.
pub(crate) fn create_dir_beneath(root: &RootDir, rel: &Path) -> Result<(), Refusal> {
    let parts = plain_components(root, rel)?;
    if parts.is_empty() {
        // The root itself, which the caller has already opened. Creating it is a no-op rather than an
        // error: an archive whose directory records include the root's own name (`./`) is ordinary,
        // and `create_beneath`'s equivalent case is an error only because a *file* cannot be the root.
        return Ok(());
    }
    sys::walk_dirs(root, &parts)
}

/// Delete the file `root/rel` names — **without ever following a link at any component, and without
/// ever resolving `rel` as a path** (CPE-1937).
///
/// # Why this had to be a new primitive rather than another caller of [`create_beneath`]
///
/// Every leg CPE-1896 and CPE-1913 wired into this module **writes**, and a write's whole product is a
/// handle: open the leaf beneath the root and the bytes can only land inside it. A delete has no
/// handle to hand back — its product is the *removal of a name* — so `std` has nothing to offer:
/// [`std::fs::remove_file`] takes a path and re-resolves every component of it, which is exactly the
/// by-path question this module exists to stop asking. CPE-1913's own enumeration listed
/// `copilot::apply_op` as deferred for the same reason: `renameat`/`unlinkat` did not exist here.
/// `unlinkat` does now, and it is the piece `apply_op` was waiting on.
///
/// # It OPENS the parent chain, it does not create it
///
/// [`create_beneath`]'s descent uses `FILE_OPEN_IF` / `mkdirat`, because a write legitimately
/// materialises its own parents. A delete must never do that: `remove_file` on a missing chain is an
/// error, and a delete that *creates* two directories on its way to failing would leave debris behind
/// a destructive operation. So the descent is parameterised on [`Act`] — one walk, two dispositions —
/// rather than copied, so the file leg and the delete leg cannot drift about what a traversable
/// component is.
///
/// # The leaf is unlinked by NAME relative to the parent HANDLE, and never followed
///
/// - **Unix**: `unlinkat(parent_fd, name, 0)`. `unlinkat` never follows a symlink at the final
///   component — it removes the name itself — which is both the never-follow discipline this module
///   holds everywhere else and exactly what `fs::remove_file` already did at that one component.
/// - **Windows**: `NtCreateFile` relative to the parent handle with `DELETE` access,
///   `FILE_OPEN_REPARSE_POINT` (so a reparse point is opened as itself and never traversed) and
///   `FILE_NON_DIRECTORY_FILE` (so a directory — junction included — is refused, as `remove_file`
///   refuses one), then `SetFileInformationByHandle(FileDispositionInfo)` on that handle. The name is
///   never presented to the filesystem again after the parent chain is opened.
///
/// # THE LEAF IS LOAD-BEARING, and it is reached on every successful delete
///
/// It is tempting to reason that the descent's `O_NOFOLLOW` / `FILE_OPEN_REPARSE_POINT` already refuses
/// anything hostile, so the leaf primitive is a formality. **That is wrong, it was written into an
/// earlier revision of this doc, and PR #1059's Reviewer disproved it by measurement.** The descent
/// only refuses when a component *is currently* a link; on every delete that is going to succeed the
/// descent hands back a handle and the leaf runs. A by-path leaf re-resolves the whole path from the
/// root at that moment, and a concurrent rename redirects it — which is the entire defect this module
/// exists for, reintroduced one line below the guard that was supposed to have removed it.
///
/// Swapping `unlinkat(parent, name, 0)` for `fs::remove_file(root.join(rel))` — leaving the descent
/// completely intact — leaves the whole static suite **green** and produces this, on real Linux:
///
/// ```text
/// unix leaf                                    trials  FILES_DELETED_OUTSIDE  swaps
/// unlinkat (this module)                         200            0            7373/7373
/// fs::remove_file, after the same descent        200           89            7742/7742
/// ```
///
/// Comparable denominators, so the 89 is signal. `cargo test -p cpe-server --lib --release --
/// --ignored cpe_1937_raced_delete` is what says so; a green static run says nothing about it. The
/// descent decides *whether* the delete may proceed; the leaf decides *what it lands on*, and both are
/// required.
///
/// # Errors
///
/// Same [`Refusal`] contract as [`create_beneath`], including `policy`: a link at a component is a
/// **verdict** (`policy: true`), while a vanished name, a permission problem or a sharing violation is
/// an I/O answer (`policy: false`).
///
/// **`revert_engine::apply_delete` currently discards `policy`**, and deliberately: the delete loop has
/// no channel to report retryability to a user, so translating it into a `Refused::permanent` nothing
/// reads would be a guard nothing can red-proof (CPE-1929). The flag is kept here — it is free, it is
/// correct, and it is what a future `DeleteRefusalGroup` would read. See `apply_delete`'s doc for what
/// wiring it would actually cost.
pub(crate) fn remove_file_beneath(root: &RootDir, rel: &Path) -> Result<(), Refusal> {
    // Disarm the seam on EVERY exit path, including the early refusals below that never reach
    // it. See `SeamGuard` for the cross-test leak this closes.
    #[cfg(test)]
    let _seam = SeamGuard;
    let parts = plain_components(root, rel)?;
    let Some((last, dirs)) = parts.split_last() else {
        return Err(Refusal {
            why: format!(
                "refusing to delete {rel:?} inside {:?}: it names the destination root itself, not a \
                 file inside it",
                root.path
            ),
            policy: true,
        });
    };
    sys::unlink(root, dirs, last)
}

/// Create a **staging** file at `root/rel` — exclusively, never opening anything that was already
/// there — and hand back a handle that can later be committed over its final name with
/// [`rename_beneath`] (CPE-1961).
///
/// # Why this is not [`create_beneath`] with a flag
///
/// [`create_beneath`] tries `FILE_CREATE`/`O_EXCL` first and then **falls back to opening** whatever is
/// at the name, because its whole job is "give me the destination, whether or not it exists". A staging
/// name has the opposite contract: if anything is already sitting at it, the only correct answer is to
/// refuse. `created` is therefore always `true` on success and the type is only kept for symmetry with
/// its sibling.
///
/// Two other differences, both load-bearing rather than cosmetic:
///
/// - **Windows always asks for `DELETE`, and for `READ_CONTROL | WRITE_DAC` only when `carrying`.**
///   `DELETE` is not optional: `NtSetInformationFile(FileRenameInformation)` — the handle-sourced
///   rename [`rename_beneath`] commits with — requires it on the *source* handle, and without it the
///   commit fails `ERROR_ACCESS_DENIED` on every file. The other two are what
///   `fsutil::HandleCarryover::apply` needs to put the destination's own DACL onto the staged file
///   before the commit.
///
///   **`carrying` exists because "asking is free" was wrong, and CPE-1961 round 2's Security Auditor
///   is why** (SEC-7). Round 1 asked for all three unconditionally and defended it as free, because
///   Windows grants an object's creator `READ_CONTROL` and `WRITE_DAC` implicitly. But
///   `HandleCarryover::apply` runs **only when `created == false`** — i.e. only when the destination
///   already existed — and the common case for every one of the five legs (a first backup, a fresh
///   extraction, a download into an empty tree) is `created == true`, where nothing ever touches the
///   DACL. That is precisely the shape `create_beneath`'s own comment refuses, in the sentence this
///   doc used to quote approvingly: *an access right nothing goes on to use is one more thing a
///   network redirector can refuse.* Local NTFS grants it; an SMB or WebDAV redirector is the one that
///   might not, and that is exactly where a backup destination lives. **Unmeasured against a real
///   SMB/WebDAV/NFS share** — stated so, rather than claimed as verified.
///
///   `create_beneath` asks for none of the three, which is the same split
///   `fsutil::create_staging_file_for_carryover` records against `fsutil::create_staging_file`; the
///   `ByPath` arm of `fsutil::claim_destination_handle` now chooses between those two on the same
///   `carrying` flag this parameter carries.
///
///   On Unix `carrying` is ignored — `openat` has no access-right knob to narrow.
/// - **Unix creates at `0600`**, not `0666 & ~umask`, for `fsutil::STAGING_MODE`'s reason:
///   POSIX checks permission at `open`, so a file created wide and narrowed afterwards leaves a window
///   in which another local process can take a descriptor it keeps. The eventual mode is applied to the
///   handle before the commit, so the staged file is never for an instant wider than it ends up.
///
/// # Errors
///
/// The same [`Refusal`] contract as [`create_beneath`]. A name that is already occupied refuses with
/// `policy: false` — it is an I/O answer about this attempt, not a verdict about what the user asked
/// for.
///
/// **There is no internal retry, and round 1's doc said there was.** It read "the caller's next
/// attempt gets a fresh pid+nanosecond stamp", which describes a loop this function does not contain
/// and no caller writes: `fsutil::staging_sibling_name` is called **once** per claim, and an occupied
/// staging name refuses the entry outright. That is the right behaviour — the name carries this
/// process's pid and a nanosecond stamp, so something already standing at it is a signal, not a
/// collision to paper over — but it is not what the sentence said. Corrected in round 2 (Reviewer,
/// ASK 4).
pub(crate) fn create_staging_beneath(
    root: &RootDir,
    rel: &Path,
    carrying: bool,
) -> Result<Opened, Refusal> {
    let parts = plain_components(root, rel)?;
    let Some((last, dirs)) = parts.split_last() else {
        return Err(Refusal {
            why: format!(
                "refusing to open {rel:?} inside {:?}: it names the destination root itself, not a \
                 file inside it",
                root.path
            ),
            policy: true,
        });
    };
    sys::walk_staged(root, dirs, last, carrying)
}

/// **The handle-relative rename this module was missing (CPE-1961).**
///
/// Commit `staged` — the open handle [`create_staging_beneath`] returned for `from_rel` — over
/// `to_rel`, replacing whatever name is there. Both operands are resolved one component at a time
/// against the held root handle, so no interior component is ever presented to the filesystem as a
/// path a concurrent rename could redirect.
///
/// # This is the primitive three tickets were waiting on
///
/// `std` has `fs::rename`, which takes two **paths** and re-resolves every component of both. That is
/// the by-path question this module exists to stop asking, and its absence is what kept three separate
/// pieces of work parked:
///
/// - **CPE-1961** — `fsutil::claim_destination_handle` could not take CPE-1958's claim-then-rename fix,
///   because its handle comes from [`create_beneath`] and staging beside it needs exactly this.
/// - **CPE-1963** — `fsutil::stage_and_replace_at`'s commit names its *source* by path
///   (`*.cpe-tmp`, enumerable, in an attacker-writable folder), so the commit itself can be aliased.
/// - **`copilot::apply_op`** — deferred through CPE-1913 and CPE-1937 with `renameat` named as the
///   missing half; `remove_file_beneath` landed, this did not.
///
/// # What each platform actually guarantees, because they are NOT the same
///
/// - **Windows: the source is the HANDLE.** `NtSetInformationFile(staged, FileRenameInformation, …)`
///   with `RootDirectory` set to the destination's parent handle renames *the object this handle is
///   open on* to a single component resolved inside that directory. Neither operand is a path. Nothing
///   an attacker does to the staging **name** between the write and the commit can change which object
///   is committed — which is the whole of CPE-1963 on this platform.
///
///   **`NtSetInformationFile`, not `SetFileInformationByHandle`**, and this doc named the wrong one
///   until CPE-1961 round 2. The Win32 wrapper takes the same `FILE_RENAME_INFO` buffer and **refuses
///   it with `ERROR_INVALID_PARAMETER (0x80070057)` the moment `RootDirectory` is non-null** — the
///   entire `transfer` and `archive` suites reddened on it before the call was moved down a layer, and
///   `sys::rename` records the measurement. It matters that the *public* doc says so, because the
///   reader most likely to "simplify" the implementation back to the Win32 call is precisely the one
///   reading this page rather than the `#[cfg(windows)]` body.
/// - **Unix: the source is a NAME, resolved against the parent directory handle.** There is no
///   fd-sourced rename in POSIX, in Linux, or in any BSD: `renameat2` has no `AT_EMPTY_PATH` form,
///   `/proc/self/fd/N` is not renameable, and `linkat(…, AT_EMPTY_PATH)` needs
///   `CAP_DAC_READ_SEARCH`. So `renameat(parent, from, parent, to)` is the strongest primitive that
///   exists, and it is strictly stronger than `fs::rename` — only the two **leaf** names are resolved,
///   and they are resolved inside a directory object that cannot be substituted. **The residual is
///   CPE-1963's and it is not closed here on Unix**: an attacker who unlinks the staging name and
///   hard-links an outside file into its place makes this commit that object's name. It is an
///   *aliasing* race, never a destruction one — the outside file's bytes are not changed — and
///   `fsutil::ClaimedDestination::commit` turns it into a loud refusal by comparing the identity at
///   the destination against the identity it wrote. Say "unblocks CPE-1963" only with that split
///   stated; a claim that this closes it on both platforms would be false.
///
/// # Precondition: one parent
///
/// `from_rel` and `to_rel` must name siblings — same parent, different final component. That is what
/// every staging commit wants, and it means the descent runs **once**, so the two operands cannot be
/// resolved against two different directory objects. A cross-directory rename is refused rather than
/// supported: nothing here needs it, and it would double the number of things a reader has to hold in
/// their head about which handle each name is relative to.
///
/// # Cost
///
/// One extra descent per commit — the walk that opened the staging file is not held open across the
/// caller's write, because the caller may be streaming gigabytes and a held directory handle is a
/// resource with a lifetime. On a backup that is one additional per-component `openat`/`NtCreateFile`
/// chain per file, on top of the one [`create_beneath`] already pays. Measured cost is recorded on
/// `fsutil::ClaimedDestination::commit`, which is the only production caller.
pub(crate) fn rename_beneath(
    root: &RootDir,
    staged: &File,
    from_rel: &Path,
    to_rel: &Path,
) -> Result<(), Refusal> {
    let from = plain_components(root, from_rel)?;
    let to = plain_components(root, to_rel)?;
    let (Some((from_last, from_dirs)), Some((to_last, to_dirs))) =
        (from.split_last(), to.split_last())
    else {
        return Err(Refusal {
            why: format!(
                "refusing to commit {from_rel:?} onto {to_rel:?} inside {:?}: one of them names the \
                 destination root itself, not a file inside it",
                root.path
            ),
            policy: true,
        });
    };
    if from_dirs != to_dirs {
        return Err(Refusal {
            why: format!(
                "refusing to commit {from_rel:?} onto {to_rel:?} inside {:?}: a staged file is \
                 committed over a name in its OWN folder, so that both names resolve against one \
                 directory handle",
                root.path
            ),
            policy: true,
        });
    }
    sys::rename(root, to_dirs, from_last, to_last, staged)
}

/// What the walk is on its way to do. Exactly two things differ between [`create_beneath`]'s descent
/// and [`remove_file_beneath`]'s — whether a missing directory is **created** on the way down, and the
/// **verb** in a refusal — and both are carried here rather than by a second copy of the walk. A user
/// deleting files must not be told that "nothing was written", and a delete must not create anything.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Act {
    Write,
    Delete,
    /// **A write that must not CREATE anything on its way down** — [`rename_beneath`]'s descent, and
    /// [`Staged::abandon`]'s cleanup reaching this module through [`remove_file_beneath`] is the same
    /// idea from the other side (CPE-1961 round 2, Security Auditor SEC-5, minor).
    ///
    /// The descent's disposition is `FILE_OPEN_IF` / `mkdirat`-if-missing for [`Act::Write`], because
    /// creating a destination's missing parents is exactly what `create_beneath` is for. A **commit**
    /// arrives after the staging file already exists inside those parents, so every directory it walks
    /// through is one that was there a moment ago. If the parent has been renamed away in between,
    /// `Act::Write` would silently **re-create an empty directory** and then fail `ENOENT` at the
    /// rename itself — fails closed, but leaves debris, which is the precise thing CPE-1937 gave the
    /// delete leg `FILE_OPEN` to avoid. This variant gets the delete leg's disposition and the write
    /// leg's wording, because the user asked for a write and the message must say so.
    Commit,
}

impl Act {
    /// The verb in the refusal's first clause and in its closing sentence.
    fn verb(self) -> &'static str {
        match self {
            Act::Write | Act::Commit => "write",
            Act::Delete => "delete",
        }
    }

    /// The participle in "Nothing was … for this entry".
    fn past(self) -> &'static str {
        match self {
            Act::Write | Act::Commit => "written",
            Act::Delete => "deleted",
        }
    }

    /// Whether this act's descent may **create** a missing intermediate directory. Only a plain
    /// [`Act::Write`] may — see [`Act::Commit`] for why a commit must not, and `descend`'s own comment
    /// for why a delete must not.
    fn descent_creates(self) -> bool {
        matches!(self, Act::Write)
    }
}

/// The one place `rel` is turned into components, so [`create_beneath`], [`create_dir_beneath`] and
/// [`remove_file_beneath`] cannot come to disagree about what a legal relative path is.
///
/// `rel` must be a plain relative path; anything with a root, a prefix or a `..` is refused here rather
/// than sanitised, because a caller handing this a `..` has a bug that a silent fix would hide.
///
/// **[`Component::CurDir`] is the one exception, and it is skipped rather than refused** (CPE-1913). A
/// lone `.` means "here", carries none of `..`'s hazard, and both new callers can produce one from
/// input they have already validated: `archive::entry_name_is_safe` explicitly passes a `.` segment
/// through untouched, and `Path::components` preserves a leading `./`. Refusing it would turn a
/// perfectly ordinary archive entry into a skip for no security gain.
fn plain_components<'a>(root: &RootDir, rel: &'a Path) -> Result<Vec<&'a OsStr>, Refusal> {
    let mut parts: Vec<&OsStr> = Vec::new();
    for c in rel.components() {
        match c {
            Component::Normal(p) => parts.push(p),
            Component::CurDir => {}
            // `policy: true` — refusing a name that is not a plain relative path is a verdict, not an
            // I/O failure: nothing about the filesystem went wrong and not writing is correct.
            _ => {
                return Err(Refusal {
                    why: format!(
                        "refusing to open {rel:?} inside {:?}: it is not a plain relative path, so it \
                         cannot be resolved one component at a time",
                        root.path
                    ),
                    policy: true,
                })
            }
        }
    }
    Ok(parts)
}

/// Why an entry was refused, and **which kind of answer that is** (CPE-1913).
///
/// CPE-1896 needed only the sentence, because the backup engine's per-entry vocabulary has exactly one
/// failure bucket. The legs CPE-1913 wires this into have two, and they are not interchangeable:
///
/// - `archive` distinguishes `EntrySlotAction::Skip` (a policy verdict about one entry — the rest of
///   the archive still extracts) from `EntrySlotAction::Abort` (an I/O answer nobody can act on, which
///   takes the run down rather than silently dropping a file the user asked for).
/// - `transfer` distinguishes [`crate::transfer::DownloadReport::skipped`] (same meaning) from
///   `undelivered`, which makes the whole `download_tree` call return `Err`.
///
/// A refusal string cannot be asked which it is, and **making the caller pattern-match on wording is
/// exactly how this repo has previously shipped guards that proved nothing** (CPE-1896 round 4: two
/// assertions matched a phrase that appeared in every refusal's shared boilerplate, so they passed for
/// any failure at all). So the answer is carried as data next to the sentence rather than encoded in
/// it.
#[derive(Debug)]
pub(crate) struct Refusal {
    /// The sentence a user sees.
    pub(crate) why: String,
    /// `true` when **not writing is the correct outcome** — a link stood at a component, the name is a
    /// hard link, a directory is in the way. `false` when the entry could not be written for a reason
    /// that is nobody's policy: a permission error, a sharing violation, a name this filesystem will
    /// not accept. The second kind means the user asked for a file and did not get it.
    pub(crate) policy: bool,
}

impl Refusal {
    /// An I/O answer: the entry was not delivered, and nothing chose that.
    pub(crate) fn failure(why: String) -> Self {
        Self { why, policy: false }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

impl From<Refusal> for String {
    fn from(r: Refusal) -> String {
        r.why
    }
}

/// The refusal wording, shared by every arm so the sentence a user sees does not depend on which
/// platform refused. `at` is the failing component's path **relative to the root**, which is the part
/// the user can act on.
fn refuse(root: &RootDir, act: Act, at: &Path, why: &str) -> Refusal {
    // **The tail must not contain the words any test uses to identify a CAUSE.** It used to end
    // "…a component that is a link, or that cannot be opened, stops the entry", which put the literal
    // phrase `is a link` into *every* refusal this module produces — so
    // `assert!(err.contains("is a link"))` passed for a permission error, a vanished name, or a plain
    // file sitting where a directory should be. Two tests were asserting exactly that and proving
    // nothing; the Linux harness for PR #1043 round 2 caught it by asserting the *negative* case.
    // Keep boilerplate and diagnosis lexically disjoint.
    //
    // **The verb comes from [`Act`], and that is not decoration** (CPE-1937). This module's first two
    // callers only ever wrote, so the sentence hard-coded "write"; the delete leg reaches the identical
    // walk, and telling a user that "nothing was written" when what was refused was a *deletion* is a
    // message about the wrong operation. The refusal wording for every writing leg is byte-for-byte
    // unchanged — the tests in `revert_engine`, `archive` and `transfer` that match it still match it.
    Refusal::failure(format!(
        "refusing to {verb} inside the {noun} {path:?}: the path component {at:?} {why}. \
         Nothing was {past} for this entry — each component is opened relative to the one before it \
         so that nothing can be swapped in underneath the {verb}, and any component that cannot be \
         opened, or that stands in for another name, stops the entry rather than being resolved.",
        verb = act.verb(),
        past = act.past(),
        noun = root.noun,
        path = root.path,
    ))
}

/// The one case that is an attack rather than an accident, worded separately so it reads as the
/// specific finding it is — and flagged `policy: true`, because refusing a link is a **verdict**, not
/// an I/O failure. That distinction is what lets `archive` count it as a skip and `transfer` keep the
/// rest of the tree, rather than every caller having to recognise this sentence (CPE-1913).
fn refuse_link(root: &RootDir, act: Act, at: &Path) -> Refusal {
    Refusal {
        why: refuse(
            root,
            act,
            at,
            &format!(
                "is a link (a symlink, junction or other reparse point), and a link inside the {} \
                 redirects the {} to wherever it points",
                root.noun,
                act.verb(),
            ),
        )
        .why,
        policy: true,
    }
}

// ---------------------------------------------------------------------------------------------
// Windows: NtCreateFile with RootDirectory = the parent handle.
// ---------------------------------------------------------------------------------------------
#[cfg(windows)]
mod sys {
    use super::{refuse, refuse_link, tick, Act, Opened, Refusal, RootDir};
    use std::ffi::OsStr;
    use std::fs::File;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
    use std::path::{Path, PathBuf};

    use windows::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows::Wdk::Storage::FileSystem::{
        NtCreateFile, FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
        FILE_OPEN_IF, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
        NTCREATEFILE_CREATE_DISPOSITION, NTCREATEFILE_CREATE_OPTIONS,
    };
    use windows::Win32::Foundation::{
        RtlNtStatusToDosError, BOOLEAN, HANDLE, NTSTATUS, UNICODE_STRING,
    };
    use windows::Win32::Storage::FileSystem::{
        GetFileInformationByHandleEx, SetFileInformationByHandle, FileBasicInfo, FileDispositionInfo,
        FileDispositionInfoEx, DELETE, FILE_ACCESS_RIGHTS, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_READONLY, FILE_BASIC_INFO, FILE_DISPOSITION_FLAG_DELETE,
        FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_INFO_EX_FLAGS,
        FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
        FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, FILE_WRITE_ATTRIBUTES,
        READ_CONTROL, SYNCHRONIZE, WRITE_DAC,
    };
    use windows::Win32::System::IO::IO_STATUS_BLOCK;

    /// `FILE_FLAG_BACKUP_SEMANTICS` — required to get a *directory* handle out of `CreateFileW`, which
    /// is what `OpenOptions` calls underneath. Hard-coded per this module's siblings
    /// (`batch_media::FILE_FLAG_OPEN_REPARSE_POINT_U32`) rather than pulling another feature in.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    /// `OBJ_CASE_INSENSITIVE`. Windows paths are case-insensitive at the Win32 layer but the **NT**
    /// layer is case-sensitive by default, so omitting this would make `Sub\file` miss a directory
    /// actually named `sub` — an entry that copies fine today would start failing. Hard-coded for the
    /// same reason as the flag above; it lives in `Win32_System_Kernel`, a feature nothing else here
    /// needs.
    const OBJ_CASE_INSENSITIVE: u32 = 0x40;

    /// `STATUS_NAME_TOO_LONG`. Returned for the two unreachable-but-not-ignorable length conversions in
    /// [`nt_child`], so they refuse rather than silently substituting a wrong value.
    const STATUS_NAME_TOO_LONG: NTSTATUS = NTSTATUS(0xC000_0106_u32 as i32);

    /// Every open here shares its object the way `CreateFileW` callers in this crate do, so holding a
    /// directory handle open for a run does not stop anyone else reading, writing or deleting names in
    /// it. Without `FILE_SHARE_DELETE` in particular, a held root handle would block the user from
    /// deleting their own backup folder until the run ended.
    const SHARE_ALL: FILE_SHARE_MODE =
        FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0);

    pub(super) fn open_root_dir(real_root: &Path) -> std::io::Result<File> {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(SHARE_ALL.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(real_root)
    }

    /// One handle-relative `NtCreateFile`. `name` is a **single** component: NT would happily parse a
    /// backslash-bearing relative name and resolve its interior components itself, following any
    /// reparse point it met, which is the whole thing this module exists to prevent.
    fn nt_child(
        parent: &File,
        name: &OsStr,
        access: FILE_ACCESS_RIGHTS,
        disposition: NTCREATEFILE_CREATE_DISPOSITION,
        options: NTCREATEFILE_CREATE_OPTIONS,
    ) -> Result<File, NTSTATUS> {
        let mut wide: Vec<u16> = name.encode_wide().collect();
        // A `UNICODE_STRING` counts BYTES in a `u16`, so a component of more than 32,767 UTF-16 units
        // cannot be described. No filesystem accepts one (NTFS caps a component at 255), so this is
        // unreachable — which is exactly why it must not be papered over. An earlier revision used
        // `unwrap_or(u16::MAX)`, which would have handed NT a *truncated* length and opened some other
        // name entirely. Refusing turns an impossible input into an impossible-to-misread refusal.
        let bytes = u16::try_from(wide.len().saturating_mul(2))
            .map_err(|_| STATUS_NAME_TOO_LONG)?;
        let mut us = UNICODE_STRING {
            Length: bytes,
            MaximumLength: bytes,
            Buffer: windows::core::PWSTR(wide.as_mut_ptr()),
        };
        let oa = OBJECT_ATTRIBUTES {
            // Same reasoning: a wrong `Length` here makes NT reject or misread the structure, and
            // `unwrap_or(0)` would have guaranteed the wrong value rather than reported it.
            Length: u32::try_from(std::mem::size_of::<OBJECT_ATTRIBUTES>())
                .map_err(|_| STATUS_NAME_TOO_LONG)?,
            RootDirectory: HANDLE(parent.as_raw_handle() as isize),
            ObjectName: &mut us,
            Attributes: OBJ_CASE_INSENSITIVE,
            SecurityDescriptor: std::ptr::null(),
            SecurityQualityOfService: std::ptr::null(),
        };
        tick();
        // SAFETY: zeroing `IO_STATUS_BLOCK` is valid — it is a plain `repr(C)` out-parameter of
        // integers and a union of two pointer-sized values, with no niche and no invariant to uphold.
        let mut iosb: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
        let mut h = HANDLE::default();
        // SAFETY: `oa` borrows `us`, which borrows `wide`; all three outlive the call. `parent` is a
        // live `File`, so its handle is valid and is only *borrowed* by `RootDirectory` — NT does not
        // take ownership of it. `h` and `iosb` are correctly-sized out-parameters.
        let status = unsafe {
            NtCreateFile(
                &mut h,
                access,
                &oa,
                &mut iosb,
                None,
                FILE_ATTRIBUTE_NORMAL,
                SHARE_ALL,
                disposition,
                options,
                None,
                0,
            )
        };
        if !status.is_ok() {
            return Err(status);
        }
        // SAFETY: NT reported success, so `h` is a fresh handle this call owns. It is wrapped exactly
        // once, and the resulting `File` is what closes it — no other path touches `h` after this.
        Ok(unsafe { File::from_raw_handle(h.0 as RawHandle) })
    }

    /// Is this directory handle a name surrogate — a link, junction or mount point — as opposed to
    /// merely carrying *some* reparse tag?
    ///
    /// The bit and the query live in [`crate::batch_media::reparse_name_surrogate`], which
    /// `fsutil::copy_file_onto_destination_handle`'s final-component guard also asks (CPE-1896 round 3,
    /// F5): two guards, one owner, so the rule cannot drift between them. This wrapper exists to hold
    /// the one thing that legitimately differs — the default when the description cannot be read —
    /// and the reason for it.
    ///
    /// **The distinction is a shipped-behaviour bug, measured on both sides** (PR #1043 Security
    /// Auditor). The first cut of this walk refused on `FILE_ATTRIBUTE_REPARSE_POINT` alone. On
    /// `origin/main` a destination holding `dst/real/` and a junction `dst/link -> dst/real` copies
    /// `link/x.txt` with `ok = true`; on that first cut the same entry came back `ok = false`. That is
    /// correct for the junction — it *is* a surrogate, and refusing it is this ticket's whole point,
    /// even pointing back inside the root — but the same attribute bit is set by **OneDrive
    /// Files-On-Demand**, and a backup destination inside a OneDrive folder with Known Folder Move is
    /// an ordinary user setup. Refusing every entry beneath a placeholder directory would mean
    /// "backups to OneDrive stop working", reported as a handful of red rows in a 100,000-entry run
    /// that nobody reads.
    ///
    /// So the question asked is the surrogate bit off `FILE_ATTRIBUTE_TAG_INFO`, which is precisely
    /// the bit that separates "this name stands for another name" from "this object is itself, with a
    /// filter attached". Same one handle query as before, a different information class.
    ///
    /// **Fails open, deliberately, and it is safe to.** A handle whose tag cannot be read returns
    /// `false` and the walk continues — because the containment does not rest here: if the object
    /// really is a surrogate, the *next* component's handle-relative open fails with
    /// `ERROR_CANT_RESOLVE_FILENAME` and the entry is refused anyway (measured by neutering this
    /// check entirely and re-running the CPE-1889 junction harm test — still refused). What this
    /// buys is the sentence that names the link, one component earlier.
    ///
    /// # It is NECESSARY, NOT SUFFICIENT — and that is measured, not asserted
    ///
    /// An earlier revision of this comment said a non-surrogate directory reparse point "needs
    /// OneDrive, dedup or ProjFS to create, none of which a unit test can stage on a CI runner". That
    /// was **wrong**, and PR #1043's Security Auditor disproved it by doing exactly that: a
    /// **non-Microsoft GUID reparse point**, planted with `FSCTL_SET_REPARSE_POINT` and a
    /// `REPARSE_GUID_DATA_BUFFER`, needs no privilege, no filter driver and no OneDrive.
    /// `cpe_1896_a_non_surrogate_reparse_point_is_traversed_not_refused` now stages one on every
    /// Windows run. The four measured shapes:
    ///
    /// ```text
    /// component                          tag          surrogate  outcome
    /// junction -> inside the root        0xA000000C   set        refused (by this check)
    /// junction -> outside the root       0xA000000C   set        refused (by this check)
    /// non-MS tag, directory bit SET      0x10001234   clear      ALLOWED, the write landed inside
    /// non-MS tag, directory bit CLEAR    0x00001234   clear      refused by NT, NOT by this check
    /// ```
    ///
    /// The last row is the "not sufficient" half: for a non-surrogate tag the **outcome is decided by
    /// NT and by whatever filter driver owns the tag**, not here. With the directory bit clear the
    /// descent fails whatever this returns. A `false` therefore means "this check does not object",
    /// never "the walk will succeed".
    ///
    /// The detail the stated motivation actually turns on is measured too: `IO_REPARSE_TAG_CLOUD`
    /// (`0x9000001A`) has the surrogate bit **clear** and the directory bit **set** — the same shape as
    /// the `0x10001234` row — so OneDrive Files-On-Demand *directory* placeholders are genuinely
    /// unblocked rather than merely believed to be.
    /// # Cost, stated accurately rather than by analogy with the leaf
    ///
    /// This runs on **every** directory component, unconditionally, and the shared helper performs the
    /// `FILE_ATTRIBUTE_REPARSE_POINT` check *internally* — returning `Some(false)` for an ordinary
    /// directory. So the ordinary-case cost here is **one extra `GetFileInformationByHandleEx` per
    /// directory component**, not zero. The leaf guard's "only asked when the reparse bit is already
    /// set" gating is real, but it is the leaf's, and does not apply to this call site.
    ///
    /// Nothing needs re-measuring on account of that: [`tick`] fires before the call, so the 5-and-6
    /// syscalls-per-file figures in `cpe_1896_report_the_walk_syscall_cost` already count it, and
    /// AC5's numbers stand as published.
    fn name_surrogate_at(dir: &File) -> bool {
        // **`unwrap_or(true)` — fail CLOSED, changed by CPE-1938 round 2 (Security Auditor, F2).**
        //
        // This was `unwrap_or(false)` — fail open — and the reason given was that containment here did
        // not rest on the check: a genuine surrogate would be caught one component later by NT itself
        // (`ERROR_CANT_RESOLVE_FILENAME`), so all the check bought was naming the link one component
        // earlier. **That reasoning was true for every caller this module had at the time, and CPE-1938
        // is the change that made it false.** It holds for [`create_beneath`], whose descent is always
        // followed by another NT open (the leaf). It does **not** hold for [`create_dir_beneath`] used
        // as a *verification-only* pass in front of a by-path third-party unpacker, which is exactly
        // what `archive::entry_component_action` does: for `sub/leaf.txt` the chain is **one
        // component**, this function is the only thing that can refuse it, and there is no next
        // component for NT to trip over.
        //
        // Measured on Windows 11 with this arm forced to its documented fail-open value, tar with a
        // single `sub/leaf.txt` entry and a junction at `dest/sub`:
        //
        // ```text
        // outcome = Ok(... done: 1, skipped: 0, errors: [])
        // dest/other/leaf.txt = "ARCHIVED LEAF"     <- the CPE-1938 defect, restored, silently
        // ```
        //
        // The `None` arm remains untestable by construction (see [`crate::batch_media::
        // reparse_name_surrogate`]): nothing can make `GetFileInformationByHandleEx` fail on a handle
        // that was just opened successfully. So this flip costs nothing observable and removes a
        // dependency on a backstop that is no longer always there — the safe direction for a default
        // that cannot be exercised. Both callers now fail closed, and the "opposite defaults" split
        // that used to justify the two values is gone with it.
        crate::batch_media::reparse_name_surrogate(dir).unwrap_or(true)
    }

    /// NT status codes are not Win32 error codes, and `io::Error` speaks Win32. Translating means a
    /// refusal reads as "Access is denied." rather than as `0xC0000022`.
    fn io_err(status: NTSTATUS) -> std::io::Error {
        // SAFETY: a pure value translation in ntdll; no pointers, no ownership.
        let win32 = unsafe { RtlNtStatusToDosError(status) };
        std::io::Error::from_raw_os_error(win32 as i32)
    }

    /// Descend (creating as needed) through `dirs`, returning the deepest handle — `None` when `dirs`
    /// is empty and the root itself is the parent. Shared by [`walk`] and [`walk_dirs`] so the file leg
    /// and the directory leg cannot drift apart about what a traversable component is (CPE-1913).
    fn descend(
        root: &RootDir,
        act: Act,
        dirs: &[&OsStr],
        sofar: &mut PathBuf,
    ) -> Result<Option<File>, Refusal> {
        let mut held: Option<File> = None;

        for name in dirs {
            sofar.push(name);
            let parent = held.as_ref().unwrap_or(&root.dir);
            // `FILE_OPEN_IF` = open it, or create it if it is not there — the per-component equivalent
            // of `create_dir_all`, except that it can only ever create *inside the handle we hold*, so
            // a refused entry cannot leave directory debris outside the root the way a path-based
            // `create_dir_all` walking a junction did (CPE-1889 check (1)'s whole reason to exist).
            //
            // **`FILE_OPEN` for a delete** (CPE-1937): `remove_file` on a path whose parents do not
            // exist is an error, and a destructive operation that silently *materialises* two
            // directories on its way to failing leaves debris behind a delete. So the disposition is
            // the one thing the acts differ on here; everything below is shared, which is the
            // point of parameterising rather than copying the walk. `Act::Commit` joins the delete
            // side of that split — see its doc.
            let disposition = if act.descent_creates() { FILE_OPEN_IF } else { FILE_OPEN };
            let dir = nt_child(
                parent,
                name,
                // `FILE_READ_ATTRIBUTES` is not decoration: without it the
                // `GetFileInformationByHandle` below **fails**, `handle_facts` returns `None`, and the
                // reparse-point refusal silently never fires. Measured — the CPE-1889 junction test
                // reddened with the refusal coming from the wrong place (a `ERROR_CANT_RESOLVE_FILENAME`
                // on the *next* component) instead of naming the junction.
                FILE_ACCESS_RIGHTS(
                    FILE_LIST_DIRECTORY.0 | FILE_TRAVERSE.0 | FILE_READ_ATTRIBUTES.0 | SYNCHRONIZE.0,
                ),
                disposition,
                NTCREATEFILE_CREATE_OPTIONS(
                    FILE_DIRECTORY_FILE.0 | FILE_OPEN_REPARSE_POINT.0 | FILE_SYNCHRONOUS_IO_NONALERT.0,
                ),
            )
            .map_err(|s| {
                refuse(root, act, sofar, &format!("could not be opened ({})", io_err(s)))
            })?;
            // `FILE_OPEN_REPARSE_POINT` means a junction here was opened **as the reparse point
            // itself** rather than followed, so nothing has escaped — but continuing through it would
            // put the file in the junction's own physical directory, which is contained and invisible
            // through the path the user sees. Refuse instead: this is the measured attack shape, and a
            // backup that silently writes somewhere the user cannot find is its own defect.
            //
            // One `GetFileInformationByHandleEx` per directory component, on a handle already open. It
            // is the only per-component cost this walk adds over the opens themselves; there is no way
            // to ask NT "and fail if it was a reparse point" as part of the create.
            //
            // **It refuses a NAME SURROGATE, not every reparse point** — see [`name_surrogate_at`].
            // This is diagnostics rather than the containment: PR #1043's Security Auditor neutered
            // this branch and re-ran the CPE-1889 junction harm test, and the write was *still*
            // refused, because NT returns `ERROR_CANT_RESOLVE_FILENAME` on the **next** component when
            // you open relative to a reparse-point handle. The kernel is what contains; this is what
            // names the cause. That is also why failing open (`is_some_and`) is acceptable here.
            tick();
            if name_surrogate_at(&dir) {
                return Err(refuse_link(root, act, sofar));
            }
            held = Some(dir);
        }
        Ok(held)
    }

    /// Every component of `parts` as a directory — [`create_dir_beneath`](super::create_dir_beneath)'s
    /// arm. The handle is dropped on return; the directory's existence is the product.
    pub(super) fn walk_dirs(root: &RootDir, parts: &[&OsStr]) -> Result<(), Refusal> {
        let mut sofar = PathBuf::new();
        descend(root, Act::Write, parts, &mut sofar).map(|_| ())
    }

    /// Ask the filesystem — never the errno — whether `name` under `parent` is a link, for a name that
    /// has already failed to open. Same rule and same reason as the Unix arm's `link_at`: a second
    /// `NtCreateFile`, same parent handle, same single component, opened as a *directory* and still
    /// `FILE_OPEN_REPARSE_POINT`, so the answer comes from an object rather than from a status code.
    /// Nothing is written or deleted on the strength of it — the entry is refused either way — so it
    /// decides the **sentence**, not the outcome.
    fn leaf_is_link(parent: &File, last: &OsStr) -> bool {
        let dir_options = NTCREATEFILE_CREATE_OPTIONS(
            FILE_DIRECTORY_FILE.0 | FILE_OPEN_REPARSE_POINT.0 | FILE_SYNCHRONOUS_IO_NONALERT.0,
        );
        nt_child(
            parent,
            last,
            FILE_ACCESS_RIGHTS(FILE_READ_ATTRIBUTES.0 | SYNCHRONIZE.0),
            FILE_OPEN,
            dir_options,
        )
        .is_ok_and(|d| name_surrogate_at(&d))
    }

    /// [`remove_file_beneath`](super::remove_file_beneath)'s arm — CPE-1937.
    ///
    /// Win32 has no handle-relative delete any more than it has a handle-relative open: `DeleteFileW`
    /// takes a path and re-parses it from the drive letter, which is the by-path question this module
    /// exists to remove. The two-step below is the `unlinkat` equivalent Windows does offer — open the
    /// leaf **relative to the parent handle** with `DELETE` access, then set the disposition **on that
    /// handle**. Between the two there is no name for anything to swap: the object marked for deletion
    /// is the object opened.
    ///
    /// `FILE_NON_DIRECTORY_FILE` keeps this to files, exactly as `fs::remove_file` does, so a directory
    /// junction standing at the leaf is refused rather than removed; `FILE_OPEN_REPARSE_POINT` means a
    /// *file* symlink at the leaf is opened as the link itself and the link — not its target — is what
    /// goes, which is also `remove_file`'s behaviour and the never-follow rule this module holds
    /// everywhere else.
    ///
    /// **Do not replace this pair with `DeleteFileW`/`fs::remove_file` on the grounds that `descend`
    /// already refused everything hostile.** It has not: `descend` refuses a component that *is* a link
    /// right now, so on every delete that succeeds it hands back a handle and this runs — and a by-path
    /// delete here re-parses the whole path from the drive letter, where a concurrent rename redirects
    /// it. The equivalent substitution was measured on the Unix arm (descent untouched): 89 bystanders
    /// destroyed outside the root over 200 trials, against 0 for the handle-relative form, with the
    /// static suite green both times. See [`remove_file_beneath`](super::remove_file_beneath).
    ///
    /// **`FileDispositionInfo` is delete-on-close, and that is the same contract `DeleteFileW` has**:
    /// the name goes when the last handle closes, and ours is dropped on return. If another process
    /// holds the file open *without* `FILE_SHARE_DELETE` the open above fails with a sharing violation
    /// — a `policy: false` refusal the revert reports as transient, which is what a locked file is.
    /// Mark an already-open handle for deletion the way `std::fs::remove_file` does — the modern
    /// `FileDispositionInfoEx` call, with **POSIX semantics** and **ignore-read-only** (CPE-1937 round 2).
    ///
    /// Both flags fix a measured defect in the first cut of this function, which used plain
    /// `FileDispositionInfo`:
    ///
    /// - **`IGNORE_READONLY_ATTRIBUTE`.** `FileDispositionInfo` **fails** on a file carrying
    ///   `FILE_ATTRIBUTE_READONLY`, so a revert that previously deleted a read-only file — `main` used
    ///   `fs::remove_file`, which handles it — stopped being able to, and reported a refusal the user
    ///   could not act on. Measured by PR #1059's Security Auditor:
    ///   `remove_file_beneath -> Err(policy=false), file survives` where `std::fs::remove_file -> Ok`
    ///   on the identical fixture. Linux was unaffected (`unlinkat` does not consult a read-only bit).
    /// - **`POSIX_SEMANTICS`.** Plain `FileDispositionInfo` is *delete-on-close*: with another handle
    ///   open (sharing delete), the call returns success and **the name stays in the directory** until
    ///   the last handle closes. That is a report-vs-filesystem divergence — `applied` counted against a
    ///   name still present — which is the exact family of defect this ticket exists to close, so it is
    ///   not left as a documented quirk. POSIX semantics unlinks the name immediately, matching
    ///   `unlinkat` on the Unix side and `std` on this one.
    ///
    /// Returns `Err` when the call is unavailable (pre-Windows 10 1709, or a filesystem that does not
    /// implement it), which is what the caller's fallback is for.
    fn dispose_posix(file: &File) -> windows::core::Result<()> {
        let info = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
                FILE_DISPOSITION_FLAG_DELETE.0
                    | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                    | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
            ),
        };
        tick();
        // SAFETY: `file` is a live handle opened with `DELETE` access; `info` is a correctly-sized,
        // correctly-typed input buffer for `FileDispositionInfoEx` and outlives the call. No ownership
        // is transferred — the handle stays ours and is closed by `File`'s drop.
        unsafe {
            SetFileInformationByHandle(
                HANDLE(file.as_raw_handle() as isize),
                FileDispositionInfoEx,
                std::ptr::addr_of!(info).cast(),
                u32::try_from(std::mem::size_of::<FILE_DISPOSITION_INFO_EX>()).unwrap_or(1),
            )
        }
    }

    /// The pre-1709 fallback: plain `FileDispositionInfo` (delete-on-close, refuses a read-only file).
    fn dispose_on_close(file: &File) -> windows::core::Result<()> {
        let info = FILE_DISPOSITION_INFO { DeleteFile: BOOLEAN(1) };
        tick();
        // SAFETY: as `dispose_posix` — live handle with `DELETE`, correctly-sized input buffer that
        // outlives the call, no ownership transfer.
        unsafe {
            SetFileInformationByHandle(
                HANDLE(file.as_raw_handle() as isize),
                FileDispositionInfo,
                std::ptr::addr_of!(info).cast(),
                u32::try_from(std::mem::size_of::<FILE_DISPOSITION_INFO>()).unwrap_or(1),
            )
        }
    }

    /// Clear `FILE_ATTRIBUTE_READONLY` **on the handle we already hold**, returning the attributes as
    /// they were so the caller can put them back if the delete still fails.
    ///
    /// `std` does this by *path* (`set_permissions`) in its own fallback. Doing it by path here would
    /// reintroduce exactly the defect this module exists to remove — a second name lookup a rename can
    /// redirect — so it is done on the open handle, which is the same object the disposition is about
    /// by construction. `None` means there was nothing to clear (or the attributes could not be read,
    /// in which case the delete is simply attempted as-is).
    ///
    /// Zero in a `FILE_BASIC_INFO` time field means "leave it alone", so this changes attributes only.
    fn clear_read_only(file: &File) -> Option<u32> {
        tick();
        // SAFETY: `file` is live; `basic` is a correctly-sized, correctly-typed out-parameter for
        // `FileBasicInfo` that outlives the call.
        let mut basic: FILE_BASIC_INFO = unsafe { std::mem::zeroed() };
        let read = unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle() as isize),
                FileBasicInfo,
                std::ptr::addr_of_mut!(basic).cast(),
                u32::try_from(std::mem::size_of::<FILE_BASIC_INFO>()).unwrap_or(1),
            )
        };
        if read.is_err() || basic.FileAttributes & FILE_ATTRIBUTE_READONLY.0 == 0 {
            return None;
        }
        let was = basic.FileAttributes;
        set_attributes(file, was & !FILE_ATTRIBUTE_READONLY.0).ok()?;
        Some(was)
    }

    /// Write `attributes` back onto an open handle, leaving every timestamp untouched.
    fn set_attributes(file: &File, attributes: u32) -> windows::core::Result<()> {
        let info = FILE_BASIC_INFO {
            CreationTime: 0,
            LastAccessTime: 0,
            LastWriteTime: 0,
            ChangeTime: 0,
            FileAttributes: attributes,
        };
        tick();
        // SAFETY: live handle; correctly-sized, correctly-typed input buffer outliving the call.
        unsafe {
            SetFileInformationByHandle(
                HANDLE(file.as_raw_handle() as isize),
                FileBasicInfo,
                std::ptr::addr_of!(info).cast(),
                u32::try_from(std::mem::size_of::<FILE_BASIC_INFO>()).unwrap_or(1),
            )
        }
    }

    pub(super) fn unlink(root: &RootDir, dirs: &[&OsStr], last: &OsStr) -> Result<(), Refusal> {
        let mut sofar = PathBuf::new();
        let held = descend(root, Act::Delete, dirs, &mut sofar)?;
        // The descent is done and its handle is held; a by-path leaf would re-resolve from here.
        // Test-only seam — compiled out of a shipped binary. See its definition for the coverage
        // hole it exists to close.
        super::between_descent_and_leaf();
        let parent = held.as_ref().unwrap_or(&root.dir);
        sofar.push(last);

        let options = NTCREATEFILE_CREATE_OPTIONS(
            FILE_NON_DIRECTORY_FILE.0 | FILE_OPEN_REPARSE_POINT.0 | FILE_SYNCHRONOUS_IO_NONALERT.0,
        );
        // `FILE_WRITE_ATTRIBUTES` is asked for so the read-only fallback below can clear the bit on
        // **this** handle rather than by path. It is asked for *optionally*: a file whose ACL grants
        // DELETE but not attribute-write must still be deletable, which is what `main`'s
        // `fs::remove_file` did, so a refusal here falls back to the minimal access set rather than
        // becoming a new way for a revert to fail.
        let full = FILE_ACCESS_RIGHTS(
            DELETE.0 | FILE_READ_ATTRIBUTES.0 | FILE_WRITE_ATTRIBUTES.0 | SYNCHRONIZE.0,
        );
        let minimal = FILE_ACCESS_RIGHTS(DELETE.0 | FILE_READ_ATTRIBUTES.0 | SYNCHRONIZE.0);
        let (file, may_write_attrs) = match nt_child(parent, last, full, FILE_OPEN, options) {
            Ok(f) => (f, true),
            Err(_) => match nt_child(parent, last, minimal, FILE_OPEN, options) {
                Ok(f) => (f, false),
                // **Classified by asking the filesystem, never by the status code** — the same rule,
                // and the same shared helper, as the write leaf's. A directory junction standing at a
                // file entry's name comes back `STATUS_FILE_IS_A_DIRECTORY` through
                // `FILE_NON_DIRECTORY_FILE`, which is not link-shaped at all.
                Err(s) => {
                    return Err(if leaf_is_link(parent, last) {
                        refuse_link(root, Act::Delete, &sofar)
                    } else {
                        refuse(
                            root,
                            Act::Delete,
                            &sofar,
                            &format!("could not be opened for deletion ({})", io_err(s)),
                        )
                    })
                }
            },
        };

        // The modern call first: it unlinks the name immediately and ignores a read-only attribute, so
        // on every supported Windows this is the whole function.
        if dispose_posix(&file).is_ok() {
            return Ok(());
        }
        // Fallback for a host or filesystem without `FileDispositionInfoEx`: clear the read-only bit
        // ourselves (on the handle, never by path) and use the delete-on-close form. If that still
        // fails, put the attribute back — a refused deletion must not leave the file's attributes
        // changed, which would be a silent edit performed by an operation that did nothing else.
        //
        // **NOT COVERED BY CI, and that is a known gap** (CPE-1937 round 2). Every supported Windows
        // answers `FileDispositionInfoEx`, so nothing in the suite reaches these four lines; the only
        // exercise they have had is PR #1059's Security Auditor forcing the branch open with an
        // environment gate and then forcing `dispose_on_close` to fail as well. That run is the
        // evidence for the restore below — `survived=true, readonly_before=true, READONLY_AFTER=true`,
        // and a plain file refused the same way came back `readonly_after=false`, so no attribute is
        // invented for a file that never had one. Treat a change here as untested until re-run that
        // way; a reachable regression test would need a pre-1709 host or a filesystem that declines
        // the Ex form.
        let restore = if may_write_attrs { clear_read_only(&file) } else { None };
        dispose_on_close(&file).map_err(|e| {
            // **Best-effort, and the sentence above is therefore conditional.** If putting the
            // attribute back fails — the handle lost its rights, the volume went away — the file is
            // left writable and nothing says so, because there is no second channel to say it in and
            // failing the refusal harder would not restore the bit either. The narrow case it can
            // happen in is the only reason it is acceptable: this is already the fallback of a
            // fallback, on a host that has just refused two deletion calls in a row.
            if let Some(was) = restore {
                let _ = set_attributes(&file, was);
            }
            let note = if !may_write_attrs {
                " — the file could not be opened for attribute changes, so a read-only attribute could \
                 not be cleared"
            } else {
                ""
            };
            refuse(
                root,
                Act::Delete,
                &sofar,
                &format!("could not be marked for deletion ({e}){note}"),
            )
        })
    }

    /// [`create_staging_beneath`](super::create_staging_beneath)'s arm — CPE-1961.
    ///
    /// `FILE_CREATE` with **no** `FILE_OPEN` fallback: a staging name that is already occupied is
    /// refused, never opened. See the public doc for why `DELETE` is always asked for here and
    /// deliberately not by [`walk`], and why `READ_CONTROL | WRITE_DAC` are asked for only when
    /// `carrying` says a DACL is actually going to be written to this handle.
    pub(super) fn walk_staged(
        root: &RootDir,
        dirs: &[&OsStr],
        last: &OsStr,
        carrying: bool,
    ) -> Result<Opened, Refusal> {
        let mut sofar = PathBuf::new();
        let held = descend(root, Act::Write, dirs, &mut sofar)?;
        let parent = held.as_ref().unwrap_or(&root.dir);
        sofar.push(last);
        let dacl = if carrying { READ_CONTROL.0 | WRITE_DAC.0 } else { 0 };
        let access = FILE_ACCESS_RIGHTS(
            FILE_GENERIC_WRITE.0 | FILE_READ_ATTRIBUTES.0 | DELETE.0 | dacl | SYNCHRONIZE.0,
        );
        let options = NTCREATEFILE_CREATE_OPTIONS(
            FILE_NON_DIRECTORY_FILE.0 | FILE_OPEN_REPARSE_POINT.0 | FILE_SYNCHRONOUS_IO_NONALERT.0,
        );
        match nt_child(parent, last, access, FILE_CREATE, options) {
            Ok(file) => Ok(Opened { file, created: true }),
            // No `leaf_is_link` classification here, and the omission is deliberate: this leaf never
            // opens an existing object, so "what is sitting at the name" cannot change the verdict —
            // the answer is refuse either way, and the caller retries with a fresh stamp. The status is
            // reported so a genuinely broken folder (no `CreateFiles` right) says so.
            Err(s) => Err(refuse(
                root,
                Act::Write,
                &sofar,
                &format!("could not be created as a staging file ({})", io_err(s)),
            )),
        }
    }

    /// [`rename_beneath`](super::rename_beneath)'s arm — CPE-1961.
    ///
    /// **The source operand is the `staged` HANDLE, not `from`.** `from` is carried only so the
    /// refusal can name it; nothing here presents it to the filesystem. `FILE_RENAME_INFO`'s
    /// `RootDirectory` field makes `FileName` a single component resolved inside the parent directory
    /// object, and `ReplaceIfExists` makes it replace whatever name is there.
    pub(super) fn rename(
        root: &RootDir,
        dirs: &[&OsStr],
        from: &OsStr,
        to: &OsStr,
        staged: &File,
    ) -> Result<(), Refusal> {
        use windows::Wdk::Storage::FileSystem::{FileRenameInformation, NtSetInformationFile};
        use windows::Win32::Storage::FileSystem::FILE_RENAME_INFO;

        let mut sofar = PathBuf::new();
        // `Act::Commit`, not `Act::Write`: the parents already exist (the staging file is sitting in
        // them), so a `FILE_OPEN_IF` descent here would only ever re-create a directory something
        // removed under us, and then fail at the rename anyway. See `Act::Commit`.
        let held = descend(root, Act::Commit, dirs, &mut sofar)?;
        let parent = held.as_ref().unwrap_or(&root.dir);
        sofar.push(to);

        let wide: Vec<u16> = to.encode_wide().collect();
        let name_bytes = match u32::try_from(wide.len().saturating_mul(2)) {
            Ok(n) => n,
            Err(_) => {
                return Err(refuse(
                    root,
                    Act::Commit,
                    &sofar,
                    "has a name too long to describe to the filesystem",
                ))
            }
        };
        // `FILE_RENAME_INFO` ends in a one-element `FileName` array, so the buffer is the struct plus
        // room for the rest of the name. Allocated as `u64`s rather than `u8`s because the struct holds
        // a `HANDLE` and must be 8-byte aligned; a `Vec<u8>` guarantees alignment 1.
        let header = std::mem::size_of::<FILE_RENAME_INFO>();
        let total = header + wide.len().saturating_mul(2);
        let mut buf: Vec<u64> = vec![0; total.div_ceil(8).max(1)];
        let ptr = buf.as_mut_ptr().cast::<u8>();
        // **`NtSetInformationFile`, not `SetFileInformationByHandle`, and the difference is not
        // stylistic — it is measured.** The Win32 wrapper takes the same `FILE_RENAME_INFO` buffer and
        // refuses it with `ERROR_INVALID_PARAMETER (0x80070057)` the moment `RootDirectory` is
        // non-null: the entire `transfer` and `archive` suites reddened on it, every entry, before the
        // call was moved down one layer. The NT form is the one that has always honoured a
        // directory-relative rename, and it is the same layer `nt_child` above already opens through,
        // so this module talks to one API rather than two. If a future reader "simplifies" this back to
        // the Win32 call, every commit on the `Beneath` arm fails; the wrapper is not a superset.
        //
        // SAFETY: `buf` owns at least `total` bytes at 8-byte alignment and outlives the call below;
        // `info` is written entirely within it, and `wide` is copied into the `FileName` tail whose
        // room was reserved above. `parent` and `staged` are borrowed from live `File`s, and `iosb` is
        // a correctly-typed out-parameter.
        let status = unsafe {
            let info = ptr.cast::<FILE_RENAME_INFO>();
            (*info).Anonymous.ReplaceIfExists = BOOLEAN(1);
            (*info).RootDirectory = HANDLE(parent.as_raw_handle() as isize);
            (*info).FileNameLength = name_bytes;
            std::ptr::copy_nonoverlapping(
                wide.as_ptr(),
                std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
                wide.len(),
            );
            let mut iosb: IO_STATUS_BLOCK = std::mem::zeroed();
            tick();
            NtSetInformationFile(
                HANDLE(staged.as_raw_handle() as isize),
                &mut iosb,
                ptr.cast(),
                u32::try_from(total).unwrap_or(u32::MAX),
                FileRenameInformation,
            )
        };
        if status.is_ok() {
            return Ok(());
        }
        Err(refuse(
            root,
            Act::Commit,
            &sofar,
            &format!(
                "could not be replaced by the staged copy of it ({}) [staged as {from:?}]",
                io_err(status)
            ),
        ))
    }

    pub(super) fn walk(root: &RootDir, dirs: &[&OsStr], last: &OsStr) -> Result<Opened, Refusal> {
        let mut sofar = PathBuf::new();
        let held = descend(root, Act::Write, dirs, &mut sofar)?;

        let parent = held.as_ref().unwrap_or(&root.dir);
        sofar.push(last);
        // Exclusive create first, so `created` is the kernel's answer rather than a guess — the same
        // order, and for the same reason, as `batch_media::open_no_follow`.
        let access = FILE_ACCESS_RIGHTS(FILE_GENERIC_WRITE.0 | FILE_READ_ATTRIBUTES.0 | SYNCHRONIZE.0);
        let options = NTCREATEFILE_CREATE_OPTIONS(
            FILE_NON_DIRECTORY_FILE.0 | FILE_OPEN_REPARSE_POINT.0 | FILE_SYNCHRONOUS_IO_NONALERT.0,
        );
        match nt_child(parent, last, access, FILE_CREATE, options) {
            Ok(file) => Ok(Opened { file, created: true }),
            Err(_) => match nt_child(parent, last, access, FILE_OPEN, options) {
                Ok(file) => Ok(Opened { file, created: false }),
                // **Classify before refusing — CPE-1913 round 2, security audit finding F1.**
                //
                // The open above carries `FILE_NON_DIRECTORY_FILE`, so a **directory junction sitting
                // at a file entry's name** comes back `STATUS_FILE_IS_A_DIRECTORY` rather than as
                // anything link-shaped, and an unclassified refusal here is `policy: false` — which
                // `archive` turns into `return Err` (the whole extraction aborts, leaving a
                // half-extracted folder) and `transfer` turns into `undelivered` (the whole call ends
                // `Err`). On `main` the same fixture was a **per-entry skip**: `Ok`, one entry
                // reported, every other entry still delivered. Measured by PR #1050's Security
                // Auditor over 7,890 planted links — containment was never affected, but one junction
                // named like any entry in the archive could abort the run.
                //
                // The `claim_destination_handle` arms that would have said `policy: true` for this —
                // the surrogate check and `facts.is_dir` — are unreachable on Windows, because the
                // open never returns a handle to reach them with. `fsutil`'s own comment says so.
                // So the classification has to happen here, where the failure is.
                //
                // **Asked of the filesystem, never of the errno**, which is this module's standing
                // rule (see `link_at` on the Unix side and the ENOTDIR/ELOOP measurements that put it
                // there). A second `NtCreateFile` — same parent handle, same single component, but as
                // a *directory* and still `FILE_OPEN_REPARSE_POINT` — either yields a handle we can
                // ask [`name_surrogate_at`], or does not, and either way nothing is written and the
                // entry is refused. It costs one syscall on a path that is already refusing.
                //
                // **Only the LINK case becomes a policy skip. A plain directory at the leaf keeps
                // `policy: false`**, deliberately: `main` aborts for that too (`fs::File::create` on
                // a directory is a hard error at every one of these call sites), so widening it would
                // be a behaviour change beyond the regression this fixes, and CPE-1935 records the
                // plain-directory abort as pre-existing. This restores parity with the Unix arm as
                // well, which has always classified a symlink at the leaf through `link_at` and
                // refused it with `refuse_link`.
                //
                // The classification itself is [`leaf_is_link`], shared with the delete leg (CPE-1937)
                // rather than spelled twice, for the same reason `descend` is shared: two copies of a
                // security classification are two things that can drift.
                Err(s) => {
                    if leaf_is_link(parent, last) {
                        Err(refuse_link(root, Act::Write, &sofar))
                    } else {
                        Err(refuse(
                            root,
                            Act::Write,
                            &sofar,
                            &format!("could not be opened for writing ({})", io_err(s)),
                        ))
                    }
                }
            },
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Unix: openat/mkdirat with O_NOFOLLOW, plus openat2(RESOLVE_BENEATH) as a Linux fast path.
// ---------------------------------------------------------------------------------------------
#[cfg(unix)]
mod sys {
    use super::{refuse, refuse_link, tick, Act, Opened, Refusal, RootDir};
    use std::ffi::{CString, OsStr};
    use std::fs::File;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
    use std::path::{Path, PathBuf};

    pub(super) fn open_root_dir(real_root: &Path) -> std::io::Result<File> {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(real_root)
    }

    fn cname(name: &OsStr) -> Result<CString, ()> {
        CString::new(name.as_bytes()).map_err(|_| ())
    }

    /// Was `name` a symlink, asked of the **parent handle** and never following anything?
    ///
    /// This runs only on a component that has already failed to open, so it costs nothing in the
    /// ordinary case, and it answers the question the errno cannot. Linux and macOS both report
    /// **`ENOTDIR`** for a symlink at a component opened with `O_DIRECTORY` — and `ENOTDIR` is equally
    /// what a *regular file* sitting at a directory component produces. Those two need different
    /// sentences: one is the attack this module exists for, the other is an ordinary mistake. So the
    /// answer comes from `fstatat(AT_SYMLINK_NOFOLLOW)` rather than from guessing at an errno.
    ///
    /// It is a second look at a name, so in principle the name could have changed since the failed
    /// open. Nothing is opened or written on the strength of it and the entry is refused either way,
    /// so the **decision** is unaffected — but "only the sentence changes" undersells what that
    /// sentence carries. Swap the symlink for a regular file in the window between the failed open and
    /// this call and both produce `ENOTDIR`, so the refusal reads "could not be opened (Not a
    /// directory)" and **the attack signature disappears from the message**; `link_at` also returns
    /// `false` when `fstatat` itself fails, biasing the same way. For an operator triaging a backup,
    /// "is a link" versus "not a directory" is the difference between seeing an attack and seeing a
    /// typo. The bias is toward under-reporting an attack, never toward inventing one.
    fn link_at(parent: RawFd, name: &CString) -> bool {
        // SAFETY: `parent` is borrowed from a live `File`, `name` outlives the call, and `st` is a
        // correctly-sized out-parameter. Read-only query, no ownership transfer.
        unsafe {
            let mut st: libc::stat = std::mem::zeroed();
            if libc::fstatat(parent, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) != 0 {
                return false;
            }
            st.st_mode & libc::S_IFMT == libc::S_IFLNK
        }
    }

    /// Open (creating if absent) one directory component relative to `parent`.
    ///
    /// `O_NOFOLLOW` is what makes it atomic: if the name is a symlink the **open itself** fails, so
    /// there is no window in which the name could be checked and then followed.
    ///
    /// **It does not fail with `ELOOP`, and an earlier revision of this file assumed it did.** With
    /// `O_DIRECTORY` also set, Linux's `do_open()` reaches the `LOOKUP_DIRECTORY` check (`-ENOTDIR`)
    /// before `may_open()`'s `S_IFLNK -> -ELOOP` case, so a symlink at an *intermediate* component
    /// returns **`ENOTDIR`**; xnu has the same `v_type != VDIR -> ENOTDIR` ordering. Only the final
    /// component — opened without `O_DIRECTORY` by [`child_file`] — actually yields `ELOOP`.
    /// Containment was never affected (the open failed either way), but the *message* read "could not
    /// be opened (Not a directory)" instead of naming the link, and two tests asserting the link
    /// wording reddened on ubuntu. Measured by PR #1043's reviewer on WSL2 kernel 6.6.87 and
    /// reproduced here. Classification is therefore [`link_at`]'s job, never the errno's.
    ///
    /// **`create` is false for a delete** (CPE-1937): `remove_file` on a missing parent chain is an
    /// error, and a destructive operation that materialises directories on its way to failing leaves
    /// debris. The `openat` below is identical either way — only the `mkdirat` is skipped — so the two
    /// acts cannot come to disagree about what a traversable component is.
    fn child_dir(parent: RawFd, name: &CString, create: bool) -> std::io::Result<File> {
        if create {
            tick();
            // SAFETY: `parent` is borrowed from a live `File`; `name` is a NUL-terminated C string that
            // outlives the call. Ordinary FFI with no ownership transfer.
            let made = unsafe { libc::mkdirat(parent, name.as_ptr(), 0o777) };
            if made != 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(e);
                }
            }
        }
        tick();
        // SAFETY: as above; on success the fd is wrapped in a `File` exactly once, which closes it.
        let fd = unsafe {
            libc::openat(
                parent,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `fd` is a fresh, owned descriptor this call just created.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    /// Open the final component for writing relative to `parent`, reporting whether we created it.
    fn child_file(parent: RawFd, name: &CString) -> std::io::Result<(File, bool)> {
        let base = libc::O_WRONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        tick();
        // SAFETY: ordinary FFI; `name` outlives the call, `parent` is borrowed from a live `File`.
        // `openat` is variadic — the mode argument is only read because `O_CREAT` is set.
        let fd = unsafe {
            libc::openat(parent, name.as_ptr(), base | libc::O_CREAT | libc::O_EXCL, 0o666 as libc::c_uint)
        };
        if fd >= 0 {
            // SAFETY: fresh owned descriptor.
            return Ok((unsafe { File::from_raw_fd(fd) }, true));
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(e);
        }
        tick();
        // SAFETY: as above.
        let fd = unsafe { libc::openat(parent, name.as_ptr(), base) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: fresh owned descriptor.
        Ok((unsafe { File::from_raw_fd(fd) }, false))
    }

    /// `openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS)` — the whole resolution, kernel-enforced, in one
    /// syscall. Returns `None` for "did not answer", on which the caller falls through to the walk.
    ///
    /// **Every** failure returns `None`, deliberately: the walk is the authority, so a fast path that
    /// declines can never be the reason an entry is refused *or* allowed. That also removes the one
    /// real hazard of hand-rolling a syscall here — misclassifying an errno — because no errno
    /// classification is performed at all.
    #[cfg(target_os = "linux")]
    fn openat2_beneath(rootfd: RawFd, rel: &Path) -> Option<(File, bool)> {
        use std::sync::atomic::{AtomicU8, Ordering};

        /// `struct open_how`, kernel ABI, `include/uapi/linux/openat2.h`. Three `u64`s, 24 bytes; the
        /// size is passed to the syscall so the kernel can reject a shape it does not know.
        #[repr(C)]
        // The fields are written and then handed to the kernel by pointer; Rust never reads them
        // back, which is exactly the shape `dead_code` flags. The ABI is the point of the struct.
        #[allow(dead_code)]
        struct OpenHow {
            flags: u64,
            mode: u64,
            resolve: u64,
        }
        /// `RESOLVE_NO_SYMLINKS` — refuse a symlink at ANY component, including the final one.
        const RESOLVE_NO_SYMLINKS: u64 = 0x04;
        /// `RESOLVE_BENEATH` — refuse anything that resolves outside the directory `dirfd` names.
        const RESOLVE_BENEATH: u64 = 0x08;

        /// 0 = not tried, 1 = usable, 2 = this kernel/sandbox does not offer it.
        static SUPPORTED: AtomicU8 = AtomicU8::new(0);
        if SUPPORTED.load(Ordering::Relaxed) == 2 {
            return None;
        }

        let c = CString::new(rel.as_os_str().as_bytes()).ok()?;
        let call = |flags: i32| -> Result<RawFd, std::io::Error> {
            let how = OpenHow {
                flags: flags as u64,
                // **Zero unless we are creating, and this is not cosmetic.** The kernel's
                // `build_open_flags()` rejects `!WILL_CREATE(flags) && how->mode != 0` with `EINVAL`,
                // so a hard-coded `0o666` made the *open-existing* call fail every single time.
                // Measured by PR #1043's reviewer:
                //
                // ```text
                // create O_CREAT|O_EXCL mode=0666 -> fd 5
                // open-existing         mode=0666 -> -1 EINVAL
                // open-existing         mode=0    -> fd 6
                // ```
                //
                // **What it actually cost, corrected — an earlier draft of this comment got the
                // mechanism wrong and the measurement disproves it.** That draft said `EINVAL` is in
                // the latch set below, so the first overwrite turned `openat2` off process-wide. It
                // cannot: the latch is only reachable from the `O_CREAT|O_EXCL` arm of the `match`, and
                // the `EINVAL` from the open-existing call is discarded by `.ok()`. The real shape is
                // worse in one way and better in another — `SUPPORTED` stayed at 1, so **every**
                // `update`-list entry re-paid two doomed `openat2` calls forever and then walked
                // anyway. The arithmetic says so: a latch would have given 6 walk-syscalls per
                // overwrite (the walk alone), and 8 were measured — the 6, plus 2 that were never
                // going to succeed. Nothing surfaced it either way, because the fast path swallows its
                // errors by design.
                mode: if flags & libc::O_CREAT != 0 { 0o666 } else { 0 },
                resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS,
            };
            tick();
            // SAFETY: `SYS_openat2` takes (dirfd, path, *const open_how, size). `c` and `how` outlive
            // the call; the size passed matches the struct the kernel is given. `rootfd` is widened to
            // `c_long` explicitly: it rides in a variadic argument list, where an `i32` and the
            // `c_long` the kernel stub reads are not the same slot on LP64.
            let r = unsafe {
                libc::syscall(
                    libc::SYS_openat2,
                    rootfd as libc::c_long,
                    c.as_ptr(),
                    std::ptr::addr_of!(how),
                    std::mem::size_of::<OpenHow>(),
                )
            };
            if r < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(r as RawFd)
            }
        };

        let base = libc::O_WRONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        match call(base | libc::O_CREAT | libc::O_EXCL) {
            Ok(fd) => {
                SUPPORTED.store(1, Ordering::Relaxed);
                // SAFETY: fresh owned descriptor.
                return Some((unsafe { File::from_raw_fd(fd) }, true));
            }
            Err(e) => {
                // ENOSYS (pre-5.6), EPERM (seccomp) and EINVAL (unknown `open_how`) all mean "this
                // kernel will never answer" — remember it so the run stops paying a syscall per file
                // to find that out again. Anything else is a per-entry answer the walk will re-derive.
                //
                // **This latch is process-wide and permanent, so it is only ever set from errors that
                // are a property of the kernel, never of the entry.** One wrongly-latching call would
                // turn the fast path off for every backup this process runs afterwards. Nothing is
                // unsafe when that happens (the walk is the authority), but it is invisible, so the
                // bar for adding an errno here is that no per-file condition can produce it.
                //
                // It is reachable **only from the `O_CREAT|O_EXCL` arm**, which is why the `mode` bug
                // above did not in fact latch: that `EINVAL` came from the open-existing call, whose
                // error is discarded by `.ok()`. Worth knowing before adding an errno — the two calls
                // are not symmetric, and only this one can turn the fast path off.
                //
                // `ENOENT` is deliberately **absent**: a missing parent chain is the ordinary
                // first-file-in-a-new-directory case, and a racing rename in the destination can
                // produce it at will (46% of 400,000 calls under a churning racer, measured). Latching
                // on it would let an actor with write access permanently disable the fast path.
                if matches!(e.raw_os_error(), Some(libc::ENOSYS) | Some(libc::EPERM) | Some(libc::EINVAL)) {
                    SUPPORTED.store(2, Ordering::Relaxed);
                    return None;
                }
                if e.kind() != std::io::ErrorKind::AlreadyExists {
                    return None;
                }
            }
        }
        SUPPORTED.store(1, Ordering::Relaxed);
        // SAFETY: fresh owned descriptor.
        call(base).ok().map(|fd| (unsafe { File::from_raw_fd(fd) }, false))
    }

    /// Descend (creating as needed) through `dirs`, returning the deepest handle — `None` when `dirs`
    /// is empty and the root itself is the parent. Shared by [`walk`] and [`walk_dirs`] so the file leg
    /// and the directory leg cannot drift apart about what a traversable component is (CPE-1913).
    ///
    /// There is deliberately **no `openat2` fast path here**: `RESOLVE_BENEATH` answers a whole
    /// multi-component *open*, and this walk's product is a chain of `mkdirat`s. The fast path stays
    /// where it pays, on the file leaf.
    fn descend(
        root: &RootDir,
        act: Act,
        dirs: &[&OsStr],
        sofar: &mut PathBuf,
    ) -> Result<Option<File>, Refusal> {
        let mut held: Option<File> = None;
        for name in dirs {
            sofar.push(name);
            let c = cname(name).map_err(|()| {
                refuse(root, act, sofar, "contains a NUL byte, which no filesystem name can hold")
            })?;
            let parent = match held.as_ref() {
                Some(f) => f.as_raw_fd(),
                None => root.dir.as_raw_fd(),
            };
            let dir = child_dir(parent, &c, act.descent_creates()).map_err(|e| {
                // Classified by asking the filesystem, NOT by reading the errno — see [`link_at`].
                // A symlink at an intermediate component reports `ENOTDIR` on Linux and macOS, and so
                // does a plain file sitting where a directory should be; they need different
                // sentences and the errno cannot tell them apart.
                if link_at(parent, &c) {
                    refuse_link(root, act, sofar)
                } else {
                    refuse(root, act, sofar, &format!("could not be opened ({e})"))
                }
            })?;
            held = Some(dir);
        }
        Ok(held)
    }

    /// Every component of `parts` as a directory — [`create_dir_beneath`](super::create_dir_beneath)'s
    /// arm. The handle is dropped on return; the directory's existence is the product.
    pub(super) fn walk_dirs(root: &RootDir, parts: &[&OsStr]) -> Result<(), Refusal> {
        let mut sofar = PathBuf::new();
        descend(root, Act::Write, parts, &mut sofar).map(|_| ())
    }

    /// [`remove_file_beneath`](super::remove_file_beneath)'s arm — CPE-1937.
    ///
    /// `unlinkat(parent_fd, name, 0)` is the primitive this module was missing: the name is resolved
    /// against **one already-open directory object**, so there is no interval in which a rename can
    /// redirect it, and it **never follows a symlink at the final component** — it removes the name
    /// itself. That is both this module's standing never-follow rule and exactly what
    /// `fs::remove_file` already did at that one component, so the only behaviour that changes is
    /// where the interior components are resolved.
    ///
    /// `AT_REMOVEDIR` is deliberately **not** passed: a directory (a symlink to one included, since
    /// `unlinkat` does not traverse) is refused with `EISDIR`/`EPERM` rather than removed, matching
    /// `fs::remove_file` and keeping a revert's delete leg to the files its plan named.
    ///
    /// **Do not replace this with `fs::remove_file(root.join(sofar))` on the grounds that the descent
    /// above already refused everything hostile.** It has not: the descent only refuses a component
    /// that *is* a link right now, so on every delete that succeeds it hands back a handle and this
    /// line runs — and a by-path call here re-resolves the whole path from the root, where a
    /// concurrent rename redirects it. Measured on real Linux, descent untouched: **89 bystanders
    /// destroyed outside the root over 200 trials** (7742 swaps), against **0** (7373 swaps) for the
    /// `unlinkat` below. The static suite is green either way — the harness that says so is
    /// `revert_engine`'s `cpe_1937_raced_delete_never_escapes_the_restore_root`, which is `#[ignore]`d.
    pub(super) fn unlink(root: &RootDir, dirs: &[&OsStr], last: &OsStr) -> Result<(), Refusal> {
        let mut sofar = PathBuf::new();
        let held = descend(root, Act::Delete, dirs, &mut sofar)?;
        // The descent is done and its handle is held; a by-path leaf would re-resolve from here.
        // Test-only seam — compiled out of a shipped binary. See its definition for the coverage
        // hole it exists to close.
        super::between_descent_and_leaf();
        sofar.push(last);
        let c = cname(last).map_err(|()| {
            refuse(root, Act::Delete, &sofar, "contains a NUL byte, which no filesystem name can hold")
        })?;
        let parent = match held.as_ref() {
            Some(f) => f.as_raw_fd(),
            None => root.dir.as_raw_fd(),
        };
        tick();
        // SAFETY: `parent` is borrowed from a live `File` (or from the held root handle); `c` is a
        // NUL-terminated C string that outlives the call. Ordinary FFI, no ownership transfer.
        if unsafe { libc::unlinkat(parent, c.as_ptr(), 0) } != 0 {
            let e = std::io::Error::last_os_error();
            return Err(refuse(
                root,
                Act::Delete,
                &sofar,
                &format!("could not be deleted ({e})"),
            ));
        }
        Ok(())
    }

    /// [`create_staging_beneath`](super::create_staging_beneath)'s arm — CPE-1961.
    ///
    /// `O_CREAT|O_EXCL|O_NOFOLLOW` with **no** fallback open: a staging name that is already occupied
    /// is refused, never opened. The mode is `crate::fsutil::STAGING_MODE`'s `0600`, spelled here
    /// rather than imported because this module takes nothing from `fsutil` — see the public doc for
    /// why the file is born narrow instead of being narrowed afterwards.
    pub(super) fn walk_staged(
        root: &RootDir,
        dirs: &[&OsStr],
        last: &OsStr,
        carrying: bool,
    ) -> Result<Opened, Refusal> {
        // `openat` has no access-right knob to narrow — the Windows arm's `carrying` split has no
        // analogue here. Named rather than `_carrying` so the two signatures read identically.
        let _ = carrying;
        let mut sofar = PathBuf::new();
        let held = descend(root, Act::Write, dirs, &mut sofar)?;
        sofar.push(last);
        let c = cname(last).map_err(|()| {
            refuse(root, Act::Write, &sofar, "contains a NUL byte, which no filesystem name can hold")
        })?;
        let parent = match held.as_ref() {
            Some(f) => f.as_raw_fd(),
            None => root.dir.as_raw_fd(),
        };
        tick();
        // SAFETY: ordinary FFI; `c` outlives the call and `parent` is borrowed from a live `File`.
        // `openat` is variadic — the mode argument is read because `O_CREAT` is set.
        let fd = unsafe {
            libc::openat(
                parent,
                c.as_ptr(),
                libc::O_WRONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_CREAT | libc::O_EXCL,
                0o600 as libc::c_uint,
            )
        };
        if fd < 0 {
            let e = std::io::Error::last_os_error();
            return Err(refuse(
                root,
                Act::Write,
                &sofar,
                &format!("could not be created as a staging file ({e})"),
            ));
        }
        // SAFETY: `fd` is a fresh, owned descriptor this call just created.
        Ok(Opened { file: unsafe { File::from_raw_fd(fd) }, created: true })
    }

    /// [`rename_beneath`](super::rename_beneath)'s arm — CPE-1961.
    ///
    /// **Both operands are single components resolved against one directory handle**, which is the
    /// strongest rename POSIX offers: there is no fd-sourced rename anywhere in Unix (see the public
    /// doc, which states the residual this leaves and which ticket owns it). `staged` is accepted and
    /// unused here so the two platforms share one signature; on Windows it is the source operand.
    pub(super) fn rename(
        root: &RootDir,
        dirs: &[&OsStr],
        from: &OsStr,
        to: &OsStr,
        staged: &File,
    ) -> Result<(), Refusal> {
        let _ = staged; // the Windows arm renames the handle itself; POSIX has no such call
        let mut sofar = PathBuf::new();
        // `Act::Commit`, not `Act::Write`: the parents already exist (the staging file is sitting in
        // them), so a descent that would `mkdirat` a missing one is only ever re-creating a directory
        // something removed under us, and then failing at the rename anyway. See `Act::Commit`.
        let held = descend(root, Act::Commit, dirs, &mut sofar)?;
        sofar.push(to);
        let cf = cname(from).map_err(|()| {
            refuse(root, Act::Commit, &sofar, "contains a NUL byte, which no filesystem name can hold")
        })?;
        let ct = cname(to).map_err(|()| {
            refuse(root, Act::Commit, &sofar, "contains a NUL byte, which no filesystem name can hold")
        })?;
        let parent = match held.as_ref() {
            Some(f) => f.as_raw_fd(),
            None => root.dir.as_raw_fd(),
        };
        tick();
        // SAFETY: ordinary FFI; both C strings outlive the call and `parent` is borrowed from a live
        // `File`. No ownership transfer.
        if unsafe { libc::renameat(parent, cf.as_ptr(), parent, ct.as_ptr()) } != 0 {
            let e = std::io::Error::last_os_error();
            return Err(refuse(
                root,
                Act::Commit,
                &sofar,
                &format!("could not be replaced by the staged copy of it ({e}) [staged as {from:?}]"),
            ));
        }
        Ok(())
    }

    pub(super) fn walk(root: &RootDir, dirs: &[&OsStr], last: &OsStr) -> Result<Opened, Refusal> {
        #[cfg(target_os = "linux")]
        {
            let mut rel = PathBuf::new();
            for d in dirs {
                rel.push(d);
            }
            rel.push(last);
            if let Some((file, created)) = openat2_beneath(root.dir.as_raw_fd(), &rel) {
                return Ok(Opened { file, created });
            }
        }

        let mut sofar = PathBuf::new();
        let held = descend(root, Act::Write, dirs, &mut sofar)?;

        sofar.push(last);
        let c = cname(last).map_err(|()| {
            refuse(root, Act::Write, &sofar, "contains a NUL byte, which no filesystem name can hold")
        })?;
        let parent = match held.as_ref() {
            Some(f) => f.as_raw_fd(),
            None => root.dir.as_raw_fd(),
        };
        let (file, created) = child_file(parent, &c).map_err(|e| {
            // The final component carries no `O_DIRECTORY`, so this one really does come back `ELOOP`
            // on Linux — but it is classified the same way as the directory components above, so this
            // function has one rule rather than two that merely agree today. That also covers
            // **FreeBSD, which returns `EMLINK` for an `O_NOFOLLOW` refusal**: an errno allow-list
            // would have been wrong on a third platform, which is the argument for asking the
            // filesystem instead of the error code.
            if link_at(parent, &c) {
                refuse_link(root, Act::Write, &sofar)
            } else {
                refuse(root, Act::Write, &sofar, &format!("could not be opened for writing ({e})"))
            }
        })?;
        Ok(Opened { file, created })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// The crate's self-cleaning scratch guard — a test here plants directory links, so it removes
    /// its own tree on drop rather than trusting a `remove_dir_all` at the end of a passing run.
    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-beneath-{tag}"))
    }

    /// [`remove_file_beneath`]'s four properties in one place (CPE-1937): it deletes a file inside the
    /// root; it refuses a **link at an interior component**, whichever way that link points; it refuses
    /// the root itself; and — the property that separates it from [`create_beneath`] — it **never
    /// creates** the parent chain on its way to failing.
    ///
    /// The last one is not decoration. A delete whose descent used `FILE_OPEN_IF`/`mkdirat` would
    /// materialise directories inside a user's tree as a *side effect of a failed deletion*, which is
    /// debris behind a destructive operation and exactly the shape CPE-1889's check (1) existed to
    /// stop.
    #[test]
    fn cpe_1937_removes_beneath_the_root_and_never_through_a_link_or_into_thin_air() {
        let d = scratch("unlink");
        let root = open_root(&d, "folder being restored").unwrap();

        // 1. The happy path, one component deep and several.
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::write(d.join("a/b/gone.txt"), b"bye").unwrap();
        std::fs::write(d.join("top.txt"), b"bye").unwrap();
        remove_file_beneath(&root, Path::new("a/b/gone.txt")).unwrap();
        remove_file_beneath(&root, Path::new("top.txt")).unwrap();
        assert!(!d.join("a/b/gone.txt").exists(), "the named file is gone");
        assert!(!d.join("top.txt").exists(), "the named file is gone");
        assert!(d.join("a/b").is_dir(), "and only the file went — its parents are untouched");

        // 2. A missing chain is refused, and NOTHING is created on the way to refusing it.
        let missing = remove_file_beneath(&root, Path::new("no/such/dir/x.txt"));
        assert!(missing.is_err(), "a delete under a chain that does not exist must fail");
        assert!(
            !d.join("no").exists(),
            "a REFUSED delete created directories inside the root — a write's `create_dir_all` \
             descent leaking into the destructive leg: {:?}",
            missing.err()
        );

        // 3. The root itself is not a file inside itself.
        assert!(remove_file_beneath(&root, Path::new("")).is_err());

        // 4. A directory link at an interior component, pointing INSIDE the root (the case
        //    `fsutil::confined_to` answers "yes" to) and OUTSIDE it.
        for point_outside in [true, false] {
            let outside = scratch("unlink-outside");
            let elsewhere =
                if point_outside { outside.to_path_buf() } else { d.join("other") };
            let _ = std::fs::create_dir_all(&elsewhere);
            let elsewhere = std::fs::canonicalize(&elsewhere).unwrap();
            std::fs::write(elsewhere.join("victim.txt"), b"BYSTANDER").unwrap();
            let link = d.join("sub");
            let _ = std::fs::remove_dir_all(&link);
            if !crate::fsutil::make_dir_link(&elsewhere, &link) {
                crate::skip_notice!(
                    "SKIPPING the link leg of cpe_1937_removes_beneath_the_root…: no directory-link \
                     mechanism here. NOTHING on this run covered the delete walking a redirected \
                     component."
                );
                let _ = std::fs::remove_dir_all(&elsewhere);
                continue;
            }
            // Liveness: the link must redirect, through the same name the delete will use.
            assert_eq!(
                std::fs::read(link.join("victim.txt")).ok().as_deref(),
                Some(&b"BYSTANDER"[..]),
                "fixture is inert: the link does not redirect (point_outside={point_outside})"
            );

            let refused = remove_file_beneath(&root, Path::new("sub/victim.txt"))
                .expect_err("a link at an interior component must stop the delete");
            // HARM, off the filesystem, before the refusal's wording is looked at.
            assert_eq!(
                std::fs::read(elsewhere.join("victim.txt")).ok().as_deref(),
                Some(&b"BYSTANDER"[..]),
                "HARM: the delete went through the link (point_outside={point_outside})"
            );
            assert!(refused.policy, "refusing a link is a VERDICT, not an I/O failure: {refused:?}");
            assert!(
                refused.why.contains("is a link"),
                "the refusal must name the link as the cause: {refused:?}"
            );
            assert!(
                refused.why.contains("Nothing was deleted for this entry"),
                "a refused DELETE must not be reported in the vocabulary of a write: {refused:?}"
            );
            let _ = std::fs::remove_dir_all(&link);
            let _ = std::fs::remove_dir_all(&elsewhere);
        }

        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1937 round 2 — the LEAF is containment, and this is the test that says so in CI.**
    ///
    /// PR #1059's Security Auditor ran the whole non-ignored suite with `unlinkat` replaced by a
    /// by-path `fs::remove_file`, descent untouched, and got **2406 passed / 0 failed** — while the
    /// `#[ignore]`d race harness caught 141 destroyed bystanders in 200 trials. The leaf had no CI
    /// coverage at all, and the PR narrative had gone further and called it unreachable.
    ///
    /// The race is what a real attacker does; this makes the same window **deterministic**. The seam
    /// fires once, exactly between the descent and the leaf — where the auditor measured the swap
    /// landing — and does what the racer was trying to hit by luck: move the real directory aside and
    /// leave a link with the same name in its place. From that instant:
    ///
    /// - a **handle-relative** leaf unlinks inside the directory it already opened → the in-tree file
    ///   goes and the bystander is untouched;
    /// - a **by-path** leaf re-resolves `<root>/sub/target.txt` from the root, walks the new link, and
    ///   destroys the bystander outside the root while reporting success.
    ///
    /// Both halves are asserted on the filesystem, and the fixture's liveness (the name really is a
    /// link when the leaf runs) is asserted too, so a machine that cannot stage the swap reddens or
    /// skips loudly rather than passing on a window that never opened.
    #[test]
    fn cpe_1937_the_leaf_and_not_only_the_descent_contains_the_delete() {
        const BYSTANDER: &[u8] = b"a bystander outside the root";
        let d = scratch("leaf-guard");
        let outside = scratch("leaf-guard-outside");
        let outside_real = std::fs::canonicalize(&outside).unwrap();
        std::fs::write(outside_real.join("target.txt"), BYSTANDER).unwrap();

        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("sub").join("target.txt"), b"in tree").unwrap();

        let root = open_root(&d, "folder being restored").unwrap();

        let swapped_at = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let (sub, moved, flag) = (d.join("sub"), d.join("moved"), swapped_at.clone());
            let outside_for_hook = outside_real.clone();
            BETWEEN_DESCENT_AND_LEAF.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || {
                    // `clippy.toml` bans bare `fs::rename`; here the rename IS the attack under test
                    // and both names are this test's own scratch tree.
                    #[allow(clippy::disallowed_methods)]
                    let moved_ok = std::fs::rename(&sub, &moved).is_ok();
                    if moved_ok && crate::fsutil::make_dir_link(&outside_for_hook, &sub) {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    } else if moved_ok {
                        #[allow(clippy::disallowed_methods)]
                        let _ = std::fs::rename(&moved, &sub);
                    }
                }));
            });
        }

        let verdict = remove_file_beneath(&root, Path::new("sub/target.txt"));

        if !swapped_at.load(std::sync::atomic::Ordering::Relaxed) {
            // Windows refuses to rename a directory whose handle another opener holds without
            // `FILE_SHARE_DELETE`; if the swap could not be staged this run has proven nothing, and
            // saying so is the only honest outcome.
            BETWEEN_DESCENT_AND_LEAF.with(|h| *h.borrow_mut() = None);
            crate::skip_notice!(
                "SKIPPING cpe_1937_the_leaf_and_not_only_the_descent_contains_the_delete: could not \
                 swap the directory for a link mid-delete on this machine. NOTHING on this run \
                 covered the LEAF's containment — only the descent's."
            );
            drop(root);
            let _ = std::fs::remove_dir_all(&d);
            let _ = std::fs::remove_dir_all(&outside);
            return;
        }

        // HARM FIRST, off the filesystem. This is the assertion a by-path leaf fails.
        assert_eq!(
            std::fs::read(outside_real.join("target.txt")).ok().as_deref(),
            Some(BYSTANDER),
            "HARM: the leaf re-resolved the path after the descent and deleted a bystander outside \
             the root; verdict was {verdict:?}"
        );
        // And the positive half: the delete really happened, through the handle, on the object the
        // descent opened. Without this the test would also pass if the leaf simply did nothing.
        assert!(
            verdict.is_ok(),
            "the delete must still succeed — the handle it holds is a perfectly good directory: \
             {verdict:?}"
        );
        assert!(
            !d.join("moved").join("target.txt").exists(),
            "the file the descent's handle addressed must be the one that went"
        );

        drop(root);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&outside);
    }

    /// **CPE-1937 round 2, Security Auditor F2 — a read-only file must still delete.**
    ///
    /// `main` used `fs::remove_file`, which clears `FILE_ATTRIBUTE_READONLY` and retries, so a revert
    /// could always delete a read-only file the plan named. The first cut of this module used plain
    /// `FileDispositionInfo`, which **fails** on one — measured as `Err(policy=false), file survives`
    /// against `std::fs::remove_file -> Ok` on the identical fixture — and, because that classified as
    /// an I/O answer, the user was told to try again at something that would never work.
    ///
    /// `FileDispositionInfoEx` with `IGNORE_READONLY_ATTRIBUTE` is what `std` itself uses and is what
    /// this now does. Unix has no equivalent bit — `unlinkat` never consults one — so the test runs on
    /// every platform with the permission staged the platform's own way, and both must delete.
    /// **CPE-1937 round 2, finding F-R2-1 — the seam must not survive the call that armed it.**
    ///
    /// [`between_descent_and_leaf`] takes the hook, so it fires once *if it is reached*. A call that
    /// ends **before** the seam never reaches it — a refused descent is the easy one — and the hook
    /// then sat armed until some later, unrelated delete ran it:
    ///
    /// ```text
    /// still_armed_after_refused_descent = true
    /// fired_on_next_unrelated_delete    = 1
    /// ```
    ///
    /// Latent in the shipped tests, but the cell is `pub(crate)`, any module's tests can arm it, and
    /// `--test-threads=1` puts them all on one thread — so it crosses tests, and a stray swap inside
    /// someone else's containment fixture is exactly the kind of thing that makes a green meaningless.
    ///
    /// Both halves are asserted: the cell is empty after the refused call (the direct property), and a
    /// following ordinary delete does **not** fire it (the consequence that actually bites). The second
    /// is what makes this test fail without the guard even if the cell were cleared somewhere else by
    /// accident.
    #[test]
    fn cpe_1937_the_test_seam_is_disarmed_even_when_the_call_never_reaches_it() {
        let d = scratch("seam-leak");
        let root = open_root(&d, "folder being restored").unwrap();
        std::fs::write(d.join("ordinary.txt"), b"unrelated").unwrap();

        let fired = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        {
            let fired = fired.clone();
            BETWEEN_DESCENT_AND_LEAF.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || {
                    fired.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }));
            });
        }

        // A descent that refuses: `missing/` is not there, and a delete never creates its parents. The
        // call returns before the seam, so nothing consumes the hook.
        let refused = remove_file_beneath(&root, Path::new("missing/x.txt"));
        assert!(refused.is_err(), "fixture is inert: this call was supposed to refuse at the descent");
        assert_eq!(
            fired.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "fixture is inert: the refused call reached the seam, so it cannot leak one"
        );

        let still_armed = BETWEEN_DESCENT_AND_LEAF.with(|h| h.borrow().is_some());
        assert!(
            !still_armed,
            "the seam survived the call that armed it — the next unrelated delete would fire another \
             test's hook"
        );

        // The consequence, which is the half that actually bites: an ordinary later delete must run
        // untouched.
        remove_file_beneath(&root, Path::new("ordinary.txt")).unwrap();
        assert_eq!(
            fired.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "a leaked seam fired inside an unrelated delete"
        );
        assert!(!d.join("ordinary.txt").exists(), "and that delete must still have happened");

        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cpe_1937_a_read_only_file_is_still_deleted_as_std_would() {
        let d = scratch("unlink-readonly");
        let target = d.join("ro.txt");
        std::fs::write(&target, b"read only").unwrap();
        let mut perms = std::fs::metadata(&target).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&target, perms).unwrap();
        assert!(
            std::fs::metadata(&target).unwrap().permissions().readonly(),
            "fixture is inert: the file is not read-only, so this proves nothing"
        );

        let root = open_root(&d, "folder being restored").unwrap();
        let verdict = remove_file_beneath(&root, Path::new("ro.txt"));

        assert!(
            !target.exists(),
            "HARM (regression): a read-only file the plan named survived a revert that `main` would \
             have deleted — `fs::remove_file` clears the attribute and retries: {verdict:?}"
        );
        assert!(verdict.is_ok(), "and it must be reported as done, not refused: {verdict:?}");

        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1937 round 2, Security Auditor F3 — `Ok` must mean the name is gone NOW.**
    ///
    /// Windows' plain `FileDispositionInfo` is *delete-on-close*: with another handle open (sharing
    /// delete) it returns success and the name **stays in the directory** until the last handle closes.
    /// A revert would then count the action `applied` against a name still sitting in the user's
    /// folder — the report-vs-filesystem divergence this whole ticket exists to close, arriving through
    /// the fix for it. `FILE_DISPOSITION_FLAG_POSIX_SEMANTICS` unlinks immediately, matching `unlinkat`
    /// on the Unix side and `std::fs::remove_file` on this one, so the test is written once for both.
    #[test]
    fn cpe_1937_a_reported_delete_has_left_the_directory_immediately() {
        let d = scratch("unlink-posix");
        let target = d.join("held.txt");
        std::fs::write(&target, b"open elsewhere").unwrap();

        // A second reader, held open ACROSS the delete — the whole point.
        // `std`'s own share mode on Windows is READ|WRITE|DELETE, so this is the ordinary
        // "another program has the file open" case rather than a contrived one.
        let holder = std::fs::File::open(&target)
            .expect("the fixture needs a second handle open across the delete");

        let root = open_root(&d, "folder being restored").unwrap();
        let verdict = remove_file_beneath(&root, Path::new("held.txt"));
        assert!(verdict.is_ok(), "an open reader must not stop the delete: {verdict:?}");

        // The name must be gone WHILE the other handle is still open. `symlink_metadata`, so a link
        // left at the name would also count as "still there".
        let still_there = std::fs::symlink_metadata(&target).is_ok();
        drop(holder);
        assert!(
            !still_there,
            "a delete reported as applied left the name in the folder until the last handle closed — \
             `applied` must not be able to disagree with the directory"
        );

        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn opens_and_creates_the_whole_chain_inside_the_root() {
        let d = scratch("chain");
        let root = open_root(&d, "backup destination").unwrap();
        let mut o = create_beneath(&root, Path::new("a/b/c/file.txt")).unwrap();
        assert!(o.created, "a name that did not exist is reported as created");
        o.file.write_all(b"hello").unwrap();
        drop(o);
        assert_eq!(std::fs::read(d.join("a/b/c/file.txt")).unwrap(), b"hello");

        // Second time round: the chain exists, the file exists, and `created` says so.
        let o = create_beneath(&root, Path::new("a/b/c/file.txt")).unwrap();
        assert!(!o.created, "an existing name is NOT reported as created");
        drop(o);
        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn refuses_a_relative_path_that_is_not_plain() {
        let d = scratch("notplain");
        let root = open_root(&d, "backup destination").unwrap();
        for bad in ["../out.txt", "a/../../out.txt"] {
            let e = create_beneath(&root, Path::new(bad)).unwrap_err();
            assert!(e.why.contains("not a plain relative path"), "{bad}: {e}");
        }
        let e = create_beneath(&root, Path::new("")).unwrap_err();
        assert!(e.why.contains("names the destination root itself"), "{e}");
        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// THE property, staged deterministically: a link sitting at an intermediate component is refused,
    /// and — the half that matters — the file at the far end is untouched.
    #[test]
    fn refuses_a_link_at_an_intermediate_component_and_writes_nothing_through_it() {
        let d = scratch("link");
        let (root_dir, outside) = (d.join("dst"), d.join("outside"));
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("victim.txt"), b"USER DATA").unwrap();
        if !crate::fsutil::make_dir_link(&outside, &root_dir.join("sub")) {
            crate::skip_notice!(
                "SKIPPING refuses_a_link_at_an_intermediate_component: could not stage a directory \
                 link, so NOTHING on this run tested the refusal"
            );
            let _ = std::fs::remove_dir_all(&d);
            return;
        }
        let root = open_root(&std::fs::canonicalize(&root_dir).unwrap(), "backup destination").unwrap();
        let e = create_beneath(&root, Path::new("sub/victim.txt")).unwrap_err();
        assert!(e.why.contains("is a link"), "the refusal must name the cause: {e}");
        assert!(e.why.contains("sub"), "the refusal must name the component: {e}");
        assert_eq!(
            std::fs::read(outside.join("victim.txt")).unwrap(),
            b"USER DATA",
            "HARM: the walk wrote through a link at an intermediate component"
        );
        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// CPE-1896 round 3: a reparse point that is **not a name surrogate** must be traversed, not
    /// refused — the OneDrive Files-On-Demand case, staged for real.
    ///
    /// This test exists because the comment it replaces was wrong. `name_surrogate_at` used to say a
    /// non-surrogate directory reparse point could not be staged on a CI runner without OneDrive,
    /// dedup or ProjFS, so the whole motivation for reading the tag rather than the attribute bit
    /// rested on documentation. It can be staged: a **non-Microsoft** tag (top bit clear) with a
    /// `REPARSE_GUID_DATA_BUFFER` needs no privilege and no filter driver.
    ///
    /// Both directions are asserted in one test on purpose — a junction beside the placeholder, on the
    /// same volume in the same run — because the claim is that the two **diverge**, and either half
    /// alone would pass under a guard that always answered the same way.
    ///
    /// The tag carries bit 28 (`0x1000_0000`, "may be set on a directory"): without it NT refuses the
    /// descent whatever this guard decides, which is the "necessary, not sufficient" limit recorded on
    /// `name_surrogate_at`.
    #[cfg(windows)]
    #[test]
    fn cpe_1896_a_non_surrogate_reparse_point_is_traversed_not_refused() {
        /// Non-Microsoft (bit 31 clear), directory-capable (bit 28 set), NOT a surrogate (bit 29 clear)
        /// — the same shape as `IO_REPARSE_TAG_CLOUD` (`0x9000001A`), which is what OneDrive
        /// Files-On-Demand directory placeholders carry.
        const NON_SURROGATE_DIR_TAG: u32 = 0x1000_1234;

        let d = scratch("surrogate");
        let (placeholder, real) = (d.join("ph"), d.join("real"));
        std::fs::create_dir_all(&placeholder).unwrap();
        std::fs::create_dir_all(&real).unwrap();

        if !crate::fsutil::make_guid_reparse_point(&placeholder, NON_SURROGATE_DIR_TAG, true) {
            crate::skip_notice!(
                "SKIPPING cpe_1896_a_non_surrogate_reparse_point_is_traversed_not_refused: could not \
                 plant a GUID reparse point on this volume. NOTHING on this run covered the \
                 non-surrogate (OneDrive placeholder) case, which is the half that keeps backups to \
                 OneDrive working."
            );
            return;
        }
        // Liveness: the fixture must really carry the reparse attribute, or the test proves nothing —
        // it would just be asserting that an ordinary directory is traversable.
        assert!(
            std::os::windows::fs::MetadataExt::file_attributes(
                &std::fs::symlink_metadata(&placeholder).unwrap()
            ) & 0x400
                != 0,
            "fixture is inert: no FILE_ATTRIBUTE_REPARSE_POINT on the placeholder"
        );

        let root = open_root(&d, "backup destination").unwrap();

        // The non-surrogate placeholder: TRAVERSED. The write lands, inside the root.
        let mut o = create_beneath(&root, Path::new("ph/inside.txt")).expect(
            "a reparse point that is not a name surrogate must be traversed — refusing it is what \
             would stop backups to a OneDrive folder working",
        );
        o.file.write_all(b"landed").unwrap();
        drop(o);

        // The surrogate junction, same volume, same run: REFUSED.
        if crate::fsutil::make_dir_link(&real, &d.join("junc")) {
            let e = create_beneath(&root, Path::new("junc/x.txt")).unwrap_err();
            assert!(
                e.why.contains("is a link"),
                "a junction IS a name surrogate and must still be refused — the two cases have to \
                 diverge, or the tag check is doing nothing: {e}"
            );
        }
        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The **negative** half of the link classification, and it is the half that keeps the positive
    /// half honest: an ordinary file sitting where a directory component should be is refused too, but
    /// must **not** be described as a link.
    ///
    /// This exists because the errno cannot tell the two apart. On Linux and macOS a symlink at a
    /// component opened with `O_DIRECTORY` reports `ENOTDIR` — and so does a plain file at that same
    /// component. An earlier revision classified on `ELOOP`, which is simply the wrong error on both
    /// platforms (and `EMLINK` on FreeBSD); the fix asks `fstatat(AT_SYMLINK_NOFOLLOW)` instead, and
    /// this test is what proves the answer discriminates rather than always saying "link".
    ///
    /// It also guards the wording split that made the sibling assertions meaningful: while
    /// [`refuse`]'s shared tail still contained the phrase "is a link", *every* refusal matched
    /// `contains("is a link")` and both link tests passed for any failure at all.
    #[test]
    fn a_plain_file_where_a_directory_belongs_is_refused_but_not_called_a_link() {
        let d = scratch("notdir");
        std::fs::write(d.join("sub"), b"i am a file, not a directory").unwrap();
        let root = open_root(&d, "backup destination").unwrap();
        let e = create_beneath(&root, Path::new("sub/x.txt")).unwrap_err();
        assert!(
            !e.why.contains("is a link"),
            "a plain file at a directory component must NOT be reported as a link — that is the \
             classification this module gets from `fstatat`, not from the errno, because both cases \
             report ENOTDIR: {e}"
        );
        assert!(e.why.contains("sub"), "the refusal must still name the component: {e}");
        assert!(!d.join("sub").is_dir(), "nothing may have replaced the file with a directory");
        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The final component is a link: refused too, and by the open itself rather than by a check
    /// afterwards. (On Windows the reparse-point handle opens; `fsutil` refuses it off the handle. Here
    /// we only assert that nothing reached the far end.)
    #[test]
    fn a_link_at_the_final_component_never_reaches_its_target() {
        let d = scratch("leaflink");
        let (root_dir, outside) = (d.join("dst"), d.join("outside"));
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("victim.txt"), b"USER DATA").unwrap();
        if !crate::fsutil::make_dir_link(&outside, &root_dir.join("junc")) {
            crate::skip_notice!(
                "SKIPPING a_link_at_the_final_component_never_reaches_its_target: could not stage a \
                 directory link"
            );
            let _ = std::fs::remove_dir_all(&d);
            return;
        }
        let root = open_root(&std::fs::canonicalize(&root_dir).unwrap(), "backup destination").unwrap();
        // `junc` is a directory link; asking for it as a FILE must not end up writing into the far
        // side's directory under any name.
        let _ = create_beneath(&root, Path::new("junc"));
        assert_eq!(std::fs::read(outside.join("victim.txt")).unwrap(), b"USER DATA");
        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// CPE-1896 acceptance criterion 5. Not an assertion on a number — a machine-dependent syscall
    /// budget asserted in CI is the "test that reds at random" this repo keeps deleting. It prints the
    /// exact count the walk performed, which is what the ticket asks to be *known*.
    #[test]
    fn cpe_1896_report_the_walk_syscall_cost() {
        fn count() -> u64 {
            WALK_SYSCALLS.with(std::cell::Cell::get)
        }

        let d = scratch("cost");
        let root = open_root(&d, "backup destination").unwrap();
        std::fs::create_dir_all(d.join("a/b")).unwrap();

        // Depth 3 (`a/b/f`), the shape of an ordinary backup entry, with the chain already present —
        // the common case, every entry after the first in a directory.
        let before = count();
        for i in 0..100 {
            drop(create_beneath(&root, Path::new(&format!("a/b/f{i}.txt"))).unwrap());
        }
        let warm = count() - before;

        // The same entries again, now that every file also already exists (the `update` list).
        let before = count();
        for i in 0..100 {
            drop(create_beneath(&root, Path::new(&format!("a/b/f{i}.txt"))).unwrap());
        }
        let overwrite = count() - before;

        let _ = writeln!(
            std::io::stderr(),
            "CPE-1896 walk cost, depth 3 (a/b/name): {} syscalls/file creating a new name, {} \
             syscalls/file overwriting an existing one, over 100 files each. Compare: the path-based \
             guard this replaces cost two `canonicalize` calls per file (each of which is itself an \
             open + a final-path query + a close on Windows) plus one `metadata` plus one open.",
            warm as f64 / 100.0,
            overwrite as f64 / 100.0
        );
        drop(root);
        let _ = std::fs::remove_dir_all(&d);
    }
}
