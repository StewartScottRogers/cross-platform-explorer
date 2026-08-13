//! Provider-agnostic recursive walk + tree transfer (CPE-684/905, epic CPE-616). These operate over the
//! [`FileSystemProvider`] trait, so **every** backend — local disk, SFTP, WebDAV, … — gets a cancellable
//! recursive walk and bidirectional (remote⇄local) tree copy for free, with the logic living once here
//! instead of duplicated per provider.
//!
//! Paths are the provider's own convention (`/`-separated for remote backends; an empty `root` means the
//! provider's root). Every step checks a `cancel` flag so a slow/large enumeration or transfer stops
//! promptly.

use crate::provider::FileSystemProvider;
use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// One entry yielded by [`walk`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    /// Full path within the provider.
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Maximum directory depth [`walk`] will descend into (CPE-1462). A hostile server can advertise an
/// infinitely deep tree — one fresh child directory per `readdir` — which would grow the DFS stack (and,
/// for [`download_tree`], the accumulated work) without bound. A legitimate remote tree is only ever a few
/// dozen levels deep, so 100 sits far above anything real while firmly bounding recursion. Reaching it
/// stops descent into *deeper* directories (surfaced as a stderr notice) rather than failing the whole
/// transfer, matching the repo's skip-on-error ethos for enumeration.
pub const MAX_WALK_DEPTH: usize = 100;

/// Maximum total entries [`walk`] will visit (CPE-1462). A hostile server can advertise millions of
/// entries — by breadth (one directory with millions of children) or depth — to exhaust memory/time on an
/// unattended transfer. Hundreds of thousands comfortably covers any legitimate large tree; exceeding it
/// aborts the walk with a surfaced error, because a bounded, failed transfer is vastly preferable to an
/// OOM or an indefinite hang.
pub const MAX_WALK_ENTRIES: usize = 500_000;

/// Whether a provider-supplied entry **name** is a safe single path segment (CPE-1461, source-side
/// defense). A name is a leaf, never a path: it must be non-empty, not `.`/`..`, contain neither a `/`
/// nor `\` separator nor a NUL, and be a single *normal* path component (rejecting a bare drive like
/// `C:`, a root, or any other prefix). Remote providers (SFTP `READDIR` filenames, WebDAV `href`
/// segments) call this to drop a hostile name at the SOURCE, before it can ever reach the local-write
/// sink in [`download_tree`]. A directory entry with an unsafe name is skipped entirely.
pub fn is_safe_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return false;
    }
    // Reject a Windows NTFS alternate-data-stream / drive selector anywhere in the leaf (`x:y`,
    // `..::$DATA`, `file:stream`) — a `:` in a name is meaningless on Unix and dangerous on Windows,
    // so fail closed. Also reject a leaf that *begins* with `..` (`..stream`, `..:$DATA`), which the
    // single-component check below would otherwise accept as `Normal` (CPE-1461 hardening).
    if name.contains(':') || name.starts_with("..") {
        return false;
    }
    // Exactly one normal component: rejects a bare drive (`C:`), a root, or any prefix, which
    // `Path::components` classifies as non-`Normal`.
    let mut comps = Path::new(name).components();
    matches!((comps.next(), comps.next()), (Some(Component::Normal(_)), None))
}

/// Characters a Windows path cannot carry in a leaf name (CPE-1709). `/` and `\` are absent on purpose —
/// [`guarded_join`] has already split the path on both, so they can never appear inside a segment.
///
/// Measured behaviour of each, written through the real sink (`download_tree` → `guarded_join` →
/// `fs::write`) on Windows 11 Pro 26200, NTFS:
/// - `:` — **the reported bug.** The write *succeeds* and the bytes go into an NTFS alternate data
///   stream: `colon:name.txt` leaves a **0-byte** file named `colon` plus a hidden stream
///   `colon:name.txt:$DATA` holding the content (`dir /r` shows both). Silent total loss to the user.
/// - `< > " | ? *` and control characters `U+0000`–`U+001F` — `CreateFileW` refuses them outright (os
///   error 123). The entry was already skipped before this fix, but only *accidentally* and with a
///   misleading notice: the CPE-1696 leaf `symlink_metadata` probe was the call that failed, so the
///   transfer reported "could not be inspected for a pre-existing symlink". Encoding them turns a
///   wrongly-diagnosed skip into a correct download.
const WINDOWS_UNSAFE_CHARS: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

/// The DOS device names Windows still reserves in every directory (CPE-1709), matched
/// case-insensitively against the name's stem — Windows reserves `CON.txt` exactly as it reserves `CON`.
///
/// Measured on Windows 11 Pro 26200 (scoped claim — this is version-dependent, which is precisely why
/// the fix does not rely on it): with the download root canonicalized to a `\\?\` verbatim path, as
/// [`download_tree`] does, every one of these was *created* as a real 5-byte file. Reading them back by
/// ordinary Win32 name (`cmd /c type`) then returned the real contents for `CON`, `PRN`, `AUX`,
/// `CON.txt`, `nul.txt` and `COM1`–`COM9` / `LPT1`–`LPT9` — but **`NUL` returned nothing at all**: the
/// name still resolves to the null device, so a user-visible 5-byte file reads as empty in every
/// application. That is the same silent-loss shape as the colon, reached by a different mechanism.
///
/// `NUL` was the only one that lost data *on this build*, but device-name resolution has moved between
/// Windows releases and is not a property we control, so all of them are escaped rather than only the
/// one that happened to misbehave here.
const WINDOWS_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Percent-escape one character as `%XX` (upper-case hex). Only ever called for ASCII.
fn push_escaped(out: &mut String, c: char) {
    out.push('%');
    let b = c as u32;
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[((b >> 4) & 0xF) as usize] as char);
    out.push(HEX[(b & 0xF) as usize] as char);
}

/// True at `i` if `chars[i..]` begins a `%HH` escape sequence.
fn is_escape_at(chars: &[char], i: usize) -> bool {
    chars[i] == '%'
        && i + 2 < chars.len()
        && chars[i + 1].is_ascii_hexdigit()
        && chars[i + 2].is_ascii_hexdigit()
}

