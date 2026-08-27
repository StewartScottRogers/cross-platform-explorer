//! Open a file **beneath** a chosen root so that the containment is atomic with the open, one path
//! component at a time (CPE-1896, acceptance criterion 1).
//!
//! # The problem this exists to remove, not to narrow
//!
//! Every other containment guard in this crate — [`crate::fsutil::confined_to`],
//! [`crate::fsutil::contained_under`], `backup::parent_contained`, `backup::landed_inside` — asks a
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
//! | other    | [`crate::batch_media::open_no_follow`] by path — **not** atomic, see [`ATOMIC`] | no |
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
//! - **On a platform with neither `openat` nor `NtCreateFile` it is not atomic at all** — see [`ATOMIC`].

use std::ffi::OsStr;
use std::fs::File;
use std::path::{Component, Path, PathBuf};

/// Whether the per-component walk on this target is genuinely atomic with the open.
///
/// `true` on Unix and Windows — every platform this app ships on. `false` on anything else, where the
/// fallback below opens by path and a caller that has its own path-based containment checks must keep
/// running them. Exposed as a `const` rather than a `cfg` so the caller reads as ordinary code and both
/// branches type-check on every target: `backup::copy_one_verified` guards its pre-write
/// `parent_contained` calls with `if !ATOMIC`, and the optimiser deletes the dead half.
pub(crate) const ATOMIC: bool = cfg!(any(unix, windows));

/// A destination root, resolved and **held open** for the life of a run.
///
/// Holding the handle is not an optimisation, it is the anchor: every write in the run is resolved
/// against *this object*, so renaming or replacing the root's name mid-run cannot redirect a single
/// entry. It also removes one path resolution per file from the inner loop, which is the cost half of
/// CPE-1896's acceptance criterion 5.
pub(crate) struct RootDir {
    /// The already-canonicalised root path. Kept for error messages only — never re-opened.
    path: PathBuf,
    /// The open directory handle every component is resolved against. Absent only on a platform with
    /// no handle-relative open, where [`ATOMIC`] is `false` and the fallback opens by path.
    #[cfg(any(unix, windows))]
    dir: File,
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
pub(crate) fn open_root(real_root: &Path) -> std::io::Result<RootDir> {
    #[cfg(any(unix, windows))]
    {
        Ok(RootDir { path: real_root.to_path_buf(), dir: sys::open_root_dir(real_root)? })
    }
    #[cfg(not(any(unix, windows)))]
    {
        if !real_root.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "the backup destination is not a directory",
            ));
        }
        Ok(RootDir { path: real_root.to_path_buf() })
    }
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
/// # Errors
///
/// A refusal names the component it stopped at, relative to the root, and says whether the component
/// was a link (the attack shape) or simply could not be opened (a permission, sharing or vanished-name
/// problem). Both are refusals: this module never guesses.
pub(crate) fn create_beneath(root: &RootDir, rel: &Path) -> Result<Opened, String> {
    let mut parts: Vec<&OsStr> = Vec::new();
    for c in rel.components() {
        match c {
            Component::Normal(p) => parts.push(p),
            _ => {
                return Err(format!(
                    "refusing to open {rel:?} inside {:?}: it is not a plain relative path, so it \
                     cannot be resolved one component at a time",
                    root.path
                ))
            }
        }
    }
    let Some((last, dirs)) = parts.split_last() else {
        return Err(format!(
            "refusing to open {rel:?} inside {:?}: it names the destination root itself, not a file \
             inside it",
            root.path
        ));
    };
    sys::walk(root, dirs, last)
}

/// The refusal wording, shared by every arm so the sentence a user sees does not depend on which
/// platform refused. `at` is the failing component's path **relative to the root**, which is the part
/// the user can act on.
fn refuse(root: &Path, at: &Path, why: &str) -> String {
    format!(
        "refusing to write inside the backup destination {root:?}: the path component {at:?} {why}. \
         Nothing was written for this entry — each component is opened relative to the one before it \
         so that nothing can be swapped in underneath the write, and a component that is a link, or \
         that cannot be opened, stops the entry rather than being resolved."
    )
}

