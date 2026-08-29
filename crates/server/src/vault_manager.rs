//! Encrypted-vault **lifecycle manager** (CPE-1248, epic CPE-738).
//!
//! The state model + OS-keychain seam layered over the pure crypto core ([`crate::vault_crypto`],
//! CPE-1247). This module owns:
//!
//! - **Detection** — [`is_vault`] recognises a `.cpevault` by reading the `CPEVLT1` magic header
//!   (not merely the extension), so a renamed/mis-typed file is classified honestly.
//! - **Create** — [`create_vault`] encrypts a folder into a blob. If (and only if) the caller opts
//!   into [`CreateOpts::shred_original`], it enforces the module's one hard **safety invariant**: the
//!   plaintext original is destroyed *only after* the persisted encrypted copy is proven recoverable
//!   by a full decrypt round-trip. `shred_original` defaults to **off**.
//! - **Unlock / lock** — [`VaultRegistry`] decrypts a blob into a *session directory* and remembers the
//!   unlocked (blob → session) mapping; locking **re-seals that directory back into the blob** and only
//!   then drops the mapping and **securely wipes** the session directory (shred each extracted file,
//!   then remove the tree). Because locking shreds, the session directory is **not** freely
//!   caller-chosen: [`ensure_session_dir_contained`] (CPE-1647) refuses any path that does not resolve
//!   strictly inside the app's own `vault-sessions` root — enforced at unlock **and re-enforced at lock,
//!   immediately before the re-seal and wipe**, so swapping a link in behind an already-validated path
//!   cannot redirect the shredder (or pull a stranger's files into the vault).
//!
//! # Locking re-seals (CPE-1645)
//! The user documentation (`src/docs/20-vaults.md`) has always told users an unlocked vault "behaves like
//! an ordinary folder — you can browse, open, and edit its contents" and that locking **re-seals** it.
//! Until CPE-1645 that was false: [`vault_crypto::encrypt_tree`] was called only from [`create_vault`],
//! so locking shredded the working copy and left the blob exactly as it was at creation — silently
//! destroying everything the user had written while it was unlocked.
//!
//! [`VaultRegistry::lock`] now re-seals, adopting [`create_vault`]'s verify-before-destroy discipline in
//! full ([`reseal_session`]): the session tree is encrypted, written to a **staging file beside the
//! blob**, proven to decrypt from disk, and only then renamed over the vault. The working copy is wiped
//! **only after** that succeeds. Every failure — an encrypt error, a bad write, a failed verify, a failed
//! rename — returns `Err` having wiped nothing and having left the old blob byte-for-byte intact, so the
//! user's edits still exist in the session directory and the lock is retryable. The one thing this module
//! will never do is destroy a working copy it has not first proven it can reproduce.
//!
//! Re-sealing needs the passphrase, so the [`Session`] holds the (zeroize-on-drop) [`SecretString`] for
//! as long as the vault is unlocked. That is a strictly smaller exposure than the one v1 already accepts:
//! the whole decrypted tree is sitting on disk for that same window. It still never persists — no file,
//! no log, no status struct — and it is dropped (and zeroized) with the mapping.
//! - **Passphrase persistence** — [`remember_passphrase`] / [`forget_passphrase`] /
//!   [`stored_passphrase`] go through the [`SecretAccess`] seam (the OS keychain in production, an
//!   in-memory fake in tests). The keychain is the **only** place a passphrase may persist — never a
//!   plaintext file, never a log line.
//!
//! # The mount tradeoff (documented honestly)
//! While a vault is unlocked, its plaintext lives **on disk** in the session directory — that is the
//! cost of a browsable mount in v1. [`VaultRegistry::lock`] shreds+removes that directory so the
//! plaintext does not linger after locking, but a crash while unlocked can leave it behind (the next
//! unlock/lock over the same session dir cleans it). A future in-memory/FUSE mount could avoid ever
//! writing plaintext; that is out of scope here.
//!
//! # Tauri-free by construction
//! Like the rest of `cpe-server`, this module never touches Tauri. The real `keyring`-backed
//! [`SecretAccess`] and the Tauri-managed [`VaultRegistry`] wiring live in the app adapter
//! (`src-tauri`); everything here is unit-testable with an in-memory keychain fake.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use age::secrecy::{ExposeSecret, SecretString};

/// Re-exported so the app adapter can wrap an IPC `String` passphrase into a zeroize-on-drop
/// [`SecretString`] at the command boundary without taking a direct `age` dependency — the manager
/// takes `&SecretString` everywhere a passphrase is passed.
pub use age::secrecy::SecretString as PassphraseSecret;

use crate::batch_media::{handle_facts, open_existing_no_follow, FileIdentity};
use crate::secure_delete::ShredScheme;
use crate::secure_shred;
use crate::vault_crypto::{self, VaultError, MAGIC};

/// Keychain "service" under which every vault passphrase is stored. The per-vault *account* is a
/// stable hash of the blob path (see [`account_for`]).
pub const VAULT_SERVICE: &str = "cpe.vault";

/// Overwrite scheme used when wiping an *unlocked session directory* on lock. A single zero pass is
/// deliberate: the session dir holds transient extracted plaintext (already an accepted mount
/// tradeoff), so the fast pass keeps lock snappy while still overwriting the bytes before unlink.
const SESSION_WIPE_SCHEME: ShredScheme = ShredScheme::Zero;

/// Brokered access to the OS secret store, mirroring the sidecar's proven `SecretBackend`/`SecretAccess`
/// shape (CPE-268/279) so `cpe-server` stays Tauri-free and unit-testable. Production wires this to the
/// cross-platform `keyring` crate in the app adapter; tests use an in-memory fake. Secret VALUES flow
/// only through here — never into a status struct, a log, or the UI.
pub trait SecretAccess: Send + Sync {
    /// Store `secret` under `(service, account)`, overwriting any existing value.
    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String>;
    /// Fetch the secret under `(service, account)`, or `Ok(None)` if there is none.
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    /// Remove the secret under `(service, account)`. Removing a missing entry is `Ok`.
    fn delete(&self, service: &str, account: &str) -> Result<(), String>;
}

/// Options for [`create_vault`]. `shred_original` is **off by default** — sealing a folder never
/// destroys the plaintext unless the caller explicitly asks, and even then only behind the
/// verify-first invariant.
#[derive(Debug, Clone)]
pub struct CreateOpts {
    /// Destroy the plaintext original after sealing — but only once the encrypted copy is proven
    /// recoverable (see [`create_vault`]). Default: `false`.
    pub shred_original: bool,
    /// Overwrite scheme for the original's plaintext when `shred_original` is set.
    pub shred_scheme: ShredScheme,
}

impl Default for CreateOpts {
    fn default() -> Self {
        Self {
            shred_original: false,
            // DoD 3-pass is a reasonable default for destroying a real user's plaintext; the caller
            // can pick another scheme. (The engine's own honest SSD/copy-on-write caveats still apply.)
            shred_scheme: ShredScheme::Dod3,
        }
    }
}

/// A snapshot of a vault path's lifecycle state for the UI (CPE-1248). Carries no secret value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct VaultStatus {
    /// The path is a `.cpevault` blob (recognised by its magic header, not its extension).
    pub is_vault: bool,
    /// The vault is currently unlocked (its plaintext is extracted in a live session directory).
    pub unlocked: bool,
    /// A passphrase for this vault is saved in the OS keychain.
    pub has_stored_passphrase: bool,
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Is `path` a CPE vault? True iff its first bytes are the [`MAGIC`] marker — a content check, so a
/// `.cpevault` that is really something else (or a vault under any other name) is classified honestly.
/// Any I/O error (missing, unreadable, a directory, shorter than the magic) is a clean `false`.
pub fn is_vault(path: &Path) -> bool {
    let mut buf = [0u8; MAGIC.len()];
    match std::fs::File::open(path) {
        Ok(mut f) => f.read_exact(&mut buf).is_ok() && &buf == MAGIC,
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Create (with the verify-before-shred safety invariant)
// ---------------------------------------------------------------------------

/// Seal `folder` into a `.cpevault` blob at `dest_blob_path` with `passphrase`.
///
/// When `opts.shred_original` is set, this enforces the module's hard safety invariant: it re-reads
/// the **on-disk** blob and decrypts it in full (a genuine round-trip, which authenticates the
/// ciphertext via the AEAD and confirms the passphrase opens it) *before* it shreds the plaintext
/// original. If that verification fails for any reason — a bad write, a truncated blob, the wrong
/// passphrase — the original is left completely intact and the error is returned. The encrypted copy
/// is never trusted, and the plaintext is never destroyed, until recovery is proven.
///
/// With `shred_original` off (the default) the plaintext is left untouched and no verification runs,
/// and `confirmed` is ignored.
///
/// `confirmed` (CPE-1630, following CPE-1611's identical treatment of `secure_shred::shred_paths`) is
/// **required, separately from `shred_original`, whenever `shred_original` is true**: sealing itself
/// always proceeds, but the destructive shred of the plaintext original refuses up front — before the
/// folder is even encrypted — unless `confirmed` is also `true`. This closes the same class of gap
/// CPE-1611 closed for `shred_paths`: without a distinct confirm flag, a devtools or automation caller
/// could invoke `vault_create(folder, dest, pass, true)` and skip `VaultCreateDialog.svelte`'s warning
/// entirely. `VaultCreateDialog.svelte` — the one place in the codebase allowed to set `confirmed: true`
/// — is now the only thing that can make the backend actually shred the original.
pub fn create_vault(
    folder: &Path,
    dest_blob_path: &Path,
    passphrase: &SecretString,
    opts: &CreateOpts,
    confirmed: bool,
) -> Result<(), VaultError> {
    create_vault_with_verifier(folder, dest_blob_path, passphrase, opts, confirmed, verify_recoverable)
}

/// [`create_vault`] with the recoverability check injected, so tests can force the verify step to
/// fail and assert the plaintext original survives (the invariant is only meaningful if it is
/// falsifiable). Production always passes [`verify_recoverable`].
fn create_vault_with_verifier(
    folder: &Path,
    dest_blob_path: &Path,
    passphrase: &SecretString,
    opts: &CreateOpts,
    confirmed: bool,
    verify: impl Fn(&Path, &SecretString) -> Result<(), VaultError>,
) -> Result<(), VaultError> {
    if opts.shred_original {
        // CONFIRM GATE (CPE-1630): checked first, before anything is written, encrypted, or destroyed —
        // a distinct `confirmed` flag, separate from the caller's `shred_original` intent, exactly the
        // shape CPE-1599/CPE-1611 established for every other conditionally-destructive engine entry
        // point. Refuses cleanly; never a panic, never a partial shred, never a silently-skipped one.
        if !confirmed {
            return Err(VaultError::Format(
                "refusing to shred: `confirmed` was not set on this vault_create call — shredding the \
                 original is a permanent, non-recoverable operation with no trash fallback, so it must \
                 be re-invoked with an explicit confirmation (only VaultCreateDialog's \"Create vault\" \
                 button, submitted with \"Securely delete the original folder\" checked, should ever \
                 set it)"
                    .to_string(),
            ));
        }
        // DATA-LOSS GUARD (checked BEFORE anything is written or destroyed): refuse to shred when the
        // destination blob would live INSIDE the folder we're about to `remove_dir_all`. Otherwise the
        // just-verified encrypted copy would be shredded along with the plaintext, losing both.
        if resolves_inside(folder, dest_blob_path)? {
            return Err(VaultError::Format(format!(
                "refusing to shred: destination vault {} is inside the folder being shredded {}",
                dest_blob_path.display(),
                folder.display()
            )));
        }
    }

    let blob = vault_crypto::encrypt_tree(folder, passphrase)?;

    // STAGE → FSYNC → VERIFY → RENAME (CPE-1669 + CPE-1670), the identical four steps
    // [`reseal_session_with_hooks`] takes, through the identical helpers.
    //
    // This used to be a bare `std::fs::write` followed by a verify. Two problems, both closed here:
    //
    // - **Durability** (CPE-1669, from PR #847's review): the *destruction* was durable (the shredder
    //   `sync_all`s every pass) and the *replacement* was not. `verify_recoverable` re-reads the file it
    //   just wrote, which on every mainstream OS is served from the page cache — it proves the bytes
    //   parse, not that they reached the disk. A power loss in that window could leave a `.cpevault`
    //   whose directory entry points at unwritten data with the plaintext already shredded, having told
    //   the user the copy was verified first. [`create_staging_exclusive`] `sync_all`s before returning,
    //   so the fsync now provably precedes the verify — asserted, not reviewed, by
    //   `create_vault_fsyncs_the_blob_before_it_is_verified_and_therefore_before_any_shred`.
    // - **Symlinked destinations** (CPE-1670): `std::fs::write` is `O_CREAT|O_TRUNC`, which *follows* a
    //   symlink and writes straight through a hard link — the opposite of what the re-seal's rename did,
    //   so the two halves of the same feature disagreed about what a symlinked vault path means. They
    //   now agree: neither writes through a link. See the note at the re-seal's rename for the decision.
    let staging = create_staging_exclusive(dest_blob_path, &blob)?;
    if opts.shred_original {
        // INVARIANT: prove the persisted copy is recoverable BEFORE destroying anything. Verify the
        // bytes that actually landed on disk (not the in-memory `blob`), so a partial/failed write is
        // caught too. On any error we return WITHOUT shredding — the original is untouched, and now the
        // destination is untouched as well rather than left holding an unverifiable blob.
        if let Err(e) = verify(&staging, passphrase) {
            let _ = std::fs::remove_file(&staging);
            return Err(e);
        }
    }
    // CPE-1710: the destination IS user-named — `dest_blob_path` is the `.cpevault` path the user chose
    // in the create dialog, so "a file we own" (the PR #895 round-1 claim) was the wrong reason even
    // though leaving this alone is the right answer. Replacing whatever is at the confirmed destination
    // is this command's contract: the user named that path and confirmed it, the staging file is written
    // with `O_EXCL` and verified before this line, and refusing here would break creating a vault at a
    // path the user deliberately pointed at. See CPE-1670 for the symlink-destination decision.
    #[allow(clippy::disallowed_methods)]
    if let Err(e) = std::fs::rename(&staging, dest_blob_path) {
        let _ = std::fs::remove_file(&staging);
        return Err(VaultError::Io(e));
    }
    sync_parent_dir(dest_blob_path);

    if opts.shred_original {
        // `ShredEveryFile`: this folder is the user's own pick, not an app-owned session tree — see
        // [`AliasPolicy`]. Unchanged behaviour; the round-3 alias fix is scoped to the session wipe.
        shred_tree(folder, opts.shred_scheme, AliasPolicy::ShredEveryFile)?;
    }
    Ok(())
}

/// Prove the blob at `blob_path` is fully recoverable — read it back from disk and authenticate +
/// parse it **entirely in memory** via [`vault_crypto::verify_blob`]. Deliberately writes no plaintext
/// to disk (an earlier version extracted into a `%TEMP%` dir, leaving a recoverable unshredded copy
/// that defeated the shred — CPE-1248 review). Success means the on-disk encrypted copy authenticates
/// and the passphrase opens it.
fn verify_recoverable(blob_path: &Path, passphrase: &SecretString) -> Result<(), VaultError> {
    let blob = std::fs::read(blob_path)?;
    vault_crypto::verify_blob(&blob, passphrase)
}

/// Will a write to `dest` **land** inside `folder` (including `folder` itself)? Guards the destructive
/// shred path so the vault blob is never written where the shred will destroy it.
///
/// **This asks where the write lands, not where the name resolves to** (SEC-861 blocking 1). Every
/// parent component is canonicalized — the writer's `rename`/`open` resolves those too, so following
/// them is correct — but the **final component is never followed**, because the writer no longer follows
/// it either. `dest.parent()` is canonicalized and `dest.file_name()` re-appended, always: not just when
/// `dest` does not exist yet.
///
/// It used to call `canonicalize(dest)` first and fall back to the parent form only for a missing path.
/// That was right while `create_vault` wrote with `std::fs::write` (which follows a final symlink), and
/// CPE-1670 silently invalidated it by switching to stage-beside + `rename` (which replaces the link).
/// The guard then reasoned about the far end of a symlinked destination while the file landed at the
/// near end — so a destination name *inside* `folder`, linked to somewhere *outside* it, read as "outside,
/// safe", the vault was written inside the folder, and `shred_tree` destroyed the plaintext **and** the
/// only encrypted copy while `create_vault` returned `Ok(())`. Pinned by
/// `a_symlinked_destination_inside_the_shredded_folder_never_loses_both_copies`.
///
/// The guard and the writer must agree about links or the guard is measuring a different file from the
/// one being written; that agreement is the invariant here, not the particular link policy. `folder`
/// must exist (it's the tree being sealed). Any resolution failure propagates as `Err` — the callers
/// treat that as a refusal, never as "probably fine".
fn resolves_inside(folder: &Path, dest: &Path) -> Result<bool, VaultError> {
    let folder_canon = std::fs::canonicalize(folder)?;
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty());
    let parent_canon = match parent {
        Some(p) => std::fs::canonicalize(p)?,
        // A bare file name with no parent resolves against the current dir.
        None => std::fs::canonicalize(".")?,
    };
    // `file_name()` is `None` for a path ending in `..` or a root/prefix. Fall back to `parent_canon` and
    // be precise about what that is (SEC-861 nit 2): for `F/..` this yields `F`, which is NOT the
    // directory `F/..` denotes — that is F's parent. It is deliberately the conservative approximation:
    // it can only ever answer "inside" where the true landing might be outside, i.e. it over-refuses and
    // never under-refuses, and the writer's `rename` onto a directory errors regardless. An earlier
    // comment here claimed the fallback *was* the landing site, which is the category of claim this whole
    // PR exists to stop making.
    let dest_landing = match dest.file_name() {
        Some(name) => parent_canon.join(name),
        None => parent_canon,
    };
    Ok(dest_landing.starts_with(&folder_canon))
}

// ---------------------------------------------------------------------------
// Unlock / lock (free functions + the managed registry)
// ---------------------------------------------------------------------------

/// The app's own `vault-sessions` base directory — the ONLY place a session directory may live
/// (CPE-1647).
///
/// A newtype rather than a bare `&Path` (CPE-1647 review #2): the containment-checked functions also
/// take a `session_dir: &Path` and a `blob_path: &Path`, so with three bare `&Path`s a transposed
/// argument pair would silently **invert** the guard (checking the session dir contains the root) and
/// still compile. Wrapping the root makes that a type error instead. It is also placed FIRST in every
/// signature, so the remaining paths are never two adjacent same-typed arguments.
#[derive(Clone, Copy, Debug)]
pub struct SessionsRoot<'a>(&'a Path);

impl<'a> SessionsRoot<'a> {
    /// Wrap the app-owned session root (`appCacheDir()/vault-sessions`; see `vault_sessions_root` in
    /// the Tauri adapter, which is the one resolver the guard, the startup sweep and the frontend's
    /// `defaultAllocSessionDir` all share).
    pub fn new(root: &'a Path) -> Self {
        Self(root)
    }

    /// The wrapped path.
    pub fn as_path(self) -> &'a Path {
        self.0
    }
}

/// Which destructive operation a containment check is guarding, so a refusal names the call the user
/// actually made ("refusing to unlock …" / "refusing to lock …") rather than a generic message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Guarded {
    /// Extracting decrypted plaintext INTO the session dir.
    Unlock,
    /// Securely SHREDDING the session dir.
    Lock,
}

impl Guarded {
    fn verb(self) -> &'static str {
        match self {
            Guarded::Unlock => "unlock",
            Guarded::Lock => "lock",
        }
    }
}

/// Resolve `path` to a canonical, symlink-free, `..`-free form suitable for a containment comparison,
/// **even when it does not exist yet** (a fresh session dir never does).
///
/// Walks up to the nearest ancestor that actually exists, canonicalizes *that* (so every symlink,
/// junction, `..`, `.` and 8.3/verbatim quirk on the existing part is resolved by the OS), then
/// re-appends the not-yet-existing tail. **Fails closed**: the tail is collected via
/// [`Path::file_name`], which yields `None` for a `..`/root/prefix component — so any path whose
/// non-existent tail tries to climb (`sessions/<uuid>/../../Documents`) or that bottoms out at a root
/// with nothing canonicalizable is rejected outright rather than "resolved" optimistically. That is the
/// property [`ensure_session_dir_contained`] relies on: a returned path is safe to compare with
/// [`Path::starts_with`] because it can no longer contain an escape hatch.
fn resolve_for_containment(op: Guarded, path: &Path) -> Result<PathBuf, VaultError> {
    let unresolvable = || {
        VaultError::Format(format!(
            "refusing to {}: session directory {} cannot be resolved to a real location",
            op.verb(),
            path.display()
        ))
    };
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        if let Ok(existing) = std::fs::canonicalize(&cur) {
            let mut resolved = existing;
            for name in tail.iter().rev() {
                resolved.push(name);
            }
            return Ok(resolved);
        }
        // Not on disk (yet): step up one component and try again. `file_name()` is deliberately the
        // only way a component is accepted — it returns `None` for `..`, a root, or a drive prefix.
        let (Some(name), Some(parent)) = (cur.file_name().map(|n| n.to_os_string()), cur.parent())
        else {
            return Err(unresolvable());
        };
        if parent.as_os_str().is_empty() {
            return Err(unresolvable());
        }
        tail.push(name);
        cur = parent.to_path_buf();
    }
}

/// CONTAINMENT GUARD (CPE-1647): refuse a `session_dir` that does not resolve **strictly inside** the
/// app's own `vault-sessions` root.
///
/// This is the same class of guard [`resolves_inside`] applies to `create_vault`'s destination, applied
/// to the other destructive path in this module. Unlocking writes decrypted plaintext INTO `session_dir`
/// and locking [`wipe_session_dir`]s it — shredding every file under it, then removing the tree — so an
/// unvalidated, caller-chosen `session_dir` arriving over IPC is a "shred any directory on this machine"
/// primitive (`vault_unlock(blob, pass, "C:\\Users\\me\\Documents")` then `vault_lock(blob)`). The
/// session root is app-owned scratch space that only the frontend's `defaultAllocSessionDir`
/// (`appCacheDir()/vault-sessions/<uuid>`, `src/lib/vaultStore.ts`) ever allocates into, so a strict
/// check has no legitimate false positives.
///
/// Fails **closed** at every step: an unresolvable root, an unresolvable session path, a `..` escape, a
/// symlink/junction pointing out of the root, and `session_dir` *being* the root itself (wiping that
/// would shred every other live session) are all refused. Both sides are canonicalized before the
/// comparison, and the comparison is [`Path::starts_with`] — component-wise, so the
/// `vault-sessions` / `vault-sessions-evil` prefix-boundary trick fails too.
///
/// **Pure** (CPE-1647 review #2): this only reads the filesystem, it never creates anything — a refused
/// unlock must not leave a `vault-sessions` directory behind as a side effect of being refused, and a
/// guard that mutates the filesystem is a surprising shape to re-run from [`VaultRegistry::lock`]. A
/// root that does not exist yet (the first-ever unlock on a fresh machine) resolves the same way a
/// not-yet-existing session dir does, via [`resolve_for_containment`]; the root is actually created, as
/// a side effect of extracting into it, only once the check has passed.
///
/// Re-run at **lock** time as well as unlock time (see [`VaultRegistry::lock`]): validating only at
/// unlock would contain the caller's path *string*, not the directory that eventually gets shredded.
pub fn ensure_session_dir_contained(
    sessions_root: SessionsRoot<'_>,
    session_dir: &Path,
) -> Result<(), VaultError> {
    ensure_contained(Guarded::Unlock, sessions_root.as_path(), session_dir)
}

/// [`ensure_session_dir_contained`] with the guarded operation injected, so a lock-time refusal reads
/// "refusing to lock …" rather than talking about unlocking.
fn ensure_contained(op: Guarded, sessions_root: &Path, session_dir: &Path) -> Result<(), VaultError> {
    let root = resolve_for_containment(op, sessions_root).map_err(|_| {
        VaultError::Format(format!(
            "refusing to {}: the app's own vault-sessions directory could not be resolved, so the \
             session directory cannot be checked for containment",
            op.verb()
        ))
    })?;
    let session = resolve_for_containment(op, session_dir)?;
    if session == root || !session.starts_with(&root) {
        return Err(VaultError::Format(format!(
            "refusing to {}: session directory {} does not resolve inside the app's own \
             vault-sessions directory — a session directory holds decrypted plaintext and is securely \
             shredded when the vault is locked, so it may only be a fresh path allocated under that \
             app-owned root",
            op.verb(),
            session_dir.display()
        )));
    }
    Ok(())
}

/// Decrypt the blob at `blob_path` with `passphrase` into `session_dir` (the crypto core extracts
/// atomically — a failure leaves `session_dir` untouched). Does not record any state; use
/// [`VaultRegistry::unlock`] to track the unlocked session.
///
/// `sessions_root` is the app's own `vault-sessions` directory; `session_dir` MUST resolve strictly
/// inside it ([`ensure_session_dir_contained`], CPE-1647). That check runs **first** — before the blob
/// is read, before anything is decrypted, and long before any wipe — so a rejected call writes nothing
/// anywhere and leaves no session mapping for a later [`VaultRegistry::lock`] to shred.
pub fn unlock_to_session(
    sessions_root: SessionsRoot<'_>,
    blob_path: &Path,
    passphrase: &SecretString,
    session_dir: &Path,
) -> Result<(), VaultError> {
    ensure_session_dir_contained(sessions_root, session_dir)?;
    let blob = std::fs::read(blob_path)?;
    vault_crypto::decrypt_tree(&blob, passphrase, session_dir)
}

// ---------------------------------------------------------------------------
// Re-seal on lock (CPE-1645)
// ---------------------------------------------------------------------------

/// The wording every "this session directory is no longer the one we extracted" refusal carries.
///
/// **Human-readable only.** It used to be a contract: `classifyLockError` in `src/lib/vaultStore.ts`
/// matched this phrase to decide how to recover. The security audit of PR #847 (finding 3) showed why
/// that was wrong — the other lock failures interpolate **full file paths** into their messages, so a
/// file named `why my landlord can no longer be trusted.txt`, held open by another program, turned a
/// genuine wipe failure into a "tamper refusal": the UI cleared its banner, reported the vault sealed,
/// and left the whole decrypted tree on disk with no in-app way back to it. Decisions now switch on
/// [`LockFailureCode`], which no file name can forge; this string is just prose for the log and the
/// developer.
const UNTRUSTED_SESSION: &str = "the session directory can no longer be trusted";

/// Which step of [`VaultRegistry::lock`] failed — the **structured** reason, decided by control flow
/// rather than by matching text that can contain user-supplied file names (SEC-847 finding 3).
///
/// Crosses the IPC boundary as `vault_lock`'s error type and is what `classifyLockError`
/// (`src/lib/vaultStore.ts`) switches on to choose the message and the recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum LockFailureCode {
    /// The session directory is no longer the one this vault was extracted into (it does not resolve
    /// inside the app-owned root any more, or a link has been put in its place). **Nothing was re-sealed
    /// and nothing was deleted**, the vault's own file is untouched — so the vault genuinely is sealed —
    /// and the mapping has been dropped. Retrying can never help: the UI must clear its "unlocked" state
    /// and must NOT navigate into that path.
    UntrustedSession,
    /// The edits could not be written back into the vault. Nothing was deleted, the vault file is
    /// unchanged, the working copy is intact and the vault is still unlocked — retryable.
    ResealFailed,
    /// The re-seal succeeded (the vault file now holds the edits) but the secure wipe of the working copy
    /// did not — a file still open in another program, a read-only file. The vault is still unlocked and
    /// its plaintext is still on disk, so the UI must keep showing it as unlocked — retryable.
    WipeFailed,
    /// A lock for this same vault is **already in flight** (SEC-847 reviewer blocker A). This call did
    /// nothing at all — it did not re-seal, wipe, or drop the mapping; the lock that is already running
    /// owns the outcome. The UI disables its Lock button for the duration, so a user should never meet
    /// this; it is the engine's own backstop against a second caller of any kind.
    AlreadyLocking,
}

/// A failed [`VaultRegistry::lock`]: a machine-readable [`LockFailureCode`] plus the human explanation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LockError {
    /// Which step failed. Decided structurally — never parsed out of `message`.
    pub code: LockFailureCode,
    /// The underlying reason, for display/logging. May contain file paths, so it must never be used to
    /// make a decision.
    pub message: String,
}