/// Rewrite one path segment into a name a **Windows** filesystem can actually hold and an ordinary
/// Windows application can actually open (CPE-1709). Pure and platform-independent so the whole rule is
/// unit-testable on all three CI OSes; [`local_safe_segment`] decides *when* to apply it.
///
/// Four passes, in order:
/// 1. A `%` that is followed by two hex digits becomes `%25`. This is the escape-the-escape step, and it
///    is the entire reason the mapping is safe (see the injectivity note below). A `%` **not** followed
///    by two hex digits is left alone, so ordinary names like `50% off.txt` are untouched.
/// 2. Every [`WINDOWS_UNSAFE_CHARS`] character and every control character becomes `%XX`.
/// 3. A trailing run of `.` or space becomes `%2E` / `%20`. Measured: with a verbatim root such a file
///    *is* created — `download_tree` reported `Ok(1)` and `read_dir` showed `trailingdot.` at 5 bytes —
///    but nothing can open it by name afterwards, because every non-verbatim Win32 path strips the
///    trailing run first (`cmd /c type` → "The system cannot find the file specified", Rust's
///    `fs::read` → os error 2). Full bytes on disk, unreachable: the colon bug's twin.
/// 4. If the resulting stem (up to the first `.`) is a [`WINDOWS_DEVICE_NAMES`] entry, its first
///    character is escaped — `CON` → `%43ON`, `COM1.txt` → `%43OM1.txt`.
///
/// **Two distinct names can never collide onto one local file**, which is the property that stops this
/// fix from replacing a silent-loss bug with a different silent-loss bug. The mapping is *injective*
/// because [`decode_windows_safe_segment`] inverts it exactly: the output never contains a `%HH`
/// sequence that did not come from an escape this function emitted. Pass 1 removes every pre-existing
/// one, and no new one can form afterwards, since an escape a later pass emits starts with `%` (never a
/// hex digit) and a literal `%` that survived pass 1 provably has no two hex digits after it in the
/// output. `decode(encode(x)) == x` for every `x` therefore forces `encode(x) == encode(y) → x == y`.
/// `transfer_windows_name_mapping_is_injective` asserts both halves on a table that includes the
/// adversarial `%`-bearing cases.
///
/// (Two remote names that differ only in **case** — `A.txt` and `a.txt` — still land on one file on a
/// case-insensitive NTFS volume. That is a pre-existing property of the platform, unchanged by and
/// independent of this mapping, which is case-preserving; it is recorded here rather than fixed, since
/// no leaf-name rewriting can address it.)
pub fn windows_safe_segment(name: &str) -> Cow<'_, str> {
    let needs = name.chars().any(|c| WINDOWS_UNSAFE_CHARS.contains(&c) || c.is_control() || c == '%')
        || name.ends_with('.')
        || name.ends_with(' ')
        || is_windows_device_name(name);
    if !needs {
        return Cow::Borrowed(name);
    }

    let chars: Vec<char> = name.chars().collect();
    // Index at which the trailing `.`/space run starts (pass 3).
    let keep = chars.len() - chars.iter().rev().take_while(|c| **c == '.' || **c == ' ').count();

    let mut out = String::with_capacity(name.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        if is_escape_at(&chars, i) {
            out.push_str("%25"); // pass 1
        } else if WINDOWS_UNSAFE_CHARS.contains(&c) || c.is_control() || i >= keep {
            push_escaped(&mut out, c); // passes 2 + 3
        } else {
            out.push(c);
        }
    }

    // Pass 4, evaluated on the ALREADY-encoded name so an escape this function just emitted can never be
    // re-flagged: a literal `%43ON` encodes to `%2543ON`, whose stem is not a device name.
    if is_windows_device_name(&out) {
        let mut first = out.chars();
        let c = first.next().expect("a device name is never empty");
        let mut escaped = String::with_capacity(out.len() + 2);
        push_escaped(&mut escaped, c);
        escaped.push_str(first.as_str());
        out = escaped;
    }
    Cow::Owned(out)
}

/// Whether `name`'s stem (up to the first `.`) is a reserved DOS device name, case-insensitively.
fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("");
    WINDOWS_DEVICE_NAMES.iter().any(|d| d.eq_ignore_ascii_case(stem))
}

/// The exact inverse of [`windows_safe_segment`] — recovers the remote name from the local one
/// (CPE-1709). Shipped, not test-only: it is what makes the mapping *demonstrably* reversible rather
/// than merely claimed to be, and it is the proof obligation behind the injectivity argument, so it must
/// live next to the encoder and be exercised by the same table.
pub fn decode_windows_safe_segment(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len());
    let mut i = 0;
    while i < chars.len() {
        if is_escape_at(&chars, i) {
            let hi = chars[i + 1].to_digit(16).unwrap_or(0);
            let lo = chars[i + 2].to_digit(16).unwrap_or(0);
            if let Some(c) = char::from_u32(hi * 16 + lo) {
                out.push(c);
            }
            i += 3;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Apply the local platform's leaf-name rule to one segment (CPE-1709). On Windows that is
/// [`windows_safe_segment`]; everywhere else the name is passed through untouched, because `:`, `*`,
/// `?`, a trailing dot and `CON` are all perfectly ordinary bytes in a Unix filename and rewriting them
/// would mangle legitimate names for no reason. This is why the sink — not any provider guard — is the
/// right place for the rule: the rule belongs to the local filesystem, and every provider that can
/// legally produce such a name arrives here.
///
/// `cfg!` rather than `#[cfg]` deliberately: both arms compile on every OS, so the encoder is never
/// dead code on Unix and its tests run on all three CI legs.
pub fn local_safe_segment(name: &str) -> Cow<'_, str> {
    if cfg!(windows) {
        windows_safe_segment(name)
    } else {
        Cow::Borrowed(name)
    }
}

/// Join an untrusted, provider-supplied relative path `rel` onto `base`, guaranteeing the result stays
/// inside `base` (CPE-1461, sink-side defense). The path is rebuilt segment-by-segment — splitting on
/// BOTH `/` and `\` so a Windows-style separator is neutralized on every OS — keeping only plain `Normal`
/// components. Any `..` segment, or any segment that is itself a root/drive/UNC prefix, makes the whole
/// entry unsafe and yields `None` (the caller skips it, and must NOT create parent directories for it).
/// Because only `Normal` segments are ever appended to `base`, the returned path is always lexically
/// contained in `base`. Callers additionally verify the on-disk parent canonicalizes back under `base`,
/// as a defense against a pre-existing symlink inside the download root.
pub fn guarded_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut out = base.to_path_buf();
    let mut pushed = false;
    for seg in rel.split(['/', '\\']) {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            return None; // parent-dir escape
        }
        // CPE-1709: rewrite the segment into something the LOCAL filesystem can actually hold before it
        // is checked or pushed. Without this a `:` sailed through as a `Normal` component and
        // `fs::write` diverted the bytes into an NTFS alternate data stream, leaving the user a 0-byte
        // file. Applied here, at the sink, because the rule belongs to the local platform, not to any
        // one provider — and after the `..` check above, which stays on the raw segment.
        //
        // Containment is unaffected: the transform only ever replaces a character with its own `%XX`
        // escape, never introduces a `/`, `\` or `..`, and the `Normal`-component check below still
        // gates what is pushed. It can only *widen* what is accepted — a segment like `C:` that used to
        // reject the whole entry (Windows parses it as a drive prefix) now becomes the ordinary
        // contained directory name `C%3A`.
        let seg = local_safe_segment(seg);
        // Each surviving segment must itself be a single normal component (rejects `C:` / rooted segments
        // on the platforms where they parse as prefixes/roots).
        let mut comps = Path::new(seg.as_ref()).components();
        match (comps.next(), comps.next()) {
            (Some(Component::Normal(s)), None) => {
                out.push(s);
                pushed = true;
            }
            _ => return None,
        }
    }
    if pushed {
        Some(out)
    } else {
        None
    }
}

