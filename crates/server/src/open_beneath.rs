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

/// The one place `rel` is turned into components, so [`create_beneath`] and [`create_dir_beneath`]
/// cannot come to disagree about what a legal relative path is.
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
fn refuse(root: &RootDir, at: &Path, why: &str) -> Refusal {
    // **The tail must not contain the words any test uses to identify a CAUSE.** It used to end
    // "…a component that is a link, or that cannot be opened, stops the entry", which put the literal
    // phrase `is a link` into *every* refusal this module produces — so
    // `assert!(err.contains("is a link"))` passed for a permission error, a vanished name, or a plain
    // file sitting where a directory should be. Two tests were asserting exactly that and proving
    // nothing; the Linux harness for PR #1043 round 2 caught it by asserting the *negative* case.
    // Keep boilerplate and diagnosis lexically disjoint.
    Refusal::failure(format!(
        "refusing to write inside the {noun} {path:?}: the path component {at:?} {why}. \
         Nothing was written for this entry — each component is opened relative to the one before it \
         so that nothing can be swapped in underneath the write, and any component that cannot be \
         opened, or that stands in for another name, stops the entry rather than being resolved.",
        noun = root.noun,
        path = root.path,
    ))
}

/// The one case that is an attack rather than an accident, worded separately so it reads as the
/// specific finding it is — and flagged `policy: true`, because refusing a link is a **verdict**, not
/// an I/O failure. That distinction is what lets `archive` count it as a skip and `transfer` keep the
/// rest of the tree, rather than every caller having to recognise this sentence (CPE-1913).
fn refuse_link(root: &RootDir, at: &Path) -> Refusal {
    Refusal {
        why: refuse(
            root,
            at,
            &format!(
                "is a link (a symlink, junction or other reparse point), and a link inside the {} \
                 redirects the write to wherever it points",
                root.noun
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
    use super::{refuse, refuse_link, tick, Opened, Refusal, RootDir};
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
    use windows::Win32::Foundation::{RtlNtStatusToDosError, HANDLE, NTSTATUS, UNICODE_STRING};
    use windows::Win32::Storage::FileSystem::{
        FILE_ACCESS_RIGHTS, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FILE_TRAVERSE, SYNCHRONIZE,
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
        // `unwrap_or(false)` — fail OPEN. See the shared helper's doc for why this caller takes the
        // opposite default from the final-component guard in `fsutil`.
        crate::batch_media::reparse_name_surrogate(dir).unwrap_or(false)
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
    fn descend(root: &RootDir, dirs: &[&OsStr], sofar: &mut PathBuf) -> Result<Option<File>, Refusal> {
        let mut held: Option<File> = None;

        for name in dirs {
            sofar.push(name);
            let parent = held.as_ref().unwrap_or(&root.dir);
            // `FILE_OPEN_IF` = open it, or create it if it is not there — the per-component equivalent
            // of `create_dir_all`, except that it can only ever create *inside the handle we hold*, so
            // a refused entry cannot leave directory debris outside the root the way a path-based
            // `create_dir_all` walking a junction did (CPE-1889 check (1)'s whole reason to exist).
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
                FILE_OPEN_IF,
                NTCREATEFILE_CREATE_OPTIONS(
                    FILE_DIRECTORY_FILE.0 | FILE_OPEN_REPARSE_POINT.0 | FILE_SYNCHRONOUS_IO_NONALERT.0,
                ),
            )
            .map_err(|s| {
                refuse(root, sofar, &format!("could not be opened ({})", io_err(s)))
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
                return Err(refuse_link(root, sofar));
            }
            held = Some(dir);
        }
        Ok(held)
    }

    /// Every component of `parts` as a directory — [`create_dir_beneath`](super::create_dir_beneath)'s
    /// arm. The handle is dropped on return; the directory's existence is the product.
    pub(super) fn walk_dirs(root: &RootDir, parts: &[&OsStr]) -> Result<(), Refusal> {
        let mut sofar = PathBuf::new();
        descend(root, parts, &mut sofar).map(|_| ())
    }

    pub(super) fn walk(root: &RootDir, dirs: &[&OsStr], last: &OsStr) -> Result<Opened, Refusal> {
        let mut sofar = PathBuf::new();
        let held = descend(root, dirs, &mut sofar)?;

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
                Err(s) => Err(refuse(
                    root,
                    &sofar,
                    &format!("could not be opened for writing ({})", io_err(s)),
                )),
            },
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Unix: openat/mkdirat with O_NOFOLLOW, plus openat2(RESOLVE_BENEATH) as a Linux fast path.
// ---------------------------------------------------------------------------------------------
#[cfg(unix)]
mod sys {
    use super::{refuse, refuse_link, tick, Opened, Refusal, RootDir};
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
    fn child_dir(parent: RawFd, name: &CString) -> std::io::Result<File> {
        tick();
        // SAFETY: `parent` is borrowed from a live `File`; `name` is a NUL-terminated C string that
        // outlives the call. Both syscalls are ordinary FFI with no ownership transfer.
        let made = unsafe { libc::mkdirat(parent, name.as_ptr(), 0o777) };
        if made != 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(e);
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
    fn descend(root: &RootDir, dirs: &[&OsStr], sofar: &mut PathBuf) -> Result<Option<File>, Refusal> {
        let mut held: Option<File> = None;
        for name in dirs {
            sofar.push(name);
            let c = cname(name).map_err(|()| {
                refuse(root, sofar, "contains a NUL byte, which no filesystem name can hold")
            })?;
            let parent = match held.as_ref() {
                Some(f) => f.as_raw_fd(),
                None => root.dir.as_raw_fd(),
            };
            let dir = child_dir(parent, &c).map_err(|e| {
                // Classified by asking the filesystem, NOT by reading the errno — see [`link_at`].
                // A symlink at an intermediate component reports `ENOTDIR` on Linux and macOS, and so
                // does a plain file sitting where a directory should be; they need different
                // sentences and the errno cannot tell them apart.
                if link_at(parent, &c) {
                    refuse_link(root, sofar)
                } else {
                    refuse(root, sofar, &format!("could not be opened ({e})"))
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
        descend(root, parts, &mut sofar).map(|_| ())
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
        let held = descend(root, dirs, &mut sofar)?;

        sofar.push(last);
        let c = cname(last).map_err(|()| {
            refuse(root, &sofar, "contains a NUL byte, which no filesystem name can hold")
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
                refuse_link(root, &sofar)
            } else {
                refuse(root, &sofar, &format!("could not be opened for writing ({e})"))
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