impl LockError {
    fn new(code: LockFailureCode, e: VaultError) -> Self {
        Self { code, message: reason(e) }
    }
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LockError {}

/// File name **prefix-suffix** of the staging blob written beside the vault while re-sealing: the full
/// name is `<vault file name><this><per-attempt nonce>`.
///
/// It used to be exactly this, with no nonce — deterministic, and that was the whole of SEC-847 finding 1
/// (see [`create_staging_exclusive`]): a name an attacker can compute is a name they can pre-create as a
/// hard link to any file they want destroyed. The fixed part survives only so an interrupted lock's debris
/// is recognisable to [`sweep_stale_staging`].
const RESEAL_STAGING_SUFFIX: &str = ".cpe-reseal-tmp";

/// How many distinct staging names to try before giving up. More than one so that a squatter who guesses
/// a name (or an unlucky nonce collision) costs the user a retry inside the same lock, not a failed lock.
const STAGING_ATTEMPTS: usize = 8;

/// Is this session directory still the one we extracted into? Run at lock time, before anything is
/// re-sealed or shredded (see [`VaultRegistry::lock_with`]).
///
/// Two independent ways it can stop being trustworthy, both fail-closed:
/// 1. it no longer resolves strictly inside the app-owned root recorded at unlock ([`ensure_contained`]);
/// 2. it is a symlink/junction rather than a real directory — the belt to (1)'s braces, and the one that
///    matters for a link planted *inside* the root, which (1) alone would wave through. Following it
///    would both shred the target and (since CPE-1645) seal the target's files into the user's vault.
fn trustworthy_session(root: &Path, dir: &Path) -> Result<(), VaultError> {
    let untrusted = |why: String| {
        VaultError::Format(format!(
            "refusing to lock: {UNTRUSTED_SESSION} — {why}. Nothing was deleted and nothing was \
             re-sealed; the vault's own file is untouched, so the vault is sealed and is now reported \
             locked."
        ))
    };
    if let Err(e) = ensure_contained(Guarded::Lock, root, dir) {
        return Err(untrusted(reason(e)));
    }
    match std::fs::symlink_metadata(dir) {
        Ok(md) if md.file_type().is_symlink() => Err(untrusted(format!(
            "{} is a symbolic link or junction, not the real directory this vault was extracted into",
            dir.display()
        ))),
        // Anything else — a real directory, or a path that has since vanished — is fine to proceed with:
        // re-sealing and wiping both treat a missing session directory as "nothing to do" (see
        // [`reseal_session_with_verifier`]), which lets a vault whose session dir was deleted out from
        // under it still lock cleanly instead of wedging forever.
        _ => Ok(()),
    }
}

/// The bare message of a [`VaultError`], without `Display`'s `vault format error: ` prefix, so nesting
/// one refusal inside another reads as one sentence rather than a stack of prefixes.
fn reason(e: VaultError) -> String {
    match e {
        VaultError::Format(m) => m,
        other => other.to_string(),
    }
}

/// Re-seal `session_dir` back into the vault at `blob_path` (CPE-1645) — the step that makes locking
/// keep the user's edits instead of destroying them.
///
/// The caller ([`VaultRegistry::lock_with`]) must already have proven the session directory trustworthy;
/// this never resolves links itself.
fn reseal_session(
    blob_path: &Path,
    session_dir: &Path,
    passphrase: &SecretString,
) -> Result<(), VaultError> {
    reseal_session_with_verifier(blob_path, session_dir, passphrase, verify_recoverable)
}

/// [`reseal_session`] with the recoverability check injected, so tests can force the verify step to fail
/// and assert the OLD blob survives untouched and the working copy is never wiped — the same
/// falsifiable-invariant shape [`create_vault_with_verifier`] uses. Production always passes
/// [`verify_recoverable`].
///
/// The invariant, identical in spirit to `create_vault`'s: **nothing is replaced or destroyed until the
/// replacement is proven recoverable.** Concretely —
///
/// 1. Encrypt the session tree in memory. A failure here (an unsealable name, an unreadable file) means
///    the working copy is untouched and the blob is untouched.
/// 2. Write it to a staging file **beside the blob** — same directory, so the later rename is a
///    same-volume atomic replace and cannot half-copy.
/// 3. Re-read the staging file **from disk** and decrypt it in full ([`verify_recoverable`], in memory,
///    no plaintext written) — so a partial or corrupted write is caught, not trusted.
/// 4. Only then rename it over the vault.
///
/// Any failure removes the staging file and returns `Err` with the old blob byte-for-byte intact; the
/// caller then skips the wipe, so the user's edits are still sitting in the session directory. A
/// **missing** session directory is `Ok(())` with the blob left alone: there is nothing to re-seal, and
/// refusing would wedge the vault "unlocked" forever with no way to clear it.
fn reseal_session_with_verifier(
    blob_path: &Path,
    session_dir: &Path,
    passphrase: &SecretString,
    verify: impl Fn(&Path, &SecretString) -> Result<(), VaultError>,
) -> Result<(), VaultError> {
    reseal_session_with_hooks(blob_path, session_dir, passphrase, verify, || {})
}

/// [`reseal_session_with_verifier`] with a second injected seam, `after_encrypt`, run in the instant
/// between [`vault_crypto::encrypt_tree`] returning and the post-encrypt alias re-check.
///
/// It exists so the re-check is pinned **deterministically** rather than by a timing thread: a test
/// plants a hard link from inside the hook, which stands in for an alias that appeared *while*
/// `encrypt_tree` was still walking the tree — the exact window the first, top-of-function walk cannot
/// see (SEC-847 round-3 audit). Production passes a no-op.
fn reseal_session_with_hooks(
    blob_path: &Path,
    session_dir: &Path,
    passphrase: &SecretString,
    verify: impl Fn(&Path, &SecretString) -> Result<(), VaultError>,
    after_encrypt: impl Fn(),
) -> Result<(), VaultError> {
    // INDEPENDENT LINK GUARD (SEC-847 finding 2, security audit of PR #847). The caller re-proves
    // containment and refuses a linked session path, but this step must fail closed **on its own** —
    // exactly the belt-and-braces `wipe_session_dir` already has for the other destructive step. Without
    // it, the re-seal was the one destructive operation in this module with no guard of its own: anything
    // that reaches it with a link at the session path (a TOCTOU between the check and here, a future
    // caller) seals the LINK TARGET's files into the user's vault, replacing its real contents.
    match std::fs::symlink_metadata(session_dir) {
        // Already gone — nothing to re-seal, and the blob keeps its last sealed contents.
        Err(_) => return Ok(()),
        Ok(md) if md.file_type().is_symlink() => {
            return Err(VaultError::Format(format!(
                "refusing to re-seal session directory {}: it is a symbolic link or junction, not the \
                 real directory this vault was extracted into — following it would seal whatever it \
                 points at into the vault, replacing the vault's real contents",
                session_dir.display()
            )))
        }
        Ok(_) => {}
    }
    // DATA-LOSS GUARD (before anything is written), mirroring `create_vault`'s `resolves_inside` check:
    // if the vault file itself lives inside the directory locking is about to wipe, re-sealing there
    // would hand the user a freshly-written vault and then shred it, losing the working copy AND the
    // vault. Refuse instead, keeping the mapping so the user can still get their files out.
    if resolves_inside(session_dir, blob_path)? {
        // Wrapped in `reseal_failed` (SEC-847 reviewer nit 4) so it reads like the other re-seal
        // refusals — nothing was deleted, your files are still there — instead of arriving at the UI as
        // an unexplained failure. The actionable part (move the vault file out) travels in the message,
        // which the frontend shows for this failure code.
        return Err(reseal_failed(VaultError::Format(format!(
            "the vault file {} is inside the session directory {} that locking wipes, so re-sealing it \
             there would destroy the vault along with the working copy — move the vault file somewhere \
             outside the session directory first",
            blob_path.display(),
            session_dir.display()
        ))));
    }
    // ALIAS GUARD (SEC-847 finding 2): refuse any file in the session tree that is a hard link. See
    // [`ensure_no_aliased_files`] — a hard link is NOT a reparse point, so the crypto core's
    // skip-every-link walk reads straight through it.
    ensure_no_aliased_files(session_dir).map_err(reseal_failed)?;

    let sealed = vault_crypto::encrypt_tree(session_dir, passphrase).map_err(reseal_failed)?;
    after_encrypt();
    // ALIAS GUARD, SECOND PASS (SEC-847 round-3 audit). The check above is a check-then-USE: the walk
    // that reads the files into `sealed` happens *after* it, so an alias planted while `encrypt_tree` was
    // still walking is sealed into the blob — the confidentiality half of the finding (a victim's
    // plaintext ending up inside a vault whose passphrase the attacker chose). Re-walking here, before
    // anything is written beside the vault and long before the rename, shrinks that window from
    // "encrypt + staging write + fsync + a full scrypt verify + rename" to the encrypt walk alone, and
    // refuses — discarding `sealed` — instead of replacing the blob. It costs one stat per file on a tree
    // we have just read end to end.
    //
    // Deliberately BEFORE `create_staging_exclusive`: the staging file's appearance beside the
    // `.cpevault` is the attacker's starting gun, so no observable signal is emitted until after the last
    // read of the tree has been vetted. The wipe's own per-file check ([`shred_tree`]) is what closes the
    // integrity half, which this cannot.
    ensure_no_aliased_files(session_dir).map_err(reseal_failed)?;
    let staging = create_staging_exclusive(blob_path, &sealed).map_err(reseal_failed)?;
    // Verify the bytes that actually LANDED (not the in-memory `sealed`), so a short/failed write is
    // caught here rather than discovered on the next unlock, after the working copy is gone.
    if let Err(e) = verify(&staging, passphrase) {
        let _ = std::fs::remove_file(&staging);
        return Err(reseal_failed(e));
    }
    // The replacing rename.
    //
    // **A symlinked `.cpevault` path is REPLACED, not written through** (SEC-847 reviewer nit 7, settled
    // by CPE-1670). `rename` replaces the *link itself* with the real file. `create_vault` used to do the
    // opposite — `std::fs::write` follows a symlink and updates its target — so the two halves of the
    // same feature disagreed about what a symlinked vault path means; `create_vault` now stages and
    // renames through this same code path, so both ends agree.
    //
    // Replacing is the direction chosen, deliberately, over resolving the link: it is the only one of the
    // three options (replace / resolve / refuse) that neither lets a re-seal be redirected into writing a
    // vault somewhere the user did not choose, nor wedges a legitimately-symlinked vault "unlocked" with
    // its plaintext still on disk because locking refuses. The consequence, stated in the user docs and
    // in VAULT-SECURITY.md §5: a deliberately-symlinked `.cpevault` stops being a link the first time it
    // is created or locked, and the file at the far end keeps whatever it last held. Nothing is
    // destroyed — both files exist and both are decryptable — and the path the user actually opens holds
    // the current contents. *Reads* still follow a link (unlock reads the blob with `std::fs::read`);
    // only writes replace it.
    //
    // CPE-1710: so the destination here IS user-named (it is the user's `.cpevault`), and the guard is
    // deliberately absent — the paragraph above is the decision, taken with its consequences written into
    // the user docs. Recorded at the site because a future sweep will otherwise read this as an oversight
    // and "fix" a settled design decision. (PR #895's first round called it "a file we own", which reached
    // the right answer for the wrong reason.)
    #[allow(clippy::disallowed_methods)]
    if let Err(e) = std::fs::rename(&staging, blob_path) {
        let _ = std::fs::remove_file(&staging);
        return Err(reseal_failed(VaultError::Io(e)));
    }
    sync_parent_dir(blob_path);
    Ok(())
}

/// fsync the directory holding `path` after a rename created its entry (SEC-847 reviewer blocker C, and
/// CPE-1669 for the create side).
///
/// The renamed file's own bytes are `sync_all`ed by [`write_new_exclusive`], but on Unix the *directory
/// entry* the rename created is itself only in the page cache until the directory is synced. Without
/// this, a power loss between the rename and the shred could leave the vault's name pointing at nothing
/// while the shredder has already destroyed the only other copy.
///
/// Best-effort: a filesystem that will not let us open the directory is not a reason to fail an
/// operation whose data is already on the disk.
///
/// **Windows is a no-op, and that leaves the window narrowed rather than closed** (SEC-861 finding 5).
/// An earlier version justified the no-op with "`rename`'s ordering is already provided", which is not
/// true: `MoveFileEx` without `MOVEFILE_WRITE_THROUGH` is not durable, so the directory entry the rename
/// creates can still be lost to a power failure. What *is* closed on both platforms is the larger half —
/// the blob's own bytes are `sync_all`ed before the verify, so the entry can never point at unwritten
/// data. What remains on Windows is the entry itself: a power loss in that window can leave the vault
/// name missing after the plaintext was shredded. Directories are not openable for `sync_all` the way
/// Unix allows, so closing it would need `MOVEFILE_WRITE_THROUGH` via a direct `MoveFileExW` — a
/// deliberate follow-up, not something to imply is already handled. **CPE-1669's window is closed on
/// Unix and only narrowed on Windows.**
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            #[cfg(test)]
            PARENT_DIR_SYNCS.with(|c| c.set(c.get() + 1));
            let _ = dir.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

// Count of parent-directory fsyncs (Unix only). **Test builds only**, for the same reason
// VAULT_BLOB_SYNCS exists: so `sync_parent_dir`'s claim is asserted rather than merely reviewed.
#[cfg(all(test, unix))]
thread_local! {
    static PARENT_DIR_SYNCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many parent-directory fsyncs this thread has performed. Test-only observation point.
#[cfg(all(test, unix))]
fn parent_dir_sync_count() -> usize {
    PARENT_DIR_SYNCS.with(|c| c.get())
}

/// Create the staging file for a re-seal **exclusively**, returning its path and the open handle
/// (SEC-847 finding 1, security audit of PR #847).
///
/// The original version composed a deterministic name and called [`std::fs::write`] — `CREATE_ALWAYS` /
/// `O_CREAT|O_TRUNC`, which follows a symlink and writes straight **through** a hard link. Because the
/// name was deterministic *by design*, that was a plant-once-and-wait primitive needing no race and no
/// privilege: `create_hard_link(victim, "<blob>.cpe-reseal-tmp")` — a registered IPC command, unelevated
/// on NTFS — and the next time the **user** clicked Lock, the victim's inode was truncated and filled
/// with vault ciphertext. Verify then read back that same inode and passed, the rename left victim and
/// vault as two names for one inode, and the UI said "Locked". Two changes close it:
///
/// - **`create_new(true)`** — `O_EXCL` / `CREATE_NEW`. The open **fails** with `AlreadyExists` if
///   anything is already at that name: a regular file, a hard link, a symlink (it is never followed),
///   a directory. One flag, every variant, no `symlink_metadata` race window in front of it.
/// - **A per-attempt random suffix** so a crashed lock cannot leave a name that blocks the next attempt,
///   and so an attacker cannot pre-create the name we are about to use. Several attempts are made, so
///   squatting a guessed name costs the attacker a retry rather than a wedged vault.
///
/// Stale staging debris from an interrupted lock is swept first, but ONLY when it is unambiguously our
/// own — see [`sweep_stale_staging`].
fn create_staging_exclusive(blob_path: &Path, bytes: &[u8]) -> Result<PathBuf, VaultError> {
    sweep_stale_staging(blob_path);
    let mut last: Option<std::io::Error> = None;
    for _ in 0..STAGING_ATTEMPTS {
        let candidate = staging_blob_path_with(blob_path, &staging_nonce());
        match write_new_exclusive(&candidate, bytes) {
            Ok(()) => return Ok(candidate),
            // `AlreadyExists` means that name is taken (a squatter, or an unlucky collision): try
            // another. Any other error is a real I/O failure — report it rather than spinning.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(VaultError::Io(e)),
        }
    }
    Err(VaultError::Io(last.unwrap_or_else(|| {
        std::io::Error::other("could not create a staging file next to the vault")
    })))
}

/// Create `path` **exclusively** and write `bytes` to it, durably.
///
/// `create_new(true)` is `O_EXCL` / `CREATE_NEW`: it fails with [`std::io::ErrorKind::AlreadyExists`] if
/// anything at all is already at that name — a regular file, a **hard link** (which no metadata check can
/// distinguish from an ordinary file), a symlink (never followed), a directory — instead of truncating it.
/// That single flag is the guard for SEC-847 finding 1, and unlike a `symlink_metadata` pre-check it has
/// no window between the check and the open.
///
/// `sync_all` before returning (SEC-847 reviewer blocker C): the caller verifies these bytes and then
/// destroys the only other copy of the data, so they must be on the disk, not merely in the page cache —
/// otherwise a power loss in that window leaves a vault entry pointing at unwritten data with the
/// plaintext already securely shredded.
fn write_new_exclusive(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    sync_durably(&file)
}

// Count of vault-blob fsyncs this process has performed. **Test builds only** — it exists so the
// "durable before verify" ordering can be *asserted* instead of merely reviewed (SEC-847 round-3: the
// `sync_all` was removable with the whole 50-test vault suite still green). Compiled out of release.
//
// Covers both writers, since CPE-1669 made `create_vault` stage through the same helper the re-seal
// uses — so the same counter pins the same ordering claim on both paths.
//
// **Thread-local, not a process-wide atomic.** It was an `AtomicUsize`, which made both ordering tests
// weaker than they read: the suite runs in parallel, so another test's fsync could satisfy
// "the count went up" without this call site having synced anything at all — verified by neutralising
// the create-side write and watching the create-side ordering test stay green. Per-thread, only the
// writes this test itself provoked can be counted.
#[cfg(test)]
thread_local! {
    static VAULT_BLOB_SYNCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many vault-blob fsyncs this thread has performed. Test-only observation point for [`sync_durably`].
#[cfg(test)]
fn vault_blob_sync_count() -> usize {
    VAULT_BLOB_SYNCS.with(|c| c.get())
}

/// `sync_all`, counted in test builds. Wrapping it is what makes the *ordering* — fsync happens before
/// the caller verifies, and therefore before anything is destroyed — observable to a test; there is no
/// portable way to ask the OS after the fact whether a given write reached the platter.
fn sync_durably(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(test)]
    VAULT_BLOB_SYNCS.with(|c| c.set(c.get() + 1));
    file.sync_all()
}

/// Remove leftover staging files beside `blob_path` from an interrupted lock — but only entries this
/// module can prove are its own debris (SEC-847 finding 1).
///
/// Fail-closed on purpose: an entry is removed only when [`std::fs::symlink_metadata`] (which does not
/// follow links) reports a **regular file**, not a symlink, and [`hard_link_count`] proves exactly one
/// name points at it. Anything else is left alone: a hard link planted at one of these names is somebody
/// else's file under an alias, and while unlinking an alias would not destroy its data, this module does
/// not delete objects it cannot prove it created. Best-effort throughout; a leftover we skip is inert
/// (nothing is ever written to a name that already exists).
///
/// The link check used to be `#[cfg(unix)]` (`st_nlink` off the metadata we already had), which left the
/// invariant unenforced on **Windows — the very platform where the unprivileged hard-link primitive
/// exists**: a planted alias matching `<vault>.cpe-reseal-tmp*` was `remove_file`d like ordinary debris.
/// Nothing was destroyed by that (unlinking one of an inode's names destroys no data), but the stated
/// rule was not the shipped rule, so it now goes through the same platform-independent
/// [`hard_link_count`] the alias guards use — which additionally makes an unreadable count a skip rather
/// than a delete, on both platforms.
fn sweep_stale_staging(blob_path: &Path) {
    let (Some(parent), Some(name)) = (blob_path.parent(), blob_path.file_name()) else { return };
    let mut prefix = name.to_os_string();
    prefix.push(RESEAL_STAGING_SUFFIX);
    let prefix = prefix.to_string_lossy().to_string();
    let Ok(entries) = std::fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(&prefix) {
            continue;
        }
        let path = entry.path();
        let Ok(md) = std::fs::symlink_metadata(&path) else { continue };
        // **CPE-1929: `is_symlink()` here is subsumed by `!is_file()` and can never be the decider.**
        // `std`'s `FileType::is_file` is false for a link on every platform, so on a `symlink_metadata`
        // result `is_symlink() => !is_file()` and the second disjunct is unreachable — deleting it
        // changes no behaviour, and no fixture can make it the reason an entry is skipped. Kept as a
        // **statement of intent** (a link wearing our staging prefix is never one of our own staged
        // blobs, and is never deleted as if it were), not as a second net. Untestable on its own,
        // deliberately, and recorded so a green sabotage on it reads as expected, not as a missing test.
        // **Measured, not just read off `std`'s definitions:** with this disjunct and its twin in
        // `archive`'s staging sweep BOTH deleted, the lib suite is **2,425 passed / 0 failed / 11
        // ignored** — identical to baseline.
        if !md.is_file() || md.file_type().is_symlink() {
            continue;
        }
        if hard_link_count(&path) != HardLinks::One {
            continue; // an alias for someone else's file, or a count we cannot read — never ours to delete
        }
        let _ = std::fs::remove_file(&path);
    }
}

/// A per-attempt, unpredictable-enough staging suffix (SEC-847 finding 1). No new dependency: the
/// process id, the nanosecond clock and a process-lifetime counter together make the name neither
/// guessable in advance nor repeatable across attempts. It does not need to be cryptographic — the
/// `create_new` open is what actually enforces safety; randomness only stops an attacker cheaply
/// squatting the one name we would otherwise always use.
fn staging_nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:x}-{:x}-{:x}",
        std::process::id(),
        nanos,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Refuse a session tree containing a **hard link** (SEC-847 finding 2, security audit of PR #847).
///
/// The re-seal's link guards — and [`vault_crypto`]'s own walk, which skips every reparse point at every
/// depth — all reason about *links you can see in the directory entry*. A hard link is not one: it is an
/// additional **name** for an existing inode, indistinguishable from an ordinary regular file to
/// `file_type()`. So `create_hard_link(victim, "<session>/loot.xlsx")` (a registered IPC command, needing
/// neither elevation nor Developer Mode) put a file from anywhere on the volume inside the session tree,
/// and locking then (a) sealed the victim's plaintext into a vault whose passphrase the attacker chose,
/// and (b) let the wipe's shredder overwrite the victim's real file through the alias.
///
/// **Refuses rather than skips** — deliberately. Silently skipping an entry would drop a file the user can
/// see in the unlocked folder, which is the same class of quiet data loss CPE-1645 exists to end. The
/// error is retryable (nothing is written or destroyed), and the user can copy the file in properly.
///
/// Fails **closed**: a link count that cannot be read at all is refused, not assumed to be 1. Directories
/// are walked, not counted (no OS here lets an ordinary user hard-link a directory), and symlinks are
/// skipped without being followed, exactly as [`shred_dir_pinned`] and the crypto core's walk do.
///
/// Scoped to the session re-seal only: [`create_vault`] still seals a user-chosen folder as it always has,
/// where hard links are the user's own arrangement of their own files and cross no trust boundary.
fn ensure_no_aliased_files(dir: &Path) -> Result<(), VaultError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue; // never followed — the crypto core skips these too
        }
        let path = entry.path();
        if ft.is_dir() {
            ensure_no_aliased_files(&path)?;
        } else if ft.is_file() {
            match hard_link_count(&path) {
                HardLinks::One => {}
                HardLinks::Many(n) => {
                    return Err(VaultError::Format(format!(
                        "refusing to re-seal {}: it is a hard link ({n} names point at this same file), so \
                         it may be another file on this machine wearing a name inside the vault — sealing \
                         it would copy that file's contents into the vault and locking would then \
                         overwrite the original. Replace it with a real copy of the data, then lock again",
                        path.display()
                    )))
                }
                HardLinks::Unknown(why) => {
                    return Err(VaultError::Format(format!(
                        "refusing to re-seal {}: could not read how many names point at this file ({why}), \
                         so it cannot be shown to be an ordinary file rather than a hard link to something \
                         outside the vault",
                        path.display()
                    )))
                }
            }
        }
    }
    Ok(())
}

/// How many directory entries name the same file. [`HardLinks::Unknown`] is a refusal, never a "probably
/// fine" — see [`ensure_no_aliased_files`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HardLinks {
    /// Exactly one name — an ordinary file.
    One,
    /// More than one name: an alias.
    Many(u64),
    /// The count could not be established (payload: why).
    Unknown(&'static str),
}

/// Everything one **no-follow** look at a path establishes: which object is there, how many names it has,
/// and what kind it is. One probe rather than three, because the shredder needs all of it and each extra
/// path resolution is another window (CPE-1672).
#[derive(Debug, Clone, Copy)]
struct EntryProbe {
    /// The object's filesystem identity, or `None` when it could not be established — including a
    /// *degenerate* identity (a zero volume/index, which some network redirectors return from a call that
    /// otherwise succeeds; see [`FileIdentity::is_degenerate`]). `None` means "unproven", never "fine".
    id: Option<FileIdentity>,
    /// Hard-link count, with the same fail-closed [`HardLinks::Unknown`] arm the alias guards rely on.
    links: HardLinks,
    /// A directory (never followed through — this is the no-follow answer).
    is_dir: bool,
    /// **A name that stands in for another name** — a symlink or a junction — and deliberately *not*
    /// "carries any reparse tag" (CPE-1957). Windows asks
    /// [`crate::batch_media::reparse_name_surrogate`]; Unix asks `file_type().is_symlink()`, which is
    /// already that same narrow question, so both platforms now answer alike. A cloud placeholder, a
    /// dedup'd or WOF-compressed file is `false` here, because it is an ordinary file that must be
    /// overwritten like any other rather than skipped.
    is_link: bool,
}

impl EntryProbe {
    /// Nothing could be established. Every field fails closed.
    fn unreadable(why: &'static str) -> Self {
        Self { id: None, links: HardLinks::Unknown(why), is_dir: false, is_link: false }
    }
}

/// Gate a raw identity on [`FileIdentity::is_degenerate`] — an identity that identifies nothing is
/// *unproven*, never a real one, exactly as [`crate::batch_media`] treats it.
fn provable(id: FileIdentity) -> Option<FileIdentity> {
    if id.is_degenerate() {
        None
    } else {
        Some(id)
    }
}

/// Unix: one `symlink_metadata` (no follow) answers all four questions — `st_dev`/`st_ino` for identity,
/// `st_nlink` for the link count, and the file type.
#[cfg(unix)]
fn probe_no_follow(path: &Path) -> EntryProbe {
    use std::os::unix::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(md) => EntryProbe {
            id: provable(FileIdentity { volume: md.dev(), index: u128::from(md.ino()) }),
            links: if md.nlink() > 1 { HardLinks::Many(md.nlink()) } else { HardLinks::One },
            is_dir: md.is_dir(),
            is_link: md.file_type().is_symlink(),
        },
        Err(_) => EntryProbe::unreadable("its metadata could not be read"),
    }
}

/// Windows: `GetFileInformationByHandle` on an attributes-only, no-follow open — `nNumberOfLinks` for the
/// count, `dwVolumeSerialNumber` + `nFileIndex{High,Low}` for the identity, `dwFileAttributes` for the
/// kind. `std::os::windows::fs::MetadataExt`'s `number_of_links()`/`file_index()`/`volume_serial_number()`
/// are all still behind the unstable `windows_by_handle` feature (rust-lang/rust#63010), so this makes the
/// same call the std wrappers would, via the `windows` crate already vendored for
/// [`crate::batch_media`]'s identity probe — whose two load-bearing details are reproduced here for the
/// same reasons (CPE-1642 finding B):
///
/// - `FILE_READ_ATTRIBUTES`, not `GENERIC_READ`: Windows' share-mode conflict check ignores
///   attribute-read rights, so this still succeeds against a file another process holds exclusively. With
///   `GENERIC_READ` a locked file would fail to open and — fail-closed — refuse every lock while any
///   editor had a vault file open.
/// - The path goes through [`crate::batch_media::verbatim_wide`] first: every other read/write in the
///   vault path goes through `std::fs`, which applies the same `\\?\` transformation and so reaches past
///   `MAX_PATH`. A raw `CreateFileW` without it is capped at `MAX_PATH`, and the mismatch would make a
///   deep session file unopenable — here that is a refusal (fail-closed), but it would wedge a legitimate
///   deep vault, so the transformation is required for the feature to work at all.
///
/// `FILE_FLAG_OPEN_REPARSE_POINT` keeps it from following a link, `FILE_FLAG_BACKUP_SEMANTICS` lets the
/// same call work for a directory as well as a file. Any failure is [`EntryProbe::unreadable`] — refused,
/// never assumed safe.
#[cfg(windows)]
fn probe_no_follow(path: &Path) -> EntryProbe {
    use std::os::windows::io::FromRawHandle;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let wide = crate::batch_media::verbatim_wide(path);
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 string kept alive for the whole call. This is an
    // attributes-only open of an already-existing object (`OPEN_EXISTING`, full sharing) — no create, no
    // write, no truncate, no data access. Ownership of the handle is handed to a `File` immediately
    // after the open, so every path below — including the early return — closes it exactly once, on
    // drop, rather than through a `CloseHandle` each new early return has to remember.
    unsafe {
        let handle = match CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            HANDLE::default(),
        ) {
            Ok(h) => h,
            Err(_) => return EntryProbe::unreadable("it could not be opened to read its link count"),
        };
        let file = std::fs::File::from_raw_handle(handle.0 as *mut std::ffi::c_void);
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(handle, &mut info).is_err() {
            return EntryProbe::unreadable("the filesystem did not report its link count");
        }
        EntryProbe {
            id: provable(FileIdentity {
                volume: u64::from(info.dwVolumeSerialNumber),
                index: (u128::from(info.nFileIndexHigh) << 32) | u128::from(info.nFileIndexLow),
            }),
            links: match info.nNumberOfLinks {
                0 => HardLinks::Unknown("the filesystem reported a link count of zero"),
                1 => HardLinks::One,
                n => HardLinks::Many(u64::from(n)),
            },
            is_dir: info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
            // **The narrow question, not the broad one (CPE-1957).** This used to read the bare
            // `FILE_ATTRIBUTE_REPARSE_POINT` bit off `info.dwFileAttributes`, which is true of a great
            // deal that is not a link — a OneDrive Files-On-Demand placeholder, NTFS dedup, WOF/WIM
            // compression, ProjFS. Every one of those is an ordinary file that is still itself, and the
            // sole reader of `is_link` on the wipe's file path (`shred_dir_pinned`) `continue`s on it,
            // so a WOF-compressed or dedup'd file in a session directory was dropped from the file list
            // and **never overwritten** — then unlinked by `remove_dir_all`, leaving its plaintext
            // extents on the volume after a lock the user asked for precisely to remove them. That is
            // CPE-1896's rule (a decorated file is a file) applied to the half of this module that was
            // still asking the 2019 question.
            //
            // `reparse_name_surrogate` is the crate's single owner of the tag rule and of
            // `IO_REPARSE_TAG_NAME_SURROGATE`; calling it rather than re-spelling the bit test is what
            // keeps this from drifting away from `fsutil`'s two callers (CPE-1933 — the rule is shared
            // by being called, not by a comment claiming it matches).
            //
            // `unwrap_or(true)` — "the description could not be read" is not a licence to walk into an
            // object that may stand in for another name, and writing shred passes through a symlink
            // destroys whatever it points at, which is strictly worse than the plaintext-retention harm
            // above. Same default as `fsutil::copy_file_onto_destination_handle` and
            // `open_beneath::sys::name_surrogate_at`, for the same reason, and — per
            // `reparse_name_surrogate`'s own doc — untestable by construction on this handle, since
            // nothing can make `GetFileInformationByHandleEx` fail on a handle just opened successfully.
            is_link: crate::batch_media::reparse_name_surrogate(&file).unwrap_or(true),
        }
    }
}

/// Neither Unix nor Windows: refuse rather than guess (no such platform ships today; this keeps the
/// guard fail-closed by construction rather than by omission).
#[cfg(not(any(unix, windows)))]
fn probe_no_follow(_path: &Path) -> EntryProbe {
    EntryProbe::unreadable("this platform cannot report a file's link count")
}

/// How many names point at `path`, probed without following a link. Kept as its own function because the
/// alias guards ([`ensure_no_aliased_files`], [`sweep_stale_staging`]) ask only this one question.
fn hard_link_count(path: &Path) -> HardLinks {
    probe_no_follow(path).links
}

/// Wrap a re-seal failure with wording that (a) tells the user their work is safe and (b) is
/// distinguishable by the frontend from a tamper refusal — it deliberately does NOT carry
/// [`UNTRUSTED_SESSION`], because the mapping is kept and retrying is the right move.
fn reseal_failed(e: VaultError) -> VaultError {
    VaultError::Format(format!(
        "could not re-seal the vault from its unlocked session ({}) — nothing was deleted, the vault file \
         is unchanged, and your files are still in the unlocked folder, so it is safe to try again or to \
         copy them out first",
        reason(e)
    ))
}

/// Where one attempt's staging blob lives: the vault's own file name, plus [`RESEAL_STAGING_SUFFIX`],
/// plus a per-attempt `nonce`, in the same directory (so the replacing rename stays same-volume).
///
/// The nonce is what stops the name being predictable (SEC-847 finding 1). It is **not** the guard —
/// [`create_staging_exclusive`]'s `create_new` open is — but a name an attacker cannot compute in advance
/// means they cannot even set the trap.
fn staging_blob_path_with(blob_path: &Path, nonce: &str) -> PathBuf {
    let mut name = blob_path.file_name().unwrap_or_default().to_os_string();
    name.push(RESEAL_STAGING_SUFFIX);
    name.push(".");
    name.push(nonce);
    match blob_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

/// Securely wipe an unlocked session directory: shred every extracted file, then remove the tree, so
/// the extracted plaintext does not linger. A missing directory is a no-op success.
///
/// **Refuses a symlinked/junctioned root** (CPE-1647 review #1, belt-and-braces): a genuine session
/// directory is a real directory this module extracted into — it is never itself a link. Without this
/// check, [`Path::exists`] and [`std::fs::read_dir`] both silently follow a reparse point, so anything
/// that could get a link planted at the session path (on Windows a *junction* needs neither Developer
/// Mode nor elevation) would redirect the shredder at whatever the link points to. The registry
/// re-validates containment before calling here; the two guards fail closed independently.
///
/// This check is a **single pass, at the top** — that is deliberate and it is no longer load-bearing on
/// its own. Since CPE-1672 the walk itself re-pins the root (and every directory under it) by filesystem
/// identity immediately before it enumerates and again before it destroys anything, so a link swapped in
/// after this check is caught by [`shred_tree`] rather than followed. This one stays because it is the
/// cheapest, clearest refusal for the common case and because the two fail closed independently.
pub fn wipe_session_dir(session_dir: &Path, scheme: ShredScheme) -> Result<(), VaultError> {
    // `symlink_metadata` does NOT follow the link, so this sees the link itself. A missing path is a
    // no-op success (the dir was already removed), matching the previous `exists()` behaviour.
    match std::fs::symlink_metadata(session_dir) {
        Ok(md) if md.file_type().is_symlink() => {
            return Err(VaultError::Format(format!(
                "refusing to wipe session directory {}: it is a symbolic link or junction, not a real \
                 directory — a session directory is never a link, so following this one would shred \
                 whatever it points at",
                session_dir.display()
            )));
        }
        Ok(_) => {}
        Err(_) => return Ok(()),
    }
    // `UnlinkAliasesInsteadOfOverwriting` (SEC-847 round-3): the wipe re-reads each file's link count
    // immediately before overwriting it, so an alias planted AFTER the re-seal's one-shot
    // `ensure_no_aliased_files` walk is unlinked, never written through. See [`shred_tree`].
    shred_tree(session_dir, scheme, AliasPolicy::UnlinkAliasesInsteadOfOverwriting)
}

/// Startup orphan-session sweep (CPE-1252, VAULT-SECURITY.md §5). Enumerates the immediate child
/// directories of `sessions_root` (the app's `vault-sessions` base dir) and securely wipes each one
/// with the SAME wiper [`lock`](VaultRegistry::lock) uses ([`wipe_session_dir`] at
/// [`SESSION_WIPE_SCHEME`]) — never a plain `remove_dir_all`, since every entry here holds decrypted
/// vault plaintext.
///
/// Every session dir under `sessions_root` is only ever "live" while the in-memory [`VaultRegistry`]
/// holds a blob→session mapping to it, and that registry is always empty at process start — v1 has no
/// persisted unlock state across a restart. So EVERY immediate child directory found here is, by
/// construction, an orphan: a leftover from the app being killed (or crashing) while a vault was
/// unlocked, or a superseded session dir whose best-effort wipe on re-unlock ([`VaultRegistry::unlock`])
/// failed. The caller (app startup, before any vault can be unlocked in the new process) is the only
/// safe place to run this — calling it while vaults may legitimately be unlocked would destroy live
/// sessions.
///
/// A missing or empty `sessions_root` is `Ok(0)`, never an error — most machines never create a vault,
/// so "no directory yet" is the common case, not a failure. Skips (rather than aborting the whole
/// sweep on) any child that is not a directory, or whose wipe fails (permissions, a file held open by
/// another process, etc.) — this is a best-effort security backstop, not a transactional operation;
/// one stubborn leftover must not stop the rest from being cleaned up. Returns the count of session
/// directories successfully wiped and removed.
///
/// **Link debris (CPE-1653).** A child that is a symlink/junction is not a session directory (this module
/// only ever creates real ones) — it is what a *refused* lock leaves behind after someone swapped a link
/// in at a session path: CPE-1647 correctly refuses, wipes nothing and drops the mapping, but the planted
/// link stays in the app's own root and accumulates. Such a child is **unlinked** here — the link itself,
/// via [`remove_link`], never traversed and with its target never touched — so the root does not silently
/// fill up with inert debris. It is not counted in the returned figure, which stays "session directories
/// wiped". This is the only cleanup point: the refused-lock path's discipline is deliberately "touch
/// nothing at a path we have decided not to trust", and the debris is inert (never followed again, and a
/// fresh unlock allocates a new UUID path regardless), so app startup — where the app already tidies its
/// own root, before any vault can be unlocked — is the right place and the earliest one that is plainly
/// safe.
///
/// Safety: this function does not walk above `sessions_root` — it only ever touches paths returned by
/// reading `sessions_root` itself, so callers must pass the exact `vault-sessions` directory, never a
/// broader ancestor.
pub fn sweep_orphan_sessions(sessions_root: &Path) -> Result<usize, VaultError> {
    sweep_orphan_sessions_with_wiper(sessions_root, |dir| {
        wipe_session_dir(dir, SESSION_WIPE_SCHEME)
    })
}

/// [`sweep_orphan_sessions`] with the wiper injected, so tests can force a wipe failure on one entry
/// and assert the sweep keeps going and still cleans up the rest. Production always calls
/// [`sweep_orphan_sessions`], which wires the real [`wipe_session_dir`] — matching the
/// `unlock_with_wiper`/`lock_with_wiper` dependency-injection shape already used by
/// [`VaultRegistry`] for the same reason.
fn sweep_orphan_sessions_with_wiper(
    sessions_root: &Path,
    wipe: impl Fn(&Path) -> Result<(), VaultError>,
) -> Result<usize, VaultError> {
    let entries = match std::fs::read_dir(sessions_root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(VaultError::Io(e)),
    };

    let mut wiped = 0usize;
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_symlink() {
            // LINK DEBRIS (CPE-1653): a symlink/junction child is never a session directory — this
            // module only ever creates real directories here — so it is either leftover from a refused
            // lock (CPE-1647 drops the mapping and wipes nothing, leaving the planted link behind) or
            // something a user dropped in. Unlink the LINK, never traverse it: `remove_file` /
            // `remove_dir` both operate on the reparse point itself, so the target is untouched. That is
            // the only safe way to clear it — loosening the `is_dir` filter below would make the sweep
            // follow links, which is exactly the property that keeps it safe. Best-effort like every
            // other step here; it is inert debris, not a hazard.
            remove_link(&entry.path());
            continue;
        }
        if !file_type.is_dir() {
            // Not a session dir (a stray file, etc.) — never touched by the sweep.
            continue;
        }
        if wipe(&entry.path()).is_ok() {
            wiped += 1;
        }
        // A failed wipe is skipped rather than aborting the sweep (best-effort backstop, see above);
        // the caller may log the returned count against the number of child dirs it expected.
    }
    Ok(wiped)
}

/// One live unlocked session: where the plaintext was extracted, the app-owned root it was proved to
/// live inside, and the passphrase that opened it.
///
/// The root is stored alongside the dir (CPE-1647 review #1) so [`VaultRegistry::lock`] can **re-run**
/// the containment check against the same root the unlock was validated against, immediately before
/// shredding. Validating only at unlock time would contain the caller's path *string*, not the
/// directory that actually gets wiped — anything that can swap the real session dir for a link between
/// unlock and lock (`deletePermanent` + `createJunction`, both registered commands) would otherwise
/// redirect the shredder.
///
/// The passphrase is held (CPE-1645) because locking **re-seals** the session directory back into the
/// blob, and sealing needs it. It is an `age` [`SecretString`]: zeroized on drop, redacted in `Debug`,
/// and dropped with the mapping the moment the vault locks. It never leaves this struct except as an
/// argument to the crypto core — never a file, never a log, never a status struct (see the module docs
/// for why holding it is a strictly smaller exposure than the decrypted tree already on disk).
#[derive(Clone, Debug)]
struct Session {
    dir: PathBuf,
    root: PathBuf,
    passphrase: SecretString,
}

/// The registry's whole mutable state, behind ONE mutex (SEC-847 reviewer blocker A).
///
/// `locking` is what makes a lock atomic with respect to another lock. Before it, the mutex was only
/// held long enough to *clone* the mapping and again to drop it — the re-seal and the wipe ran with
/// nothing held, so two `lock` calls for the same vault interleaved: the second re-sealed the tree the
/// first had already begun shredding, wrote it over the vault, and **both returned `Ok`**. The result
/// was a vault of zero bytes and a UI that said "Locked". It needed no attacker — the Lock button is
/// fired un-awaited and stays mounted across a re-seal that is slow by design, so a double-click on a
/// large vault did it.
#[derive(Default)]
struct RegistryState {
    /// Currently-unlocked vaults: blob path → live [`Session`].
    sessions: HashMap<PathBuf, Session>,
    /// Blob paths with a [`lock`](VaultRegistry::lock) in flight right now. Entered and left under the
    /// same mutex as `sessions`, so "is this vault already locking?" and "what is its session?" are
    /// answered in one indivisible step.
    locking: std::collections::HashSet<PathBuf>,
}