/// The longest ancestor of `path` (inclusive) that currently exists on disk as *some* node — file,
/// directory, or symlink — using `symlink_metadata` so a symlink is detected without being followed.
/// `Ok(None)` only if nothing up the chain (not even a root) resolves. Used to canonicalize-and-verify
/// the real, already-existing portion of a to-be-created path BEFORE creating anything, so a pre-existing
/// symlink pointing outside the download root is caught before any `mkdir` follows it (CPE-1461).
///
/// **`Err` = "this chain cannot be inspected" (CPE-1696).** The walk used to be
/// `if p.symlink_metadata().is_ok() { return Some(..) }`, which treats *every* `lstat` failure as "this
/// level doesn't exist" and keeps climbing. That is a fail-open in a security guard: a level whose `lstat`
/// is refused (permission denied, a dead mount, an I/O error) is skipped, so the containment check lands
/// on a **shallower** ancestor, and if the skipped level is a symlink pointing out of the download root
/// then `create_dir_all` follows it with nothing having verified it. Only a genuine `NotFound` means "not
/// here, keep climbing"; anything else returns `Err` and the caller fails closed by skipping the entry.
///
/// (Mitigation note, recorded per the ticket: a path you cannot `lstat` you very probably cannot traverse
/// either, so in practice the subsequent `create_dir_all` would fail anyway and no escape would occur.
/// That is a *probable* consequence of one OS's permission model, not an invariant. `symlink_metadata`
/// and `create_dir_all` are separate syscalls whose access requirements are independent, so the outcome
/// of one cannot be inferred from the other — and not every `lstat` failure is a permission failure at
/// all: a dead network mount, an `EIO`, or a transient resolve failure produces the same fail-open with
/// no permission model behind it to save us. (The tempting stronger claim — that Windows *breaks* the
/// coincidence, because the ACL refusing an attributes query is not the one refusing a directory create —
/// does not hold here: a Windows deny ACE cannot refuse `symlink_metadata` at all, since it opens with a
/// desired-access mask of `0`, so on Windows the ACL precondition is simply unreachable and there is no
/// coincidence there to break.) A guard whose correctness rests on a different call happening to fail
/// later is not a guard, so it is closed here rather than argued about.)
fn existing_ancestor(path: &Path) -> Result<Option<PathBuf>, String> {
    let mut cur = Some(path);
    while let Some(p) = cur {
        match classify_ancestor_probe(p.symlink_metadata().map(|_| ()).map_err(|e| e.kind())) {
            AncestorProbe::Here => return Ok(Some(p.to_path_buf())),
            AncestorProbe::KeepClimbing => {}
            AncestorProbe::Uninspectable => {
                return Err(format!(
                    "could not inspect {} while locating the deepest existing ancestor",
                    p.display()
                ))
            }
        }
        cur = p.parent();
    }
    Ok(None)
}

/// What one level's `lstat` outcome means to [`existing_ancestor`]'s walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AncestorProbe {
    /// Something is here — this is the deepest existing ancestor.
    Here,
    /// Genuinely nothing here; try the parent.
    KeepClimbing,
    /// The `lstat` failed for a reason other than absence, so we cannot say this level is empty and must
    /// not silently step over it (CPE-1696).
    Uninspectable,
}

/// The pure classifier behind [`existing_ancestor`]'s per-level decision, split out so the
/// `NotFound`-vs-everything-else taxonomy of a **security** guard is unit-testable on every OS and account:
/// the real conditions that produce a non-`NotFound` `lstat` failure are platform- and
/// privilege-dependent (inert as root; and on Windows a deny ACE does not refuse `symlink_metadata` at
/// all, since it opens with a desired-access mask of `0` — PR #874's measurement), so an ACL-based test
/// alone would leave this taxonomy unverified on some machines. Mirrors
/// `crate::dispatch::classify_path_error`'s own rationale.
fn classify_ancestor_probe(lstat: Result<(), std::io::ErrorKind>) -> AncestorProbe {
    match lstat {
        Ok(()) => AncestorProbe::Here,
        Err(std::io::ErrorKind::NotFound) => AncestorProbe::KeepClimbing,
        Err(_) => AncestorProbe::Uninspectable,
    }
}

/// What the leaf-level `lstat` in [`download_tree`] means for the CPE-1461 leaf-symlink guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeafProbe {
    /// Nothing there, or a real non-symlink node — safe to create.
    SafeToWrite,
    /// A pre-existing symlink. Writing through it could land outside the download root; skip the entry.
    PreExistingSymlink,
    /// The `lstat` failed for a reason other than absence, so we cannot say the leaf is not a symlink
    /// (CPE-1696).
    Uninspectable,
}

/// The pure classifier behind [`download_tree`]'s leaf-symlink check — the second half of the same
/// CPE-1461 guard [`classify_ancestor_probe`] serves, and split out for the same reason, which here is
/// not merely convenient but **necessary**: this taxonomy is not reachable through an ACL on either CI
/// platform, so a permission-based test could not cover it anywhere. On Windows a deny ACE cannot refuse
/// `symlink_metadata` at all (it opens with a desired-access mask of `0` — PR #874's measurement); on
/// Unix the only lever is `chmod 0o000` on the parent, which equally refuses the `create_dir_all` /
/// `fs::write` that follow, so the entry never reaches this check. A pure classifier is the only shape
/// that can be driven red at all.
///
/// `lstat` is `Ok(is_symlink)`, or the `ErrorKind` of the failure. The bug (PR #889 review, R2) was that
/// the pre-CPE-1696 code read `if let Ok(md) = symlink_metadata(..)` with **no `else`**: every failure —
/// permission-denied, a dead mount, `EIO` — fell straight through to `fs::write`, skipping the guard
/// entirely. Only a genuine `NotFound` means "no leaf here, safe to create".
fn classify_leaf_probe(lstat: Result<bool, std::io::ErrorKind>) -> LeafProbe {
    match lstat {
        Ok(true) => LeafProbe::PreExistingSymlink,
        Ok(false) => LeafProbe::SafeToWrite,
        Err(std::io::ErrorKind::NotFound) => LeafProbe::SafeToWrite,
        Err(_) => LeafProbe::Uninspectable,
    }
}

/// Join a directory + child name. An empty `dir` (the provider root) yields the bare name — so a
/// remote root of `/` produces `/name` while a `FakeProvider`/relative root of `` produces `name`.
fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", dir.trim_end_matches('/'), name)
    }
}

/// Recursively walk the tree under `root` (depth-first), invoking `on_entry` for every file and directory.
/// `cancel` is checked before each directory listing and each entry. A directory that can't be listed is
/// skipped rather than aborting the walk. Returns the number of entries visited.
///
/// Bounded against a hostile/runaway server (CPE-1462): descent stops at [`MAX_WALK_DEPTH`] (a surfaced
/// notice, the rest of the walk continues) and the whole walk aborts with an `Err` past
/// [`MAX_WALK_ENTRIES`] total entries. (`ProviderEntry` carries no symlink signal, so a symlink loop
/// advertised by the server as an ordinary directory is bounded by these same caps rather than by
/// real-path tracking.)
pub fn walk(
    provider: &dyn FileSystemProvider,
    root: &str,
    cancel: &AtomicBool,
    mut on_entry: impl FnMut(WalkEntry),
) -> Result<usize, String> {
    // Each stack item carries its depth so descent past MAX_WALK_DEPTH can be capped (CPE-1462).
    let mut stack = vec![(root.to_string(), 0usize)];
    let mut visited = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entries) = provider.list(&dir) else { continue };
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                return Ok(visited);
            }
            let path = join(&dir, &entry.name);
            visited += 1;
            if visited > MAX_WALK_ENTRIES {
                return Err(format!(
                    "transfer: aborted — tree exceeds the {MAX_WALK_ENTRIES}-entry safety cap \
                     (possible hostile or runaway remote server)"
                ));
            }
            let is_dir = entry.is_dir;
            on_entry(WalkEntry { path: path.clone(), name: entry.name, is_dir, size: entry.size });
            if is_dir {
                if depth < MAX_WALK_DEPTH {
                    stack.push((path, depth + 1));
                } else {
                    // Bound infinite depth: don't descend further, but keep the rest of the walk going.
                    eprintln!("transfer: not descending past depth {MAX_WALK_DEPTH} at {path} (depth safety cap)");
                }
            }
        }
    }
    Ok(visited)
}