/// The wording for the one case that is an attack rather than an accident, kept separate so it reads
/// as the specific finding it is.
const WHY_LINK: &str =
    "is a link (a symlink, junction or other reparse point), and a link inside a backup destination \
     redirects the write to wherever it points";

// ---------------------------------------------------------------------------------------------
// Windows: NtCreateFile with RootDirectory = the parent handle.
// ---------------------------------------------------------------------------------------------
#[cfg(windows)]
mod sys {
    use super::{refuse, tick, Opened, RootDir, WHY_LINK};
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
        let bytes = u16::try_from(wide.len().saturating_mul(2)).unwrap_or(u16::MAX);
        let mut us = UNICODE_STRING {
            Length: bytes,
            MaximumLength: bytes,
            Buffer: windows::core::PWSTR(wide.as_mut_ptr()),
        };
        let oa = OBJECT_ATTRIBUTES {
            Length: u32::try_from(std::mem::size_of::<OBJECT_ATTRIBUTES>()).unwrap_or(0),
            RootDirectory: HANDLE(parent.as_raw_handle() as isize),
            ObjectName: &mut us,
            Attributes: OBJ_CASE_INSENSITIVE,
            SecurityDescriptor: std::ptr::null(),
            SecurityQualityOfService: std::ptr::null(),
        };
        tick();
        // SAFETY: `oa` borrows `us`, which borrows `wide`; all three outlive the call. `parent` is a
        // live `File`, so its handle is valid and is only *borrowed* by `RootDirectory` — NT does not
        // take ownership of it. `h` and `iosb` are correctly-sized out-parameters. On success the
        // returned handle is wrapped in a `File` exactly once, which is what closes it.
        unsafe {
            let mut h = HANDLE::default();
            let mut iosb: IO_STATUS_BLOCK = std::mem::zeroed();
            let status = NtCreateFile(
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
            );
            if status.is_ok() {
                Ok(File::from_raw_handle(h.0 as RawHandle))
            } else {
                Err(status)
            }
        }
    }

    /// NT status codes are not Win32 error codes, and `io::Error` speaks Win32. Translating means a
    /// refusal reads as "Access is denied." rather than as `0xC0000022`.
    fn io_err(status: NTSTATUS) -> std::io::Error {
        // SAFETY: a pure value translation in ntdll; no pointers, no ownership.
        let win32 = unsafe { RtlNtStatusToDosError(status) };
        std::io::Error::from_raw_os_error(win32 as i32)
    }

    pub(super) fn walk(root: &RootDir, dirs: &[&OsStr], last: &OsStr) -> Result<Opened, String> {
        let mut held: Option<File> = None;
        let mut sofar = PathBuf::new();

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
                refuse(&root.path, &sofar, &format!("could not be opened ({})", io_err(s)))
            })?;
            // `FILE_OPEN_REPARSE_POINT` means a junction here was opened **as the reparse point
            // itself** rather than followed, so nothing has escaped — but continuing through it would
            // put the file in the junction's own physical directory, which is contained and invisible
            // through the path the user sees. Refuse instead: this is the measured attack shape, and a
            // backup that silently writes somewhere the user cannot find is its own defect.
            //
            // One `GetFileInformationByHandle` per directory component, on a handle already open. It
            // is the only per-component cost this walk adds over the opens themselves; there is no way
            // to ask NT "and fail if it was a reparse point" as part of the create.
            tick();
            if crate::batch_media::handle_facts(&dir).is_some_and(|f| f.is_reparse_point) {
                return Err(refuse(&root.path, &sofar, WHY_LINK));
            }
            held = Some(dir);
        }

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
                    &root.path,
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
    use super::{refuse, tick, Opened, RootDir, WHY_LINK};
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

    /// Open (creating if absent) one directory component relative to `parent`.
    ///
    /// `O_NOFOLLOW` is what makes it atomic: if the name is a symlink the **open itself** fails with
    /// `ELOOP`, so there is no window in which the name could be checked and then followed.
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
                mode: 0o666,
                resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS,
            };
            tick();
            // SAFETY: `SYS_openat2` takes (dirfd, path, *const open_how, size). `c` and `how` outlive
            // the call; the size passed matches the struct the kernel is given.
            let r = unsafe {
                libc::syscall(
                    libc::SYS_openat2,
                    rootfd,
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

    pub(super) fn walk(root: &RootDir, dirs: &[&OsStr], last: &OsStr) -> Result<Opened, String> {
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

        let mut held: Option<File> = None;
        let mut sofar = PathBuf::new();
        for name in dirs {
            sofar.push(name);
            let c = cname(name).map_err(|()| {
                refuse(&root.path, &sofar, "contains a NUL byte, which no filesystem name can hold")
            })?;
            let parent = match held.as_ref() {
                Some(f) => f.as_raw_fd(),
                None => root.dir.as_raw_fd(),
            };
            let dir = child_dir(parent, &c).map_err(|e| {
                // `ELOOP` here is `O_NOFOLLOW` refusing a symlink at this component — the measured
                // attack shape — and it deserves the sentence that says so rather than an errno.
                if e.raw_os_error() == Some(libc::ELOOP) {
                    refuse(&root.path, &sofar, WHY_LINK)
                } else {
                    refuse(&root.path, &sofar, &format!("could not be opened ({e})"))
                }
            })?;
            held = Some(dir);
        }

        sofar.push(last);
        let c = cname(last).map_err(|()| {
            refuse(&root.path, &sofar, "contains a NUL byte, which no filesystem name can hold")
        })?;
        let parent = match held.as_ref() {
            Some(f) => f.as_raw_fd(),
            None => root.dir.as_raw_fd(),
        };
        let (file, created) = child_file(parent, &c).map_err(|e| {
            if e.raw_os_error() == Some(libc::ELOOP) {
                refuse(&root.path, &sofar, WHY_LINK)
            } else {
                refuse(&root.path, &sofar, &format!("could not be opened for writing ({e})"))
            }
        })?;
        Ok(Opened { file, created })
    }
}

// ---------------------------------------------------------------------------------------------
// Anything else: no handle-relative open exists, so this is the pre-CPE-1896 path-based behaviour.
// ---------------------------------------------------------------------------------------------
#[cfg(not(any(unix, windows)))]
mod sys {
    use super::{refuse, Opened, RootDir};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    pub(super) fn walk(root: &RootDir, dirs: &[&OsStr], last: &OsStr) -> Result<Opened, String> {
        // Not atomic, and [`super::ATOMIC`] says so, so the caller has kept its path-based containment
        // checks running. Nothing here pretends otherwise.
        let mut path = root.path.to_path_buf();
        let mut rel = PathBuf::new();
        for d in dirs {
            path.push(d);
            rel.push(d);
        }
        if !dirs.is_empty() {
            std::fs::create_dir_all(&path)
                .map_err(|e| refuse(root.path, &rel, &format!("could not be created ({e})")))?;
        }
        path.push(last);
        rel.push(last);
        let (file, created) = crate::batch_media::open_no_follow(&path)
            .map_err(|e| refuse(root.path, &rel, &format!("could not be opened for writing ({e})")))?;
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
        let root = open_root(&d).unwrap();
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
        let root = open_root(&d).unwrap();
        for bad in ["../out.txt", "a/../../out.txt"] {
            let e = create_beneath(&root, Path::new(bad)).unwrap_err();
            assert!(e.contains("not a plain relative path"), "{bad}: {e}");
        }
        let e = create_beneath(&root, Path::new("")).unwrap_err();
        assert!(e.contains("names the destination root itself"), "{e}");
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
        let root = open_root(&std::fs::canonicalize(&root_dir).unwrap()).unwrap();
        let e = create_beneath(&root, Path::new("sub/victim.txt")).unwrap_err();
        assert!(e.contains("is a link"), "the refusal must name the cause: {e}");
        assert!(e.contains("sub"), "the refusal must name the component: {e}");
        assert_eq!(
            std::fs::read(outside.join("victim.txt")).unwrap(),
            b"USER DATA",
            "HARM: the walk wrote through a link at an intermediate component"
        );
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
        let root = open_root(&std::fs::canonicalize(&root_dir).unwrap()).unwrap();
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
        let root = open_root(&d).unwrap();
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