/// The set of currently-unlocked vaults plus the in-flight lock set. Cheaply cloneable (an `Arc` around
/// the state) and zero-cost until a vault is unlocked, mirroring
/// [`crate::terminal_tabs::TerminalDockState`] — the shape the Tauri app manages as state.
#[derive(Clone, Default)]
pub struct VaultRegistry(Arc<Mutex<RegistryState>>);

/// Holds a blob's place in [`RegistryState::locking`] for exactly as long as its lock is running, and
/// gives it up on **every** exit — the `?` early returns, and a panic mid-re-seal. Without the RAII drop,
/// one failed lock would wedge the vault as "already locking" for the life of the process.
struct LockInFlight<'a> {
    registry: &'a VaultRegistry,
    blob_path: PathBuf,
}

impl Drop for LockInFlight<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.registry.0.lock() {
            state.locking.remove(&self.blob_path);
        }
    }
}

impl VaultRegistry {
    /// Unlock `blob_path` into `session_dir` and record the mapping. If decryption fails, no state is
    /// recorded (the vault stays locked).
    ///
    /// Re-unlock safety (CPE-1249 review #1): if `blob_path` is ALREADY unlocked into a *different* session
    /// dir, that prior session dir's plaintext is securely wiped before the mapping is overwritten — so a
    /// direct/double unlock never leaves the old plaintext orphaned on disk. The new decrypt runs first, so
    /// a failed re-unlock leaves the existing unlocked state (and its plaintext) untouched.
    ///
    /// `session_dir` must resolve strictly inside `sessions_root`, the app's own `vault-sessions`
    /// directory (CPE-1647) — see [`ensure_session_dir_contained`]. A refused call records no mapping, so
    /// [`lock`](Self::lock) can never be steered into shredding a directory outside that root.
    pub fn unlock(
        &self,
        sessions_root: SessionsRoot<'_>,
        blob_path: &Path,
        passphrase: &SecretString,
        session_dir: &Path,
    ) -> Result<(), VaultError> {
        self.unlock_with_wiper(sessions_root, blob_path, passphrase, session_dir, |dir| {
            wipe_session_dir(dir, SESSION_WIPE_SCHEME)
        })
    }

    /// [`unlock`](Self::unlock) with the stale-session wipe injected, so tests can assert a re-unlock wipes
    /// the prior session dir. Production always passes [`wipe_session_dir`].
    ///
    /// The superseded-dir wipe is **best-effort by design** (a deliberate asymmetry vs [`lock`](Self::lock),
    /// where wiping IS the operation and stays retryable): once the NEW session is decrypted and mapped the
    /// unlock has already succeeded, so a failure to wipe the OLD (superseded) dir must NOT turn the whole
    /// unlock into an `Err` — that would falsely report a failure while a valid new session is live, and
    /// orphan the new session in the UI. We swallow that wipe error and return `Ok`; CPE-1252's startup
    /// sweep is the backstop for any old dir that lingers.
    fn unlock_with_wiper(
        &self,
        sessions_root: SessionsRoot<'_>,
        blob_path: &Path,
        passphrase: &SecretString,
        session_dir: &Path,
        wipe: impl Fn(&Path) -> Result<(), VaultError>,
    ) -> Result<(), VaultError> {
        // Containment (CPE-1647) + decrypt into the NEW session dir first — a refused or failed unlock must
        // leave any existing unlocked state (and its plaintext) intact, so this happens before we touch the
        // map. An out-of-root `session_dir` therefore never becomes a mapping `lock` would shred.
        unlock_to_session(sessions_root, blob_path, passphrase, session_dir)?;
        // Record the new mapping, capturing any prior session dir for the same blob. Inserting BEFORE the
        // wipe keeps the (freshly-decrypted) new session always reachable/lockable even if the wipe below
        // fails on a stubborn file — the new plaintext is never the orphan. The root travels with the dir
        // so `lock` can re-prove containment against the very root this unlock was validated against.
        let prev = {
            let mut map = self.0.lock().unwrap();
            map.sessions.insert(
                blob_path.to_path_buf(),
                Session {
                    dir: session_dir.to_path_buf(),
                    root: sessions_root.as_path().to_path_buf(),
                    // Retained for the re-seal on lock (CPE-1645) — zeroize-on-drop, dropped with the
                    // mapping. See the module docs' "Locking re-seals" section.
                    passphrase: passphrase.clone(),
                },
            )
        };
        if let Some(Session { dir: old, .. }) = prev {
            if old != session_dir {
                // Best-effort (see the doc comment): the unlock has already succeeded, so a failed wipe of
                // the superseded dir is swallowed rather than propagated. The startup sweep (CPE-1252) is
                // the backstop for any dir left behind here.
                let _ = wipe(&old);
            }
        }
        Ok(())
    }

    /// Lock `blob_path`: **re-seal its session directory back into the blob**, securely wipe that
    /// directory, then drop the unlocked mapping. Locking a vault that is not unlocked is a no-op
    /// success.
    ///
    /// The order is the whole point (CPE-1645): nothing is wiped until the re-sealed blob has been
    /// written *and proven to decrypt*, so edits made while the vault was unlocked are never destroyed.
    /// The mapping is dropped **only after** the wipe succeeds: if the re-seal or the wipe fails (a
    /// read-only extracted file, a file held open by another process, a full disk), the vault stays
    /// reported unlocked so the lock is retryable and never claims "locked" while the working copy — and
    /// the user's unsaved work — is still on disk.
    ///
    /// Containment is **re-validated here** (CPE-1647 review #1) against the root recorded at unlock,
    /// immediately before the re-seal and wipe — see [`lock_with`](Self::lock_with).
    pub fn lock(&self, blob_path: &Path) -> Result<(), LockError> {
        self.lock_with_wiper(blob_path, |dir| wipe_session_dir(dir, SESSION_WIPE_SCHEME))
    }

    /// [`lock`](Self::lock) with the wipe injected, so tests can force a wipe failure and assert the
    /// vault stays unlocked (retryable). Production always passes [`wipe_session_dir`].
    fn lock_with_wiper(
        &self,
        blob_path: &Path,
        wipe: impl Fn(&Path) -> Result<(), VaultError>,
    ) -> Result<(), LockError> {
        self.lock_with(blob_path, reseal_session, wipe)
    }

    /// [`lock`](Self::lock) with **both** destructive-order steps injected — the re-seal and the wipe —
    /// so tests can force either to fail and assert the other never ran. Production always passes
    /// [`reseal_session`] + [`wipe_session_dir`].
    ///
    /// The three steps, in the order that makes the guarantee (CPE-1645 + CPE-1647 review #1):
    ///
    /// 1. **Prove the session is still trustworthy** ([`trustworthy_session`]). Checking only at unlock
    ///    time contained the caller's path *string*, not the directory that eventually gets shredded:
    ///    three registered commands are enough to break that apart with no elevation and no race —
    ///    unlock legitimately into `<sessions_root>/<uuid>`, `deletePermanent` (or `moveExact`) that
    ///    directory away, `createJunction` a Windows junction at the same path pointing at the victim (a
    ///    junction needs neither Developer Mode nor admin), then lock. The attacker controls the timing
    ///    entirely, since nothing happens until `vault_lock` is called. Re-resolving the stored path here
    ///    canonicalizes the junction to the victim, which fails the `starts_with(root)` test. Since
    ///    CPE-1645 this step guards a *second* hazard as well: a re-seal that followed a planted link
    ///    would pull a stranger's files INTO the vault and overwrite the real contents with them.
    ///
    ///    **On a failed check the session is dropped, not retried** (the decide-and-log call from
    ///    CPE-1647): we re-seal nothing, shred nothing, drop the mapping, and return a clear error.
    ///    Retaining it would wedge the vault permanently "unlocked" with no user-reachable way to clear
    ///    it — every retry re-resolves the same tampered path and fails again — and there is nothing of
    ///    ours left at that path to protect anyway: the real session dir is already gone (removing it is
    ///    a precondition of planting the link). The blob itself is untouched, so the vault really is
    ///    sealed; the refusal says so, and the frontend clears its banner on that wording (CPE-1654).
    /// 2. **Re-seal** the session directory into the blob, verify-before-replace. `Err` here leaves the
    ///    mapping AND the working copy exactly as they were — retryable, nothing lost.
    /// 3. **Wipe** the working copy, and only then forget the mapping.
    ///
    /// **All three run under a per-blob in-flight claim** (SEC-847 reviewer blocker A). Steps 2 and 3 are
    /// slow and hold no mutex, so without the claim a second concurrent `lock` for the same vault ran
    /// step 2 against a tree step 3 was already shredding, and wrote *that* over the vault — both calls
    /// returning `Ok` over a vault of zero bytes. The claim is taken in the SAME mutex acquisition that
    /// reads the session (so the check and the claim cannot be split), and released by [`LockInFlight`]'s
    /// `Drop` on every exit including a panic. The second caller is refused with
    /// [`LockFailureCode::AlreadyLocking`] having done nothing whatsoever.
    fn lock_with(
        &self,
        blob_path: &Path,
        reseal: impl Fn(&Path, &Path, &SecretString) -> Result<(), VaultError>,
        wipe: impl Fn(&Path) -> Result<(), VaultError>,
    ) -> Result<(), LockError> {
        // Read (don't remove) the session, and claim the in-flight slot, in ONE acquisition — so two
        // callers can never both come away believing they own this vault's lock.
        let session = {
            let mut state = self.0.lock().unwrap();
            // Not unlocked → nothing to re-seal, nothing to wipe. Claim nothing, so a no-op lock never
            // makes a real one wait.
            let Some(session) = state.sessions.get(blob_path).cloned() else {
                return Ok(());
            };
            if !state.locking.insert(blob_path.to_path_buf()) {
                return Err(LockError {
                    code: LockFailureCode::AlreadyLocking,
                    message: format!(
                        "refusing to lock {}: a lock for this vault is already running. Nothing was \
                         re-sealed, deleted or changed by this call — the lock already in progress owns \
                         the outcome",
                        blob_path.display()
                    ),
                });
            }
            session
        };
        // Releases the claim on EVERY exit below, including a panic inside the re-seal.
        let _in_flight = LockInFlight { registry: self, blob_path: blob_path.to_path_buf() };
        let Session { dir, root, passphrase } = session;

        // Each step's failure carries its OWN code, decided here by which call returned `Err` — the
        // reason the caller acts on can therefore never be forged by a file name inside the vault
        // (SEC-847 finding 3).
        if let Err(e) = trustworthy_session(&root, &dir) {
            // Tampered: re-seal NOTHING, shred NOTHING, and forget the session rather than leaving the
            // vault wedged (see the doc comment).
            self.forget_session_at(blob_path, &dir);
            return Err(LockError::new(LockFailureCode::UntrustedSession, e));
        }
        // On Err from either step: the mapping is untouched → is_unlocked stays true → retryable, and
        // the session directory (holding the user's edits) is still there to retry from.
        if let Err(e) = reseal(blob_path, &dir, &passphrase) {
            return Err(LockError::new(LockFailureCode::ResealFailed, e));
        }
        if let Err(e) = wipe(&dir) {
            return Err(LockError::new(LockFailureCode::WipeFailed, e));
        }
        self.forget_session_at(blob_path, &dir);
        Ok(())
    }

    /// Drop `blob_path`'s mapping, but only if it STILL points at `dir` — guards the narrow
    /// unlock-during-lock race (a concurrent re-unlock into a different session dir), so we never clear
    /// a fresh mapping whose plaintext we didn't wipe.
    fn forget_session_at(&self, blob_path: &Path, dir: &Path) {
        let mut state = self.0.lock().unwrap();
        if state.sessions.get(blob_path).map(|s| s.dir.as_path()) == Some(dir) {
            state.sessions.remove(blob_path);
        }
    }

    /// Is `blob_path` currently unlocked?
    pub fn is_unlocked(&self, blob_path: &Path) -> bool {
        self.0.lock().unwrap().sessions.contains_key(blob_path)
    }

    /// The live session directory for an unlocked `blob_path`, if any.
    pub fn session_dir(&self, blob_path: &Path) -> Option<PathBuf> {
        self.0.lock().unwrap().sessions.get(blob_path).map(|s| s.dir.clone())
    }
}

// ---------------------------------------------------------------------------
// Passphrase persistence (keychain seam)
// ---------------------------------------------------------------------------

/// Save `passphrase` for `blob_path` in the keychain. This is the ONLY place a passphrase persists.
pub fn remember_passphrase(
    access: &dyn SecretAccess,
    blob_path: &Path,
    passphrase: &SecretString,
) -> Result<(), String> {
    access.set(VAULT_SERVICE, &account_for(blob_path), passphrase.expose_secret())
}

/// Delete any saved passphrase for `blob_path`. Deleting a missing entry is `Ok`.
pub fn forget_passphrase(access: &dyn SecretAccess, blob_path: &Path) -> Result<(), String> {
    access.delete(VAULT_SERVICE, &account_for(blob_path))
}

/// Fetch the saved passphrase for `blob_path` as a zeroize-on-drop [`SecretString`], or `None`.
pub fn stored_passphrase(
    access: &dyn SecretAccess,
    blob_path: &Path,
) -> Result<Option<SecretString>, String> {
    Ok(access
        .get(VAULT_SERVICE, &account_for(blob_path))?
        .map(SecretString::from))
}

/// Whether a passphrase is saved for `blob_path` (without materialising its value).
pub fn has_stored_passphrase(access: &dyn SecretAccess, blob_path: &Path) -> bool {
    matches!(access.get(VAULT_SERVICE, &account_for(blob_path)), Ok(Some(_)))
}

/// Compose the lifecycle status for `blob_path`. `unlocked` is supplied by the caller (from a
/// [`VaultRegistry`]) so this stays a pure function over the keychain + filesystem.
pub fn compute_status(blob_path: &Path, unlocked: bool, access: &dyn SecretAccess) -> VaultStatus {
    VaultStatus {
        is_vault: is_vault(blob_path),
        unlocked,
        has_stored_passphrase: has_stored_passphrase(access, blob_path),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// A stable per-vault keychain account id: the hex SHA-256 of the blob path string. Deterministic for
/// a given path, and it never embeds the raw path (which could be long or contain awkward characters)
/// into the credential-store key.
fn account_for(blob_path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(blob_path.to_string_lossy().as_bytes());
    format!("{:x}", h.finalize())
}

/// What the shredder is allowed to do to a file that turns out to have more than one name
/// (SEC-847 round-3 audit: the alias guard was check-then-USE and the destructive step had no guard).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AliasPolicy {
    /// Overwrite every regular file found, aliases included. Used by [`create_vault`]'s optional
    /// shred-original: that folder is the **user's own**, chosen in a file picker, so a hard link in it
    /// is their own arrangement of their own data and crosses no trust boundary. Unchanged behaviour.
    ///
    /// Since CPE-1672 this is also the module's "is this tree adversarial?" switch, because the identity
    /// pinning the session wipe needs has one arm that has to differ: an identity that cannot be
    /// established at all. See [`same_object_or_refuse`].
    ShredEveryFile,
    /// Overwrite only files this module can prove have exactly one name; leave anything else alone
    /// instead of writing through it. Used by the session wipe — see [`wipe_disposition`].
    ///
    /// "Leave alone" means exactly that: no open and no write. The *name* still goes away, because
    /// [`shred_tree`]'s single `remove_dir_all` at the end removes the tree — and unlinking one of an
    /// inode's names destroys no data, which is the whole point.
    UnlinkAliasesInsteadOfOverwriting,
}

/// How the session wipe must dispose of one file, given how many names point at it. Pure, so the
/// fail-closed decision is pinned by a test rather than only by review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WipeDisposition {
    /// Exactly one name: this file is the session's own, so overwrite it before unlinking.
    Shred,
    /// More than one name — or a count that could not be read at all. **Do not overwrite it**; the
    /// tree removal at the end of [`shred_tree`] takes the name, which destroys no data.
    UnlinkOnly,
}

/// The session wipe's per-file verdict (SEC-847 round-3).
///
/// Fails **closed against destroying data**: [`HardLinks::Unknown`] disposes exactly like
/// [`HardLinks::Many`]. A file whose link count cannot be read cannot be shown to be ours, and
/// overwriting is irreversible while leaving one un-shredded (already unlinked) session file is not.
/// [`ensure_no_aliased_files`] refuses the whole re-seal on `Unknown`, so reaching the wipe with one is
/// already anomalous — a file that became unreadable *after* the guard passed is exactly the shape of
/// the attack this arm exists for.
fn wipe_disposition(links: &HardLinks) -> WipeDisposition {
    match links {
        HardLinks::One => WipeDisposition::Shred,
        HardLinks::Many(_) | HardLinks::Unknown(_) => WipeDisposition::UnlinkOnly,
    }
}

/// Overwrite every file under `root`, then remove the tree. Symlinks/junctions are skipped — never
/// followed, at any depth — and removed with the tree.
///
/// # What this destroys, and how it knows (CPE-1672)
///
/// **Objects, not names.** Every overwrite goes through a handle opened no-follow, whose filesystem
/// identity is compared against the identity probed when the entry was enumerated; every descent into a
/// subdirectory compares the same way. Nothing is ever overwritten because a *path* still spells the
/// right thing — the two are different questions, and only the second one is safe to answer late.
///
/// That is the fix for a finding the security auditor reproduced 3/3 through the public
/// [`VaultRegistry::lock`], with `lock` returning `Ok(())` and the UI saying "Locked" while a file
/// outside the vault was securely overwritten and unlinked. The old shape was **collect-then-shred**:
/// `collect_files` froze absolute paths, then the loop called `hard_link_count` and
/// `secure_shred::shred_file`, each of which re-resolved the whole path from scratch. The per-file link
/// check SEC-847 added had no-follow semantics on the **final component only** — every parent component
/// was resolved by the OS — so the attacker skipped hard links entirely: plant an innocuous real
/// subdirectory before locking (link count 1, not a reparse point, so every alias walk passes and it is
/// sealed into the blob), wait for the first shredded file to vanish, then `remove_dir_all` it and drop a
/// **junction** in its place pointing at `Documents`. The frozen path resolved through the junction, the
/// victim's link count read `One`, and it was shredded.
///
/// The walk now runs in five steps per directory, and the order is the guarantee:
///
/// 0. **Re-pin this directory** against the identity its parent recorded for it at enumeration, before
///    reading anything. This is the first line of [`shred_dir_pinned`], and on the exploit's path it is
///    the check that fires. It is also the near side of the residual gap described below.
/// 1. **Enumerate and probe**, recording each entry's identity as it is seen. Nothing is destroyed in
///    this pass, so every subdirectory's identity is captured *before* the first byte is overwritten
///    anywhere in this directory — i.e. before the attacker's starting gun can possibly have fired.
///    (Step 1 *records* identities; it re-checks nothing. Step 0 is the re-check.)
/// 2. **Re-pin the directory itself again**, before anything is destroyed. A swap that lands during
///    step 1 is refused with nothing written. This is the far side of the residual gap.
/// 3. **Overwrite the files**, each through one no-follow handle that must carry the identity step 1
///    recorded — so a parent swapped in after the gun fires makes the open land on a different object and
///    is refused, rather than shredding whatever it found.
/// 4. **Descend**, each subdirectory re-pinned against the identity from step 1 — which is step 0 of
///    that child's own invocation.
///
/// **Names are not objects, and on NTFS a name is not even one object's worth of bytes (CPE-1986).**
/// `read_dir` returns names; a name's **alternate data streams** are separate runs of extents that an
/// overwrite through that name never touches, and `remove_dir_all` frees them without writing them. So
/// step 3 shreds each file's named `$DATA` streams as well as its default one, and step 3 also shreds
/// the directory's own — a directory can carry them too. See [`shred_alternate_streams`], which also
/// states what the Unix arm does **not** cover and why.
///
/// **Nothing here unlinks by path.** The removal is left entirely to one `remove_dir_all(root)`, which
/// std hardened against exactly this swap in 1.58.1 (CVE-2022-21658): it recurses through directory
/// handles rather than re-resolving path strings, and deletes a reparse point instead of descending into
/// it. That is also how an alias's *name* goes away — see [`AliasPolicy`].
///
/// # The residual, stated with its real size
///
/// This is not "no window", and this comment must not say that it is. Two remain.
///
/// **The gap.** Step 0's re-pin and the `read_dir` on the very next line are separated by a single
/// syscall, and step 2 re-pins the same directory once the enumeration is done — so the two re-pins
/// bracket the enumeration, and a swap must be **in and back out** between them to survive. Both halves
/// of that must land, and the same is true one level down, so the real shape is a **four-phase
/// alignment across two gaps**, not one lucky insertion.
///
/// **What is NOT hard about it, stated because two earlier drafts claimed otherwise (SEC-861 blocking 2,
/// and PR #861 review):**
///
/// - **The attacker is not blind.** An earlier draft said the starting gun was gone "because nothing is
///   unlinked mid-walk any more". Wrong: the gun merely changed form. Step 3 overwrites file *contents*
///   in a directory the attacker can read, so polling content is a signal that fires at the same instant
///   the old "file vanished" signal did — which this module's own regression test says out loud, since
///   that is exactly how it arms. The auditor armed off it and it fired **400/400**.
/// - **Restoring the same identity is free**, not the hard part. `rename` aside and back preserves the
///   identity, one syscall per phase. The bracketing re-pins constrain *what* the attacker must put back,
///   not how expensive it is to put back.
/// - **An earlier draft also mislocated this gap**, placing its near side at step 1's per-entry probe —
///   which puts step 2's re-pin, all of step 3's overwrites and the child's own step 0 inside it. That
///   was wrong in the *unsafe* direction (it described a far wider window than exists) and is corrected
///   above: the near side is step 0, on the line before the `read_dir`.
///
/// **What is actually hard about it is the timing, and that is measured, not asserted.** Armed with the
/// content-change signal and this exact four-phase pattern, the auditor got **0 victims in 600 rounds**,
/// with 91 of those rounds refused outright. The honest claim is therefore not "no signal" but "the
/// signal does not buy enough": the window is a syscall wide, it must be hit twice in alignment, and it
/// was not hit once in 600 attempts by someone trying to.
///
/// **The second residual:** `remove_dir_all` re-resolves `root` once, at the very end. By then every file
/// has already been overwritten, so what is at stake is an unlink, not an overwrite, and std's own
/// hardening (above) is what stands behind it.
///
/// Closing both entirely needs handle-relative traversal (`openat`/`NtCreateFile` with a root directory
/// handle), which std does not expose and which is not worth a new dependency here; that is the remaining
/// gap and its size, not a claim that there is none.
fn shred_tree(root: &Path, scheme: ShredScheme, aliases: AliasPolicy) -> Result<(), VaultError> {
    // No separate root-is-a-link check here: `shred_dir_pinned` re-probes `root` before it reads it, and
    // that probe's link arm refuses. An extra check in front of it would be a guard no test could turn
    // red on its own — which is how a guard quietly stops working (SEC-847 round-3 found exactly that
    // shape). One check, pinned by `shred_tree_refuses_a_root_that_is_itself_a_link`.
    let root_probe = probe_no_follow(root);
    shred_dir_pinned(root, root_probe.id, scheme, aliases)?;
    // The ONLY removal in this function, and the only place a path is resolved for a destructive
    // purpose after the overwrites are done — see the doc comment on why std's hardened implementation
    // is what the unlink safety rests on.
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// One directory of [`shred_tree`]'s walk, pinned to the identity `expected` that its parent recorded
/// (or, for the root, that [`shred_tree`] probed immediately before). See [`shred_tree`] for the ordering
/// argument; this is that order, expressed.
fn shred_dir_pinned(
    dir: &Path,
    expected: Option<FileIdentity>,
    scheme: ShredScheme,
    aliases: AliasPolicy,
) -> Result<(), VaultError> {
    // Before we read it: is this still the directory our caller enumerated? On the exploit's path this
    // is the check that fires — the junction was swapped in after the starting gun, so the identity here
    // no longer matches the one captured before any file was touched.
    same_object_or_refuse(dir, expected, aliases, "directory")?;

    // STEP 1 — enumerate + probe. Purely observational: nothing below this point can be destroyed
    // without step 2 agreeing first.
    let mut files: Vec<(PathBuf, EntryProbe)> = Vec::new();
    let mut subdirs: Vec<(PathBuf, Option<FileIdentity>)> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue; // never followed — the crypto core skips these too; `remove_dir_all` unlinks them
        }
        let path = entry.path();
        let probe = probe_no_follow(&path);
        if probe.is_link {
            // A name-surrogate the directory entry did not report as a symlink (a Windows junction reads
            // as a plain directory through some APIs). Same treatment: never followed.
            //
            // **This is the check that decides, and until CPE-1957 it asked the wrong question.**
            // `probe.is_link` was the bare `FILE_ATTRIBUTE_REPARSE_POINT` bit, so this `continue`
            // silently dropped every cloud placeholder, dedup'd and WOF-compressed file out of the wipe
            // — `remove_dir_all` then unlinked the name and left the plaintext extents behind. It now
            // asks `reparse_name_surrogate` (see `EntryProbe::is_link`), so those are enumerated as the
            // ordinary files they are and overwritten. The handle-side refusal in `overwrite_pinned_file`
            // asks the same narrow question a second time, against the object rather than the name; it
            // is a backstop for a swap in the window between here and that open, and the sabotage
            // numbers proving it cannot fire from a test are recorded at that site.
            continue;
        }
        if probe.is_dir {
            subdirs.push((path, probe.id));
        } else if ft.is_file() {
            files.push((path, probe));
        }
    }

    // STEP 2 — re-pin, before a single byte is destroyed. The probe it returns is what step 3 pins this
    // directory's own alternate data streams to (CPE-1986): it is the identity that was just verified,
    // and re-asking the path for one would pin to whatever is there *now*.
    let dir_probe = same_object_or_refuse(dir, expected, aliases, "directory")?;

    // STEP 3 — overwrite, each file pinned to the identity captured in step 1.
    //
    // **A directory carries alternate data streams too, and they are the same defect (CPE-1986,
    // measured — see [`shred_alternate_streams`]).** `read_dir` cannot see them and `remove_dir_all`
    // frees their extents without writing them, so they are shredded here, after step 2 has agreed
    // this is still the right directory and before anything descends.
    shred_alternate_streams(dir, &dir_probe, scheme, aliases)?;
    for (path, probe) in &files {
        overwrite_pinned_file(path, probe, scheme, aliases)?;
        // `read_dir` returns NAMES; a name's alternate data streams are separate runs of the user's
        // bytes that the default-stream overwrite above never touched (CPE-1986).
        shred_alternate_streams(path, probe, scheme, aliases)?;
    }

    // STEP 4 — descend, each subdirectory pinned to the identity captured in step 1, which was recorded
    // BEFORE step 3 gave an observer anything to react to.
    for (path, id) in &subdirs {
        shred_dir_pinned(path, *id, scheme, aliases)?;
    }
    Ok(())
}

/// Is the object at `path` right now the same object `expected` names? Refuses when it provably is not,
/// and — for the app-owned session tree — when that cannot be proven either way.
///
/// The `Unknown` arm is where the two [`AliasPolicy`] trust levels genuinely differ. A session directory
/// lives in the app's own cache dir on a local volume, where an identity is always readable, so an
/// unreadable one there is anomalous and refused (the lock stays retryable and nothing is destroyed).
/// **Precisely: refused for a directory, and silently declined for a file** — `overwrite_pinned_file`
/// leaves the name for `remove_dir_all` rather than writing through an object it cannot vouch for, which
/// is the safer half of the same rule. The two cannot diverge in practice, since identity and link count
/// come from the same syscall on both platforms (PR #861 review — the earlier wording read as a
/// module-wide "refused" and was narrower than the module behaves).
/// `create_vault`'s optional shred-original runs over a folder the **user** picked in a file picker,
/// which may sit on a network redirector that reports a degenerate identity from a call that otherwise
/// succeeds — refusing there would break a legitimate feature to defend against an attacker who, by that
/// path's own threat model ([`AliasPolicy::ShredEveryFile`]), is not present.
///
/// **Returns the probe it took** (CPE-1986) rather than `()`. The caller that pins a *directory* now
/// also has to shred that directory's own alternate data streams, and it must do that against the
/// identity **this** call verified — re-probing the path afterwards would pin to whatever is there
/// *now*, which is the object an attacker just swapped in, i.e. it would defeat the pinning rather
/// than perform it.
fn same_object_or_refuse(
    path: &Path,
    expected: Option<FileIdentity>,
    aliases: AliasPolicy,
    what: &str,
) -> Result<EntryProbe, VaultError> {
    let now = probe_no_follow(path);
    if now.is_link {
        // Deliberately does NOT claim the link "was swapped in" — this same call guards the very first
        // look at the wipe's root, where the link may have been there all along. It says only what is
        // certainly true: there is a link here, and this module does not overwrite through one.
        //
        // **Measured, not assumed: this is NOT a shadowed guard, and CPE-1957 expected it to be.**
        // That ticket filed it as a probable duplicate of `shred_dir_pinned`'s `probe.is_link`, worth
        // only an "unreachable backstop" note. The two sabotages say otherwise, on Windows 11
        // (`cargo test --lib`, `crates/server`, baseline 2,460 passed / 0 failed / 14 ignored at base
        // `eca04c22`, re-confirmed against `2c7f69ff`; **2,461 in the tree this comment ships in — the
        // same baseline plus CPE-1957's one new test — so each figure below reads one higher here**;
        // see `overwrite_pinned_file` for why the revision and the +1 are named):
        // disabling it (`if false && now.is_link`) is **2,458 passed / 2 failed** —
        // `a_link_is_refused_even_when_there_is_no_identity_to_compare_it_against` and
        // `shred_tree_refuses_a_root_that_is_itself_a_link` — and forcing the predicate to lie
        // (`if true || now.is_link`) is **2,433 passed / 27 failed**. Both legs red, so it has live
        // coverage and no note claiming otherwise belongs here. The reason the duplicate reading was
        // wrong is in the sentence above: the *root* call reaches this before any enumeration has
        // happened, so there is no earlier by-path check in front of it to shadow it. Separately, with
        // both of `shred_dir_pinned`'s by-path checks disabled, this is also what catches a planted
        // directory link on the descent route (**2,459 / 1**) — the handle check in
        // `overwrite_pinned_file` never sees that shape, because a junction is a directory.
        return Err(VaultError::Format(format!(
            "refusing to wipe {}: a symbolic link or junction is at this {what}, not the real one a wipe \
             walks — following it would overwrite whatever it points at",
            path.display()
        )));
    }
    match (expected, now.id) {
        (Some(before), Some(after)) if before == after => Ok(now),
        (Some(_), Some(_)) => Err(VaultError::Format(format!(
            "refusing to wipe {}: this {what} is no longer the same object it was when the wipe \
             enumerated it, so something replaced it while the wipe was running — nothing further will \
             be overwritten",
            path.display()
        ))),
        _ => match aliases {
            AliasPolicy::UnlinkAliasesInsteadOfOverwriting => Err(VaultError::Format(format!(
                "refusing to wipe {}: this {what} could not be shown to be the same object the wipe \
                 enumerated, so there is no way to tell what an overwrite would land on",
                path.display()
            ))),
            AliasPolicy::ShredEveryFile => Ok(now),
        },
    }
}

/// Overwrite one file **through a single no-follow handle**, or decline to overwrite it at all.
///
/// The order matters, and every step fails closed against destroying data:
///
/// 1. The link count probed at enumeration decides the [`wipe_disposition`]. An alias (or an unreadable
///    count) is left alone entirely — no open, no write; `remove_dir_all` takes the *name* at the end,
///    which destroys nothing, since a name that has another name is not ours to overwrite.
/// 2. Open once, no-follow, for writing.
/// 3. Ask that handle who it is. Wrong object, a reparse point, a directory — refuse the whole wipe.
/// 4. Re-read the link count **from that same handle**. An alias that appeared between step 1 and the
///    open is caught here with nothing written.
/// 5. Only then write, and write through that handle for every pass.
fn overwrite_pinned_file(
    path: &Path,
    probed: &EntryProbe,
    scheme: ShredScheme,
    aliases: AliasPolicy,
) -> Result<(), VaultError> {
    let unlink_only = aliases == AliasPolicy::UnlinkAliasesInsteadOfOverwriting;
    if unlink_only && wipe_disposition(&probed.links) == WipeDisposition::UnlinkOnly {
        return Ok(());
    }

    let mut file = open_existing_no_follow(path).map_err(|e| {
        VaultError::Format(format!("shred {}: cannot open it for overwriting: {e}", path.display()))
    })?;

    let facts = match handle_facts(&file) {
        Some(f) if !f.id.is_degenerate() => f,
        // The handle exists but will not say what it is. Under the session tree's threat model that is
        // exactly the shape of an attack; under `create_vault`'s it is an exotic volume, and the old
        // behaviour (overwrite) is kept.
        _ => {
            if unlink_only {
                return Ok(());
            }
            return shred_through(&mut file, path, scheme);
        }
    };
    // **Narrowed from `facts.is_reparse_point` to the surrogate question, and the two halves had to move
    // together — CPE-1957.** The broad bit refused *any* reparse point, so a cloud placeholder, a dedup'd
    // or WOF-compressed file reaching this point failed the whole lock mid-wipe. It never actually
    // reached it, because `shred_dir_pinned`'s by-path `probe.is_link` asked the same broad question one
    // step earlier and `continue`d — which is what made the defect invisible. Fixing either alone makes
    // things worse: narrowing only the path check turns a silent skip into a mid-wipe refusal, and
    // narrowing only this one changes nothing at all, since control never arrives.
    //
    // **Shadowed-guard measurement, run by hand on Windows 11 (`cargo test --lib`, `crates/server`),
    // baseline 2,460 passed / 0 failed / 14 ignored at base `eca04c22`; re-confirmed against `2c7f69ff`
    // after rebasing, where the baseline came back identical, so #1099/#1100 moved nothing here.**
    // **In the tree these comments ship in the suite is 2,461 — the same 2,460 baseline plus this
    // ticket's one new test — so every figure below reads one lower than what you will measure here.**
    // That +1 is stated at the site on purpose: the commit that writes a count is usually the commit
    // that falsifies it, and without this clause the instruction in the next sentence fires spuriously
    // on day one. A number is a fact about a revision *and* about the predicate it was measured
    // against, so both are named rather than left to the reader (CPE-1933) — if a later change moves
    // the count beyond that +1, these are stale and must be re-run, not adjusted.
    //
    // **The two sabotage figures below pre-date the line they sit under.** They were measured against
    // the pre-fix predicate `facts.is_reparse_point || facts.is_dir`, which this same diff replaces
    // with the narrowed `reparse_name_surrogate(&file).unwrap_or(true) || facts.is_dir`. Re-run on the
    // shipped predicate the legs are **2,461 / 0** and **2,434 / 27**, and the verdict is unchanged.
    //  Disabling this refusal (`if false && (..)`):
    // **2,460 / 0** — identical, so nothing in the suite makes its predicate true. Forcing the predicate
    // to lie (`if true || ..`): **2,434 passed / 26 failed** — which proves only that the *line* is on
    // the hot path of every ordinary file, not that the *refusal* is reachable, and is why the second
    // sabotage is uninformative for a guard sitting where this one sits. The measurement that does
    // answer it: disabling both by-path checks in `shred_dir_pinned` (`ft.is_symlink()` and
    // `probe.is_link`) gives **2,459 / 1**, and the one failure is refused by
    // `same_object_or_refuse`'s link check on the *directory* route — not here. So on Windows a
    // surrogate at a **file** name cannot reach this guard at all: `entry.file_type().is_symlink()`
    // already catches the symlink spelling, and a junction is a directory. This is therefore kept as a
    // **deliberate backstop against a swap between the enumeration probe and the open above**, which is
    // the one shape no by-path check can see — and it is untestable from outside, because staging that
    // race needs the swap to land inside a window this process does not expose. Do not be alarmed that
    // sabotaging it leaves the suite green; that is the expected result, not a missing test.
    if crate::batch_media::reparse_name_surrogate(&file).unwrap_or(true) || facts.is_dir {
        return Err(VaultError::Format(format!(
            "refusing to wipe {}: the handle opened at this name is a link or a directory, not the \
             ordinary file the wipe enumerated",
            path.display()
        )));
    }
    // THE identity check, made against the object we are holding rather than against the name again.
    match probed.id {
        Some(before) if before == facts.id => {}
        Some(_) => {
            return Err(VaultError::Format(format!(
                "refusing to wipe {}: the handle opened at this name is a different object from the one \
                 the wipe enumerated, so something was swapped in behind this path while the wipe was \
                 running — nothing further will be overwritten",
                path.display()
            )))
        }
        // Nothing was established at enumeration time, so there is nothing to compare against.
        None if unlink_only => return Ok(()),
        None => {}
    }
    // And the link count that actually decides, likewise read from the object rather than from a name —
    // so an alias created between the enumeration probe and this open is caught with nothing written.
    let links = match facts.links {
        0 => HardLinks::Unknown("the filesystem reported a link count of zero"),
        1 => HardLinks::One,
        n => HardLinks::Many(n),
    };
    if unlink_only && wipe_disposition(&links) == WipeDisposition::UnlinkOnly {
        return Ok(());
    }
    shred_through(&mut file, path, scheme)
}