/// Download the tree under `remote_root` into `local_dir`, recreating the directory structure. Returns the
/// number of files written. Cancellable.
///
/// Hardened against a hostile remote server (CPE-1461/CPE-1462):
/// - Each entry is **streamed straight to disk as it is walked** rather than collecting the whole tree
///   into a `Vec` first, so accumulation is bounded even for an enormous remote tree.
/// - Every server-named path is run through [`guarded_join`] so a traversal name (`..`, an absolute/drive/
///   UNC path, a `\`-separated segment) can never write outside `local_dir`. An entry that would escape is
///   **skipped with a surfaced notice** (skip-on-error — one hostile entry does not fail the whole
///   transfer), and its parent directories are NOT created.
/// - The download root is canonicalized once up front. **Before** creating any directory, the longest
///   already-existing ancestor of the target is canonicalized and verified to still live under the root,
///   so a pre-existing symlink inside `local_dir` pointing outward is caught *before* any `mkdir` can
///   follow it. A file whose leaf path is itself a pre-existing symlink is skipped (never followed on
///   write). Both are defenses against a symlink planted by some other channel — the remote can't create
///   one, but defense-in-depth is the point.
pub fn download_tree(
    provider: &dyn FileSystemProvider,
    remote_root: &str,
    local_dir: &Path,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let base = remote_root.trim_end_matches('/').to_string();
    std::fs::create_dir_all(local_dir).map_err(|e| format!("{}: {e}", local_dir.display()))?;
    // Canonicalize the download root ONCE; every written path is verified to stay under this.
    let canonical_root =
        std::fs::canonicalize(local_dir).map_err(|e| format!("{}: {e}", local_dir.display()))?;

    let mut files = 0usize;
    // A callback can't use `?`; capture the first hard I/O error and stop doing work once it's set.
    let mut hard_err: Option<String> = None;

    walk(provider, remote_root, cancel, |entry| {
        if hard_err.is_some() {
            return;
        }
        let rel = entry.path.strip_prefix(&base).unwrap_or(&entry.path).trim_start_matches('/');
        // Reject/skip any entry whose server-supplied path would escape the download root.
        let Some(local) = guarded_join(&canonical_root, rel) else {
            eprintln!("transfer: skipped unsafe entry name from remote (path traversal): {}", entry.path);
            return;
        };
        // The directory to materialize: the entry itself if a dir, else the file's parent.
        let dir_to_make: &Path = if entry.is_dir { local.as_path() } else { local.parent().unwrap_or(&canonical_root) };

        // VALIDATE BEFORE MUTATING (CPE-1461 defect fix): canonicalize the longest *already-existing*
        // ancestor of the target and confirm it is still under the root, so `create_dir_all` can never
        // follow a pre-existing symlink out of the root before the check runs. The portion we then create
        // is brand-new (no symlinks), rooted at a verified-contained real directory. Fail closed on a
        // dangling/unresolvable ancestor (skip the entry with a surfaced notice).
        let ancestor = match existing_ancestor(dir_to_make) {
            Ok(a) => a,
            // CPE-1696: an `lstat` that failed for a reason OTHER than absence used to be silently
            // skipped, moving the containment check onto a shallower ancestor. Fail closed instead.
            Err(e) => {
                eprintln!(
                    "transfer: skipped entry whose existing ancestors could not be inspected ({e}): {}",
                    entry.path
                );
                return;
            }
        };
        match ancestor.as_deref().map(std::fs::canonicalize) {
            Some(Ok(c)) if c.starts_with(&canonical_root) => {}
            Some(Ok(_)) => {
                eprintln!("transfer: skipped entry escaping the download root (symlinked dir?): {}", entry.path);
                return;
            }
            _ => {
                eprintln!("transfer: skipped entry with an unresolvable parent under the root: {}", entry.path);
                return;
            }
        }
        if let Err(e) = std::fs::create_dir_all(dir_to_make) {
            hard_err = Some(format!("{}: {e}", dir_to_make.display()));
            return;
        }
        if !entry.is_dir {
            // A pre-existing leaf that is itself a symlink must NOT be followed on write (it could point
            // outside the root). Skip it — fail closed (CPE-1461 leaf-symlink defect fix).
            //
            // CPE-1696: this was `if let Ok(md) = symlink_metadata(&local)` with no `else`, so an `lstat`
            // that failed for a reason other than absence skipped the symlink check entirely and fell
            // through to `fs::write` — the same fail-open as `existing_ancestor`'s, in the same guard. Only
            // a genuine `NotFound` means "no leaf there, safe to create".
            let leaf = std::fs::symlink_metadata(&local).map(|md| md.file_type().is_symlink());
            match classify_leaf_probe(leaf.as_ref().copied().map_err(|e| e.kind())) {
                LeafProbe::SafeToWrite => {}
                LeafProbe::PreExistingSymlink => {
                    eprintln!("transfer: skipped entry whose local path is a pre-existing symlink: {}", entry.path);
                    return;
                }
                LeafProbe::Uninspectable => {
                    let cause = leaf.err().map(|e| e.to_string()).unwrap_or_default();
                    eprintln!(
                        "transfer: skipped entry whose local path could not be inspected for a \
                         pre-existing symlink ({cause}): {}",
                        entry.path
                    );
                    return;
                }
            }
            match provider.read(&entry.path) {
                Ok(data) => match std::fs::write(&local, data) {
                    Ok(()) => files += 1,
                    Err(e) => hard_err = Some(format!("{}: {e}", local.display())),
                },
                Err(e) => hard_err = Some(e),
            }
        }
    })?;

    if let Some(e) = hard_err {
        return Err(e);
    }
    Ok(files)
}