/// Run every overwrite pass through `file`. The name is used only to size the file and to name it in an
/// error — it is never re-opened (CPE-1672).
fn shred_through(file: &mut std::fs::File, path: &Path, scheme: ShredScheme) -> Result<(), VaultError> {
    let label = path.to_string_lossy().to_string();
    let size = file
        .metadata()
        .map_err(|e| VaultError::Format(format!("shred {label}: cannot size it: {e}")))?
        .len();
    secure_shred::shred_open_file(file, size, scheme, &label)
        .map(|_| ())
        .map_err(|e| VaultError::Format(format!("shred {label}: {e}")))
}

/// Overwrite every **alternate data stream** on `path` as well as its default one (CPE-1986).
///
/// # The defect this closes
///
/// `shred_dir_pinned` enumerates with [`std::fs::read_dir`], which returns **names**, and
/// [`shred_through`] writes through a handle opened at a name — which on Windows is the **default data
/// stream** and nothing else. An NTFS file (or directory) may carry any number of *named* `$DATA`
/// streams alongside it, each its own run of extents holding its own bytes. `remove_dir_all` at the end
/// of [`shred_tree`] unlinks the whole file record, which frees those extents **without writing them**.
/// So before this existed, the session wipe returned `Ok(())`, the lock said "Locked", the default
/// stream was genuinely zeroed — and the plaintext in a named stream was still on the volume.
///
/// Measured on Windows 11 with the **production** policy
/// ([`AliasPolicy::UnlinkAliasesInsteadOfOverwriting`], the one [`wipe_session_dir`] passes), before the
/// fix: `wipe_ok=true main_all_zero=true ads_readable=true ads_still_secret=true` — which is PR #1101's
/// Security Auditor's original reading, reproduced here rather than taken on trust. The same run showed
/// a **directory** stream (`sub:dirsecret`) surviving identically, which is why this is called for
/// directories too and not only for files.
///
/// This is the same shape as CPE-1957 one layer down: **a skip is indistinguishable from a success at
/// the API**, so every existing assertion on this path was satisfied by not touching the data. That is
/// why `cpe_1986_*` asserts on **bytes**, never on `is_ok()`.
///
/// # Why this reuses [`overwrite_pinned_file`] rather than writing its own loop
///
/// A named stream is not a second object: measured, a handle opened at `file:name` reports — through
/// `GetFileInformationByHandle` — **the same volume serial and file index** as the file itself, its link
/// count, `FILE_ATTRIBUTE_DIRECTORY` clear (even for a stream on a directory), and no reparse tag. So
/// the `EntryProbe` the walk already captured for the *name* is exactly the right thing to pin a stream
/// open against, and every refusal [`overwrite_pinned_file`] already makes — wrong object, a link, a
/// directory, an alias that appeared after enumeration — applies to a stream unchanged and with one
/// implementation. `metadata().len()` on such a handle returns the **stream's** length (61 of 61 in the
/// measurement, against a 21-byte default stream), so [`shred_through`] sizes the right thing.
///
/// The call is made from [`shred_dir_pinned`], **not** from inside [`overwrite_pinned_file`]: a stream
/// path enumerated for its own streams would recurse without end.
///
/// # What an unshreddable stream does: it **refuses**, and that is deliberate
///
/// A stream that cannot be opened for writing — held by another process, say — fails
/// [`overwrite_pinned_file`]'s own open with `shred …: cannot open it for overwriting`, which aborts the
/// whole wipe. That is not an accident of reuse; it is the answer this ticket weighed and chose:
///
/// - It is **exactly what a locked default stream already does**, so the file's two halves cannot
///   disagree about what "this file is busy" means.
/// - A refusal here happens **before** `remove_dir_all`, so nothing is unlinked. The user's plaintext
///   stays in the session directory — visible, in a known place, and the lock is retryable. CPE-1957's
///   lesson is that over-refusing at a *wipe* costs retained plaintext; the cost is real, but retained
///   plaintext **the user can see and retry on** is strictly better than retained plaintext in extents
///   that no longer have a name, which is what a skip leaves behind and what this whole ticket is about.
/// - The one thing that is never right is a silent skip, because that is the defect.
///
/// # The enumeration failing is the one place the two policies differ, for the module's existing reason
///
/// `FindFirstStreamW` reporting `ERROR_HANDLE_EOF` means "no streams" — measured: that is what a
/// directory with none returns (a *file* always reports at least `::$DATA`). Any other failure is
/// treated like [`same_object_or_refuse`]'s `Unknown` arm and for the same stated reason: refused under
/// [`AliasPolicy::UnlinkAliasesInsteadOfOverwriting`], because the session tree is the app's own
/// directory on a local volume where the call does not fail; **accepted as "no streams" under
/// [`AliasPolicy::ShredEveryFile`]**, because that folder is the user's own pick and may sit on a
/// volume with no stream support at all (FAT/exFAT) or behind a network redirector — refusing there
/// would break vault creation against an attacker who, by that path's threat model, is not present.
/// **Not measured:** no FAT-formatted volume was available on this machine, so what
/// `FindFirstStreamW` returns on one is not a number this comment can quote. That is precisely why the
/// lenient arm is scoped to the policy whose threat model tolerates it rather than applied everywhere.
///
/// # Non-Windows, stated because silence here would be the same defect (CPE-1986)
///
/// **There is no Unix arm and this is a declared residual, not an oversight.** Streams are NTFS; the
/// analogue on Linux and macOS is **extended attributes** — including `com.apple.ResourceFork`, where a
/// macOS resource fork lives, and `com.apple.FinderInfo`. They have the same property that matters here:
/// writing the file's data does not touch them, and `unlink` frees their storage without overwriting it.
/// So the same class of residue exists there. It is **not** closed here, for two reasons stated plainly
/// rather than left to be inferred: (a) an xattr cannot be *overwritten in place* through any portable
/// API — setting a same-length zeroed value is a request the filesystem may satisfy by allocating
/// elsewhere (ext4's external attribute block, APFS), so it would buy a weaker guarantee than this
/// Windows arm gives while reading like the same one; and (b) it would be destruction logic for two
/// platforms that cannot be exercised on the machine this was written on, which is the axis
/// PR #1103 went red on. It is written up in `docs/design/VAULT-SECURITY.md` and wants its own ticket.
///
/// **How the non-Windows arm was actually checked, since "clippy is clean" only ever meant Windows
/// here.** A real `cargo check --target x86_64-unknown-linux-gnu` is **not possible on this machine**:
/// five of this crate's transitive dependencies build C (`bzip2-sys`, `lzma-sys`, `zstd-sys`,
/// `libsqlite3-sys`, `ring`) and every one fails with `ToolNotFound: x86_64-linux-gnu-gcc`. So the
/// derivation run instead was to flip this ticket's ten `#[cfg(windows)]` attributes to `#[cfg(any())]`
/// and the `#[cfg(not(windows))]` one to `#[cfg(not(any()))]` — i.e. select the non-Windows arm *on
/// Windows* — and run `cargo clippy --locked --all-targets -- -D warnings`, which **finished clean**.
/// That is the check PR #1103 needed and did not have: it runs anywhere, and it is what proves no
/// ungated caller names a Windows-gated item.
#[cfg(windows)]
fn shred_alternate_streams(
    path: &Path,
    probed: &EntryProbe,
    scheme: ShredScheme,
    aliases: AliasPolicy,
) -> Result<(), VaultError> {
    // Same first question, and the same answer, as `overwrite_pinned_file`'s opening lines: a file with
    // more than one name (or an unreadable count) is not ours to write through, and its streams belong
    // to that same file record. Asked here as well as there so an alias is never even *enumerated* —
    // otherwise an enumeration failure would refuse a wipe over an object the wipe was never going to
    // touch. If one of these two dispositions is ever changed, change both.
    if aliases == AliasPolicy::UnlinkAliasesInsteadOfOverwriting
        && wipe_disposition(&probed.links) == WipeDisposition::UnlinkOnly
    {
        return Ok(());
    }
    let names = match alternate_stream_names(path) {
        Ok(names) => names,
        // **CPE-1929 sabotage pair, run by hand on WINDOWS 11** (`cargo test --lib`, `crates/server`;
        // baseline **2,461 passed / 0 failed / 14 ignored** at base `2f7b3206`, and **re-measured at
        // `9bfb21d7` after rebasing — identical, so #1103's 511 lines in `batch_media` moved nothing
        // here, and all three figures below were re-run there and came back the same**; **2,466 in the
        // tree this ships in** — the same baseline plus this ticket's five new tests, so the figures
        // below read five lower than what you will measure here). Disabling this refusal (returning
        // `Ok(())` from the arm) is **2,465 / 1** —
        // `cpe_1986_an_unlistable_object_refuses_the_session_wipe_and_is_waved_through_by_create_vault`.
        // Forcing the predicate to lie (`alternate_stream_names` always `Err`) is **2,439 / 27**. Both
        // legs red, so this refusal is reachable and covered, not a guard shadowed by an earlier one.
        // **The platform is named because it is the axis nobody checks:** on Linux and macOS the whole
        // `#[cfg(windows)]` arm is absent and neither leg exists at all, so a green run there says
        // nothing about either number.
        Err(why) => match aliases {
            AliasPolicy::UnlinkAliasesInsteadOfOverwriting => {
                return Err(VaultError::Format(format!(
                    "refusing to wipe {}: its alternate data streams could not be listed ({why}), so \
                     there is no way to tell whether this name is hiding another copy of your data",
                    path.display()
                )))
            }
            AliasPolicy::ShredEveryFile => return Ok(()),
        },
    };
    for name in names {
        let mut stream = path.as_os_str().to_os_string();
        stream.push(&name);
        overwrite_pinned_file(&PathBuf::from(stream), probed, scheme, aliases)?;
    }
    Ok(())
}

/// The non-Windows arm: alternate data streams are an NTFS concept and there are none to shred. See the
/// Windows arm's doc comment for the extended-attribute residual this deliberately does **not** cover on
/// Linux and macOS, and `docs/design/VAULT-SECURITY.md` for the same statement in the design doc.
#[cfg(not(windows))]
fn shred_alternate_streams(
    _path: &Path,
    _probed: &EntryProbe,
    _scheme: ShredScheme,
    _aliases: AliasPolicy,
) -> Result<(), VaultError> {
    Ok(())
}

/// Every named `$DATA` stream on `path`, as a suffix ready to append to `path` verbatim (`:name:$DATA`).
///
/// `FindFirstStreamW`/`FindNextStreamW` with `FindStreamInfoStandard`, which is the only way to see
/// these at all — `read_dir` returns names, and a name says nothing about the streams behind it.
///
/// - **`ERROR_HANDLE_EOF` from the first call is "none", not a failure.** Measured: that is what a
///   directory with no streams returns. A file always reports at least `::$DATA`.
/// - The default stream is filtered out by [`is_shreddable_alternate_stream`] — the caller has already
///   overwritten it through the plain path, and a directory has none at all.
/// - The returned name is used **verbatim**: measured, `path` + `":hidden:$DATA"` opens the stream.
/// - [`MAX_STREAMS_PER_OBJECT`] bounds the walk so a filesystem filter that never reports the end
///   cannot hang a lock. Hitting it is an error, never a truncated list quietly returned as complete.
///
/// The `Err` payload is a reason for the caller's message; the *policy* decision about what a failure
/// means lives in [`shred_alternate_streams`], with the two alias trust levels.
#[cfg(windows)]
fn alternate_stream_names(path: &Path) -> Result<Vec<std::ffi::OsString>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_HANDLE_EOF, WIN32_ERROR};
    use windows::Win32::Storage::FileSystem::{
        FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard, WIN32_FIND_STREAM_DATA,
    };

    // Same `\\?\` transformation every other open in this module goes through, so a deep session file is
    // reachable here too — a raw `CreateFileW`-family call without it is capped at `MAX_PATH`, and the
    // mismatch would make this refuse on exactly the files the rest of the wipe handles fine.
    let wide = crate::batch_media::verbatim_wide(path);
    let mut found: Vec<std::ffi::OsString> = Vec::new();
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 string alive for the whole call, and `data` is a
    // correctly-sized out-parameter of the type `FindStreamInfoStandard` names. The search handle is
    // closed on every path out of the loop below, including the error ones.
    unsafe {
        let mut data: WIN32_FIND_STREAM_DATA = std::mem::zeroed();
        let handle = match FindFirstStreamW(
            PCWSTR(wide.as_ptr()),
            FindStreamInfoStandard,
            std::ptr::addr_of_mut!(data).cast::<std::ffi::c_void>(),
            0,
        ) {
            Ok(h) => h,
            // "Reached the end of the file" is how this API says an object has no streams to report.
            Err(e) if WIN32_ERROR::from_error(&e) == Some(ERROR_HANDLE_EOF) => return Ok(found),
            Err(e) => return Err(format!("{e}")),
        };
        let mut result = Ok(());
        // Counts every entry the API hands back, NOT every entry kept: the runaway this bounds is the
        // enumeration, and a filter driver reporting an endless run of names this function *filters out*
        // would sail past a cap on `found.len()`.
        let mut seen = 0_usize;
        loop {
            // No NUL in the buffer means the name filled it, which cannot happen for a real stream name
            // (255 chars plus the two colons and `$DATA`, in 296 `u16`s). Taking the whole buffer rather
            // than an empty slice is the fail-toward-noticing direction: the resulting name will not
            // open, which refuses, where an empty one would be filtered out and silently skipped.
            let end = data.cStreamName.iter().position(|&c| c == 0).unwrap_or(data.cStreamName.len());
            let raw = &data.cStreamName[..end];
            // The decision is taken on a lossy decode while the *path* is built from the raw UTF-16, so
            // an unpaired surrogate cannot change the answer: `is_shreddable_alternate_stream` reads
            // only ASCII `:` and `$DATA`, and U+FFFD is neither.
            if is_shreddable_alternate_stream(&String::from_utf16_lossy(raw)) {
                found.push(std::ffi::OsString::from_wide(raw));
            }
            seen += 1;
            if seen > MAX_STREAMS_PER_OBJECT {
                result = Err(format!("it reports more than {MAX_STREAMS_PER_OBJECT} of them"));
                break;
            }
            if let Err(e) = FindNextStreamW(handle, std::ptr::addr_of_mut!(data).cast::<std::ffi::c_void>())
            {
                if WIN32_ERROR::from_error(&e) != Some(ERROR_HANDLE_EOF) {
                    result = Err(format!("{e}"));
                }
                break;
            }
        }
        let _ = FindClose(handle);
        result.map(|()| found)
    }
}

/// A ceiling on how many streams one object may report before the walk gives up and errors. NTFS puts
/// no small bound on the count, so this is not a correctness limit — it is there so a filesystem filter
/// that never reports the end cannot spin a lock forever. Far above anything real: the streams seen in
/// the wild are a handful (`Zone.Identifier`, `AFP_AfpInfo`, `com.dropbox.attrs`).
#[cfg(windows)]
const MAX_STREAMS_PER_OBJECT: usize = 4096;

/// Does this `FindStreamInfoStandard` stream name identify a **named `$DATA` stream** — a separate run
/// of the user's own bytes that a wipe has to overwrite?
///
/// Pure, so the rule is pinned by a table rather than only by review — the same reason
/// [`wipe_disposition`] is its own function. Names arrive in the form `:<name>:<type>`, and the default
/// stream is the degenerate `::$DATA`.
///
/// **The `$DATA` test did not fire in any measurement taken for CPE-1986, and that is said here rather
/// than left for the next reader to discover.** `FindStreamInfoStandard` returned *only* `$DATA` streams
/// on this machine: an EFS-encrypted file (`cipher /e`, exit 0) reported `::$DATA` alone with no `:$EFS:`
/// entry, and a file carrying a GUID reparse point reported `::$DATA` plus its named stream with no
/// `:$REPARSE_POINT:` entry. It is kept as a **deliberate, unexercised safety valve**: a build or a
/// filesystem filter that does report a non-`$DATA` attribute would otherwise have it opened for an
/// ordinary write, which cannot succeed, turning every wipe on such a volume into a refusal. A non-`$DATA`
/// attribute is filesystem metadata (`$EFS`, `$INDEX_ALLOCATION`, `$BITMAP`, `$REPARSE_POINT`) and not
/// somewhere an ordinary write puts a user's plaintext — a **declared residual**, in
/// `docs/design/VAULT-SECURITY.md`. Do not read a green suite as evidence that this branch was reached.
#[cfg(windows)]
fn is_shreddable_alternate_stream(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(':') else {
        return false;
    };
    let Some((stream, kind)) = rest.rsplit_once(':') else {
        return false;
    };
    !stream.is_empty() && kind.eq_ignore_ascii_case("$DATA")
}