/// Upload the local tree under `local_dir` into `remote_root`, recreating the structure — the symmetric
/// counterpart to [`download_tree`]. Returns the number of files written. Cancellable. Local `\` are mapped
/// to `/` so a Windows source produces provider-native paths.
pub fn upload_tree(
    provider: &mut dyn FileSystemProvider,
    local_dir: &Path,
    remote_root: &str,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let base = remote_root.trim_end_matches('/').to_string();
    provider.mkdir(&base)?; // ensure the remote root exists
    let mut stack = vec![local_dir.to_path_buf()];
    let mut files = 0usize;
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(read_dir) = std::fs::read_dir(&dir) else { continue };
        for entry in read_dir.flatten() {
            if cancel.load(Ordering::Relaxed) {
                return Ok(files);
            }
            let local = entry.path();
            let Ok(rel) = local.strip_prefix(local_dir) else { continue };
            let remote = join(&base, &rel.to_string_lossy().replace('\\', "/"));
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                provider.mkdir(&remote)?;
                stack.push(local);
            } else {
                let data = std::fs::read(&local).map_err(|e| format!("{}: {e}", local.display()))?;
                provider.write(&remote, &data)?;
                files += 1;
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{FakeProvider, ProviderEntry};

    /// A `FakeProvider` seeded with `a.txt` + `sub/b.txt` (an empty `""` root, no leading slash).
    fn seeded() -> FakeProvider {
        let mut fs = FakeProvider::new();
        fs.write("a.txt", b"alpha").unwrap();
        fs.write("sub/b.txt", b"bravo").unwrap();
        fs
    }

    #[test]
    fn walk_recurses_every_file_and_dir() {
        let fs = seeded();
        let cancel = AtomicBool::new(false);
        let mut paths: Vec<_> = Vec::new();
        let n = walk(&fs, "", &cancel, |e| paths.push((e.path, e.is_dir))).unwrap();
        paths.sort();
        assert_eq!(n, 3, "a.txt + sub + sub/b.txt; got {paths:?}");
        assert!(paths.contains(&("a.txt".to_string(), false)));
        assert!(paths.contains(&("sub".to_string(), true)));
        assert!(paths.contains(&("sub/b.txt".to_string(), false)));
    }

    #[test]
    fn walk_stops_when_cancelled() {
        let fs = seeded();
        let cancel = AtomicBool::new(false);
        let mut count = 0;
        let visited = walk(&fs, "", &cancel, |_| {
            count += 1;
            cancel.store(true, Ordering::Relaxed);
        })
        .unwrap();
        assert_eq!((visited, count), (1, 1));
    }

    #[test]
    fn download_tree_writes_the_provider_files_locally() {
        let fs = seeded();
        let out = std::env::temp_dir().join(format!("cpe-xfer-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let cancel = AtomicBool::new(false);
        let files = download_tree(&fs, "", &out, &cancel).unwrap();
        assert_eq!(files, 2);
        assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(std::fs::read(out.join("sub").join("b.txt")).unwrap(), b"bravo");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn upload_tree_writes_local_files_into_the_provider() {
        let src = std::env::temp_dir().join(format!("cpe-xfer-up-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("x.txt"), b"ex").unwrap();
        std::fs::write(src.join("inner").join("y.txt"), b"why").unwrap();

        let mut fs = FakeProvider::new();
        let cancel = AtomicBool::new(false);
        let files = upload_tree(&mut fs, &src, "dest", &cancel).unwrap();
        assert_eq!(files, 2);
        assert_eq!(fs.read("dest/x.txt").unwrap(), b"ex");
        assert_eq!(fs.read("dest/inner/y.txt").unwrap(), b"why");
        let _ = std::fs::remove_dir_all(&src);
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1461 (path traversal) + CPE-1462 (unbounded walk/accumulation DoS) hardening battery.
    // ---------------------------------------------------------------------------------------------

    /// The canonical set of hostile entry names a remote server could return (the ticket's list). Every
    /// one must be neutralized: either rejected outright, or contained strictly inside the download root.
    const TRAVERSAL_INPUTS: &[&str] = &[
        "../../../../../home/x/.bashrc",                                                              // unix relative escape
        r"C:\Users\x\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\evil.bat",          // windows drive-absolute
        r"\\host\share\x",                                                                             // UNC
        r"\x",                                                                                         // rooted
        r"x\..\..\y",                                                                                  // backslash-separated `..`
        "a/../../b",                                                                                   // mixed `..`
        "%2e%2e",                                                                                      // percent-encoded (literal at the sink)
    ];

    #[test]
    fn guarded_join_never_escapes_the_base() {
        let base = std::env::temp_dir().join("cpe-gj-base-dir");
        // The security invariant: for EVERY hostile input, the join is either rejected or stays under base.
        for inp in TRAVERSAL_INPUTS {
            if let Some(p) = guarded_join(&base, inp) {
                assert!(p.starts_with(&base), "guarded_join({inp:?}) escaped base: {p:?}");
            }
        }
        // The clearly-escaping ones must be rejected outright on EVERY platform (we split on `\` too, so a
        // backslash-separated `..` is caught even on Unix, where `\` is otherwise a legal filename char).
        assert!(guarded_join(&base, "../../../../../home/x/.bashrc").is_none());
        assert!(guarded_join(&base, "a/../../b").is_none());
        assert!(guarded_join(&base, r"x\..\..\y").is_none());
        assert!(guarded_join(&base, "..").is_none());
        // A legit nested path is preserved exactly (no over-rejection).
        let ok = guarded_join(&base, "normal/nested/file.txt").expect("legit nested must join");
        assert_eq!(ok, base.join("normal").join("nested").join("file.txt"));
    }

    #[test]
    fn is_safe_name_accepts_leaves_and_rejects_paths() {
        for good in ["readme.txt", "my file (1).txt", "résumé.pdf", ".hidden"] {
            assert!(is_safe_name(good), "{good:?} should be safe");
        }
        for bad in [
            "", ".", "..", "a/b", r"a\b", "/etc", r"\x", "a\0b", r"C:\x", "sub/",
            // Windows ADS / drive-selector + `..`-prefixed leaves (CPE-1461 hardening):
            "x:y", "..:stream", "..::$DATA", "file:$DATA", "..evil", "C:",
        ] {
            assert!(!is_safe_name(bad), "{bad:?} should be unsafe");
        }
    }

    /// A provider whose root listing returns exactly the hostile names it was handed (as regular files),
    /// and nothing for any other directory — so `download_tree` tries to write each name straight into the
    /// download root. `read` returns a payload that would be the planted file's contents.
    struct HostileNames {
        names: Vec<String>,
    }
    impl FileSystemProvider for HostileNames {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            if path.is_empty() {
                Ok(self.names.iter().map(|n| ProviderEntry { name: n.clone(), is_dir: false, size: 3 }).collect())
            } else {
                Ok(vec![])
            }
        }
        fn read(&self, _path: &str) -> Result<Vec<u8>, String> {
            Ok(b"pwn".to_vec())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    #[test]
    fn download_tree_neutralizes_every_traversal_input() {
        let base = std::env::temp_dir().join(format!("cpe-xfer-trav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // Sentinel targets that MUST NOT be created outside the download root.
        let parent = base.parent().unwrap().to_path_buf();
        let sentinel_up = parent.join("cpe-PWNED-marker.txt");
        let _ = std::fs::remove_file(&sentinel_up);

        // Hostile names: the full traversal set, plus concrete single-level escapes aimed at a sentinel.
        let mut names: Vec<String> = TRAVERSAL_INPUTS.iter().map(|s| s.to_string()).collect();
        names.push("../cpe-PWNED-marker.txt".into());
        names.push(r"..\cpe-PWNED-marker.txt".into());
        names.push("sub/../../cpe-PWNED-marker.txt".into());

        let provider = HostileNames { names };
        let cancel = AtomicBool::new(false);
        let n = download_tree(&provider, "", &base, &cancel).expect("hostile transfer must not error, just skip");

        // The escaping sentinel must not exist anywhere outside the root.
        assert!(!sentinel_up.exists(), "path traversal escaped the download root: {sentinel_up:?}");

        // Whatever WAS written (a contained input like `%2e%2e`, or a UNC/drive path contained on Unix)
        // must live strictly inside the download root — nothing escaped.
        let mut stack = vec![base.clone()];
        let mut written = 0usize;
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                assert!(p.starts_with(&base), "a written path escaped the root: {p:?}");
                if p.is_dir() {
                    stack.push(p);
                } else {
                    written += 1;
                }
            }
        }
        assert_eq!(written, n, "reported file count must match what actually landed under the root");
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&sentinel_up);
    }

    #[test]
    fn download_tree_still_downloads_a_legit_nested_tree() {
        // Regression guard: the hardening must not over-reject an ordinary tree.
        let fs = seeded();
        let out = std::env::temp_dir().join(format!("cpe-xfer-legit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let cancel = AtomicBool::new(false);
        let files = download_tree(&fs, "", &out, &cancel).unwrap();
        assert_eq!(files, 2);
        assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(std::fs::read(out.join("sub").join("b.txt")).unwrap(), b"bravo");
        let _ = std::fs::remove_dir_all(&out);
    }

    // Symlink-escape defenses (CPE-1461 review follow-up). Gated to Unix: creating a symlink on Windows
    // needs admin/developer-mode, and the fix (validate-before-mutate + no-follow leaf) is cross-platform
    // — these tests just need a real symlink to exercise it.

    /// A provider that reports one directory `d` (holding a file `d/inner.txt`), so `download_tree` will
    /// want to `mkdir local_dir/d` and then write into it.
    struct DirThenFile;
    impl FileSystemProvider for DirThenFile {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            if path.is_empty() {
                Ok(vec![ProviderEntry { name: "d".into(), is_dir: true, size: 0 }])
            } else if path == "d" {
                Ok(vec![ProviderEntry { name: "inner.txt".into(), is_dir: false, size: 3 }])
            } else {
                Ok(vec![])
            }
        }
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(b"pwn".to_vec())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    #[cfg(unix)]
    #[test]
    fn download_tree_does_not_create_a_child_through_a_preexisting_symlinked_dir() {
        use std::os::unix::fs::symlink;
        let pid = std::process::id();
        let root = std::env::temp_dir().join(format!("cpe-symdir-root-{pid}"));
        let evil = std::env::temp_dir().join(format!("cpe-symdir-evil-{pid}"));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&evil);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&evil).unwrap();
        // Plant `root/d` as a symlink to the outside `evil` directory (as some other channel might).
        symlink(&evil, root.join("d")).unwrap();

        let cancel = AtomicBool::new(false);
        // Must NOT error the whole transfer, and must NOT create anything inside `evil`.
        let _ = download_tree(&DirThenFile, "", &root, &cancel).expect("must skip, not fail");
        assert!(
            std::fs::read_dir(&evil).unwrap().next().is_none(),
            "a child was created outside the root by following a symlinked directory"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&evil);
    }

    /// A provider that reports a single top-level file `target.txt`.
    struct OneFile;
    impl FileSystemProvider for OneFile {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            if path.is_empty() {
                Ok(vec![ProviderEntry { name: "target.txt".into(), is_dir: false, size: 3 }])
            } else {
                Ok(vec![])
            }
        }
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(b"pwn".to_vec())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    #[cfg(unix)]
    #[test]
    fn download_tree_does_not_follow_a_preexisting_symlinked_leaf_on_write() {
        use std::os::unix::fs::symlink;
        let pid = std::process::id();
        let root = std::env::temp_dir().join(format!("cpe-symleaf-root-{pid}"));
        let outside = std::env::temp_dir().join(format!("cpe-symleaf-outside-{pid}.txt"));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&outside, b"original").unwrap();
        // Plant `root/target.txt` as a symlink to the outside file.
        symlink(&outside, root.join("target.txt")).unwrap();

        let cancel = AtomicBool::new(false);
        let n = download_tree(&OneFile, "", &root, &cancel).expect("must skip, not fail");
        assert_eq!(n, 0, "the symlinked leaf must be skipped, not written");
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            b"original",
            "the write followed a symlink and clobbered a file outside the root"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
    }

    /// A provider that advertises an infinitely deep tree: every `list` returns one fresh child directory.
    struct InfiniteDepth;
    impl FileSystemProvider for InfiniteDepth {
        fn list(&self, _path: &str) -> Result<Vec<ProviderEntry>, String> {
            Ok(vec![ProviderEntry { name: "a".into(), is_dir: true, size: 0 }])
        }
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    #[test]
    fn walk_depth_cap_terminates_an_infinitely_deep_tree() {
        let p = InfiniteDepth;
        let cancel = AtomicBool::new(false);
        let mut count = 0usize;
        // Without the depth cap this never returns; with it, it must terminate quickly and bounded.
        let visited = walk(&p, "", &cancel, |_| count += 1).unwrap();
        assert_eq!(visited, count);
        assert!(visited <= MAX_WALK_DEPTH + 1, "depth cap must bound the walk; got {visited}");
    }

    /// A provider that advertises a tree far larger than the entry cap: 1000 subdirs, each with 1000
    /// files (~1,001,000 entries), so the total-entries cap fires. Per-call listings stay small (1000
    /// entries), so the test's own memory is modest.
    struct HugeTree;
    impl FileSystemProvider for HugeTree {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            if path.is_empty() {
                Ok((0..1000).map(|i| ProviderEntry { name: format!("d{i}"), is_dir: true, size: 0 }).collect())
            } else {
                Ok((0..1000).map(|i| ProviderEntry { name: format!("f{i}"), is_dir: false, size: 1 }).collect())
            }
        }
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    #[test]
    fn walk_entry_cap_aborts_a_huge_tree() {
        let p = HugeTree;
        let cancel = AtomicBool::new(false);
        let err = walk(&p, "", &cancel, |_| {}).unwrap_err();
        assert!(err.contains("safety cap"), "expected a safety-cap abort, got: {err}");
    }

    // ---- CPE-1696: the CPE-1461 symlink-escape guard must not step over a level it cannot lstat -----
    //
    // `existing_ancestor` climbed on `p.symlink_metadata().is_ok()` being false, which is true both for
    // "nothing here" and for "I was refused" — so a level whose lstat failed was silently skipped, the
    // containment check landed on a SHALLOWER ancestor, and a symlink at the skipped level went unverified
    // before `create_dir_all` followed it. The taxonomy is asserted here rather than through an ACL,
    // because on Windows a deny ACE does not refuse `symlink_metadata` at all (it opens with a
    // desired-access mask of 0 — PR #874's measurement) and on Unix the mechanism is inert as root, so a
    // permission-based test would be unverified on some of CI's three OSes. See the PR body for the
    // written-out reasoning on this guard, including its practical mitigation.

    #[test]
    fn cpe_1696_an_uninspectable_ancestor_level_is_never_treated_as_absent() {
        assert_eq!(classify_ancestor_probe(Ok(())), AncestorProbe::Here);
        assert_eq!(
            classify_ancestor_probe(Err(std::io::ErrorKind::NotFound)),
            AncestorProbe::KeepClimbing,
            "a genuine absence is the ONLY reason to keep climbing"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::NotADirectory,
        ] {
            assert_eq!(
                classify_ancestor_probe(Err(kind)),
                AncestorProbe::Uninspectable,
                "{kind:?} must stop the walk, not be mistaken for an empty level — stepping over it is \
                 what leaves a symlink at that level unverified"
            );
        }
    }

    /// **The leaf half of the same CPE-1461 guard (PR #889 review, R2).** The pre-CPE-1696 code was
    /// `if let Ok(md) = symlink_metadata(&local)` with no `else`, so any non-`NotFound` `lstat` failure
    /// skipped the symlink check outright and fell through to `fs::write`. The first cut of this fix
    /// shipped the correction with **no test at all** — reverting it alone left the whole crate green,
    /// which is precisely the hole this ticket exists to close. See `classify_leaf_probe`'s doc comment
    /// for why an ACL cannot reach this taxonomy on either CI platform, making the pure classifier the
    /// only shape that can be driven red.
    #[test]
    fn cpe_1696_a_leaf_that_cannot_be_lstatted_is_never_assumed_not_to_be_a_symlink() {
        assert_eq!(
            classify_leaf_probe(Ok(true)),
            LeafProbe::PreExistingSymlink,
            "a real symlink is still refused — the CPE-1461 behaviour this guard exists for"
        );
        assert_eq!(
            classify_leaf_probe(Ok(false)),
            LeafProbe::SafeToWrite,
            "a real non-symlink leaf is still writable"
        );
        assert_eq!(
            classify_leaf_probe(Err(std::io::ErrorKind::NotFound)),
            LeafProbe::SafeToWrite,
            "a genuine absence is the ONLY failure that means safe-to-create"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::NotADirectory,
        ] {
            assert_eq!(
                classify_leaf_probe(Err(kind)),
                LeafProbe::Uninspectable,
                "{kind:?} must skip the entry, not be mistaken for \"no leaf here\" — falling through \
                 writes straight through a symlink that may point outside the download root"
            );
        }
    }

    /// The honest cases against real syscalls, on every OS: the deepest existing ancestor of a
    /// partly-existing path is found, and an entirely-present path returns itself.
    #[test]
    fn cpe_1696_existing_ancestor_still_finds_the_deepest_real_level() {
        let d = std::env::temp_dir().join(format!("cpe-xfer-anc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("real")).unwrap();
        assert_eq!(
            existing_ancestor(&d.join("real").join("nope").join("deeper")).unwrap(),
            Some(d.join("real")),
            "the deepest level that actually exists must still be found"
        );
        assert_eq!(
            existing_ancestor(&d.join("real")).unwrap(),
            Some(d.join("real")),
            "a path that exists is its own deepest existing ancestor"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1709: a name the LOCAL filesystem cannot hold must not silently lose the file's contents.
    //
    // The bug: `guarded_join` had no `:` rule. It never needed one while every remote listing was
    // pre-filtered by `is_safe_name` (which refuses `:`); CPE-1704 correctly relaxed that for S3, where
    // `:` is an ordinary legal key byte, and removed the accidental protection the sink was leaning on.
    // `fs::write("…/colon:name.txt", b"…")` then returned `Ok(())` while diverting the bytes into an
    // NTFS alternate data stream, leaving the user a 0-byte file named `colon`.
    //
    // Every test below therefore asserts on **the file the user would open**, never on the `Result` of
    // the write — the write was already returning success when the data was being lost.
    // ---------------------------------------------------------------------------------------------

    /// The enumerated table: `(remote leaf, expected local leaf on Windows, expected local leaf
    /// elsewhere)`. The expectations are written out as literals rather than computed, so a bug in the
    /// encoder cannot make the test agree with it.
    ///
    /// Measured behaviour of each remote leaf **before** the fix, through the real sink on Windows 11
    /// Pro 26200 / NTFS — they do not group, which is why each is listed rather than reasoned about:
    /// `:` wrote a 0-byte file + a hidden stream; `< > " | ? *` and controls were refused by
    /// `CreateFileW` (os error 123) and skipped under a misleading symlink-inspection notice; a trailing
    /// dot/space produced a full-size file that no Win32 path can reopen; `NUL` produced a full-size
    /// file that reads back empty; the other device names happened to behave on this build.
    const CPE_1709_NAMES: &[(&str, &str, &str)] = &[
        ("normal.txt", "normal.txt", "normal.txt"),
        ("colon:name.txt", "colon%3Aname.txt", "colon:name.txt"),
        ("file:$DATA", "file%3A$DATA", "file:$DATA"),
        ("less<name.txt", "less%3Cname.txt", "less<name.txt"),
        ("greater>name.txt", "greater%3Ename.txt", "greater>name.txt"),
        ("quote\"name.txt", "quote%22name.txt", "quote\"name.txt"),
        ("pipe|name.txt", "pipe%7Cname.txt", "pipe|name.txt"),
        ("question?name.txt", "question%3Fname.txt", "question?name.txt"),
        ("star*name.txt", "star%2Aname.txt", "star*name.txt"),
        ("ctrl\u{1}name.txt", "ctrl%01name.txt", "ctrl\u{1}name.txt"),
        ("trailingdot.", "trailingdot%2E", "trailingdot."),
        ("trailingspace ", "trailingspace%20", "trailingspace "),
        ("CON", "%43ON", "CON"),
        ("CON.txt", "%43ON.txt", "CON.txt"),
        ("nul.txt", "%6Eul.txt", "nul.txt"),
        ("NUL", "%4EUL", "NUL"),
        ("PRN", "%50RN", "PRN"),
        ("AUX", "%41UX", "AUX"),
        ("COM1", "%43OM1", "COM1"),
        ("LPT9", "%4CPT9", "LPT9"),
        // Left alone: a `%` that does NOT begin an escape is an ordinary character.
        ("50% off.txt", "50% off.txt", "50% off.txt"),
        // Escaped: a `%` that DOES begin an escape, which is what keeps the mapping injective.
        ("literal%3Aname", "literal%253Aname", "literal%3Aname"),
    ];

    /// The local leaf `CPE_1709_NAMES` expects on the OS the test is running on.
    fn expected_local<'a>(row: &(&'a str, &'a str, &'a str)) -> &'a str {
        if cfg!(windows) {
            row.1
        } else {
            row.2
        }
    }

    #[test]
    fn cpe_1709_every_enumerated_windows_hostile_name_maps_to_its_literal_expectation() {
        for row in CPE_1709_NAMES {
            assert_eq!(
                windows_safe_segment(row.0).as_ref(),
                row.1,
                "windows_safe_segment({:?}) — the enumerated Windows rule",
                row.0
            );
        }
    }

    /// The property that stops this fix from replacing one silent-loss bug with another: **no two
    /// distinct remote names can land on the same local file.** Proved by exhibiting the inverse —
    /// `decode(encode(x)) == x` forces injectivity — and by checking the outputs really are all
    /// distinct, including the adversarial pairs where one input is the *encoding* of another.
    #[test]
    fn cpe_1709_windows_name_mapping_is_injective() {
        let mut inputs: Vec<String> = CPE_1709_NAMES.iter().map(|r| r.0.to_string()).collect();
        // Adversarial pairs: a raw name and the exact text its encoding produces. A naive scheme that
        // only escaped `:` would collide these two onto one file and silently lose one of them.
        inputs.extend(["a:b".into(), "a%3Ab".into(), "a%253Ab".into(), "CON".into(), "%43ON".into()]);

        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for inp in &inputs {
            let enc = windows_safe_segment(inp).into_owned();
            assert_eq!(
                decode_windows_safe_segment(&enc),
                *inp,
                "decode(encode({inp:?})) must return the original exactly — reversibility is what \
                 makes the mapping injective"
            );
            if let Some(other) = seen.insert(enc.clone(), inp.clone()) {
                if other != *inp {
                    panic!("COLLISION: {other:?} and {inp:?} both map to the local name {enc:?} — two \
                            distinct remote keys would overwrite one local file");
                }
            }
        }
    }

    /// Over-rejection guard: an ordinary name must come out byte-identical, on every OS.
    #[test]
    fn cpe_1709_ordinary_names_are_left_untouched() {
        for good in ["readme.txt", "my file (1).txt", "résumé.pdf", ".hidden", "50% off.txt", "a.b.c"] {
            assert_eq!(windows_safe_segment(good).as_ref(), good, "{good:?} needs no rewriting");
            assert_eq!(local_safe_segment(good).as_ref(), good);
        }
    }

    /// **The headline test, ungated so all three CI legs assert something real.** Downloads a key
    /// containing `:` through the REAL sink and reads the bytes back from the path the user would open —
    /// `colon%3Aname.txt` on Windows, `colon:name.txt` (where `:` is an ordinary byte) elsewhere.
    ///
    /// Before the fix this failed on Windows with a 0-byte file named `colon`; the `download_tree`
    /// return value was `Ok(1)` both before and after, which is exactly why nothing is asserted about it.
    #[test]
    fn cpe_1709_download_tree_delivers_the_bytes_of_a_colon_bearing_key() {
        let out = std::env::temp_dir().join(format!("cpe-1709-colon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let provider = HostileNames { names: vec!["colon:name.txt".into()] };
        let cancel = AtomicBool::new(false);
        download_tree(&provider, "", &out, &cancel).expect("the transfer must not fail");

        let expected = if cfg!(windows) { "colon%3Aname.txt" } else { "colon:name.txt" };
        assert_eq!(
            std::fs::read(out.join(expected)).unwrap_or_else(|e| panic!("{expected}: {e}")),
            b"pwn",
            "the FILE THE USER OPENS must hold the full contents"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    /// The Windows-only half: prove the alternate-data-stream outcome is gone. `:` is the only character
    /// with this behaviour and it has no analogue on Unix, so the assertion cannot be made there — the
    /// ungated sibling above covers the Linux and macOS legs.
    #[cfg(windows)]
    #[test]
    fn cpe_1709_windows_leaves_no_zero_byte_stub_and_no_alternate_data_stream() {
        let out = std::env::temp_dir().join(format!("cpe-1709-ads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let provider = HostileNames { names: vec!["colon:name.txt".into()] };
        let cancel = AtomicBool::new(false);
        download_tree(&provider, "", &out, &cancel).expect("the transfer must not fail");

        let entries: Vec<(String, u64)> = std::fs::read_dir(&out)
            .unwrap()
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.metadata().map(|m| m.len()).unwrap_or(0)))
            .collect();
        assert_eq!(
            entries,
            vec![("colon%3Aname.txt".to_string(), 3u64)],
            "the download root must hold exactly the one full-size file — the bug left a 0-byte \
             \"colon\" here with the bytes hidden in a colon:name.txt:$DATA stream"
        );
        assert!(!out.join("colon").exists(), "the 0-byte alternate-data-stream stub is back");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[cfg(not(windows))]
    #[test]
    fn cpe_1709_windows_leaves_no_zero_byte_stub_and_no_alternate_data_stream() {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "SKIP cpe_1709_windows_leaves_no_zero_byte_stub_and_no_alternate_data_stream: NTFS \
             alternate data streams do not exist on {}; the ungated sibling \
             cpe_1709_download_tree_delivers_the_bytes_of_a_colon_bearing_key asserts the real \
             behaviour on this platform",
            std::env::consts::OS
        );
    }

    /// Every enumerated name, end to end through the real sink: the bytes must be readable back from the
    /// local name this platform is supposed to produce, and the transfer must report exactly as many
    /// files as landed. This is the test that would have caught the trailing-dot and `NUL` variants,
    /// which lose data by mechanisms entirely unlike the colon's.
    #[test]
    fn cpe_1709_every_enumerated_name_arrives_intact_through_download_tree() {
        let out = std::env::temp_dir().join(format!("cpe-1709-all-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let provider =
            HostileNames { names: CPE_1709_NAMES.iter().map(|r| r.0.to_string()).collect() };
        let cancel = AtomicBool::new(false);
        let n = download_tree(&provider, "", &out, &cancel).expect("the transfer must not fail");
        assert_eq!(n, CPE_1709_NAMES.len(), "every enumerated name must be written, none skipped");

        for row in CPE_1709_NAMES {
            let leaf = expected_local(row);
            assert_eq!(
                std::fs::read(out.join(leaf)).unwrap_or_else(|e| panic!("{:?} -> {leaf:?}: {e}", row.0)),
                b"pwn",
                "remote {:?} must be readable at {leaf:?} with its full contents",
                row.0
            );
        }
        let _ = std::fs::remove_dir_all(&out);
    }

    /// **PR #890's traversal measurements, re-run (CPE-1709 acceptance criterion).** The sink now
    /// rewrites `:` instead of letting `CreateFileW` refuse it, so the containment property is asserted
    /// directly rather than resting on the OS happening to reject a name — a strictly stronger
    /// guarantee. Nothing may land outside the download root.
    #[test]
    fn cpe_1709_traversal_property_is_unchanged_by_the_name_rewriting() {
        let base = std::env::temp_dir().join(format!("cpe-1709-trav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let parent = base.parent().unwrap().to_path_buf();
        let sentinel = parent.join("cpe-1709-PWNED-marker.txt");
        let _ = std::fs::remove_file(&sentinel);

        // PR #890's two inputs, plus the full CPE-1461 battery and concrete single-level escapes.
        let mut names: Vec<String> = vec!["..::$DATA".into(), "..:stream".into()];
        names.extend(TRAVERSAL_INPUTS.iter().map(|s| s.to_string()));
        names.push("../cpe-1709-PWNED-marker.txt".into());
        names.push(r"..\cpe-1709-PWNED-marker.txt".into());

        // The join is either refused outright or lexically contained — never an escape.
        for n in &names {
            if let Some(p) = guarded_join(&base, n) {
                assert!(p.starts_with(&base), "guarded_join({n:?}) escaped the base: {p:?}");
            }
        }
        assert!(guarded_join(&base, "..").is_none(), "a bare `..` is still refused");
        assert!(guarded_join(&base, "../x").is_none(), "a `..` segment still refuses the whole entry");

        let cancel = AtomicBool::new(false);
        let n = download_tree(&HostileNames { names }, "", &base, &cancel)
            .expect("a hostile transfer must skip, not fail");
        assert!(!sentinel.exists(), "path traversal escaped the download root: {sentinel:?}");

        let mut stack = vec![base.clone()];
        let mut written = 0usize;
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                assert!(p.starts_with(&base), "a written path escaped the root: {p:?}");
                if p.is_dir() {
                    stack.push(p);
                } else {
                    written += 1;
                }
            }
        }
        assert_eq!(written, n, "the reported count must match what actually landed under the root");
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&sentinel);
    }

    /// And the guard still lets a legitimate nested download through — `download_tree`'s happy path is
    /// already covered by `download_tree_still_downloads_a_legit_nested_tree`, so this pins the narrower
    /// claim that the new `Err` arm did not turn the common case into a skip.
    #[test]
    fn cpe_1696_a_normal_download_is_not_skipped_by_the_hardened_ancestor_walk() {
        let fs = seeded();
        let out = std::env::temp_dir().join(format!("cpe-xfer-anc-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let cancel = AtomicBool::new(false);
        assert_eq!(download_tree(&fs, "", &out, &cancel).unwrap(), 2);
        assert_eq!(std::fs::read(out.join("sub").join("b.txt")).unwrap(), b"bravo");
        let _ = std::fs::remove_dir_all(&out);
    }
}