/// Remove a symlink/junction **itself**, never its target (CPE-1653). A file symlink unlinks with
/// `remove_file`; a directory symlink or a Windows junction unlinks with `remove_dir` (`RemoveDirectoryW`
/// deletes the reparse point, it does not recurse into the target) — so try one, then the other. Neither
/// call follows the link, and neither can touch a single byte at the other end of it. Best-effort: a link
/// we cannot remove is left as the harmless debris it already was.
fn remove_link(link: &Path) {
    if std::fs::remove_file(link).is_err() {
        let _ = std::fs::remove_dir(link);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as StdHashMap;

    /// In-memory keychain fake keyed by `(service, account)` — mirrors the sidecar's test fakes so no
    /// real credential store is touched in tests.
    #[derive(Default)]
    struct MemAccess {
        map: Mutex<StdHashMap<(String, String), String>>,
    }
    impl SecretAccess for MemAccess {
        fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
            self.map
                .lock()
                .unwrap()
                .insert((service.into(), account.into()), secret.into());
            Ok(())
        }
        fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
            Ok(self
                .map
                .lock()
                .unwrap()
                .get(&(service.into(), account.into()))
                .cloned())
        }
        fn delete(&self, service: &str, account: &str) -> Result<(), String> {
            self.map
                .lock()
                .unwrap()
                .remove(&(service.into(), account.into()));
            Ok(())
        }
    }

    fn pass(s: &str) -> SecretString {
        SecretString::from(s.to_owned())
    }

    /// The app-owned session root every legitimate unlock extracts into — the test stand-in for the
    /// frontend's `appCacheDir()/vault-sessions` (CPE-1647). Deliberately NOT created here: the
    /// containment guard creates it on demand, exactly as on a fresh machine's very first unlock.
    fn sessions_root(dir: &Path) -> PathBuf {
        dir.join("vault-sessions")
    }

    /// Build a small source folder: a nested dir, a text file, an empty dir. Kept tiny so the (real,
    /// ~1s-calibrated) scrypt KDF dominates test time rather than the payload.
    fn sample_folder(dir: &Path) {
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::create_dir_all(dir.join("emptydir")).unwrap();
        std::fs::write(dir.join("top.txt"), b"top secret").unwrap();
        std::fs::write(dir.join("sub/inner.bin"), [0u8, 1, 2, 255, 254]).unwrap();
    }

    // ---- detection ------------------------------------------------------

    #[test]
    fn is_vault_detects_by_magic_and_rejects_others() {
        let dir = tempfile::tempdir().unwrap();
        // A real vault blob starts with the magic.
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("data.cpevault");
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default(), false).unwrap();
        assert!(is_vault(&blob_path), "a real sealed blob must be detected");

        // A non-vault file (even with the extension) is rejected — detection is by content.
        let impostor = dir.path().join("fake.cpevault");
        std::fs::write(&impostor, b"this is not a vault").unwrap();
        assert!(!is_vault(&impostor), "a non-magic file must not be a vault");

        // A too-short file and a missing path are clean falses (no panic).
        let tiny = dir.path().join("tiny");
        std::fs::write(&tiny, b"CPE").unwrap();
        assert!(!is_vault(&tiny));
        assert!(!is_vault(&dir.path().join("does-not-exist")));
        // A directory is not a vault.
        assert!(!is_vault(dir.path()));
    }

    // ---- create → unlock → lock round-trip ------------------------------

    #[test]
    fn create_unlock_lock_round_trips_and_lock_leaves_no_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");

        // Create does NOT touch the original by default.
        create_vault(&src, &blob_path, &pass("open sesame"), &CreateOpts::default(), false).unwrap();
        assert!(src.join("top.txt").exists(), "default create must not shred the original");
        assert!(is_vault(&blob_path));

        // Unlock into a session dir → the plaintext tree comes back byte-identical.
        let reg = VaultRegistry::default();
        let root = sessions_root(dir.path());
        let session = root.join("session");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("open sesame"), &session).unwrap();
        assert!(reg.is_unlocked(&blob_path));
        assert_eq!(std::fs::read(session.join("top.txt")).unwrap(), b"top secret");
        assert_eq!(
            std::fs::read(session.join("sub/inner.bin")).unwrap(),
            [0u8, 1, 2, 255, 254]
        );
        assert!(session.join("emptydir").is_dir(), "empty dir survives the round-trip");

        // Lock → the session dir is gone entirely (no lingering plaintext) and state is dropped.
        reg.lock(&blob_path).unwrap();
        assert!(!reg.is_unlocked(&blob_path));
        assert!(!session.exists(), "lock must wipe the session dir — no plaintext left behind");
        // The vault blob itself is untouched.
        assert!(is_vault(&blob_path));
    }

    #[test]
    fn re_unlocking_wipes_the_prior_session_dir_leaving_no_orphaned_plaintext() {
        // CPE-1249 review #1: unlocking an ALREADY-unlocked vault into a new session dir must securely wipe
        // the previous session dir's plaintext, not orphan it on disk.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default(), false).unwrap();

        let reg = VaultRegistry::default();
        let root = sessions_root(dir.path());
        let session1 = root.join("session1");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session1).unwrap();
        assert!(session1.join("top.txt").exists(), "first unlock extracts plaintext");

        // Re-unlock into a DIFFERENT session dir.
        let session2 = root.join("session2");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session2).unwrap();

        // The registry now points at the new dir, and the OLD session dir has been wiped away entirely —
        // no lingering, unreferenced plaintext.
        assert_eq!(reg.session_dir(&blob_path).as_deref(), Some(session2.as_path()));
        assert!(!session1.exists(), "re-unlock must wipe the prior session dir (no orphaned plaintext)");
        assert!(session2.join("top.txt").exists(), "the new session dir holds the plaintext");

        // And a normal lock still cleans up the current session.
        reg.lock(&blob_path).unwrap();
        assert!(!session2.exists());
        assert!(!reg.is_unlocked(&blob_path));
    }

    #[test]
    fn re_unlock_with_a_failing_old_dir_wipe_still_succeeds_and_maps_the_new_session() {
        // CPE-1249 re-review (A): the superseded-dir wipe is best-effort. If wiping the OLD session dir
        // fails, the re-unlock must STILL return Ok and leave the NEW session mapped + browsable — never
        // report the whole unlock as failed (which would orphan the live new session in the UI). Any
        // lingering old dir is left for CPE-1252's startup sweep.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default(), false).unwrap();

        let reg = VaultRegistry::default();
        let root = sessions_root(dir.path());
        let session1 = root.join("session1");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session1).unwrap();

        // Re-unlock into a new dir with a wiper that always fails on the OLD dir.
        let session2 = root.join("session2");
        let result = reg.unlock_with_wiper(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session2, |_| {
            Err(VaultError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "cannot remove",
            )))
        });

        assert!(result.is_ok(), "a failed old-dir wipe must NOT fail the unlock: {result:?}");
        // The new session is live: mapped + decrypted + browsable.
        assert_eq!(reg.session_dir(&blob_path).as_deref(), Some(session2.as_path()));
        assert!(session2.join("top.txt").exists(), "the new session dir must be valid/browsable");
        // The old dir lingers here (the injected wipe failed) — that's the accepted tradeoff, swept later.
        assert!(session1.exists(), "old dir lingers when its wipe fails (startup sweep is the backstop)");
    }

    #[test]
    fn unlock_with_wrong_passphrase_fails_and_records_no_state() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");
        create_vault(&src, &blob_path, &pass("right"), &CreateOpts::default(), false).unwrap();

        let reg = VaultRegistry::default();
        let root = sessions_root(dir.path());
        let session = root.join("session");
        let result = reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("wrong"), &session);
        assert!(matches!(result, Err(VaultError::BadPassphrase)), "got {result:?}");
        assert!(!reg.is_unlocked(&blob_path), "a failed unlock must not record unlocked state");
    }

    // ---- CPE-1645: locking RE-SEALS the session dir back into the blob -----------------------------
    //
    // The bug this pins (reported from the CPE-1630 UAT, reproduced verbatim below): `encrypt_tree` was
    // only ever called from `create_vault`, so `lock` shredded the session directory without writing
    // anything back. Everything the user created or edited while the vault was unlocked — the very
    // affordance `src/docs/20-vaults.md` advertises — was destroyed silently, with no confirmation and
    // no warning, while the docs promised locking would "re-seal" the vault.

    /// THE bug (CPE-1645), as the tester ran it: seal → unlock → write a new file + edit an existing one
    /// → lock → unlock into a FRESH session dir. The edits must still be there. Asserted by reading the
    /// bytes back off disk out of the second session, so it can only pass if the blob really carries them.
    #[test]
    fn locking_re_seals_edits_made_while_unlocked_into_the_blob() {
        let dir = tempfile::tempdir().unwrap();
        // (1) Seal a vault.
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();

        // (2) Unlock it: the contents are extracted into a session directory.
        let first = root.join("11111111-1111-1111-1111-111111111111");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &first).unwrap();
        assert_eq!(std::fs::read(first.join("top.txt")).unwrap(), b"top secret");

        // (3) Edit its contents — exactly the documented "browse, open, and edit" affordance: a brand-new
        //     file, an edit to an existing one, and a new file in a nested directory.
        std::fs::write(first.join("new-notes.txt"), b"written while unlocked").unwrap();
        std::fs::write(first.join("top.txt"), b"edited while unlocked").unwrap();
        std::fs::write(first.join("sub/also-new.bin"), [9u8; 4]).unwrap();

        // (4) Lock it.
        reg.lock(&blob_path).unwrap();
        assert!(!first.exists(), "lock must still wipe the session dir — no plaintext left behind");
        assert!(is_vault(&blob_path), "the blob must still be a readable vault after re-sealing");

        // (5) Unlock again into a FRESH session dir: every edit must have survived.
        let second = root.join("22222222-2222-2222-2222-222222222222");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &second).unwrap();
        assert_eq!(
            std::fs::read(second.join("new-notes.txt")).unwrap_or_default(),
            b"written while unlocked",
            "a file CREATED while the vault was unlocked was DESTROYED by locking"
        );
        assert_eq!(
            std::fs::read(second.join("top.txt")).unwrap(),
            b"edited while unlocked",
            "an edit made while the vault was unlocked was DESTROYED by locking"
        );
        assert_eq!(
            std::fs::read(second.join("sub/also-new.bin")).unwrap_or_default(),
            [9u8; 4],
            "a nested file created while unlocked was DESTROYED by locking"
        );
        // The rest of the original tree is still intact — re-sealing is a whole-tree seal, not a patch.
        assert_eq!(std::fs::read(second.join("sub/inner.bin")).unwrap(), [0u8, 1, 2, 255, 254]);
        assert!(second.join("emptydir").is_dir(), "an untouched empty dir survives the re-seal");
        reg.lock(&blob_path).unwrap();
    }

    /// A deletion made while unlocked is a change too: re-sealing must carry it, so the file does not
    /// reappear on the next unlock. (The flip side of the test above — proves re-seal is a faithful
    /// snapshot of the session dir, not an additive merge.)
    #[test]
    fn locking_re_seals_deletions_made_while_unlocked() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();

        let first = root.join("33333333-3333-3333-3333-333333333333");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &first).unwrap();
        std::fs::remove_file(first.join("top.txt")).unwrap();
        reg.lock(&blob_path).unwrap();

        let second = root.join("44444444-4444-4444-4444-444444444444");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &second).unwrap();
        assert!(
            !second.join("top.txt").exists(),
            "a file deleted while unlocked must stay deleted after a re-seal"
        );
        assert_eq!(std::fs::read(second.join("sub/inner.bin")).unwrap(), [0u8, 1, 2, 255, 254]);
        reg.lock(&blob_path).unwrap();
    }

    /// The verify-before-replace half of the invariant, made falsifiable exactly the way
    /// `shred_original_refuses_to_shred_when_verify_fails` makes `create_vault`'s: with a verifier that
    /// always fails, the OLD blob must survive **byte-for-byte** and the staging file must not be left
    /// lying beside it. If this ever regressed, locking would swap in an unverified blob and then shred
    /// the only other copy of the data.
    #[test]
    fn a_failed_verify_leaves_the_old_blob_byte_for_byte_and_removes_the_staging_file() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();
        let session = root.join("55555555-5555-5555-5555-555555555555");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();
        std::fs::write(session.join("top.txt"), b"edited while unlocked").unwrap();
        let before = std::fs::read(&blob_path).unwrap();

        let result = reseal_session_with_verifier(&blob_path, &session, &pass("pw"), |_, _| {
            Err(VaultError::Corrupt)
        });

        assert!(matches!(result, Err(VaultError::Format(_))), "a failed verify must refuse: {result:?}");
        assert_eq!(
            std::fs::read(&blob_path).unwrap(),
            before,
            "an unverified re-seal must NEVER replace the vault file"
        );
        assert_eq!(
            staging_leftovers(&blob_path),
            Vec::<PathBuf>::new(),
            "the staging blob must be cleaned up, not left beside the vault"
        );
        // And the working copy — the user's edit — is still there to retry from.
        assert_eq!(std::fs::read(session.join("top.txt")).unwrap(), b"edited while unlocked");
        reg.lock(&blob_path).unwrap();
        assert_eq!(
            staging_leftovers(&blob_path),
            Vec::<PathBuf>::new(),
            "a successful lock must leave no staging blob behind either"
        );
    }

    /// Every `<blob><RESEAL_STAGING_SUFFIX>*` sibling of `blob_path` currently on disk. The staging name
    /// carries a per-attempt nonce since SEC-847 finding 1, so tests match the family, not one name.
    fn staging_leftovers(blob_path: &Path) -> Vec<PathBuf> {
        let parent = blob_path.parent().unwrap();
        let prefix = format!(
            "{}{}",
            blob_path.file_name().unwrap().to_string_lossy(),
            RESEAL_STAGING_SUFFIX
        );
        let mut found: Vec<PathBuf> = std::fs::read_dir(parent)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().map(|n| n.to_string_lossy().starts_with(&prefix)).unwrap_or(false)
            })
            .collect();
        found.sort();
        found
    }

    /// ORDERING, pinned directly: when the re-seal fails, the wipe **never runs**. Both steps are
    /// injected, so this asserts the sequencing itself rather than a downstream symptom — the vault stays
    /// unlocked, the working copy (with the user's edit in it) is untouched, and the failure is reported.
    #[test]
    fn a_failed_reseal_never_reaches_the_wipe_and_leaves_the_vault_unlocked() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();
        let session = root.join("66666666-6666-6666-6666-666666666666");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();
        std::fs::write(session.join("unsaved.txt"), b"hours of work").unwrap();

        let wiped = AtomicBool::new(false);
        let result = reg.lock_with(
            &blob_path,
            |_, _, _| Err(VaultError::Io(std::io::Error::other("disk full"))),
            |_| {
                wiped.store(true, Ordering::SeqCst);
                Ok(())
            },
        );

        assert!(result.is_err(), "a failed re-seal must surface an error");
        assert!(
            !wiped.load(Ordering::SeqCst),
            "the WIPE must not run when the re-seal failed — that is the data-loss bug itself"
        );
        assert!(reg.is_unlocked(&blob_path), "a failed re-seal must leave the vault unlocked (retryable)");
        assert_eq!(
            std::fs::read(session.join("unsaved.txt")).unwrap(),
            b"hours of work",
            "the user's unsaved work must still be on disk after a failed lock"
        );

        // POSITIVE CONTROL: the same call with a re-seal that succeeds DOES reach the wipe, so the
        // assertion above is about ordering, not about the wipe being unreachable.
        let wiped = AtomicBool::new(false);
        reg.lock_with(
            &blob_path,
            |_, _, _| Ok(()),
            |_| {
                wiped.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .unwrap();
        assert!(wiped.load(Ordering::SeqCst), "a successful re-seal must reach the wipe");
        assert!(!reg.is_unlocked(&blob_path));
    }

    /// A vault file living INSIDE the session directory locking is about to wipe is refused before
    /// anything is written — the re-seal analogue of
    /// `shred_original_refuses_when_dest_is_inside_the_folder_to_be_shredded`. Re-sealing there would
    /// write a perfectly good vault and then immediately shred it along with the working copy, losing
    /// both copies of the data.
    #[test]
    fn reseal_refuses_when_the_vault_file_is_inside_the_session_dir_it_would_wipe() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("vault-sessions").join("77777777-7777-7777-7777-777777777777");
        sample_folder(&session);
        // The vault file itself sits inside the session dir.
        let inside = session.join("self.cpevault");
        std::fs::write(&inside, b"pre-existing bytes").unwrap();

        let result = reseal_session(&inside, &session, &pass("pw"));

        assert!(matches!(result, Err(VaultError::Format(_))), "must refuse: {result:?}");
        assert_eq!(
            std::fs::read(&inside).unwrap(),
            b"pre-existing bytes",
            "the refusal must happen before anything is written"
        );
        assert_eq!(std::fs::read(session.join("top.txt")).unwrap(), b"top secret");
        // A nested-inside vault file is refused too (canonicalized prefix, not a name match).
        let nested = session.join("sub").join("deep.cpevault");
        std::fs::write(&nested, b"nested bytes").unwrap();
        assert!(matches!(reseal_session(&nested, &session, &pass("pw")), Err(VaultError::Format(_))));
    }

    /// A session directory that vanished from under us (deleted by the user or another tool) must still
    /// lock CLEANLY: there is nothing left to re-seal, so the blob keeps its last sealed contents and the
    /// mapping is dropped. The alternative — erroring — would wedge the vault "unlocked" forever with no
    /// user-reachable way to clear it.
    #[test]
    fn locking_a_vault_whose_session_dir_vanished_still_locks_and_leaves_the_blob_alone() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();
        let session = root.join("88888888-8888-8888-8888-888888888888");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();
        let before = std::fs::read(&blob_path).unwrap();

        std::fs::remove_dir_all(&session).unwrap();
        reg.lock(&blob_path).expect("a vanished session dir must not wedge the vault unlocked");

        assert!(!reg.is_unlocked(&blob_path));
        assert_eq!(std::fs::read(&blob_path).unwrap(), before, "the blob must be left exactly as it was");
        assert!(is_vault(&blob_path));
    }

    /// A link planted at the session path that points at another directory **inside** the root passes the
    /// containment check — so this pins the second, independent guard (CPE-1645): a session path that is
    /// *itself* a link is refused outright. Without it, re-sealing would follow the link and seal a
    /// stranger's files INTO the user's vault, overwriting the real contents, and the wipe would then
    /// shred them.
    #[test]
    fn lock_refuses_an_in_root_link_before_re_sealing_anything_through_it() {
        let dir = worktree_tempdir();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();
        let session = root.join("99999999-9999-9999-9999-999999999999");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();

        // Another vault's live session, INSIDE the root — so a link to it still passes containment.
        let decoy = root.join("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        std::fs::create_dir_all(&decoy).unwrap();
        std::fs::write(decoy.join("someone-elses.txt"), b"live plaintext").unwrap();

        std::fs::remove_dir_all(&session).unwrap();
        #[cfg(windows)]
        let made = try_junction_dir(&decoy, &session);
        #[cfg(not(windows))]
        let made = try_symlink_dir(&decoy, &session);
        if !made {
            crate::skip_notice!(
                "SKIPPED lock_refuses_an_in_root_link_before_re_sealing_anything_through_it: this \
                 OS/account cannot create a directory link. The in-root link case was NOT verified."
            );
            return;
        }
        let before = std::fs::read(&blob_path).unwrap();

        let result = reg.lock(&blob_path);

        // Checked FIRST, because it is the sharpest symptom: with this guard removed, the re-seal runs
        // through the link and the vault's real contents are replaced by the link target's — a second,
        // quieter kind of data loss that the wipe-side refusal (which fires later) does not prevent.
        assert_eq!(
            std::fs::read(&blob_path).unwrap(),
            before,
            "nothing may be re-sealed through a link — the vault's own contents would be replaced"
        );
        match result {
            Err(LockError { code, message }) => {
                assert_eq!(
                    code,
                    LockFailureCode::UntrustedSession,
                    "the frontend recovers on the CODE, not the text: {message}"
                );
                assert!(message.contains(UNTRUSTED_SESSION), "the reason must be named: {message}");
            }
            other => panic!("locking through an in-root link must be refused, got {other:?}"),
        }
        assert_eq!(
            std::fs::read(decoy.join("someone-elses.txt")).unwrap(),
            b"live plaintext",
            "the link's target must not be read into the vault or shredded"
        );
        assert!(!reg.is_unlocked(&blob_path), "a tamper refusal must not leave the vault wedged unlocked");
    }

    /// Each failure step reports its OWN code, and — SEC-847 finding 3 — the code is decided by which
    /// step failed, so **a file inside the vault cannot forge it**. The regression this pins: a wipe
    /// failure naming a file called `…can no longer be trusted.txt` used to be classified as a tamper
    /// refusal, which cleared the user's banner and told them the vault was sealed while its entire
    /// decrypted tree was still on disk.
    #[test]
    fn each_lock_failure_reports_its_own_code_and_no_file_name_can_forge_one() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();
        let session = root.join("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();

        // (a) A wipe failure whose message contains the tamper wording, because a FILE IN THE VAULT is
        //     named that way and the shred error interpolates its full path. It must still be WipeFailed.
        let hostile = format!("shred {}: cannot access — {UNTRUSTED_SESSION}", session.display());
        let wipe_failed = reg
            .lock_with_wiper(&blob_path, |_| Err(VaultError::Format(hostile.clone())))
            .unwrap_err();
        assert_eq!(
            wipe_failed.code,
            LockFailureCode::WipeFailed,
            "a wipe failure must stay a wipe failure however the files inside the vault are named"
        );
        assert!(
            wipe_failed.message.contains(UNTRUSTED_SESSION),
            "the fixture must actually carry the impersonating text, or it proves nothing"
        );
        assert!(reg.is_unlocked(&blob_path), "a wipe failure must leave the vault unlocked (retryable)");

        // (b) A re-seal failure is its own code, and also leaves the vault unlocked.
        let reseal_failed_err = reg
            .lock_with(
                &blob_path,
                |_, _, _| Err(VaultError::Format(hostile.clone())),
                |_| Ok(()),
            )
            .unwrap_err();
        assert_eq!(reseal_failed_err.code, LockFailureCode::ResealFailed);
        assert!(reg.is_unlocked(&blob_path));

        // (c) Only the containment step produces UntrustedSession — and it drops the mapping.
        let outside = dir.path().join("Documents");
        precious_dir(&outside);
        let tamper = trustworthy_session(&root, &outside).unwrap_err().to_string();
        assert!(tamper.contains(UNTRUSTED_SESSION), "{tamper}");
    }

    // ---- CPE-1647: the session dir is contained, so lock can never shred an arbitrary directory ----
    //
    // Threat model these tests encode (from the ticket, not from the implementation): a devtools or
    // automation caller holding a valid `.cpevault` blob AND its passphrase calls
    // `vault_unlock(blob, pass, <any directory>)` and then `vault_lock(blob)`. Before the guard, unlock
    // decrypted into that directory and lock securely SHREDDED everything under it. The required
    // behaviour is that unlock refuses any `session_dir` that does not resolve strictly inside the app's
    // own `vault-sessions` root — refusing up front, writing nothing, and recording no mapping — while a
    // legitimate in-root session still unlocks, is browsable, and is still wiped on lock.

    /// Seal a tiny vault under `dir` (passphrase `pw`) and return the blob path. Every refusal test uses
    /// a REAL, openable vault so the refusal can only come from the containment guard — with a bogus blob
    /// the call would fail for an unrelated reason and the test would pass even with the guard removed.
    fn sealed_vault(dir: &Path) -> PathBuf {
        let src = dir.join("src");
        sample_folder(&src);
        let blob_path = dir.join("v.cpevault");
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default(), false).unwrap();
        blob_path
    }

    /// A directory of the user's own irreplaceable files, well outside any app-owned scratch space —
    /// the thing an uncontained `session_dir` would have gotten shredded.
    fn precious_dir(path: &Path) {
        std::fs::create_dir_all(path.join("nested")).unwrap();
        std::fs::write(path.join("keepsake.txt"), b"the only copy").unwrap();
        std::fs::write(path.join("nested/photo.raw"), [7u8; 32]).unwrap();
    }

    /// Every byte of a [`precious_dir`] is still exactly where it was — read back OFF DISK, never
    /// inferred from a returned `Err`. Also asserts no vault plaintext was extracted into it.
    fn assert_precious_intact(path: &Path, why: &str) {
        assert!(path.is_dir(), "{why}: the directory itself must survive");
        // `unwrap_or_else` rather than `unwrap` so a regression reads as "this file was DESTROYED"
        // rather than an opaque `NotFound` — the failure mode these tests exist to catch.
        let read = |name: &str| {
            std::fs::read(path.join(name)).unwrap_or_else(|e| {
                panic!("{why}: pre-existing file {name} is gone/unreadable ({e}) — it was DESTROYED")
            })
        };
        assert_eq!(read("keepsake.txt"), b"the only copy", "{why}: must be byte-identical");
        assert_eq!(
            read("nested/photo.raw"),
            vec![7u8; 32],
            "{why}: pre-existing nested file must be byte-identical"
        );
        assert!(
            !path.join("top.txt").exists(),
            "{why}: no decrypted vault plaintext may be written here"
        );
    }

    /// A refusal must be a clean, specific `Format` error in the CPE-1599/1611/1630 house style
    /// ("refusing to …") — never a panic, never a bare I/O error, never a silent success.
    fn assert_refused(result: Result<(), VaultError>, what: &str) {
        match result {
            Err(VaultError::Format(msg)) => assert!(
                msg.contains("refusing to unlock"),
                "{what}: the refusal must read like the other destructive-path refusals, got: {msg}"
            ),
            other => panic!("{what}: must be refused with a clear Format error, got {other:?}"),
        }
    }

    /// Create a directory symlink/junction; `false` when the OS refuses (an unprivileged Windows box
    /// without Developer Mode) so the caller SKIPS loudly rather than passing silently.
    fn try_symlink_dir(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(target, link).is_ok()
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (target, link);
            false
        }
    }

    /// THE bug (CPE-1647): an arbitrary out-of-root `session_dir` must be refused, and a following
    /// `lock` must therefore never shred it. Verified by reading the victim's bytes back off disk after
    /// BOTH calls.
    #[test]
    fn unlock_refuses_an_out_of_root_session_dir_and_lock_never_shreds_it() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());

        // The victim: a normal user directory that has nothing to do with any vault.
        let documents = dir.path().join("Documents");
        precious_dir(&documents);

        let reg = VaultRegistry::default();
        assert_refused(
            reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &documents),
            "unlocking into an arbitrary directory",
        );
        assert_precious_intact(&documents, "after a refused unlock");
        assert!(
            !reg.is_unlocked(&blob_path),
            "a refused unlock must record no state — there must be nothing for lock to act on"
        );
        assert_eq!(reg.session_dir(&blob_path), None);

        // The second half of the attack: lock now, with the REAL wiper. Nothing was mapped, so nothing
        // is shredded.
        reg.lock(&blob_path).expect("locking a vault that never unlocked is a no-op success");
        assert_precious_intact(&documents, "after the follow-up lock");
    }

    /// `..` traversal out of the root is refused — both when the escaping path already exists on disk
    /// and when it climbs through components that do NOT exist (the case a naive
    /// "canonicalize, else trust it" resolver would wave through).
    #[test]
    fn unlock_refuses_dot_dot_traversal_out_of_the_session_root() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        std::fs::create_dir_all(&root).unwrap();

        let outside = dir.path().join("Outside");
        precious_dir(&outside);

        let reg = VaultRegistry::default();

        // (a) Every component exists: <root>/../Outside
        assert_refused(
            reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &root.join("..").join("Outside")),
            "a `..` escape through existing components",
        );
        assert_precious_intact(&outside, "after a refused `..` escape");

        // (b) The climb passes through a component that does not exist yet:
        //     <root>/<fresh-uuid>/../../Outside
        let through_missing = root
            .join("11111111-1111-1111-1111-111111111111")
            .join("..")
            .join("..")
            .join("Outside");
        assert_refused(
            reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &through_missing),
            "a `..` escape through a not-yet-existing component",
        );
        assert_precious_intact(&outside, "after a refused `..` escape through a missing component");
        assert!(!reg.is_unlocked(&blob_path), "neither escape may record unlocked state");
    }

    /// A symlink/junction INSIDE the root that points outside it is refused — the reason both sides are
    /// canonicalized rather than string-compared. Covers the link itself and a not-yet-existing child
    /// underneath it.
    #[test]
    fn unlock_refuses_a_session_dir_that_symlinks_out_of_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        std::fs::create_dir_all(&root).unwrap();

        let outside = dir.path().join("Outside");
        precious_dir(&outside);

        let link = root.join("looks-like-a-session");
        if !try_symlink_dir(&outside, &link) {
            crate::skip_notice!(
                "SKIPPED unlock_refuses_a_session_dir_that_symlinks_out_of_the_root: this OS/account \
                 cannot create a directory symlink (on Windows this needs Developer Mode or admin). \
                 The symlink-escape case was NOT verified on this run."
            );
            return;
        }

        let reg = VaultRegistry::default();
        assert_refused(
            reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &link),
            "unlocking into a symlink that leaves the root",
        );
        assert_precious_intact(&outside, "after a refused symlinked session dir");

        // And a fresh, not-yet-existing child under that symlinked ancestor is refused too.
        assert_refused(
            reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &link.join("child")),
            "unlocking into a fresh dir under a symlinked ancestor",
        );
        assert_precious_intact(&outside, "after a refused child of a symlinked session dir");
        assert!(!link.join("child").exists(), "nothing may be created through the escaping link");
        assert!(!reg.is_unlocked(&blob_path));
    }

    /// Three more fail-closed boundaries: the root ITSELF (wiping it would shred every other live
    /// session), a sibling whose name merely starts with the root's name (the `Photos`/`Photos2`
    /// prefix trap), and a root that cannot be created at all.
    #[test]
    fn unlock_refuses_the_root_itself_a_prefix_sibling_and_an_unresolvable_root() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());

        // A live session belonging to some OTHER unlocked vault, sitting inside the root.
        let other_live_session = root.join("22222222-2222-2222-2222-222222222222");
        std::fs::create_dir_all(&other_live_session).unwrap();
        std::fs::write(other_live_session.join("someone-elses.txt"), b"live plaintext").unwrap();

        let reg = VaultRegistry::default();

        // (a) The root itself.
        assert_refused(
            reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &root),
            "unlocking into the session root itself",
        );
        assert_eq!(
            std::fs::read(other_live_session.join("someone-elses.txt")).unwrap(),
            b"live plaintext",
            "another vault's live session must not be endangered"
        );

        // (b) A sibling directory whose path is a string prefix match but NOT a component match.
        let prefix_sibling = dir.path().join("vault-sessions-evil");
        precious_dir(&prefix_sibling);
        assert_refused(
            reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &prefix_sibling),
            "unlocking into a sibling whose name starts with the root's name",
        );
        assert_precious_intact(&prefix_sibling, "after a refused prefix-sibling session dir");

        // (c) A root that cannot exist: its parent is a regular file, so it can neither be created nor
        //     canonicalized. With no root to compare against, the call must fail CLOSED.
        let a_file = dir.path().join("not-a-directory");
        std::fs::write(&a_file, b"x").unwrap();
        let impossible_root = a_file.join("vault-sessions");
        let victim = dir.path().join("Victim");
        precious_dir(&victim);
        assert_refused(
            reg.unlock(SessionsRoot::new(&impossible_root), &blob_path, &pass("pw"), &victim),
            "unlocking when the session root cannot be resolved",
        );
        assert_precious_intact(&victim, "after a refused unlock with an unresolvable root");
        assert!(!reg.is_unlocked(&blob_path), "no refusal may record unlocked state");
    }

    /// NEGATIVE CONTROL: the guard must not break the feature. A legitimate session dir — a fresh UUID
    /// child of a `vault-sessions` root that does not even exist yet, exactly what
    /// `vaultStore.ts`'s `defaultAllocSessionDir` allocates on a first-ever unlock — still unlocks, is
    /// browsable, and is still securely wiped by lock. Without this, "refuse everything" would pass.
    #[test]
    fn a_legitimate_fresh_session_dir_still_unlocks_and_is_still_wiped_on_lock() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        assert!(!root.exists(), "the fixture must start with no session root at all");

        let session = root.join("33333333-3333-3333-3333-333333333333");
        let reg = VaultRegistry::default();
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session)
            .expect("a properly allocated session dir must still unlock");

        assert!(reg.is_unlocked(&blob_path));
        assert_eq!(std::fs::read(session.join("top.txt")).unwrap(), b"top secret");
        assert_eq!(
            std::fs::read(session.join("sub/inner.bin")).unwrap(),
            [0u8, 1, 2, 255, 254],
            "the whole tree must still come back byte-identical"
        );

        // A file edited/added while unlocked lives in the session dir too — lock still wipes the lot.
        std::fs::write(session.join("added-while-unlocked.txt"), b"new work").unwrap();

        reg.lock(&blob_path).expect("locking a contained session must still succeed");
        assert!(!reg.is_unlocked(&blob_path));
        assert!(!session.exists(), "lock must still securely wipe the session dir");
        assert!(root.is_dir(), "wiping a session must never remove the session root itself");
    }

    /// The guard is enforced by the ENGINE, not by the app adapter: the free function every caller goes
    /// through refuses too, so a future/alternative caller cannot reintroduce the hole by forgetting a
    /// check at its own boundary. Also pins that the refusal happens BEFORE the blob is even read.
    #[test]
    fn unlock_to_session_itself_refuses_an_out_of_root_target_before_reading_the_blob() {
        let dir = tempfile::tempdir().unwrap();
        let root = sessions_root(dir.path());
        let outside = dir.path().join("Outside");
        precious_dir(&outside);

        // Note the blob does not exist: if the containment check ran AFTER the read, this would fail
        // with an I/O error instead of the refusal, and the assertion below would catch it.
        let missing_blob = dir.path().join("nope.cpevault");
        assert_refused(
            unlock_to_session(SessionsRoot::new(&root), &missing_blob, &pass("pw"), &outside),
            "the free function's own containment check",
        );
        assert_precious_intact(&outside, "after a refused unlock_to_session");
    }

    // ---- CPE-1647 review #1: the SWAP — containment is re-proved at wipe time, not just at unlock ----
    //
    // The unlock-time guard above contains the caller's path *string*. It does not, on its own, contain
    // the *directory* that eventually gets shredded, because the two happen at different times and three
    // registered commands are enough to change what the path means in between:
    //
    //   1. vaultUnlock(blob, pass, "<sessions_root>/<uuid>")   — passes containment legitimately
    //   2. deletePermanent(["<sessions_root>/<uuid>"])          — removes the real session dir
    //   3. createJunction("C:\\Users\\me\\Documents", "<sessions_root>/<uuid>")
    //                                                           — a Windows junction: no Developer
    //                                                             Mode, no elevation (see links.rs)
    //   4. vaultLock(blob)                                      — shreds everything under Documents
    //
    // There is no race to win: nothing is wiped until `vaultLock` is called, and the caller chooses when
    // that is. These tests build the whole thing out of real filesystem objects and assert the victim's
    // bytes are still readable off disk afterwards.

    /// A temp dir for these tests created **inside this crate's own `target/`** rather than the system
    /// temp dir, so every scratch object they make — including the links they plant — stays inside the
    /// repository working tree.
    fn worktree_tempdir() -> tempfile::TempDir {
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("cpe-1647-scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        tempfile::Builder::new()
            .prefix("swap")
            .tempdir_in(&scratch)
            .expect("scratch dir inside the crate's target/ must be creatable")
    }

    /// Create a directory **junction** at `link` pointing at `target` — the no-elevation Windows
    /// primitive the exploit uses, via the very same helper the `createJunction` command exposes.
    #[cfg(windows)]
    fn try_junction_dir(target: &Path, link: &Path) -> bool {
        crate::links::create_junction(&target.to_string_lossy(), &link.to_string_lossy()).is_ok()
    }

    /// Shared body of the swap regression: unlock legitimately, remove the real session dir, plant a
    /// link at that exact path pointing at a victim seeded with real files, then lock. Asserts the
    /// victim survives byte-for-byte, that the lock refused with a reason, and that the vault is not
    /// left wedged. Returns `false` (asserting nothing) when `make_link` could not create the link, so
    /// the caller can skip LOUDLY instead of passing silently.
    fn lock_must_refuse_a_link_swapped_session(
        make_link: impl Fn(&Path, &Path) -> bool,
        kind: &str,
    ) -> bool {
        let dir = worktree_tempdir();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let session = root.join("44444444-4444-4444-4444-444444444444");

        // The victim: the user's own irreplaceable files, nowhere near any app-owned scratch space.
        let victim = dir.path().join("Documents");
        precious_dir(&victim);

        // (1) A completely legitimate unlock — the session dir passes containment on the way in.
        let reg = VaultRegistry::default();
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session)
            .expect("the legitimate unlock must succeed — the exploit starts from a VALID session");
        assert!(
            session.join("top.txt").is_file(),
            "the vault really was extracted into the session dir, so lock has something to shred"
        );

        // (2) `deletePermanent` / `moveExact` take the real session dir away, and
        // (3) a link is planted at exactly that path, pointing at the victim.
        std::fs::remove_dir_all(&session).unwrap();
        if !make_link(&victim, &session) {
            return false;
        }
        assert_eq!(
            std::fs::read(session.join("keepsake.txt")).unwrap(),
            b"the only copy",
            "the {kind} must really resolve to the victim, or this test proves nothing"
        );

        // (4) The payload. The registry still maps the blob to that — now hostile — path.
        let result = reg.lock(&blob_path);

        // The whole point, read back OFF DISK rather than inferred from the returned Err.
        assert_precious_intact(
            &victim,
            &format!("after locking a vault whose session dir was swapped for a {kind}"),
        );
        match result {
            Err(LockError { code, message }) => {
                assert_eq!(
                    code,
                    LockFailureCode::UntrustedSession,
                    "the frontend recovers on the CODE: {message}"
                );
                assert!(
                    message.contains("refusing to lock") || message.contains("refusing to wipe"),
                    "the refusal must say why, got: {message}"
                );
            }
            other => panic!("locking a {kind}-swapped session dir must be refused, got {other:?}"),
        }
        assert!(
            !reg.is_unlocked(&blob_path),
            "a refused lock must not leave the vault wedged 'unlocked' pointing at a path we have \
             decided we cannot trust — there would be no way for the user to clear it"
        );
        true
    }

    /// THE reviewer's demonstrated exploit (CPE-1647 review #1), via a Windows **junction** — the sharp
    /// end, because a junction needs neither Developer Mode nor elevation. Before the lock-time re-check
    /// this shredded every file under the victim directory.
    #[cfg(windows)]
    #[test]
    fn lock_refuses_to_shred_a_victim_dir_junctioned_over_the_session_path() {
        assert!(
            lock_must_refuse_a_link_swapped_session(try_junction_dir, "junction"),
            "creating a directory junction must succeed on Windows/NTFS — it needs no elevation, so a \
             failure here means the fixture is broken, not that the case is untestable"
        );
    }

    /// The same swap built from a **symbolic link**, so the regression is also covered on the Linux and
    /// macOS legs of the 3-OS backend matrix. Skips LOUDLY when the OS/account will not create a
    /// directory symlink — it must never pass silently.
    #[test]
    fn lock_refuses_to_shred_a_victim_dir_symlinked_over_the_session_path() {
        if !lock_must_refuse_a_link_swapped_session(try_symlink_dir, "symlink") {
            crate::skip_notice!(
                "SKIPPED lock_refuses_to_shred_a_victim_dir_symlinked_over_the_session_path: this \
                 OS/account cannot create a directory symlink (on Windows this needs Developer Mode or \
                 admin). The symlink form of the swap was NOT verified on this run — the junction form \
                 still was."
            );
        }
    }

    /// Belt-and-braces, independent of the registry (CPE-1647 review #1): called directly — bypassing
    /// `lock`'s re-check entirely — [`wipe_session_dir`] still refuses a session path that is *itself* a
    /// link, because `exists()` and `read_dir()` both silently follow a reparse point. A genuine session
    /// dir is a real directory this module extracted into and is never a link, so this has no false
    /// positives. The two guards fail closed independently.
    #[test]
    fn wipe_session_dir_refuses_a_session_path_that_is_itself_a_link() {
        let dir = worktree_tempdir();
        let victim = dir.path().join("Documents");
        precious_dir(&victim);
        let link = dir.path().join("session");

        #[cfg(windows)]
        let made = try_junction_dir(&victim, &link);
        #[cfg(not(windows))]
        let made = try_symlink_dir(&victim, &link);
        if !made {
            crate::skip_notice!(
                "SKIPPED wipe_session_dir_refuses_a_session_path_that_is_itself_a_link: this OS/account \
                 cannot create a directory link. The wipe-side refusal was NOT verified on this run."
            );
            return;
        }

        let result = wipe_session_dir(&link, SESSION_WIPE_SCHEME);
        assert_precious_intact(&victim, "after wipe_session_dir was pointed straight at a link");
        match result {
            Err(VaultError::Format(msg)) => assert!(
                msg.contains("refusing to wipe"),
                "the refusal must name the reason, got: {msg}"
            ),
            other => panic!("wiping a linked session path must be refused, got {other:?}"),
        }
    }

    // ---- CPE-1630: engine-side refusal of an unconfirmed shred_original create_vault call ----------

    /// The core defence-in-depth guarantee (mirrors CPE-1611's `shred_paths_refuses_the_whole_batch_
    /// when_not_confirmed`): `shred_original: true, confirmed: false` refuses the WHOLE call — `Err`,
    /// nothing written and nothing shredded, not even a partial vault blob — with a specific reason, not
    /// a panic and not a "vault created but shred silently skipped". Verified by reading the originals'
    /// **bytes back off disk**, not by trusting the `Err`.
    #[test]
    fn create_vault_refuses_the_whole_call_when_shred_original_is_not_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");

        let opts = CreateOpts { shred_original: true, shred_scheme: ShredScheme::Zero };
        let err = create_vault(&src, &blob_path, &pass("pw"), &opts, false)
            .expect_err("an unconfirmed shred_original create_vault call must be refused, not executed");

        let msg = err.to_string();
        assert!(!msg.is_empty(), "the refusal must carry a specific reason");
        assert!(msg.to_lowercase().contains("confirm"), "refusal reason: {msg}");

        // Nothing was shredded: read the original bytes back off disk (not merely `Err`, and not merely
        // `exists()` — the actual plaintext content must be intact).
        assert!(src.exists(), "the original folder must survive an unconfirmed call");
        assert_eq!(
            std::fs::read(src.join("top.txt")).unwrap(),
            b"top secret",
            "top.txt bytes must be untouched by a refused shred"
        );
        assert_eq!(
            std::fs::read(src.join("sub/inner.bin")).unwrap(),
            [0u8, 1, 2, 255, 254],
            "sub/inner.bin bytes must be untouched by a refused shred"
        );
        // Nothing was written either: no vault blob, no partial encrypt — the whole call is refused up
        // front, before sealing even runs.
        assert!(!blob_path.exists(), "no vault blob should be written when the shred is refused");
    }

    /// The flip side: the identical call proceeds — including the existing verify-before-shred
    /// guarantee — once `confirmed` is explicitly `true`, proving the flag is load-bearing, not
    /// decorative, and that CPE-1630 doesn't regress CPE-1248's invariant.
    #[test]
    fn create_vault_proceeds_and_still_verifies_before_shred_once_confirmed_is_true() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");

        let opts = CreateOpts { shred_original: true, shred_scheme: ShredScheme::Zero };
        create_vault(&src, &blob_path, &pass("pw"), &opts, true)
            .expect("a confirmed shred_original create_vault call must be allowed to run");

        assert!(!src.exists(), "a confirmed call must actually shred the original away");
        assert!(is_vault(&blob_path));

        // The verify-before-shred guarantee still holds end to end: the sealed blob is genuinely
        // recoverable (a real unlock round-trips the plaintext back).
        let reg = VaultRegistry::default();
        let root = sessions_root(dir.path());
        let session = root.join("session");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();
        assert_eq!(std::fs::read(session.join("top.txt")).unwrap(), b"top secret");
        reg.lock(&blob_path).unwrap();
    }

    /// `confirmed` is a no-op when `shred_original` is off — the common case. A caller that (say)
    /// hardcodes `confirmed: false` for every non-destructive create must not be penalised: sealing
    /// succeeds normally and the original (never at risk) is left alone.
    #[test]
    fn confirmed_is_ignored_when_shred_original_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");

        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default(), false)
            .expect("shred_original: false must succeed regardless of confirmed");
        assert!(is_vault(&blob_path));
        assert!(src.exists(), "shred_original: false must never touch the original");
        assert_eq!(std::fs::read(src.join("top.txt")).unwrap(), b"top secret");
    }

    // ---- verify-before-shred safety invariant ---------------------------

    #[test]
    fn shred_original_refuses_to_shred_when_verify_fails() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");

        // Inject a verifier that always fails: the plaintext MUST survive.
        let opts = CreateOpts { shred_original: true, shred_scheme: ShredScheme::Zero };
        let result = create_vault_with_verifier(&src, &blob_path, &pass("pw"), &opts, true, |_, _| {
            Err(VaultError::Corrupt)
        });

        assert!(matches!(result, Err(VaultError::Corrupt)), "verify failure must propagate: {result:?}");
        assert!(src.exists(), "the original folder must survive a failed verify");
        assert_eq!(
            std::fs::read(src.join("top.txt")).unwrap(),
            b"top secret",
            "the original plaintext must be intact after a refused shred"
        );
    }

    #[test]
    fn shred_original_destroys_plaintext_only_after_a_good_verify() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");

        // Real verifier, real round-trip: after this returns, the original is gone AND the blob is
        // provably recoverable (unlock below succeeds).
        let opts = CreateOpts { shred_original: true, shred_scheme: ShredScheme::Zero };
        create_vault(&src, &blob_path, &pass("pw"), &opts, true).unwrap();
        assert!(!src.exists(), "a good verify must let the original be shredded away");
        assert!(is_vault(&blob_path));

        let reg = VaultRegistry::default();
        let root = sessions_root(dir.path());
        let session = root.join("session");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();
        assert_eq!(std::fs::read(session.join("top.txt")).unwrap(), b"top secret");
        reg.lock(&blob_path).unwrap();
    }

    /// CPE-1248 review #1 (data-loss): if the destination blob would land INSIDE the folder being
    /// shredded, `create_vault` with `shred_original` must refuse BEFORE writing/destroying anything —
    /// otherwise the just-verified encrypted copy would be shredded with the plaintext, losing both.
    #[test]
    fn shred_original_refuses_when_dest_is_inside_the_folder_to_be_shredded() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("secret");
        sample_folder(&folder);
        // A pre-existing blob living INSIDE the folder — the exact data-loss scenario.
        let dest_inside = folder.join("archive.cpevault");
        std::fs::write(&dest_inside, b"pre-existing bytes").unwrap();

        let opts = CreateOpts { shred_original: true, shred_scheme: ShredScheme::Zero };
        let result = create_vault(&folder, &dest_inside, &pass("pw"), &opts, true);
        assert!(matches!(result, Err(VaultError::Format(_))), "must refuse a dest inside the folder: {result:?}");

        // Nothing was shredded or overwritten: plaintext AND the pre-existing blob both survive intact.
        assert_eq!(std::fs::read(folder.join("top.txt")).unwrap(), b"top secret");
        assert_eq!(std::fs::read(&dest_inside).unwrap(), b"pre-existing bytes");

        // A nested-inside dest is refused too (guard is by canonicalized path prefix, not name match).
        let nested = folder.join("sub").join("deep.cpevault");
        assert!(matches!(
            create_vault(&folder, &nested, &pass("pw"), &opts, true),
            Err(VaultError::Format(_))
        ));
        assert_eq!(std::fs::read(folder.join("top.txt")).unwrap(), b"top secret");

        // A sibling dest just OUTSIDE the folder (shared name prefix) is allowed and shreds cleanly.
        let sibling = dir.path().join("secret.cpevault");
        create_vault(&folder, &sibling, &pass("pw"), &opts, true).unwrap();
        assert!(!folder.exists(), "an outside dest must still let the folder be shredded");
        assert!(is_vault(&sibling));
    }

    /// CPE-1248 review follow-up (seal⟺extract symmetry): a folder containing a name that seals but
    /// can't extract (`\` in a filename, legal on Unix) must make `create_vault` fail at ENCRYPT time —
    /// before any write or shred — so `shred_original` never destroys the original against an
    /// unextractable vault.
    #[cfg(unix)]
    #[test]
    fn shred_original_preserves_original_when_a_name_is_unextractable() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("keep.txt"), b"precious").unwrap();
        std::fs::write(src.join("my\\notes.txt"), b"legal-on-unix").unwrap();
        let blob_path = dir.path().join("v.cpevault");

        let opts = CreateOpts { shred_original: true, shred_scheme: ShredScheme::Zero };
        let result = create_vault(&src, &blob_path, &pass("pw"), &opts, true);
        assert!(matches!(result, Err(VaultError::Format(_))), "must refuse an unsealable name: {result:?}");

        // Nothing was written or shredded: the original survives and no blob was produced.
        assert_eq!(std::fs::read(src.join("keep.txt")).unwrap(), b"precious");
        assert!(!blob_path.exists(), "no vault blob should be written when sealing is refused");
    }

    /// CPE-1248 review #2 (plaintext leak): the recoverability check must run in memory and never
    /// extract plaintext to the temp dir. Guards against re-introducing the `%TEMP%` extraction that
    /// left a recoverable unshredded copy. (`vault_crypto::verify_blob`'s own unit test proves the
    /// in-memory authentication path; this asserts no on-disk artefact escapes into temp.)
    #[test]
    fn verify_before_shred_leaves_no_plaintext_in_the_temp_dir() {
        let temp = std::env::temp_dir();
        let before = count_cpe_vault_temp_dirs(&temp);

        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");
        let opts = CreateOpts { shred_original: true, shred_scheme: ShredScheme::Zero };
        create_vault(&src, &blob_path, &pass("pw"), &opts, true).unwrap();

        assert_eq!(
            before,
            count_cpe_vault_temp_dirs(&temp),
            "the recoverability verify must not extract any plaintext into the temp dir"
        );
    }

    /// Count leftover `cpe-vault*` scratch entries in `temp` (the prefix the removed disk-based verify
    /// used) — a regression tripwire for a temp-extracting verify creeping back in.
    fn count_cpe_vault_temp_dirs(temp: &Path) -> usize {
        std::fs::read_dir(temp)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with("cpe-vault"))
                    .count()
            })
            .unwrap_or(0)
    }

    /// CPE-1248 review #3: a failed wipe must leave the vault UNLOCKED (retryable), never report it
    /// "locked" while the plaintext session dir still lingers. The mapping is dropped only on a
    /// successful wipe.
    #[test]
    fn lock_stays_unlocked_and_retryable_when_the_wipe_fails() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default(), false).unwrap();

        let reg = VaultRegistry::default();
        let root = sessions_root(dir.path());
        let session = root.join("session");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();
        assert!(reg.is_unlocked(&blob_path));

        // Inject a failing wipe.
        let result = reg.lock_with_wiper(&blob_path, |_| {
            Err(VaultError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "cannot remove",
            )))
        });
        assert!(result.is_err(), "a failed wipe must surface an error");
        assert!(reg.is_unlocked(&blob_path), "a failed wipe must leave the vault unlocked (retryable)");
        assert!(session.exists(), "the session dir (plaintext) must still exist after a failed lock");

        // A subsequent real lock succeeds: state cleared and the session dir securely wiped away.
        reg.lock(&blob_path).unwrap();
        assert!(!reg.is_unlocked(&blob_path));
        assert!(!session.exists(), "a successful lock must wipe the session dir");
    }

    // ---- passphrase persistence through the fake ------------------------

    #[test]
    fn remember_stored_forget_passphrase_round_trips_through_the_keychain_seam() {
        let access = MemAccess::default();
        let blob_path = Path::new("/vaults/mine.cpevault");

        // Nothing stored initially.
        assert!(!has_stored_passphrase(&access, blob_path));
        assert!(stored_passphrase(&access, blob_path).unwrap().is_none());

        // Remember → stored value round-trips.
        remember_passphrase(&access, blob_path, &pass("hunter2")).unwrap();
        assert!(has_stored_passphrase(&access, blob_path));
        let got = stored_passphrase(&access, blob_path).unwrap().unwrap();
        assert_eq!(got.expose_secret(), "hunter2");

        // It's filed under the shared service + a per-path account (a hash, not the raw path).
        let account = account_for(blob_path);
        assert_ne!(account, blob_path.to_string_lossy());
        assert_eq!(access.get(VAULT_SERVICE, &account).unwrap().as_deref(), Some("hunter2"));

        // A different vault path has its own independent slot.
        let other = Path::new("/vaults/other.cpevault");
        assert!(!has_stored_passphrase(&access, other));

        // Forget → gone.
        forget_passphrase(&access, blob_path).unwrap();
        assert!(!has_stored_passphrase(&access, blob_path));
        assert!(stored_passphrase(&access, blob_path).unwrap().is_none());
    }

    // ---- status composition ---------------------------------------------

    #[test]
    fn compute_status_reflects_vault_unlocked_and_stored_flags() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default(), false).unwrap();

        let access = MemAccess::default();

        // Sealed, locked, no stored passphrase.
        let s = compute_status(&blob_path, false, &access);
        assert_eq!(
            s,
            VaultStatus { is_vault: true, unlocked: false, has_stored_passphrase: false }
        );

        // Remember a passphrase and mark it unlocked → both flags flip.
        remember_passphrase(&access, &blob_path, &pass("pw")).unwrap();
        let s = compute_status(&blob_path, true, &access);
        assert_eq!(
            s,
            VaultStatus { is_vault: true, unlocked: true, has_stored_passphrase: true }
        );

        // A non-vault path reports is_vault=false.
        let plain = dir.path().join("plain.txt");
        std::fs::write(&plain, b"hi").unwrap();
        assert!(!compute_status(&plain, false, &access).is_vault);
    }

    // ---- orphan-session sweep (CPE-1252) ---------------------------------

    /// The whole point: seeded "orphan" session dirs (fake plaintext, exactly what a crash-while-
    /// unlocked would leave behind) are wiped AND removed, and the sweep reports how many.
    #[test]
    fn sweep_wipes_and_removes_every_orphan_session_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_root = dir.path().join("vault-sessions");
        let orphan_a = sessions_root.join("11111111-1111-1111-1111-111111111111");
        let orphan_b = sessions_root.join("22222222-2222-2222-2222-222222222222");
        sample_folder(&orphan_a);
        sample_folder(&orphan_b);
        assert!(orphan_a.join("top.txt").exists());

        let wiped = sweep_orphan_sessions(&sessions_root).unwrap();

        assert_eq!(wiped, 2, "both orphan session dirs must be counted as wiped");
        assert!(!orphan_a.exists(), "orphan dir a must be gone after the sweep");
        assert!(!orphan_b.exists(), "orphan dir b must be gone after the sweep");
        // The root itself is left behind (only its children are session dirs); a fresh unlock can
        // still create new session dirs under it.
        assert!(sessions_root.exists());
    }

    /// A missing `vault-sessions` root (the common case — most machines never create a vault) is a
    /// clean `Ok(0)`, never an error.
    #[test]
    fn sweep_missing_root_is_ok_zero() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist").join("vault-sessions");
        assert!(!missing.exists());
        assert_eq!(sweep_orphan_sessions(&missing).unwrap(), 0);
    }

    /// An existing-but-empty root is also `Ok(0)` — nothing to wipe, no error.
    #[test]
    fn sweep_empty_root_is_ok_zero() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_root = dir.path().join("vault-sessions");
        std::fs::create_dir_all(&sessions_root).unwrap();
        assert_eq!(sweep_orphan_sessions(&sessions_root).unwrap(), 0);
        assert!(sessions_root.exists(), "the sweep must not remove the root itself");
    }

    /// A stray FILE sitting directly in `vault-sessions` (never a real session, which is always a
    /// dir) is skipped, not wiped/removed — proves the sweep only ever touches directories.
    #[test]
    fn sweep_skips_stray_files_and_keeps_going() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_root = dir.path().join("vault-sessions");
        std::fs::create_dir_all(&sessions_root).unwrap();
        let stray_file = sessions_root.join("not-a-session.txt");
        std::fs::write(&stray_file, b"stray").unwrap();
        let real_orphan = sessions_root.join("33333333-3333-3333-3333-333333333333");
        sample_folder(&real_orphan);

        let wiped = sweep_orphan_sessions(&sessions_root).unwrap();

        assert_eq!(wiped, 1, "only the real (directory) orphan counts as wiped");
        assert!(stray_file.exists(), "a stray file must be left untouched, never wiped/deleted");
        assert!(!real_orphan.exists(), "the real orphan dir must still be wiped");
    }

    /// An already-empty orphan directory (no plaintext to shred) still counts as wiped and removed —
    /// `wipe_session_dir`'s "missing directory is a no-op success" only covers a directory that's
    /// gone entirely; an empty-but-present one must still be removed by the sweep.
    #[test]
    fn sweep_counts_and_removes_an_already_empty_orphan_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_root = dir.path().join("vault-sessions");
        let empty_orphan = sessions_root.join("44444444-4444-4444-4444-444444444444");
        std::fs::create_dir_all(&empty_orphan).unwrap();

        let wiped = sweep_orphan_sessions(&sessions_root).unwrap();

        assert_eq!(wiped, 1);
        assert!(!empty_orphan.exists());
    }

    /// A dir whose wipe fails (permissions, a file held open, etc.) must NOT abort the whole sweep —
    /// the failure is injected via [`sweep_orphan_sessions_with_wiper`] (the same DI shape
    /// `lock_with_wiper`/`unlock_with_wiper` use to test their own failure paths), matching one
    /// specific dir by name and letting the rest wipe normally. Proves the "keep going" contract from
    /// the ticket's acceptance criteria without relying on a platform-specific way to make a real
    /// directory unremovable.
    #[test]
    fn sweep_keeps_going_past_a_dir_whose_wipe_fails() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_root = dir.path().join("vault-sessions");
        let failing = sessions_root.join("bad");
        let ok_a = sessions_root.join("ok-a");
        let ok_b = sessions_root.join("ok-b");
        std::fs::create_dir_all(&failing).unwrap();
        std::fs::create_dir_all(&ok_a).unwrap();
        std::fs::create_dir_all(&ok_b).unwrap();

        let wiped = sweep_orphan_sessions_with_wiper(&sessions_root, |p| {
            if p.file_name().and_then(|n| n.to_str()) == Some("bad") {
                Err(VaultError::Io(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope")))
            } else {
                std::fs::remove_dir_all(p).map_err(VaultError::Io)
            }
        })
        .unwrap();

        assert_eq!(wiped, 2, "the two good dirs must still be wiped despite one failure");
        assert!(failing.exists(), "the dir whose wipe failed must be left in place, not half-deleted");
        assert!(!ok_a.exists());
        assert!(!ok_b.exists());
    }

    // ---- CPE-1653: link debris left in the sessions root by a REFUSED lock is cleared -------------

    /// The whole of CPE-1653: a link-shaped child of the sessions root (what a refused lock leaves
    /// behind) is unlinked as debris, the LINK itself is what goes, and the victim it points at is
    /// untouched — proven by reading the target's bytes back off disk after the sweep. A real orphan dir
    /// alongside it is still wiped and still counted, and a stray file is still left alone.
    ///
    /// Runs the junction form on Windows (the sharp end: no Developer Mode, no elevation) and the symlink
    /// form elsewhere, skipping LOUDLY rather than passing silently if the OS/account will not make one.
    #[test]
    fn sweep_unlinks_link_debris_without_ever_touching_what_it_points_at() {
        let dir = worktree_tempdir();
        let sessions_root = dir.path().join("vault-sessions");
        std::fs::create_dir_all(&sessions_root).unwrap();

        // The victim the planted link points at: the user's own files, well outside the app's root.
        let victim = dir.path().join("Documents");
        precious_dir(&victim);
        let victim_bytes_before = std::fs::read(victim.join("keepsake.txt")).unwrap();

        // The debris: a link sitting at what looks like a session path, exactly as CPE-1647's refused
        // lock leaves it.
        let debris = sessions_root.join("cccccccc-cccc-cccc-cccc-cccccccccccc");
        #[cfg(windows)]
        let made = try_junction_dir(&victim, &debris);
        #[cfg(not(windows))]
        let made = try_symlink_dir(&victim, &debris);
        if !made {
            crate::skip_notice!(
                "SKIPPED sweep_unlinks_link_debris_without_ever_touching_what_it_points_at: this \
                 OS/account cannot create a directory link. Link-debris cleanup was NOT verified."
            );
            return;
        }
        assert_eq!(
            std::fs::read(debris.join("keepsake.txt")).unwrap(),
            victim_bytes_before,
            "the link must really resolve to the victim, or this test proves nothing"
        );

        // A genuine orphan and a stray file alongside it, so the existing contract is re-checked too.
        let orphan = sessions_root.join("dddddddd-dddd-dddd-dddd-dddddddddddd");
        sample_folder(&orphan);
        let stray = sessions_root.join("not-a-session.txt");
        std::fs::write(&stray, b"stray").unwrap();

        let wiped = sweep_orphan_sessions(&sessions_root).unwrap();

        // THE point: the link is gone and every byte at the other end of it is still there.
        assert!(
            std::fs::symlink_metadata(&debris).is_err(),
            "the link debris must be unlinked from the app's own sessions root"
        );
        assert_precious_intact(&victim, "after the sweep unlinked a link pointing at it");
        assert_eq!(std::fs::read(victim.join("keepsake.txt")).unwrap(), victim_bytes_before);
        // ...and the sweep's existing behaviour is unchanged.
        assert_eq!(wiped, 1, "only the real session directory counts as wiped");
        assert!(!orphan.exists(), "a real orphan dir must still be wiped");
        assert!(stray.exists(), "a stray FILE must still be left untouched");
        assert!(sessions_root.is_dir(), "the root itself is never removed");
    }

    /// A **file** symlink among the debris is unlinked too (and its target file survives), so the cleanup
    /// isn't accidentally directory-only. Unix-only: a file symlink needs privileges Windows may withhold,
    /// and the junction case above already covers the no-privilege Windows attack shape.
    #[cfg(unix)]
    #[test]
    fn sweep_unlinks_a_file_symlink_in_the_root_without_deleting_its_target() {
        let dir = worktree_tempdir();
        let sessions_root = dir.path().join("vault-sessions");
        std::fs::create_dir_all(&sessions_root).unwrap();
        let target = dir.path().join("important.txt");
        std::fs::write(&target, b"the only copy").unwrap();

        let link = sessions_root.join("eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        sweep_orphan_sessions(&sessions_root).unwrap();

        assert!(std::fs::symlink_metadata(&link).is_err(), "the file symlink must be unlinked");
        assert_eq!(std::fs::read(&target).unwrap(), b"the only copy", "its target must survive");
    }

    /// A non-`NotFound` error reading `sessions_root` itself (as opposed to a missing root) propagates
    /// as an `Err` rather than being silently swallowed to `Ok(0)` — only "missing" and "empty" are
    /// defined as clean successes per the ticket.
    #[test]
    fn sweep_propagates_a_non_missing_root_read_error() {
        // A regular FILE where a directory is expected makes `read_dir` fail with something other
        // than `NotFound` (typically `NotADirectory` / `PermissionDenied`-ish) on every platform.
        let dir = tempfile::tempdir().unwrap();
        let not_a_dir = dir.path().join("vault-sessions");
        std::fs::write(&not_a_dir, b"not a directory").unwrap();

        let result = sweep_orphan_sessions(&not_a_dir);
        assert!(result.is_err(), "reading a non-directory root must not be treated as Ok(0)");
    }

    /// Falsifiable delete-test: without the sweep, orphan dirs are exactly what's left behind, proving
    /// the test isn't vacuously true. (Companion to `sweep_wipes_and_removes_every_orphan_session_dir`
    /// above, which proves the sweep DOES clean them up.)
    #[test]
    fn without_the_sweep_orphan_dirs_would_linger() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_root = dir.path().join("vault-sessions");
        let orphan = sessions_root.join("55555555-5555-5555-5555-555555555555");
        sample_folder(&orphan);
        assert!(orphan.join("top.txt").exists(), "an un-swept orphan still holds its plaintext");
    }

    // ==== SECURITY AUDIT + REVIEW of PR #847 — regressions for every finding =======================
    //
    // Each of these was written by the independent checkers as a working exploit against the first
    // version of this feature, reproduced verbatim here (with the assertion flipped to the fixed
    // behaviour) so the hole cannot reopen. The shared adversary model is CPE-1647's: a caller holding a
    // vault and its passphrase, using only registered IPC commands (`create_hard_link`, `vault_unlock`,
    // `vault_lock`, `deletePermanent`), unelevated, with no Developer Mode and no race to win.

    /// SEC-847 finding 1: the staging file. The name used to be deterministic (`<blob>.cpe-reseal-tmp`)
    /// and was opened with `std::fs::write`, which follows links and writes THROUGH a hard link. So the
    /// attacker pre-created that name as a hard link to a victim file and simply waited: the next time
    /// the **user** clicked Lock, the victim's inode was truncated and filled with vault ciphertext
    /// (`CPEVLT1…`), verify read that same inode back and passed, and the UI reported "Locked".
    ///
    /// The fix is two-part and this pins both: the name now carries a per-attempt nonce (so the trap
    /// cannot be set at all) **and** the open is `create_new` (so a trap at the name we happen to pick is
    /// refused rather than followed). The lock must still succeed — the planted file is not ours and is
    /// simply left alone.
    #[test]
    fn a_hard_link_planted_at_the_staging_name_is_never_written_through() {
        let dir = worktree_tempdir();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();

        let victim = dir.path().join("tax-2025.xlsx");
        std::fs::write(&victim, b"VICTIM SPREADSHEET - the only copy").unwrap();

        // The old, guessable name — all the attacker ever needed was the vault's path, which `list_dir`
        // hands out.
        let legacy_staging = {
            let mut name = blob_path.file_name().unwrap().to_os_string();
            name.push(RESEAL_STAGING_SUFFIX);
            blob_path.parent().unwrap().join(name)
        };
        if crate::links::create_hard_link(&victim.to_string_lossy(), &legacy_staging.to_string_lossy())
            .is_err()
        {
            crate::skip_notice!(
                "SKIPPED a_hard_link_planted_at_the_staging_name_is_never_written_through: this volume \
                 refused a hard link. The staging-trap case was NOT verified on this run."
            );
            return;
        }

        let session = root.join("12121212-1212-1212-1212-121212121212");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();
        std::fs::write(session.join("top.txt"), b"edited while unlocked").unwrap();
        reg.lock(&blob_path).expect("a planted staging name must not stop a legitimate lock");

        // THE point, read back off disk: the victim still holds its own bytes, not vault ciphertext.
        assert_eq!(
            std::fs::read(&victim).unwrap(),
            b"VICTIM SPREADSHEET - the only copy",
            "an unrelated file was DESTROYED (and replaced with vault ciphertext) by locking"
        );
        // ...and the lock really did its job: the edit is in the vault.
        let after = root.join("13131313-1313-1313-1313-131313131313");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &after).unwrap();
        assert_eq!(std::fs::read(after.join("top.txt")).unwrap(), b"edited while unlocked");
        reg.lock(&blob_path).unwrap();
    }

    /// The guard itself, independent of naming (SEC-847 finding 1): the staging open is exclusive, so it
    /// **fails** rather than truncating whatever is already at that path — a plain file, a hard link, or
    /// a symlink. Pinned directly on the helper, because the production name is unpredictable by design
    /// and a test that had to guess it would be pinning the nonce instead of the guard.
    #[test]
    fn the_staging_open_is_exclusive_so_it_can_never_truncate_an_existing_file() {
        let dir = worktree_tempdir();
        let victim = dir.path().join("victim.txt");
        std::fs::write(&victim, b"the only copy").unwrap();

        // (a) An ordinary pre-existing file at the exact target path.
        let err = write_new_exclusive(&victim, b"vault ciphertext").unwrap_err();
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::AlreadyExists,
            "the staging open must refuse an existing path, not truncate it"
        );
        assert_eq!(std::fs::read(&victim).unwrap(), b"the only copy", "and must not have written a byte");

        // (b) A hard link to it under another name — indistinguishable from a regular file to `stat`,
        //     which is exactly why `O_EXCL` (not a metadata pre-check) is the right guard.
        let alias = dir.path().join("innocent.cpevault.cpe-reseal-tmp.deadbeef");
        if crate::links::create_hard_link(&victim.to_string_lossy(), &alias.to_string_lossy()).is_ok() {
            let err = write_new_exclusive(&alias, b"vault ciphertext").unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
            assert_eq!(std::fs::read(&victim).unwrap(), b"the only copy", "through the alias either");
        }

        // (c) A symlink pointing at it (skipped loudly where the OS will not create one).
        let link = dir.path().join("link.cpevault.cpe-reseal-tmp.cafe");
        if try_symlink_file(&victim, &link) {
            let err = write_new_exclusive(&link, b"vault ciphertext").unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
            assert_eq!(std::fs::read(&victim).unwrap(), b"the only copy", "nor through the symlink");
        } else {
            // CPE-1717: this is a skip notice, and it spent the whole conversion pass looking like it
            // was not — it never says "skip", so both the conversion sweep and the first version of
            // `fsutil`'s scan walked past it. An independent audit found it. `skip_notice!` writes to
            // the process's stderr handle, which libtest's `print!`-macro capture does not intercept;
            // the `eprintln!` this replaces reached nobody on a passing run.
            crate::skip_notice!(
                "SKIPPED part of the_staging_open_is_exclusive…: no symlink privilege here, so only \
                 the regular-file and hard-link forms were verified."
            );
        }

        // NEGATIVE CONTROL: a genuinely fresh name still opens, or the guard would just be "always fail".
        let fresh = dir.path().join("fresh.cpevault.cpe-reseal-tmp.1234");
        write_new_exclusive(&fresh, b"ok").expect("a fresh staging name must still be creatable");
        assert_eq!(std::fs::read(&fresh).unwrap(), b"ok");
    }

    /// Each attempt picks a DIFFERENT staging name, so the trap above cannot be set in advance — and a
    /// squatted name costs a retry inside the same lock rather than a failed lock.
    #[test]
    fn every_staging_attempt_uses_a_fresh_unpredictable_name() {
        let dir = tempfile::tempdir().unwrap();
        let blob = dir.path().join("v.cpevault");
        let names: std::collections::HashSet<PathBuf> =
            (0..32).map(|_| staging_blob_path_with(&blob, &staging_nonce())).collect();
        assert_eq!(names.len(), 32, "staging names must not repeat");
        for n in &names {
            let name = n.file_name().unwrap().to_string_lossy().to_string();
            assert!(name.starts_with("v.cpevault"), "must stay beside the vault, same volume: {name}");
            assert!(name.contains(RESEAL_STAGING_SUFFIX), "must stay recognisable to the sweep: {name}");
            assert_ne!(
                name,
                format!("v.cpevault{RESEAL_STAGING_SUFFIX}"),
                "the old deterministic name is the vulnerability — it must never be produced"
            );
            assert_eq!(n.parent(), Some(dir.path()));
        }
    }

    /// SEC-847 finding 2, the sharp one: a **hard link** planted as a CHILD of the session dir. It is not
    /// a reparse point, so every link guard — and the crypto core's skip-every-link walk — sees an
    /// ordinary regular file. Locking therefore (a) sealed a file from anywhere on the volume into a
    /// vault whose passphrase the attacker chose, and (b) let the wipe's shredder overwrite the victim's
    /// real file through the alias. Now the lock refuses, and the exploit's own two impact checks are
    /// asserted off disk.
    #[test]
    fn lock_refuses_a_hard_linked_file_inside_the_session_dir_and_touches_nothing() {
        let dir = worktree_tempdir();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();

        let victim_dir = dir.path().join("Documents");
        std::fs::create_dir_all(&victim_dir).unwrap();
        let victim = victim_dir.join("taxes.xlsx");
        std::fs::write(&victim, b"VICTIM PLAINTEXT - the only copy").unwrap();

        // (1) A perfectly legitimate, contained unlock of the ATTACKER's own vault.
        let session = root.join("cccccccc-cccc-cccc-cccc-cccccccccccc");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();

        // (2) createHardLink(victim, <session>/loot.xlsx) — no elevation, no Developer Mode.
        let loot = session.join("loot.xlsx");
        if crate::links::create_hard_link(&victim.to_string_lossy(), &loot.to_string_lossy()).is_err() {
            crate::skip_notice!(
                "SKIPPED lock_refuses_a_hard_linked_file_inside_the_session_dir_and_touches_nothing: \
                 this OS/volume refused a hard link. The alias case was NOT verified on this run."
            );
            return;
        }

        // (3) The whole exploit was this one call. It must now refuse — and refuse RETRYABLY, having
        //     written and destroyed nothing.
        let err = reg.lock(&blob_path).expect_err("a hard-linked file in the session dir must refuse");
        assert_eq!(err.code, LockFailureCode::ResealFailed, "retryable, nothing destroyed: {err:?}");
        assert!(err.message.contains("hard link"), "the refusal must say why: {}", err.message);

        // IMPACT A (integrity): the victim's own file is untouched — the shredder never ran.
        assert_eq!(
            std::fs::read(&victim).unwrap(),
            b"VICTIM PLAINTEXT - the only copy",
            "the victim's file outside the session dir was DESTROYED by the wipe"
        );
        // IMPACT B (confidentiality): nothing of the victim's reached the vault. Unlock a fresh session
        // from the blob and prove the loot is not in it.
        assert!(reg.is_unlocked(&blob_path), "a refused re-seal leaves the vault unlocked (retryable)");
        std::fs::remove_file(&loot).unwrap(); // the user removes the alias, as the message tells them to
        reg.lock(&blob_path).expect("with the alias gone, the same vault locks normally");
        let check = root.join("dddddddd-dddd-dddd-dddd-dddddddddddd");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &check).unwrap();
        assert!(
            std::fs::read(check.join("loot.xlsx")).is_err(),
            "a file from OUTSIDE the session dir was sealed into the vault"
        );
        reg.lock(&blob_path).unwrap();
    }

    /// SEC-847 finding 2, the belt-and-braces half: the re-seal refuses a linked session path **on its
    /// own**, without relying on the caller having checked. `wipe_session_dir` has always had this for
    /// the other destructive step; the re-seal had only the caller's check in front of it, so anything
    /// reaching it directly sealed the link target's files over the vault.
    #[test]
    fn the_reseal_refuses_a_linked_session_path_on_its_own() {
        let dir = worktree_tempdir();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());

        let victim = dir.path().join("Documents");
        precious_dir(&victim);

        std::fs::create_dir_all(&root).unwrap();
        let session = root.join("eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee");
        #[cfg(windows)]
        let made = try_junction_dir(&victim, &session);
        #[cfg(not(windows))]
        let made = try_symlink_dir(&victim, &session);
        if !made {
            crate::skip_notice!(
                "SKIPPED the_reseal_refuses_a_linked_session_path_on_its_own: this OS/account cannot \
                 create a directory link. The independent re-seal guard was NOT verified on this run."
            );
            return;
        }
        let before = std::fs::read(&blob_path).unwrap();

        // Called exactly the way `lock_with` calls it once `trustworthy_session` has returned Ok.
        let result = reseal_session(&blob_path, &session, &pass("pw"));

        assert!(
            result.is_err(),
            "the re-seal must refuse a link at the session path on its own, like wipe_session_dir does"
        );
        assert_eq!(
            std::fs::read(&blob_path).unwrap(),
            before,
            "the vault's real contents were REPLACED by the link target's files"
        );
        assert_precious_intact(&victim, "after a refused re-seal through a linked session path");
    }

    /// SEC-847 reviewer blocker A: two concurrent locks. Deterministic, not timing-dependent — thread A
    /// is held INSIDE its re-seal until the second lock has been attempted, which is precisely the window
    /// the reviewer exploited: B re-sealed the tree A was already shredding, wrote it over the vault, and
    /// both calls returned `Ok` over a vault of zero bytes.
    ///
    /// Reachable from the UI, not just automation: the Lock button fires un-awaited and stays mounted
    /// across a re-seal that is slow by design, so a double-click on a large vault did it.
    #[test]
    fn a_second_concurrent_lock_is_refused_and_the_vault_survives_intact() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Barrier;

        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();
        let session = root.join("aaaa1111-aaaa-1111-aaaa-111111111111");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();
        std::fs::write(session.join("top.txt"), b"edited while unlocked").unwrap();

        // Two rendezvous points make the interleaving DETERMINISTIC rather than timing-dependent:
        // `inside` proves thread A has entered its re-seal before the second lock is attempted, and
        // `attempted` keeps A parked in there until that attempt has been made.
        let inside = Barrier::new(2);
        let attempted = Barrier::new(2);
        let reseals = AtomicUsize::new(0);

        std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                reg.lock_with(
                    &blob_path,
                    |blob, dir, pass| {
                        reseals.fetch_add(1, Ordering::SeqCst);
                        inside.wait(); // "A is now inside the re-seal"
                        attempted.wait(); // ...and stays there until the second lock has been tried
                        reseal_session(blob, dir, pass)
                    },
                    |dir| wipe_session_dir(dir, SESSION_WIPE_SCHEME),
                )
            });

            inside.wait();
            // THE second lock, arriving while the first is mid-flight — a second Lock click.
            let second = reg.lock(&blob_path);
            attempted.wait();
            let first = a.join().unwrap();

            assert!(first.is_ok(), "the first lock must still succeed: {first:?}");
            let err = second.expect_err("a concurrent second lock must be refused, not run in parallel");
            assert_eq!(err.code, LockFailureCode::AlreadyLocking, "{err:?}");
        });

        assert_eq!(
            reseals.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the refused call must not have re-sealed anything — that second re-seal IS the data loss"
        );
        assert!(!reg.is_unlocked(&blob_path));
        assert!(!session.exists(), "the (single) lock still wiped the working copy");

        // The vault is intact and holds the edit — not the empty/shredded tree the race produced.
        let check = root.join("bbbb2222-bbbb-2222-bbbb-222222222222");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &check).unwrap();
        assert_eq!(
            std::fs::read(check.join("top.txt")).unwrap(),
            b"edited while unlocked",
            "the vault was corrupted by the concurrent lock"
        );
        assert_eq!(std::fs::read(check.join("sub/inner.bin")).unwrap(), [0u8, 1, 2, 255, 254]);
        reg.lock(&blob_path).unwrap();
    }

    /// The in-flight claim is released on EVERY exit, including a failing one — otherwise one failed lock
    /// would wedge the vault as "already locking" for the life of the process, with no way back.
    #[test]
    fn the_in_flight_claim_is_released_after_a_failed_lock() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();
        let session = root.join("cccc3333-cccc-3333-cccc-333333333333");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();

        let failed = reg
            .lock_with(&blob_path, |_, _, _| Err(VaultError::Corrupt), |_| Ok(()))
            .expect_err("the injected re-seal fails");
        assert_eq!(failed.code, LockFailureCode::ResealFailed);
        assert!(reg.0.lock().unwrap().locking.is_empty(), "the claim must not outlive the call");

        // And the retry the user is told to make actually works.
        reg.lock(&blob_path).expect("a retry after a failed lock must not be refused as 'already locking'");
        assert!(!reg.is_unlocked(&blob_path));
    }

    /// SEC-847 reviewer blocker B: the cross-language contract, guarded rather than merely documented.
    ///
    /// The reviewer changed the Rust side's wording and **all 62 Rust vault tests and all 13 vaultStore
    /// tests stayed green** — the Rust test asserted `msg.contains(UNTRUSTED_SESSION)` (self-referential)
    /// and the TS test used its own verbatim fixture. In production that silently reclassified every
    /// tamper refusal as transient, leaving the banner up and navigating the user INTO the tampered path
    /// — the exact exploit CPE-1654 closed.
    ///
    /// Every [`LockFailureCode`] variant, enumerated through an **exhaustive `match`** so the list cannot
    /// silently fall behind the enum (SEC-847 round-3 nit).
    ///
    /// The guard below used to iterate a hand-written array of four. Because `classifyLockError` has a
    /// `default:` arm, a FIFTH variant added later would compile, regenerate `bindings.gen.ts` cleanly,
    /// be classified as `transient` in the UI, and leave both guards green — the exact "documentation,
    /// not a guard" shape blocker B was about. Written as a successor chain: adding a variant makes this
    /// `match` non-exhaustive, which is a compile error, and the only way to satisfy it is to give the
    /// new variant a place in the chain — which puts it in front of the frontend guard.
    fn every_lock_failure_code() -> Vec<LockFailureCode> {
        fn next(c: LockFailureCode) -> Option<LockFailureCode> {
            match c {
                LockFailureCode::UntrustedSession => Some(LockFailureCode::ResealFailed),
                LockFailureCode::ResealFailed => Some(LockFailureCode::WipeFailed),
                LockFailureCode::WipeFailed => Some(LockFailureCode::AlreadyLocking),
                LockFailureCode::AlreadyLocking => None,
            }
        }
        let mut all = vec![LockFailureCode::UntrustedSession];
        while let Some(n) = next(*all.last().unwrap()) {
            all.push(n);
        }
        all
    }

    /// Since SEC-847 finding 3 the contract is the serialised [`LockFailureCode`], not prose, so this
    /// pins the code strings: it serialises each variant through serde (so a `rename_all` change is
    /// caught too) and asserts the frontend classifier still spells it the same way. The variants come
    /// from [`every_lock_failure_code`], which the compiler keeps complete.
    #[test]
    fn the_lock_failure_codes_are_spelled_the_same_in_the_frontend() {
        let store = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("src")
            .join("lib")
            .join("vaultStore.ts");
        let source = std::fs::read_to_string(&store).unwrap_or_else(|e| {
            panic!(
                "the frontend half of the lock-failure contract must be readable at {} ({e}) — this guard \
                 exists because reciprocal doc comments are documentation, not a guard",
                store.display()
            )
        });

        for code in every_lock_failure_code() {
            let wire = serde_json::to_string(&code).expect("a code must serialise");
            let literal = wire.trim_matches('"').to_string();
            assert!(
                source.contains(&format!("\"{literal}\"")),
                "LockFailureCode::{code:?} serialises as \"{literal}\", which does not appear in \
                 vaultStore.ts — the frontend would fall back to 'transient' for it, so a tamper refusal \
                 would leave the banner up and navigate the user into the tampered path. Update \
                 `classifyLockError`/`LockFailureCode` there to match."
            );
        }
    }

    /// SEC-847 finding 4 / reviewer nit 5, pinned as **documented behaviour rather than a bug**: emptying
    /// the session dir's contents (the directory itself survives, so every guard passes) makes lock
    /// re-seal an empty tree over the vault. That is inherent to "always re-seal, never diff" — a
    /// deletion made while unlocked must be carried — but it means `vault_lock` can now empty a vault,
    /// which a **vanished** session dir deliberately cannot. Recorded in VAULT-SECURITY.md §5 and in the
    /// user docs; asserted here so the asymmetry is a decision the suite states, not a surprise.
    #[test]
    fn emptying_the_session_dir_empties_the_vault_and_this_is_deliberate() {
        let dir = tempfile::tempdir().unwrap();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();
        let session = root.join("ffffffff-ffff-ffff-ffff-ffffffffffff");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();

        for e in std::fs::read_dir(&session).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                std::fs::remove_dir_all(&p).unwrap()
            } else {
                std::fs::remove_file(&p).unwrap()
            }
        }

        reg.lock(&blob_path).expect("deleting everything and locking is a legitimate user action");

        let after = root.join("00000000-0000-0000-0000-00000000ffff");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &after).unwrap();
        let survivors: Vec<_> = std::fs::read_dir(&after).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert!(
            survivors.is_empty(),
            "documented behaviour: locking carries the deletions, so an emptied session empties the \
             vault. If this ever changes, VAULT-SECURITY.md §5 and src/docs/20-vaults.md must change too. \
             Found: {survivors:?}"
        );
        // ...and the vault is still a working vault, not a corrupt file.
        assert!(is_vault(&blob_path));
        reg.lock(&blob_path).unwrap();
    }

    // ==== SEC-847 ROUND-3 AUDIT — the alias guard was check-then-USE ==============================
    //
    // `ensure_no_aliased_files` walked the tree ONCE, at the top of the re-seal, and the counts were
    // never consulted again. `wipe_session_dir` → `shred_tree` → `collect_files` re-walked at the END and
    // overwrote every regular file it found, hard links included, with no link check of its own. Between
    // the two walks sat encrypt + the staging write + `sync_all` + a full verifying decrypt (a real
    // scrypt KDF, ~1s by design) + the rename. And there was no race to win: the staging file appearing
    // beside the `.cpevault` is a publicly observable starting gun proving the guard has already passed.
    // The auditor demonstrated a victim file zero-filled while `lock` returned `Ok(())`.

    /// The exploit itself, timing and all — the auditor's own test with the assertion flipped to the
    /// fixed behaviour. Polls for the staging file next to the vault (the starting gun) and plants
    /// `create_hard_link(victim, "<session>/loot.xlsx")` the instant it appears, i.e. strictly after both
    /// alias walks have passed. The wipe's own per-file check must now unlink that name instead of
    /// writing through it.
    ///
    /// Timing-dependent by nature, so it says so loudly when the window is missed rather than passing
    /// quietly; the deterministic half of the same guard is
    /// [`the_session_wipe_unlinks_an_alias_instead_of_overwriting_it`], which needs no thread at all.
    #[test]
    fn an_alias_planted_after_the_alias_guards_is_unlinked_not_shredded_through() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = worktree_tempdir();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let reg = VaultRegistry::default();

        let victim_dir = dir.path().join("Documents");
        std::fs::create_dir_all(&victim_dir).unwrap();
        let victim = victim_dir.join("taxes.xlsx");
        std::fs::write(&victim, b"VICTIM PLAINTEXT - the only copy").unwrap();

        let session = root.join("eeee0000-eeee-0000-eeee-000000000000");
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session).unwrap();
        std::fs::write(session.join("top.txt"), b"edited while unlocked").unwrap();

        let parent = blob_path.parent().unwrap().to_path_buf();
        let loot = session.join("loot.xlsx");
        let loot_for_thread = loot.clone();
        let victim_for_thread = victim.clone();
        let planted = Arc::new(AtomicBool::new(false));
        let planted_c = planted.clone();
        let watcher = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            while std::time::Instant::now() < deadline {
                let seen = std::fs::read_dir(&parent)
                    .map(|es| {
                        es.flatten()
                            .any(|e| e.file_name().to_string_lossy().contains(RESEAL_STAGING_SUFFIX))
                    })
                    .unwrap_or(false);
                if seen {
                    let ok = crate::links::create_hard_link(
                        &victim_for_thread.to_string_lossy(),
                        &loot_for_thread.to_string_lossy(),
                    )
                    .is_ok();
                    planted_c.store(ok, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        let result = reg.lock(&blob_path);
        watcher.join().unwrap();

        if !planted.load(Ordering::SeqCst) {
            crate::skip_notice!(
                "SKIPPED an_alias_planted_after_the_alias_guards_is_unlinked_not_shredded_through: the \
                 alias could not be planted inside the window on this run (no hard-link support, or the \
                 lock outran the watcher). The timing form of this attack was NOT verified here — the \
                 deterministic form still was."
            );
            return;
        }

        // The headline: the victim's own bytes, read back off disk after a lock that reported success.
        assert_eq!(
            std::fs::read(&victim).unwrap(),
            b"VICTIM PLAINTEXT - the only copy",
            "the victim was destroyed through an alias planted AFTER the link-count check (lock \
             returned {result:?})"
        );
        // Whether the lock succeeded or refused is not the contract here — either is acceptable, since
        // the plant may land before or after the post-encrypt re-check — but it must never both succeed
        // AND have destroyed the victim, which the assertion above is what pins.
        assert!(!loot.exists(), "the planted alias's NAME must be gone either way");
    }

    /// The same guard, deterministically and with no thread: a hard link is sitting in the session tree
    /// when the wipe runs, exactly as it would be after being planted past the re-seal's walks. The
    /// shredder must **unlink the name** rather than overwrite the inode through it.
    ///
    /// This is the test that goes red if the per-file check in [`shred_tree`] is removed.
    #[test]
    fn the_session_wipe_unlinks_an_alias_instead_of_overwriting_it() {
        let dir = worktree_tempdir();
        let victim = dir.path().join("taxes.xlsx");
        std::fs::write(&victim, b"VICTIM PLAINTEXT - the only copy").unwrap();

        let session = dir.path().join("session");
        std::fs::create_dir_all(session.join("nested")).unwrap();
        let ours = session.join("nested").join("mine.txt");
        std::fs::write(&ours, b"the vault's own file").unwrap();

        let loot = session.join("nested").join("loot.xlsx");
        if crate::links::create_hard_link(&victim.to_string_lossy(), &loot.to_string_lossy()).is_err() {
            crate::skip_notice!(
                "SKIPPED the_session_wipe_unlinks_an_alias_instead_of_overwriting_it: this OS/volume \
                 refused a hard link. The wipe's alias case was NOT verified on this run."
            );
            return;
        }

        wipe_session_dir(&session, SESSION_WIPE_SCHEME).expect("the wipe must still succeed");

        assert!(!session.exists(), "the session tree must still be removed");
        assert_eq!(
            std::fs::read(&victim).unwrap(),
            b"VICTIM PLAINTEXT - the only copy",
            "the wipe overwrote a file OUTSIDE the session dir through a hard link inside it"
        );
        assert!(!ours.exists(), "the session's own files must still be shredded and removed");
    }

    /// The wipe's per-file verdict, pinned as the pure decision it is — including the arm no filesystem
    /// will reliably produce on demand: a link count that cannot be read **must not** be treated as
    /// "probably one name, safe to overwrite". Fail closed against destroying data.
    #[test]
    fn the_wipe_never_overwrites_a_file_it_cannot_prove_is_ours() {
        assert_eq!(wipe_disposition(&HardLinks::One), WipeDisposition::Shred);
        assert_eq!(
            wipe_disposition(&HardLinks::Many(2)),
            WipeDisposition::UnlinkOnly,
            "a file with another name is not ours to overwrite"
        );
        assert_eq!(
            wipe_disposition(&HardLinks::Unknown("the filesystem did not say")),
            WipeDisposition::UnlinkOnly,
            "an unreadable link count must fail CLOSED — overwriting is irreversible, unlinking one of \
             an inode's names destroys nothing"
        );
    }

    /// The confidentiality half: an alias that appears while `encrypt_tree` is still walking the tree is
    /// invisible to the walk at the top of the re-seal, so a second walk runs after the encrypt and
    /// before anything is written beside the vault. The `after_encrypt` hook plants the link in exactly
    /// that window, deterministically.
    ///
    /// Goes red if the post-encrypt `ensure_no_aliased_files` call is removed: the re-seal would return
    /// `Ok` and replace the blob.
    #[test]
    fn an_alias_appearing_during_the_encrypt_walk_is_caught_before_the_blob_is_replaced() {
        let dir = worktree_tempdir();
        let blob_path = sealed_vault(dir.path());
        let before = std::fs::read(&blob_path).unwrap();

        let victim = dir.path().join("taxes.xlsx");
        std::fs::write(&victim, b"VICTIM PLAINTEXT - the only copy").unwrap();

        let session = dir.path().join("session");
        std::fs::create_dir_all(&session).unwrap();
        std::fs::write(session.join("top.txt"), b"edited while unlocked").unwrap();

        let loot = session.join("loot.xlsx");
        let planted = std::cell::Cell::new(false);
        let result = reseal_session_with_hooks(&blob_path, &session, &pass("pw"), verify_recoverable, || {
            planted.set(
                crate::links::create_hard_link(&victim.to_string_lossy(), &loot.to_string_lossy())
                    .is_ok(),
            );
        });

        if !planted.get() {
            crate::skip_notice!(
                "SKIPPED an_alias_appearing_during_the_encrypt_walk_is_caught_before_the_blob_is_replaced: \
                 this OS/volume refused a hard link."
            );
            return;
        }
        let why = reason(result.expect_err("an alias that appeared during the encrypt walk must be refused"));
        assert!(why.contains("hard link"), "the refusal must say why: {why}");
        assert_eq!(
            std::fs::read(&blob_path).unwrap(),
            before,
            "the vault must be byte-for-byte unchanged — nothing is replaced on a refusal"
        );
        assert!(
            std::fs::read_dir(blob_path.parent().unwrap())
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().contains(RESEAL_STAGING_SUFFIX)),
            "and the refusal must land BEFORE the staging file — the attacker's starting gun — exists"
        );
    }

    /// [`hard_link_count`] fails **closed** when the count cannot be established at all: a path that
    /// cannot be opened/stat'd is [`HardLinks::Unknown`], never [`HardLinks::One`]. Both platform
    /// implementations have an error arm that was previously assertion-free — flipping the Windows
    /// `CreateFileW` failure to `One` left the whole vault suite green.
    #[test]
    fn a_link_count_that_cannot_be_read_is_refused_not_assumed_to_be_one() {
        let dir = worktree_tempdir();
        let missing = dir.path().join("no-such-file.txt");
        assert!(
            matches!(hard_link_count(&missing), HardLinks::Unknown(_)),
            "a file whose link count cannot be read must be Unknown (refused), not assumed unaliased"
        );
        // NEGATIVE CONTROL: an ordinary file must still read as exactly one name, or the guard would
        // just be "always refuse" and every lock would fail.
        let real = dir.path().join("ordinary.txt");
        std::fs::write(&real, b"hello").unwrap();
        assert_eq!(hard_link_count(&real), HardLinks::One);
        // ...and a genuine alias reads as Many, on whichever platform will make one.
        let alias = dir.path().join("alias.txt");
        if crate::links::create_hard_link(&real.to_string_lossy(), &alias.to_string_lossy()).is_ok() {
            assert!(matches!(hard_link_count(&real), HardLinks::Many(n) if n >= 2));
            assert!(matches!(hard_link_count(&alias), HardLinks::Many(n) if n >= 2));
        }
    }

    /// The staging sweep never deletes an object it cannot prove it created — **on every platform**. The
    /// check used to be `#[cfg(unix)]`, leaving it unenforced on Windows, which is the platform where the
    /// unprivileged hard-link primitive actually exists. No data was at risk (unlinking one name of an
    /// inode destroys nothing), but the shipped rule was not the stated rule.
    #[test]
    fn the_sweep_leaves_a_hard_link_planted_at_a_staging_name() {
        let dir = worktree_tempdir();
        let blob = dir.path().join("v.cpevault");
        std::fs::write(&blob, b"not really a vault").unwrap();

        let victim = dir.path().join("taxes.xlsx");
        std::fs::write(&victim, b"VICTIM PLAINTEXT - the only copy").unwrap();
        let alias = staging_blob_path_with(&blob, "planted");
        if crate::links::create_hard_link(&victim.to_string_lossy(), &alias.to_string_lossy()).is_err() {
            crate::skip_notice!(
                "SKIPPED the_sweep_leaves_a_hard_link_planted_at_a_staging_name: this OS/volume refused \
                 a hard link."
            );
            return;
        }
        // NEGATIVE CONTROL: genuine debris from an interrupted lock, which the sweep must still clear —
        // otherwise "leave everything alone" would pass this test.
        let ours = staging_blob_path_with(&blob, "ours");
        std::fs::write(&ours, b"interrupted lock debris").unwrap();

        sweep_stale_staging(&blob);

        assert!(
            alias.exists(),
            "the sweep deleted a name it cannot prove it created — an alias for somebody else's file"
        );
        assert_eq!(std::fs::read(&victim).unwrap(), b"VICTIM PLAINTEXT - the only copy");
        assert!(!ours.exists(), "genuine staging debris must still be swept");
    }

    /// The staging blob is `sync_all`ed **before** it is verified, and therefore before the working copy
    /// is destroyed — verifying a page-cache copy proves the bytes parse, not that they reached the disk
    /// (SEC-847 reviewer blocker C). There is no portable way to interrogate the OS about this after the
    /// fact, so [`sync_durably`] counts its calls in test builds and the injected verifier reads the
    /// counter at the moment it runs: removing the `sync_all` makes the count stand still and this fails.
    #[test]
    fn the_staging_blob_is_fsynced_before_it_is_verified() {
        let dir = worktree_tempdir();
        let blob_path = sealed_vault(dir.path());
        let session = dir.path().join("session");
        std::fs::create_dir_all(&session).unwrap();
        std::fs::write(session.join("top.txt"), b"edited while unlocked").unwrap();

        // The counter is THREAD-LOCAL (see `VAULT_BLOB_SYNCS`), so the value observed inside our own
        // verify was produced by this test's own call site and nothing else. Do not "simplify" it back
        // to a process-wide atomic: with a shared counter another test's fsync can satisfy "the count
        // went up" while this call site synced nothing. Measured on a faithful reconstruction of `main`
        // (PR #861 re-review): the ordering mutation — moving the fsync to AFTER the verify — was masked
        // 4 times in 10. Not "always weaker" and not "genuinely falsifiable"; ~60% reliable, because a
        // shared counter makes falsification depend on parallel interleaving.
        //
        // The before-snapshot is **load-bearing here**, unlike at the create-side copy of this comment:
        // `sealed_vault` above calls `create_vault`, which since CPE-1669 fsyncs through the very same
        // `write_new_exclusive` **on this thread**, so the counter is already non-zero when the re-seal
        // starts (measured: `RESEAL-TEST before=1`, `CREATE-TEST before=0`). Delete the snapshot and the
        // assertion degenerates to `at_verify > 0`, which the fixture's own create satisfies — the same
        // masking the thread-local fixed cross-thread, reappearing same-thread. CPE-1669 is what created
        // that coupling, so this sentence and the code it guards were written in the same change.
        let before = vault_blob_sync_count();
        let at_verify = std::cell::Cell::new(usize::MAX);
        reseal_session_with_verifier(&blob_path, &session, &pass("pw"), |p, pw| {
            at_verify.set(vault_blob_sync_count());
            verify_recoverable(p, pw)
        })
        .expect("the re-seal itself must succeed");

        assert!(
            at_verify.get() > before,
            "the staging blob must be fsynced BEFORE it is verified — the verify read back a copy that \
             may never have reached the disk, and the caller destroys the only other copy next \
             (syncs before={before}, at verify={})",
            at_verify.get()
        );
    }

    // ---- CPE-1672: a link swapped in at a PARENT directory mid-wipe --------------------------------
    //
    // The security auditor's re-audit of PR #847 reproduced this 3/3 through the public
    // `VaultRegistry::lock`, with `lock` returning `Ok(())` and the UI saying "Locked" while a file
    // outside the vault was securely overwritten and unlinked. It is strictly worse than the hard-link
    // variant CPE-1645 closed: there the victim's inode kept its other name, so nothing was lost; here
    // the victim's ONLY name is overwritten and removed.
    //
    //   1. Before locking, plant an innocuous REAL subdirectory `<session>/zsub/` holding a real file
    //      whose name matches one in the victim directory. Every guard finds it innocent — link count 1,
    //      not a reparse point — and it is sealed into the blob.
    //   2. Starting gun: poll the session dir. The first shredded file disappearing proves the wipe is
    //      running (and, on the old collect-then-shred code, that the path list is already frozen).
    //   3. `remove_dir_all(<session>/zsub)`, then plant a junction there pointing at the victim dir.
    //   4. The wipe reaches `<session>/zsub/taxes.xlsx`, which now resolves THROUGH the junction.
    //
    // The un-named `bystander.txt` is what proves this is the shredder writing through the junctioned
    // parent rather than `remove_dir_all` recursing into it: only the name the attacker chose is hit.

    /// Bytes the victim must still have when this is over, read back OFF DISK.
    const VICTIM_BYTES: &[u8] = b"VICTIM PLAINTEXT - the only copy";

    /// Whether the timing exploit could actually be *staged* on this run — kept distinct from whether
    /// the fix held, so a missed window can never be reported as a link failure or, worse, as a pass
    /// (PR #861 audit, finding 4: the junction form hard-failed twice in seven Windows runs because the
    /// watcher thread was descheduled past the overwrite phase and woke after `remove_dir_all(root)`,
    /// and the assertion then blamed junction creation for a test that had detected nothing).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SwapOutcome {
        /// The link was planted mid-wipe and every assertion in the harness ran.
        Swapped,
        /// The wipe finished before the watcher could act — nothing was proven either way.
        WindowMissed,
        /// The window was hit, but this OS/account refused to create the link.
        LinkRefused,
    }

    impl SwapOutcome {
        const SWAPPED: u8 = 0;
        const WINDOW_MISSED: u8 = 1;
        const LINK_REFUSED: u8 = 2;

        fn from(raw: u8) -> Self {
            match raw {
                Self::SWAPPED => Self::Swapped,
                Self::LINK_REFUSED => Self::LinkRefused,
                _ => Self::WindowMissed,
            }
        }
    }

    /// Shared body of the CPE-1672 regression. Returns `false` (asserting nothing) when the swap could
    /// not be staged on this run, so the caller skips LOUDLY instead of passing silently.
    fn wipe_must_not_write_through_a_swapped_parent(
        make_link: impl Fn(&Path, &Path) -> bool + Send + 'static,
        kind: &str,
    ) -> SwapOutcome {
        use std::sync::atomic::{AtomicU8, Ordering};
        use std::sync::Arc;

        let dir = worktree_tempdir();
        let blob_path = sealed_vault(dir.path());
        let root = sessions_root(dir.path());
        let session = root.join("16720000-1672-0000-1672-000000000000");

        // The victim: the user's own Documents. `taxes.xlsx` shares its name with the decoy the attacker
        // plants inside the session tree; `bystander.txt` does not, and must survive either way.
        let victim_dir = dir.path().join("Documents");
        std::fs::create_dir_all(&victim_dir).unwrap();
        let victim = victim_dir.join("taxes.xlsx");
        std::fs::write(&victim, VICTIM_BYTES).unwrap();
        let bystander = victim_dir.join("bystander.txt");
        std::fs::write(&bystander, b"a file the attacker never named").unwrap();

        let reg = VaultRegistry::default();
        reg.unlock(SessionsRoot::new(&root), &blob_path, &pass("pw"), &session)
            .expect("the legitimate unlock must succeed — the exploit starts from a VALID session");

        // (1) The innocuous real subdirectory, planted before the lock so it is sealed into the blob.
        let zsub = session.join("zsub");
        std::fs::create_dir_all(&zsub).unwrap();
        std::fs::write(zsub.join("taxes.xlsx"), b"the attacker's decoy").unwrap();
        // Padding that sorts after the starting gun (`top.txt`) and before `zsub`, so the wipe spends a
        // measurable time shredding between the gun firing and the frozen path being reached. Without it
        // the window is a couple of syscalls wide and the exploit is a coin flip rather than 3/3.
        for i in 0..8 {
            std::fs::write(session.join(format!("y_pad_{i}.bin")), vec![0xABu8; 512 * 1024]).unwrap();
        }

        // (2) + (3) The watcher: swap the moment the gun fires.
        let session_c = session.clone();
        let victim_dir_c = victim_dir.clone();
        let outcome = Arc::new(AtomicU8::new(SwapOutcome::WINDOW_MISSED));
        let outcome_c = outcome.clone();
        let watcher = std::thread::spawn(move || {
            let gun = session_c.join("top.txt");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
            while std::time::Instant::now() < deadline {
                // The starting gun is the FIRST file's plaintext stopping being its plaintext. The
                // original code unlinked each file as it shredded it, so "gone" was the signal; the fixed
                // code only overwrites (the tree removal comes at the very end), so "gone" would never
                // fire until it was far too late to prove anything. Watching the *content* fires at the
                // same instant in both, which is what makes this one test meaningful against both.
                if std::fs::read(&gun).map(|b| b != b"top secret").unwrap_or(true) {
                    let zsub = session_c.join("zsub");
                    // Distinguish "the wipe outran us" from "this OS refuses the link". Getting these
                    // two confused is what made the Windows leg red intermittently while blaming
                    // junction creation for a test that had detected nothing (PR #861 audit, finding 4).
                    if std::fs::remove_dir_all(&zsub).is_err() {
                        outcome_c.store(SwapOutcome::WINDOW_MISSED, Ordering::SeqCst);
                    } else if make_link(&victim_dir_c, &zsub) {
                        outcome_c.store(SwapOutcome::SWAPPED, Ordering::SeqCst);
                    } else {
                        outcome_c.store(SwapOutcome::LINK_REFUSED, Ordering::SeqCst);
                    }
                    return;
                }
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
        });

        // (4) The payload.
        let result = reg.lock(&blob_path);
        watcher.join().unwrap();
        let outcome = SwapOutcome::from(outcome.load(Ordering::SeqCst));
        if outcome != SwapOutcome::Swapped {
            return outcome;
        }

        // The auditor's own probe line, printed on red AND green runs so the evidence is in the log
        // either way — in particular that the bystander survives, which is what distinguishes "the
        // shredder wrote through the junctioned parent" from "`remove_dir_all` recursed into it".
        eprintln!(
            "CPE-1672 PROBE ({kind}): swapped=true lock_ok={} victim_exists={} victim_dir_exists={} \
             bystander_exists={}",
            result.is_ok(),
            victim.exists(),
            victim_dir.is_dir(),
            bystander.exists()
        );

        // THE headline, read back off disk rather than inferred from the returned Result.
        assert_eq!(
            std::fs::read(&victim).unwrap_or_default(),
            VICTIM_BYTES,
            "the wipe shredded a file OUTSIDE the vault by resolving a path through a {kind} swapped in \
             at a PARENT directory mid-wipe (lock returned {result:?})"
        );
        assert_eq!(
            std::fs::read(&bystander).unwrap_or_default(),
            b"a file the attacker never named",
            "a file the attacker never named was destroyed too — that would mean the whole victim \
             directory was recursed into, not written through by name"
        );
        assert!(victim_dir.is_dir(), "the victim directory itself must survive");

        // The *outcome* of the lock is deliberately NOT pinned here, and that is not a weakened
        // assertion — it is the honest one. Where the swap lands inside the wipe decides it: land it
        // before the walk descends and the identity pin refuses (`WipeFailed`); land it during the final
        // `remove_dir_all` and std either reports an I/O error or unlinks the reparse point without
        // following it and the lock legitimately succeeds — having destroyed nothing outside the vault,
        // which is the only thing that was ever at stake. Pinning one of those made this test flaky
        // under load for exactly that reason. The refusal's own wording is pinned deterministically, and
        // without a thread, by `the_wipe_refuses_a_directory_that_is_not_the_object_it_was_told_to_wipe`.
        if let Err(LockError { code, message }) = &result {
            assert_eq!(
                *code,
                LockFailureCode::WipeFailed,
                "a swap that lands mid-wipe can only fail the WIPE — the re-seal is long finished by \
                 then, and the code the frontend recovers on must not be forgeable: {message}"
            );
            assert!(reg.is_unlocked(&blob_path), "a refused wipe must stay retryable");
        }
        SwapOutcome::Swapped
    }

    /// THE bug (CPE-1672), via a Windows **junction** — the sharp end, because a junction needs neither
    /// Developer Mode nor elevation. On the unfixed code this shreds `Documents\taxes.xlsx` while `lock`
    /// returns `Ok(())`.
    #[cfg(windows)]
    #[test]
    fn the_wipe_refuses_a_junction_swapped_in_at_a_parent_directory_mid_wipe() {
        match wipe_must_not_write_through_a_swapped_parent(try_junction_dir, "junction") {
            SwapOutcome::Swapped => {}
            // A junction needs no elevation on NTFS, so THIS really would mean a broken fixture.
            SwapOutcome::LinkRefused => panic!(
                "creating a directory junction must succeed on Windows/NTFS — it needs no elevation, so \
                 a refusal here means the fixture is broken, not that the case is untestable"
            ),
            // Not a failure: the wipe simply finished before the watcher could plant anything, so this
            // run detected nothing either way. Failing here would red the Windows leg intermittently
            // while blaming junction creation for a missed window (PR #861 audit, finding 4).
            SwapOutcome::WindowMissed => crate::skip_notice!(
                "SKIPPED the_wipe_refuses_a_junction_swapped_in_at_a_parent_directory_mid_wipe: the \
                 watcher thread was descheduled past the overwrite phase and the wipe finished first, so \
                 the swap could not be staged. The junction form of the parent swap was NOT verified on \
                 this run — the deterministic pins still were."
            ),
        }
    }

    /// The same swap built from a **symbolic link**, so the regression is also covered on the Linux and
    /// macOS legs of the 3-OS backend matrix.
    #[test]
    fn the_wipe_refuses_a_symlink_swapped_in_at_a_parent_directory_mid_wipe() {
        match wipe_must_not_write_through_a_swapped_parent(try_symlink_dir, "symlink") {
            SwapOutcome::Swapped => {}
            SwapOutcome::LinkRefused => crate::skip_notice!(
                "SKIPPED the_wipe_refuses_a_symlink_swapped_in_at_a_parent_directory_mid_wipe: this \
                 OS/account cannot create a directory symlink (on Windows this needs Developer Mode or \
                 admin). The symlink form of the parent swap was NOT verified on this run."
            ),
            SwapOutcome::WindowMissed => crate::skip_notice!(
                "SKIPPED the_wipe_refuses_a_symlink_swapped_in_at_a_parent_directory_mid_wipe: the wipe \
                 finished before the watcher could stage the swap. The symlink form of the parent swap \
                 was NOT verified on this run — the deterministic pins still were."
            ),
        }
    }

    /// The identity pin, deterministically and with no thread: hand the walk the identity of a
    /// *different* directory and it must refuse before overwriting a single byte. This is the arm the
    /// timing test above exercises through a real junction; here it is pinned as the plain comparison it
    /// is, so the guard cannot be removed on a machine where the swap could not be staged.
    ///
    /// Goes red if the `same_object_or_refuse` calls in `shred_dir_pinned` are removed.
    #[test]
    fn the_wipe_refuses_a_directory_that_is_not_the_object_it_was_told_to_wipe() {
        let dir = worktree_tempdir();
        let session = dir.path().join("session");
        std::fs::create_dir_all(session.join("nested")).unwrap();
        std::fs::write(session.join("nested").join("mine.txt"), b"the vault's own file").unwrap();

        // Some *other* real directory's identity, standing in for "what this path used to be".
        let other = dir.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        let wrong_id = probe_no_follow(&other).id.expect("a real local directory must have an identity");
        assert_ne!(
            wrong_id,
            probe_no_follow(&session).id.unwrap(),
            "the fixture is only meaningful if the two directories really are different objects"
        );

        let result = shred_dir_pinned(
            &session,
            Some(wrong_id),
            SESSION_WIPE_SCHEME,
            AliasPolicy::UnlinkAliasesInsteadOfOverwriting,
        );

        match result {
            Err(VaultError::Format(msg)) => assert!(
                msg.contains("refusing to wipe"),
                "the refusal must say why, got: {msg}"
            ),
            other => panic!("wiping a directory that is not the pinned object must be refused, got {other:?}"),
        }
        assert_eq!(
            std::fs::read(session.join("nested").join("mine.txt")).unwrap(),
            b"the vault's own file",
            "a refused wipe must not have overwritten anything"
        );

        // NEGATIVE CONTROL: the SAME call with the right identity must actually do the work, or the
        // guard would just be "always refuse" and every lock would fail.
        let right_id = probe_no_follow(&session).id.unwrap();
        shred_dir_pinned(
            &session,
            Some(right_id),
            SESSION_WIPE_SCHEME,
            AliasPolicy::UnlinkAliasesInsteadOfOverwriting,
        )
        .expect("the pinned wipe must succeed when the identity matches");
        assert_ne!(
            std::fs::read(session.join("nested").join("mine.txt")).unwrap(),
            b"the vault's own file",
            "the session's own file must really have been overwritten"
        );
    }

    // The destructive step's two **handle-side** checks. Both are pinned by handing `overwrite_pinned_file`
    // a probe that LIES — which is exactly what a probe taken one instant before a swap becomes. Neither
    // is reachable through the directory-level pin, and they get a test each rather than sharing one, so
    // that neutralising either turns a *distinct* test red.

    /// The hard-link count that decides is re-read from the handle the overwrite will write through, not
    /// taken from the probe. Here the probe claims one name while the object really has two — the shape
    /// of an alias planted between the enumeration and the open.
    #[test]
    fn the_overwrite_re_reads_the_link_count_from_the_handle_it_will_write_through() {
        let dir = worktree_tempdir();
        let victim = dir.path().join("taxes.xlsx");
        std::fs::write(&victim, VICTIM_BYTES).unwrap();
        let loot = dir.path().join("loot.xlsx");
        if crate::links::create_hard_link(&victim.to_string_lossy(), &loot.to_string_lossy()).is_err() {
            crate::skip_notice!(
                "SKIPPED the_overwrite_re_reads_the_link_count_from_the_handle_it_will_write_through: \
                 this OS/volume refused a hard link."
            );
            return;
        }

        let mut lying = probe_no_follow(&loot);
        assert!(
            matches!(lying.links, HardLinks::Many(_)),
            "the fixture must really be an alias, or this proves nothing"
        );
        lying.links = HardLinks::One; // ...as the probe would have read a moment before it was planted

        overwrite_pinned_file(&loot, &lying, ShredScheme::Zero, AliasPolicy::UnlinkAliasesInsteadOfOverwriting)
            .expect("an alias is declined, not an error — the tree removal takes the name");

        assert_eq!(
            std::fs::read(&victim).unwrap(),
            VICTIM_BYTES,
            "the wipe overwrote a file through an alias the probe it was handed did not know about"
        );
    }

    /// An alias is declined from the **enumeration probe**, before a write handle is ever taken on it —
    /// this module does not open a stranger's file for writing just to then decide it is not its own.
    /// That is a separate decision from the handle-side re-read above (which exists for an alias that
    /// appears *later*), so it gets its own test: pinned by making the object un-openable for writing,
    /// which a correct decline never notices and an open-first version fails the whole wipe on.
    #[test]
    fn an_alias_is_declined_before_a_write_handle_is_ever_taken_on_it() {
        let dir = worktree_tempdir();
        let victim = dir.path().join("taxes.xlsx");
        std::fs::write(&victim, VICTIM_BYTES).unwrap();
        let loot = dir.path().join("loot.xlsx");
        if crate::links::create_hard_link(&victim.to_string_lossy(), &loot.to_string_lossy()).is_err() {
            crate::skip_notice!(
                "SKIPPED an_alias_is_declined_before_a_write_handle_is_ever_taken_on_it: this OS/volume \
                 refused a hard link."
            );
            return;
        }

        let restore = |writable: bool| {
            let mut perms = std::fs::metadata(&loot).unwrap().permissions();
            perms.set_readonly(!writable);
            let _ = std::fs::set_permissions(&loot, perms);
        };
        restore(false);
        if open_existing_no_follow(&loot).is_ok() {
            restore(true);
            crate::skip_notice!(
                "SKIPPED an_alias_is_declined_before_a_write_handle_is_ever_taken_on_it: this account can \
                 open a read-only file for writing (running as root?), so the \"declined before opening\" \
                 ordering was NOT verified on this run."
            );
            return;
        }

        let probe = probe_no_follow(&loot);
        assert!(matches!(probe.links, HardLinks::Many(_)), "the fixture must really be an alias");
        let result = overwrite_pinned_file(
            &loot,
            &probe,
            ShredScheme::Zero,
            AliasPolicy::UnlinkAliasesInsteadOfOverwriting,
        );
        restore(true); // before any assertion, so a failure still leaves a cleanable temp dir

        result.expect("an alias must be declined from the probe, with no write handle ever taken on it");
        assert_eq!(std::fs::read(&victim).unwrap(), VICTIM_BYTES, "and nothing may be written to it");
    }

    /// The object behind the name is checked against the one that was enumerated. `ShredEveryFile` so the
    /// identity mismatch is the only thing that can refuse — this arm is not the alias policy's doing.
    #[test]
    fn the_overwrite_refuses_a_name_that_now_denotes_a_different_object() {
        let dir = worktree_tempdir();
        let ours = dir.path().join("mine.txt");
        std::fs::write(&ours, b"the vault's own file").unwrap();
        let other = dir.path().join("other.txt");
        std::fs::write(&other, b"someone else's file").unwrap();

        let mut lying = probe_no_follow(&ours);
        lying.id = probe_no_follow(&other).id;
        assert_ne!(lying.id, probe_no_follow(&ours).id, "the fixture must be two distinct objects");

        let err = overwrite_pinned_file(&ours, &lying, ShredScheme::Zero, AliasPolicy::ShredEveryFile)
            .expect_err("a name that now denotes a different object than the one enumerated is refused");
        assert!(reason(err).contains("refusing to wipe"), "the refusal must say why");
        assert_eq!(
            std::fs::read(&ours).unwrap(),
            b"the vault's own file",
            "and it must be refused BEFORE the first byte is written"
        );

        // NEGATIVE CONTROL: an honest probe still lets the overwrite happen, or the guard would just be
        // "always refuse" and no vault could ever be locked.
        let honest = probe_no_follow(&ours);
        overwrite_pinned_file(&ours, &honest, ShredScheme::Zero, AliasPolicy::ShredEveryFile)
            .expect("an honest probe must let the overwrite proceed");
        assert_ne!(
            std::fs::read(&ours).unwrap(),
            b"the vault's own file",
            "the file the wipe really does own must really be overwritten"
        );
    }

    /// The **link arm** of [`same_object_or_refuse`], isolated. It has to be pinned on its own because
    /// the identity arm beside it independently catches every case where an identity is readable — each
    /// of the two is sufficient for the junction swap, which is exactly why neither can be left resting
    /// on the other's test. The case only this arm answers is a link at a path whose identity could not
    /// be established, under the trust level that lets an unprovable identity through.
    #[test]
    fn a_link_is_refused_even_when_there_is_no_identity_to_compare_it_against() {
        let dir = worktree_tempdir();
        let victim = dir.path().join("Documents");
        precious_dir(&victim);
        let link = dir.path().join("as-a-link");

        #[cfg(windows)]
        let made = try_junction_dir(&victim, &link);
        #[cfg(not(windows))]
        let made = try_symlink_dir(&victim, &link);
        if !made {
            crate::skip_notice!(
                "SKIPPED a_link_is_refused_even_when_there_is_no_identity_to_compare_it_against: this \
                 OS/account cannot create a directory link."
            );
            return;
        }

        // `None` + `ShredEveryFile` is the one combination the identity arm waves through, so a refusal
        // here can only have come from the link check.
        let result = same_object_or_refuse(&link, None, AliasPolicy::ShredEveryFile, "directory");
        match result {
            Err(VaultError::Format(msg)) => {
                assert!(msg.contains("refusing to wipe"), "the refusal must say why, got: {msg}")
            }
            other => panic!("a link must be refused with no identity to compare, got {other:?}"),
        }
        // NEGATIVE CONTROL: the same call against the real directory must pass, or this arm would just
        // be "always refuse".
        same_object_or_refuse(&victim, None, AliasPolicy::ShredEveryFile, "directory")
            .expect("a real directory with an unprovable identity is allowed through at this trust level");
    }

    /// [`shred_tree`]'s own root check, independent of [`wipe_session_dir`]'s (which fires first on the
    /// production path and so would mask this one). Two guards, each pinned, failing closed separately.
    #[test]
    fn shred_tree_refuses_a_root_that_is_itself_a_link() {
        let dir = worktree_tempdir();
        let victim = dir.path().join("Documents");
        precious_dir(&victim);
        let link = dir.path().join("as-a-link");

        #[cfg(windows)]
        let made = try_junction_dir(&victim, &link);
        #[cfg(not(windows))]
        let made = try_symlink_dir(&victim, &link);
        if !made {
            crate::skip_notice!(
                "SKIPPED shred_tree_refuses_a_root_that_is_itself_a_link: this OS/account cannot create \
                 a directory link."
            );
            return;
        }

        let result = shred_tree(
            &link,
            SESSION_WIPE_SCHEME,
            AliasPolicy::UnlinkAliasesInsteadOfOverwriting,
        );
        assert_precious_intact(&victim, "after shred_tree was pointed straight at a link");
        match result {
            Err(VaultError::Format(msg)) => {
                assert!(msg.contains("refusing to wipe"), "the refusal must say why, got: {msg}")
            }
            other => panic!("shredding a linked root must be refused, got {other:?}"),
        }
    }

    /// A link planted **inside** the session tree before the wipe starts is never followed and never
    /// removed by recursing through it: the walk skips it, and the single `remove_dir_all` at the end
    /// unlinks the reparse point itself. Pins the behaviour the shredder's removal now rests on (std's
    /// `remove_dir_all` has been hardened against symlink swaps since 1.58.1, CVE-2022-21658).
    #[test]
    fn a_link_planted_inside_the_session_tree_is_never_followed_and_its_target_survives() {
        let dir = worktree_tempdir();
        let victim = dir.path().join("Documents");
        precious_dir(&victim);

        let session = dir.path().join("session");
        std::fs::create_dir_all(&session).unwrap();
        std::fs::write(session.join("mine.txt"), b"the vault's own file").unwrap();

        let planted = session.join("shortcut");
        #[cfg(windows)]
        let made = try_junction_dir(&victim, &planted);
        #[cfg(not(windows))]
        let made = try_symlink_dir(&victim, &planted);
        if !made {
            crate::skip_notice!(
                "SKIPPED a_link_planted_inside_the_session_tree_is_never_followed_and_its_target_survives: \
                 this OS/account cannot create a directory link."
            );
            return;
        }

        wipe_session_dir(&session, SESSION_WIPE_SCHEME).expect("the wipe must still succeed");

        assert!(!session.exists(), "the session tree must still be removed");
        assert_precious_intact(&victim, "after a wipe of a tree containing a link into it");
    }

    /// CPE-1957: a reparse point that does **not** stand in for another name — a OneDrive
    /// Files-On-Demand placeholder, an NTFS dedup'd file, a WOF/WIM-compressed file — is an ordinary
    /// file holding the user's plaintext, and a session wipe must **overwrite** it.
    ///
    /// **The bug this pins is a silent one, which is why it asserts on bytes and not on a `Result`.**
    /// Both of this module's link questions used to read the bare `FILE_ATTRIBUTE_REPARSE_POINT` bit:
    /// `shred_dir_pinned`'s `probe.is_link`, which `continue`s, and `overwrite_pinned_file`'s handle
    /// check, which refuses. The by-path one ran first, so such a file was dropped from the file list
    /// and never overwritten, `remove_dir_all` then unlinked the name, and the wipe reported success
    /// with the plaintext extents still on the volume. `shred_dir_pinned` is called directly rather
    /// than through `wipe_session_dir` for exactly that reason — the public entry point removes the
    /// tree, so there would be nothing left to read back, and "the call returned `Ok`" is satisfied
    /// just as well by the skip as by the fix.
    ///
    /// The two halves differ in exactly one bit (`0x2000_1957` is `0x0000_1957` with
    /// `IO_REPARSE_TAG_NAME_SURROGATE` set), which is what lets this claim the **tag** is what decides
    /// rather than the attribute. `make_guid_reparse_point` needs no privilege and no filter driver.
    /// Windows-only by construction: Unix has no reparse points and its `is_link` is already
    /// `file_type().is_symlink()`.
    ///
    /// **The tag is not merely an accident of cloud sync — it is plantable (CPE-1957 review).** The
    /// reviewer built a `REPARSE_DATA_BUFFER` carrying the real OneDrive Files-On-Demand tag
    /// `0x9000_001A` and Windows-Container-Isolation `0x8000_0018`, and `FSCTL_SET_REPARSE_POINT`
    /// accepted both **from an unprivileged process**. So the pre-fix behaviour was not only a
    /// silent skip that a OneDrive user could stumble into; it was a locally plantable way to make
    /// the lock report success over untouched plaintext. This fixture uses a GUID tag rather than
    /// those two because the bit under test is `IO_REPARSE_TAG_NAME_SURROGATE`, and the two halves
    /// below isolate exactly that bit — but the shape it stands in for is reachable, not theoretical.
    ///
    /// **Both alias policies run below**, because `wipe_session_dir` — the route the defect actually
    /// travelled — passes `UnlinkAliasesInsteadOfOverwriting`, not the `ShredEveryFile` that a reader
    /// might assume from `create_vault`. The production policy runs first so a red-proof names it:
    /// un-narrowing `EntryProbe::is_link` back to the bare bit fails here with *"the wipe left the
    /// user's plaintext on the volume under UnlinkAliasesInsteadOfOverwriting"*, measured, not assumed.
    /// A single-name file per policy keeps the alias question out of it — with exactly one name both
    /// policies must overwrite, so their agreement is a result rather than a restatement of the setup.
    ///
    /// **Red-proofed, both halves, on Windows 11 (`cargo test --lib`, `crates/server`, base `eca04c22`,
    /// re-confirmed against `2c7f69ff`).** Un-narrowing
    /// `EntryProbe::is_link` back to the bare bit gives **2,460 passed / 1 failed**, failing here on
    /// the non-surrogate half with the secret still readable — that is the live bug, reproduced. And
    /// with `probe.is_link` narrowed but `overwrite_pinned_file`'s handle check left on the bare bit,
    /// the same run gives **2,460 / 1** failing on this test's `expect` instead, with a mid-wipe
    /// refusal — which is the measurement behind the claim at both sites that narrowing either one
    /// alone makes matters worse.
    #[cfg(windows)]
    #[test]
    fn cpe_1957_a_non_surrogate_reparse_point_in_the_session_tree_is_overwritten_not_skipped() {
        use std::io::Read as _;

        const NON_SURROGATE_FILE_TAG: u32 = 0x0000_1957;
        const SURROGATE_FILE_TAG: u32 = 0x2000_1957;
        const SECRET: &[u8] = b"the user's plaintext, which a lock exists to destroy";

        let dir = worktree_tempdir();
        let session = dir.path().join("session");
        std::fs::create_dir_all(&session).unwrap();

        // **Both alias policies, because the production caller does not use the obvious one
        // (CPE-1957 review).** `wipe_session_dir` — the path this whole fix is about — passes
        // `UnlinkAliasesInsteadOfOverwriting`, while `create_vault`'s shred-original passes
        // `ShredEveryFile`. Driving only the latter would leave the defect's actual route uncovered,
        // and the two policies reach different arms of `wipe_disposition`, so agreement between them
        // is a result rather than an assumption. A fresh single-name file per policy keeps the alias
        // question out of it: with exactly one name both policies must overwrite.
        // The production policy runs FIRST, deliberately: it is the route the defect actually
        // travelled, so a red-proof that reintroduces the bug should name it rather than tripping on
        // the `create_vault` policy and never reaching it.
        for (policy, stem) in [
            (AliasPolicy::UnlinkAliasesInsteadOfOverwriting, "placeholder_session_wipe.txt"),
            (AliasPolicy::ShredEveryFile, "placeholder_shred_every.txt"),
        ] {
            let placeholder = session.join(stem);
            std::fs::write(&placeholder, SECRET).unwrap();
            if !crate::fsutil::make_guid_reparse_point(&placeholder, NON_SURROGATE_FILE_TAG, false) {
                crate::skip_notice!(
                    "SKIPPED cpe_1957_a_non_surrogate_reparse_point_in_the_session_tree_is_overwritten_not_skipped: \
                     could not plant a GUID reparse point on this volume. NOTHING on this run covered the \
                     vault wipe's treatment of a cloud placeholder."
                );
                return;
            }
            // Liveness: without the attribute this is a test that an ordinary file gets overwritten.
            assert!(
                std::os::windows::fs::MetadataExt::file_attributes(
                    &std::fs::symlink_metadata(&placeholder).unwrap()
                ) & 0x400
                    != 0,
                "fixture is inert ({policy:?}): no FILE_ATTRIBUTE_REPARSE_POINT on the placeholder"
            );
            // The control that makes this mean anything: `shred_dir_pinned`'s FIRST check is
            // `entry.file_type().is_symlink()`, which would `continue` for free and prove nothing about
            // the narrowed question.
            assert!(
                !std::fs::symlink_metadata(&placeholder).unwrap().file_type().is_symlink(),
                "fixture is shadowed ({policy:?}): std calls the non-surrogate placeholder a symlink"
            );

            let expected = probe_no_follow(&session).id;
            assert!(
                expected.is_some(),
                "fixture is unusable: the session dir has no provable identity"
            );
            shred_dir_pinned(&session, expected, ShredScheme::Zero, policy).unwrap_or_else(|e| {
                panic!(
                    "a reparse point that does not stand in for another name is an ordinary file, and \
                     refusing it fails the whole lock mid-wipe on a vault the user is trying to close \
                     (policy {policy:?}): {e}"
                )
            });

            // Read back through a no-follow open: an unrecognised reparse tag makes an ordinary open
            // fail.
            let mut after = Vec::new();
            crate::batch_media::open_existing_no_follow_read(&placeholder)
                .expect("the placeholder must still be there to read back")
                .read_to_end(&mut after)
                .unwrap();
            assert_ne!(
                after.as_slice(),
                SECRET,
                "HARM: the wipe left the user's plaintext on the volume under {policy:?} — a file \
                 carrying a non-surrogate reparse tag was skipped by the wipe and would have been \
                 unlinked by `remove_dir_all` with its extents intact, while the lock reported success"
            );
            assert!(
                after.iter().all(|&b| b == 0),
                "the zero scheme must have written zeros over the whole file under {policy:?}, not \
                 merely changed it"
            );
        }

        // The surrogate half, differing in exactly one bit: still skipped, never followed.
        let surrogate = session.join("surrogate.txt");
        std::fs::write(&surrogate, SECRET).unwrap();
        if !crate::fsutil::make_guid_reparse_point(&surrogate, SURROGATE_FILE_TAG, false) {
            crate::skip_notice!(
                "SKIPPED the surrogate half of \
                 cpe_1957_a_non_surrogate_reparse_point_in_the_session_tree_is_overwritten_not_skipped: \
                 could not plant a surrogate GUID reparse point on this volume."
            );
            return;
        }
        let expected = probe_no_follow(&session).id;
        shred_dir_pinned(&session, expected, ShredScheme::Zero, AliasPolicy::ShredEveryFile)
            .expect("a name-surrogate is skipped, not refused — the wipe walks past it");
        let mut after = Vec::new();
        crate::batch_media::open_existing_no_follow_read(&surrogate)
            .expect("the surrogate must still be there to read back")
            .read_to_end(&mut after)
            .unwrap();
        assert_eq!(
            after.as_slice(),
            SECRET,
            "a name-surrogate must never be written THROUGH — the wipe leaves the name for \
             `remove_dir_all` to unlink rather than overwriting whatever it stands for"
        );
    }

    // ---- CPE-1986: alternate data streams -----------------------------------------------------------

    /// `<base>:<stream>` — the path of one named `$DATA` stream on `base`. Built by appending rather
    /// than by `join`, because a stream is not a child name: `join` would escape the colon into a
    /// separate component on some paths and quietly test nothing.
    #[cfg(windows)]
    fn ads_path(base: &Path, stream: &str) -> PathBuf {
        let mut p = base.as_os_str().to_os_string();
        p.push(":");
        p.push(stream);
        PathBuf::from(p)
    }

    /// CPE-1986: a named `$DATA` stream on a session file — or on a session **directory** — holds the
    /// user's plaintext in its own extents, and a wipe must overwrite it.
    ///
    /// **Asserts on BYTES, never on `is_ok()`, and that is the whole point.** Before the fix the wipe
    /// returned `Ok(())`, the default stream was genuinely zeroed, and the named stream was untouched —
    /// `remove_dir_all` then took the name and left the plaintext extents on the volume while the lock
    /// reported success. Every assertion that existed on this path was satisfied by not touching the
    /// data, exactly as in CPE-1957 one layer up.
    ///
    /// `shred_dir_pinned` is driven directly rather than `wipe_session_dir`, for the same reason
    /// CPE-1957's test does: the public entry point removes the tree, so there would be nothing left to
    /// read back — and "the call returned `Ok`" is satisfied just as well by the skip as by the fix.
    /// **Both alias policies run, production first**, because `wipe_session_dir` passes
    /// `UnlinkAliasesInsteadOfOverwriting` and a reader might assume the `ShredEveryFile` that
    /// `create_vault` uses; a red-proof should name the route the defect actually travelled.
    ///
    /// **Red-proofed on Windows 11** (`cargo test --lib`, `crates/server`, baseline **2,461 passed /
    /// 0 failed / 14 ignored** at base `2f7b3206`, **re-measured identical at `9bfb21d7` after
    /// rebasing, where this red-proof was also re-run and returned the same numbers**; **2,466 in the
    /// tree this ships in** — the same baseline plus this ticket's five new tests, so the figures below
    /// read five lower than what you will measure here). Commenting out both
    /// `shred_alternate_streams` calls in `shred_dir_pinned`
    /// gives **2,464 passed / 2 failed**: this test, reporting the named stream still readable under
    /// `UnlinkAliasesInsteadOfOverwriting` — that is the live bug, reproduced — and
    /// `cpe_1986_a_stream_that_cannot_be_opened_refuses_the_wipe_rather_than_skipping_it`. The other
    /// two of the five stay green on purpose and it is worth knowing which:
    /// `..._an_aliased_files_streams_are_left_alone_...` asserts data **survives**, which the defect
    /// satisfies as well as the fix does, and `..._an_unlistable_object_refuses_...` drives
    /// `shred_alternate_streams` directly rather than through the walk. Neither speaks for the wiring;
    /// the two that fail are the ones that do.
    #[cfg(windows)]
    #[test]
    fn cpe_1986_a_named_stream_in_the_session_tree_is_overwritten_not_left_behind() {
        use std::io::Read as _;

        const MAIN: &[u8] = b"the default stream, which the wipe always overwrote";
        const HIDDEN: &[u8] = b"the user's plaintext in a named stream, which a lock exists to destroy";

        let dir = worktree_tempdir();

        // The production policy FIRST — it is the route `wipe_session_dir` takes.
        for (policy, stem) in [
            (AliasPolicy::UnlinkAliasesInsteadOfOverwriting, "session_wipe"),
            (AliasPolicy::ShredEveryFile, "shred_every"),
        ] {
            let session = dir.path().join(stem);
            let sub = session.join("sub");
            std::fs::create_dir_all(&sub).unwrap();

            let top = session.join("secret.txt");
            std::fs::write(&top, MAIN).unwrap();
            let top_ads = ads_path(&top, "hidden");
            let deep = sub.join("deep.txt");
            std::fs::write(&deep, MAIN).unwrap();
            let deep_ads = ads_path(&deep, "Zone.Identifier");
            let dir_ads = ads_path(&sub, "dirsecret");
            let empty_ads = ads_path(&top, "empty");
            for p in [&top_ads, &deep_ads, &dir_ads] {
                if std::fs::write(p, HIDDEN).is_err() {
                    crate::skip_notice!(
                        "SKIPPED cpe_1986_a_named_stream_in_the_session_tree_is_overwritten_not_left_behind: \
                         this volume cannot hold alternate data streams, so NOTHING on this run covered \
                         the vault wipe's treatment of one."
                    );
                    return;
                }
            }
            std::fs::write(&empty_ads, b"").unwrap();

            // FIXTURE LIVENESS, and it is the defect in one line: a stream is invisible to the walk.
            // If any of these were an ordinary file, `shred_dir_pinned` would have overwritten it for
            // free and this test would prove nothing about streams.
            for (root, count) in [(&session, 2_usize), (&sub, 1)] {
                let names: Vec<_> = std::fs::read_dir(root)
                    .unwrap()
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.contains(':'))
                    .collect();
                assert!(
                    names.is_empty(),
                    "fixture is inert ({policy:?}): read_dir can see {names:?} in {}, so these are \
                     ordinary files and not streams",
                    root.display()
                );
                assert_eq!(
                    std::fs::read_dir(root).unwrap().flatten().count(),
                    count,
                    "fixture is unusable ({policy:?}): unexpected entries in {}",
                    root.display()
                );
            }
            assert_eq!(
                std::fs::read(&top_ads).unwrap(),
                HIDDEN,
                "fixture is inert ({policy:?}): the named stream was not written"
            );

            let expected = probe_no_follow(&session).id;
            assert!(expected.is_some(), "fixture is unusable: the session dir has no provable identity");
            shred_dir_pinned(&session, expected, ShredScheme::Zero, policy).unwrap_or_else(|e| {
                panic!("the wipe must overwrite an ordinary file's streams, not refuse ({policy:?}): {e}")
            });

            for (stream, what) in [
                (&top_ads, "a file at the top of the session tree"),
                (&deep_ads, "a file one directory down"),
                (&dir_ads, "the subdirectory itself"),
            ] {
                let mut after = Vec::new();
                crate::batch_media::open_existing_no_follow_read(stream)
                    .expect("the stream must still be there to read back")
                    .read_to_end(&mut after)
                    .unwrap();
                assert_ne!(
                    after.as_slice(),
                    HIDDEN,
                    "HARM: the wipe left the user's plaintext on the volume under {policy:?} — a named \
                     data stream on {what} was never written, and `remove_dir_all` would have unlinked \
                     the name with the stream's extents intact while the lock reported success"
                );
                assert!(
                    after.iter().all(|&b| b == 0),
                    "the zero scheme must have written zeros over the whole stream on {what} \
                     ({policy:?}), not merely changed it"
                );
                assert_eq!(
                    after.len(),
                    HIDDEN.len(),
                    "the overwrite must cover the STREAM's length on {what} ({policy:?}) — sizing it \
                     from the default stream would leave a tail of plaintext"
                );
            }

            // The control that keeps this from being a test about streams only: the default stream is
            // still overwritten, and a zero-length stream is neither a failure nor a refusal.
            let mut main_after = Vec::new();
            crate::batch_media::open_existing_no_follow_read(&top)
                .unwrap()
                .read_to_end(&mut main_after)
                .unwrap();
            assert!(
                main_after.iter().all(|&b| b == 0) && main_after.len() == MAIN.len(),
                "the default stream must still be zeroed under {policy:?}"
            );
            assert_eq!(
                std::fs::metadata(&empty_ads).unwrap().len(),
                0,
                "a zero-length stream must survive the wipe as an ordinary no-op ({policy:?})"
            );
        }
    }

    /// CPE-1986: an **aliased** file's streams are left alone exactly like its default stream is.
    ///
    /// The session wipe's whole alias rule (SEC-847 round 3) is that a file with a second name is not
    /// ours to overwrite — the tree removal takes *our* name, which destroys nothing. A named stream
    /// lives in the same file record as the default one and is reachable through every name the record
    /// has, so writing it would destroy the other name's data just as surely. This pins that the
    /// disposition is asked **before** the streams are enumerated, not after.
    #[cfg(windows)]
    #[test]
    fn cpe_1986_an_aliased_files_streams_are_left_alone_like_its_default_stream() {
        use std::io::Read as _;

        const HIDDEN: &[u8] = b"data reachable through a name that is not the session's";

        let dir = worktree_tempdir();
        let victim = dir.path().join("Documents");
        std::fs::create_dir_all(&victim).unwrap();
        let outside = victim.join("keep.txt");
        std::fs::write(&outside, b"the other name's default stream").unwrap();
        let outside_ads = ads_path(&outside, "hidden");
        if std::fs::write(&outside_ads, HIDDEN).is_err() {
            crate::skip_notice!(
                "SKIPPED cpe_1986_an_aliased_files_streams_are_left_alone_like_its_default_stream: \
                 this volume cannot hold alternate data streams."
            );
            return;
        }

        let session = dir.path().join("session");
        std::fs::create_dir_all(&session).unwrap();
        let alias = session.join("loot.txt");
        if crate::links::create_hard_link(&outside.to_string_lossy(), &alias.to_string_lossy()).is_err()
        {
            crate::skip_notice!(
                "SKIPPED cpe_1986_an_aliased_files_streams_are_left_alone_like_its_default_stream: \
                 this volume cannot create a hard link."
            );
            return;
        }

        let expected = probe_no_follow(&session).id;
        shred_dir_pinned(
            &session,
            expected,
            ShredScheme::Zero,
            AliasPolicy::UnlinkAliasesInsteadOfOverwriting,
        )
        .expect("an alias is skipped, not refused — the wipe walks past it");

        let mut after = Vec::new();
        crate::batch_media::open_existing_no_follow_read(&outside_ads)
            .expect("the other name's stream must still be there")
            .read_to_end(&mut after)
            .unwrap();
        assert_eq!(
            after.as_slice(),
            HIDDEN,
            "HARM: the wipe wrote through an alias's named stream — the second name's data is gone, and \
             an alias is exactly what this policy exists never to overwrite"
        );
    }

    /// CPE-1986: a stream that cannot be opened for writing **refuses the whole wipe**, and it refuses
    /// before anything is unlinked.
    ///
    /// This is the decision the ticket asked to be taken deliberately rather than by default. A refusal
    /// costs retained plaintext — but it is plaintext still sitting in the session directory, where the
    /// user can see it and retry the lock, which is strictly better than plaintext in extents that no
    /// longer have a name. It is also **the same thing a locked default stream already does**, so the
    /// two halves of one file cannot disagree about what "busy" means.
    ///
    /// The exclusive handle is taken on the STREAM, and the assertion names the stream in the message,
    /// so this cannot pass on a refusal that came from the default stream instead.
    #[cfg(windows)]
    #[test]
    fn cpe_1986_a_stream_that_cannot_be_opened_refuses_the_wipe_rather_than_skipping_it() {
        use std::os::windows::fs::OpenOptionsExt as _;

        let dir = worktree_tempdir();
        let session = dir.path().join("session");
        std::fs::create_dir_all(&session).unwrap();
        let f = session.join("secret.txt");
        std::fs::write(&f, b"the default stream").unwrap();
        let ads = ads_path(&f, "locked");
        if std::fs::write(&ads, b"the user's plaintext").is_err() {
            crate::skip_notice!(
                "SKIPPED cpe_1986_a_stream_that_cannot_be_opened_refuses_the_wipe_rather_than_skipping_it: \
                 this volume cannot hold alternate data streams."
            );
            return;
        }

        // share_mode(0) — no sharing at all, the shape another process holding the stream produces.
        let Ok(held) = std::fs::OpenOptions::new().read(true).share_mode(0).open(&ads) else {
            crate::skip_notice!(
                "SKIPPED cpe_1986_a_stream_that_cannot_be_opened_refuses_the_wipe_rather_than_skipping_it: \
                 the stream could not be opened exclusively."
            );
            return;
        };

        let expected = probe_no_follow(&session).id;
        let result = shred_dir_pinned(
            &session,
            expected,
            ShredScheme::Zero,
            AliasPolicy::UnlinkAliasesInsteadOfOverwriting,
        );
        drop(held);
        match result {
            Err(VaultError::Format(msg)) => assert!(
                msg.contains(":locked") && msg.contains("cannot open it for overwriting"),
                "the refusal must name the STREAM it could not write and say that the OPEN is what \
                 failed — otherwise this passes on a refusal that came from the default stream, or \
                 from some other guard entirely; got: {msg}"
            ),
            other => panic!(
                "a stream that cannot be overwritten must refuse the wipe — skipping it is the defect \
                 CPE-1986 exists to close. Got {other:?}"
            ),
        }
        assert!(session.exists(), "a refusal must happen before anything is unlinked");
    }

    /// CPE-1986: the two alias trust levels genuinely differ when the streams cannot be **listed**, and
    /// both arms are pinned here because neither is reachable from an ordinary tree.
    ///
    /// `FindFirstStreamW` does not fail on a real file on a real NTFS volume, so this drives
    /// `shred_alternate_streams` directly with a name that is not there — the same failure shape a
    /// vanished or unlistable object produces. The session tree is the app's own directory on a local
    /// volume, so a failure there is anomalous and refused; `create_vault`'s folder is the user's own
    /// pick and may sit on a volume with no stream support at all, where refusing would break vault
    /// creation against an attacker its threat model says is absent. Same split, same reason, as
    /// [`same_object_or_refuse`]'s `Unknown` arm.
    #[cfg(windows)]
    #[test]
    fn cpe_1986_an_unlistable_object_refuses_the_session_wipe_and_is_waved_through_by_create_vault() {
        let dir = worktree_tempdir();
        let missing = dir.path().join("not-there.txt");
        let probe = EntryProbe {
            id: probe_no_follow(dir.path()).id,
            links: HardLinks::One,
            is_dir: false,
            is_link: false,
        };

        match shred_alternate_streams(
            &missing,
            &probe,
            ShredScheme::Zero,
            AliasPolicy::UnlinkAliasesInsteadOfOverwriting,
        ) {
            Err(VaultError::Format(msg)) => assert!(
                msg.contains("alternate data streams could not be listed"),
                "the refusal must say what it could not establish, got: {msg}"
            ),
            other => panic!(
                "the session wipe must refuse an object whose streams cannot be listed, got {other:?}"
            ),
        }

        shred_alternate_streams(&missing, &probe, ShredScheme::Zero, AliasPolicy::ShredEveryFile)
            .expect(
                "create_vault's shred-original must not break on a volume that cannot report streams — \
                 the user picked that folder and its threat model has no attacker in it",
            );
    }

    /// The pure stream-name rule, pinned by a table rather than by review — the same treatment
    /// [`wipe_disposition`] gets, and for the same reason: the decision is the security-relevant part.
    ///
    /// The `$DATA` rows are the safety valve described at [`is_shreddable_alternate_stream`], which no
    /// measurement taken for CPE-1986 reached: `FindStreamInfoStandard` reported only `$DATA` streams
    /// here, for an EFS-encrypted file and for one carrying a GUID reparse point alike. A green run of
    /// this table is evidence about the **rule**, not about what Windows hands it.
    #[cfg(windows)]
    #[test]
    fn cpe_1986_only_a_named_data_stream_is_shreddable() {
        for (name, want, why) in [
            (":hidden:$DATA", true, "an ordinary named data stream"),
            (":Zone.Identifier:$DATA", true, "the mark-of-the-web stream a browser writes"),
            (":b b:$DATA", true, "a stream name with a space in it"),
            (":x:$data", true, "the type compared case-insensitively"),
            ("::$DATA", false, "the default stream, already overwritten through the plain path"),
            (":$EFS:$LOGGED_UTILITY_STREAM", false, "EFS key material, not a data stream"),
            (":$I30:$INDEX_ALLOCATION", false, "a directory index, not a data stream"),
            (":$DATA", false, "no type field at all"),
            ("hidden:$DATA", false, "no leading colon, so not a stream name at all"),
            ("", false, "empty"),
        ] {
            assert_eq!(is_shreddable_alternate_stream(name), want, "{name:?} — {why}");
        }
    }

    // ---- CPE-1669 / CPE-1670: create_vault writes the blob the way the re-seal does -----------------

    /// CPE-1669: the blob is `sync_all`ed **before** it is verified, and therefore before `shred_tree`
    /// destroys the plaintext original. Same falsifiable shape as
    /// [`the_staging_blob_is_fsynced_before_it_is_verified`] — [`sync_durably`] counts its calls in test
    /// builds and the injected verifier reads the counter at the moment it runs, because there is no
    /// portable way to ask the OS after the fact whether a write reached the platter. Removing the
    /// `sync_all` makes the count stand still and this fails.
    #[test]
    fn create_vault_fsyncs_the_blob_before_it_is_verified_and_therefore_before_any_shred() {
        let dir = worktree_tempdir();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");
        let opts = CreateOpts { shred_original: true, shred_scheme: ShredScheme::Zero };

        // The counter is THREAD-LOCAL (see `VAULT_BLOB_SYNCS`), so the value observed inside our own
        // verify was produced by this test's own call site and nothing else. Do not "simplify" it back
        // to a process-wide atomic: that is exactly what made both ordering tests weaker than they read,
        // because another test's fsync could satisfy "the count went up" while this call site synced
        // nothing (PR #861 review — reconstructed and confirmed against `main`'s atomic version). The
        // before-snapshot is kept as belt-and-braces, not because a shared counter needs it.
        let before = vault_blob_sync_count();
        let at_verify = std::cell::Cell::new(usize::MAX);
        create_vault_with_verifier(&src, &blob_path, &pass("pw"), &opts, true, |p, pw| {
            at_verify.set(vault_blob_sync_count());
            verify_recoverable(p, pw)
        })
        .expect("the create itself must succeed");

        assert!(
            at_verify.get() > before,
            "the blob must be fsynced BEFORE it is verified — the verify read back a copy that may never \
             have reached the disk, and the shred destroys the only other copy next (syncs before \
             ={before}, at verify={})",
            at_verify.get()
        );
        // ...and the ordering claim is only worth anything if the shred really did run afterwards.
        assert!(!src.exists(), "the confirmed shred must still have destroyed the original");
        assert!(is_vault(&blob_path));
    }

    /// CPE-1669, the Unix half: the destination's parent directory is fsynced after the rename, so the
    /// new directory entry is durable too. Counted for the same reason the blob fsync is.
    #[cfg(unix)]
    #[test]
    fn create_vault_fsyncs_the_destination_directory_after_the_rename() {
        let dir = worktree_tempdir();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");

        let before = parent_dir_sync_count();
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default(), false).unwrap();

        assert!(
            parent_dir_sync_count() > before,
            "the rename created a directory entry that is only in the page cache until the directory \
             itself is synced"
        );
    }

    /// **The containment guard must ask where the write LANDS, not where the name resolves to**
    /// (SEC-861 blocking 1, found by the security audit of this PR).
    ///
    /// CPE-1670 changed `create_vault` from following a symlinked destination to replacing it — but
    /// `resolves_inside`, the guard whose entire job is "never write the vault where the shred will
    /// destroy it", still decided by canonicalizing `dest`, which **follows** a final-component symlink.
    /// The guard reasoned about the far end while the file landed at the near end. So a destination name
    /// *inside* the folder, symlinked to somewhere *outside* it, read as "outside, safe" — and the vault
    /// was written inside the folder that `shred_tree` then destroyed, losing the plaintext AND the
    /// encrypted copy, with `create_vault` returning `Ok(())`. Measured on the unfixed branch:
    /// `folder_shredded=true blob_at_far_end_len=Some(11)` — the far end still held its 11-byte
    /// placeholder, so there was no vault anywhere.
    ///
    /// This is the auditor's own case. It is the deterministic, non-racy half of the finding: no threads,
    /// no timing, no elevation (an unprivileged file symlink on Linux/macOS).
    #[test]
    fn a_symlinked_destination_inside_the_shredded_folder_never_loses_both_copies() {
        let dir = worktree_tempdir();

        // The user's folder, about to be sealed and then securely deleted.
        let folder = dir.path().join("MyStuff");
        std::fs::create_dir_all(folder.join("sub")).unwrap();
        std::fs::write(folder.join("irreplaceable.txt"), b"the only copy of the user's data").unwrap();

        // A real file OUTSIDE the folder, which the destination name links to.
        let outside = dir.path().join("elsewhere.cpevault");
        std::fs::write(&outside, b"placeholder").unwrap();

        // The destination: a name INSIDE the folder that happens to be a symlink pointing OUT.
        let dest = folder.join("backup.cpevault");
        if !try_symlink_file(&outside, &dest) {
            crate::skip_notice!(
                "SKIPPED a_symlinked_destination_inside_the_shredded_folder_never_loses_both_copies: \
                 this OS/account cannot create a file symlink (on Windows this needs Developer Mode or \
                 admin). The sharp end of this finding is Linux/macOS, where it needs neither."
            );
            return;
        }

        let opts = CreateOpts { shred_original: true, shred_scheme: ShredScheme::Zero };
        let result = create_vault(&folder, &dest, &pass("pw"), &opts, true);

        let landed_inside = folder.join("backup.cpevault");
        let is_vault_at = |p: &Path| std::fs::read(p).map(|b| b.starts_with(MAGIC)).unwrap_or(false);
        eprintln!(
            "SEC-861 CONTAINMENT: create_vault -> {:?}; folder_shredded={} \
             vault_at_link_name={} vault_at_far_end={}",
            result.as_ref().map(|_| "Ok").map_err(|e| e.to_string()),
            !folder.exists(),
            is_vault_at(&landed_inside),
            is_vault_at(&outside),
        );

        // THE invariant, read back off disk: the user must never end up with neither copy.
        let plaintext_survives = std::fs::read(folder.join("irreplaceable.txt"))
            .map(|b| b == b"the only copy of the user's data")
            .unwrap_or(false);
        assert!(
            plaintext_survives || is_vault_at(&landed_inside) || is_vault_at(&outside),
            "BOTH copies are gone — the plaintext was shredded and no readable vault exists anywhere \
             (create_vault returned {result:?})"
        );
        // And specifically: this must be REFUSED, not merely survived by luck. The destination lands
        // inside the folder being shredded, which is exactly what the guard exists to stop.
        match result {
            Err(VaultError::Format(msg)) => assert!(
                msg.contains("refusing to shred"),
                "the refusal must read like the other data-loss refusals, got: {msg}"
            ),
            other => panic!("a destination landing inside the shredded folder must be refused, got {other:?}"),
        }
        assert_eq!(
            std::fs::read(folder.join("irreplaceable.txt")).unwrap(),
            b"the only copy of the user's data",
            "a refused call must leave every byte of the folder alone"
        );
    }

    /// CPE-1670: a symlinked `.cpevault` path is **replaced**, not written through — and `create_vault`
    /// and the lock-time re-seal now agree about that, which is the whole point of the ticket. Covers the
    /// full lifecycle through the linked path (seal → unlock → edit → lock → unlock) and asserts the file
    /// at the far end of the link is left holding exactly what it held, since nothing may be destroyed.
    #[test]
    fn a_symlinked_vault_path_is_replaced_by_both_create_and_lock_never_written_through() {
        let dir = worktree_tempdir();

        // The far end of the link: a real, openable vault somewhere else entirely.
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let real_blob = sealed_vault(&elsewhere);
        let real_before = std::fs::read(&real_blob).unwrap();

        // The user's linked-in name for it.
        let linked = dir.path().join("linked.cpevault");
        if !try_symlink_file(&real_blob, &linked) {
            crate::skip_notice!(
                "SKIPPED a_symlinked_vault_path_is_replaced_by_both_create_and_lock_never_written_through: \
                 this OS/account cannot create a file symlink (on Windows this needs Developer Mode or \
                 admin)."
            );
            return;
        }
        assert!(is_vault(&linked), "reads follow the link — the linked name really opens the vault");

        // (a) The LOCK half. Unlock through the link, edit, lock.
        let root = sessions_root(dir.path());
        let session = root.join("16700000-1670-0000-1670-000000000000");
        let reg = VaultRegistry::default();
        reg.unlock(SessionsRoot::new(&root), &linked, &pass("pw"), &session).unwrap();
        std::fs::write(session.join("edited.txt"), b"written while unlocked").unwrap();
        reg.lock(&linked).expect("locking a vault reached through a symlink must succeed");

        // The link was REPLACED by the real file — the behaviour, stated and now pinned.
        assert!(
            !std::fs::symlink_metadata(&linked).unwrap().file_type().is_symlink(),
            "the re-seal replaces a symlinked vault path rather than writing through it"
        );
        // Nothing was destroyed: the file at the far end still holds exactly what it held.
        assert_eq!(
            std::fs::read(&real_blob).unwrap(),
            real_before,
            "the link's target must be byte-for-byte untouched — a replaced link destroys nothing"
        );
        // And the path the user opens holds the current contents.
        let session2 = root.join("16700000-1670-0000-1670-000000000001");
        reg.unlock(SessionsRoot::new(&root), &linked, &pass("pw"), &session2).unwrap();
        assert_eq!(
            std::fs::read(session2.join("edited.txt")).unwrap(),
            b"written while unlocked",
            "the edit made while unlocked must be in the vault the linked path now names"
        );
        reg.lock(&linked).unwrap();

        // (b) The CREATE half must agree — this is the asymmetry CPE-1670 was filed for. `create_vault`
        // used to `std::fs::write` straight THROUGH the link and update its target.
        let linked2 = dir.path().join("linked2.cpevault");
        assert!(try_symlink_file(&real_blob, &linked2), "the same OS just made one of these");
        let src = dir.path().join("fresh");
        sample_folder(&src);
        create_vault(&src, &linked2, &pass("pw2"), &CreateOpts::default(), false).unwrap();
        assert!(
            !std::fs::symlink_metadata(&linked2).unwrap().file_type().is_symlink(),
            "create_vault must treat a symlinked destination exactly as the re-seal does — replace it"
        );
        assert_eq!(
            std::fs::read(&real_blob).unwrap(),
            real_before,
            "and it must NOT have written through the link into its target"
        );
    }

    /// Create a FILE symlink; `false` when the OS refuses (unprivileged Windows without Developer Mode),
    /// so the caller notes it rather than passing silently.
    fn try_symlink_file(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link).is_ok()
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (target, link);
            false
        }
    }
}
