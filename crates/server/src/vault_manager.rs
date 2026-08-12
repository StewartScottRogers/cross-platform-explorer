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
    std::fs::write(dest_blob_path, &blob)?;

    if opts.shred_original {
        // INVARIANT: prove the persisted copy is recoverable BEFORE destroying anything. Verify the
        // bytes that actually landed on disk (not the in-memory `blob`), so a partial/failed write is
        // caught too. On any error we return WITHOUT shredding — the original is untouched.
        verify(dest_blob_path, passphrase)?;
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

/// Does `dest` resolve to a location inside `folder` (including `folder` itself)? Used to guard the
/// destructive shred path so the vault blob is never written where the shred will destroy it.
///
/// `folder` must exist (it's the tree being sealed). `dest` typically does **not** exist yet, so its
/// parent is canonicalized and the file name re-appended rather than canonicalizing a missing path.
/// Comparing canonicalized paths collapses `..`/symlinks/`.\` on both sides so the containment check
/// can't be fooled by a non-normalized destination.
fn resolves_inside(folder: &Path, dest: &Path) -> Result<bool, VaultError> {
    let folder_canon = std::fs::canonicalize(folder)?;
    let dest_canon = match std::fs::canonicalize(dest) {
        Ok(p) => p,
        Err(_) => {
            let parent = dest.parent().filter(|p| !p.as_os_str().is_empty());
            let parent_canon = match parent {
                Some(p) => std::fs::canonicalize(p)?,
                // A bare file name with no parent resolves against the current dir.
                None => std::fs::canonicalize(".")?,
            };
            match dest.file_name() {
                Some(name) => parent_canon.join(name),
                None => parent_canon,
            }
        }
    };
    Ok(dest_canon.starts_with(&folder_canon))
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
    // The replacing rename. NOTE (SEC-847 reviewer nit 7): if `blob_path` is itself a symlink, this
    // replaces the *link* with the real file rather than writing through it — the opposite of
    // `create_vault`'s `fs::write`. That is the safer of the two (a re-seal can never be redirected into
    // writing a vault somewhere else), but it is a deliberate asymmetry, not an accident, and it means a
    // deliberately-symlinked `.cpevault` stops being a link the first time it is locked. Filed as a
    // follow-up rather than changed here, so the behaviour is not altered under a security fix.
    if let Err(e) = std::fs::rename(&staging, blob_path) {
        let _ = std::fs::remove_file(&staging);
        return Err(reseal_failed(VaultError::Io(e)));
    }
    // DURABILITY (SEC-847 reviewer blocker C): the file's own bytes were `sync_all`ed above, but on Unix
    // the *directory entry* the rename created is itself only in the page cache until the directory is
    // synced. Without this, a power loss in the window between here and the wipe could leave the vault's
    // name pointing at nothing while the shredder has already destroyed the only other copy. Best-effort:
    // a filesystem that will not let us open the directory (or Windows, where directories are not synced
    // this way and `rename` is already ordered) is not a reason to fail a lock whose data is on disk.
    #[cfg(unix)]
    if let Some(parent) = blob_path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
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

/// Count of staging fsyncs this process has performed. **Test builds only** — it exists so the
/// "durable before verify" ordering can be *asserted* instead of merely reviewed (SEC-847 round-3: the
/// `sync_all` was removable with the whole 50-test vault suite still green). Compiled out of release.
#[cfg(test)]
static STAGING_SYNCS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many staging fsyncs have happened so far. Test-only observation point for [`sync_durably`].
#[cfg(test)]
fn staging_sync_count() -> usize {
    STAGING_SYNCS.load(std::sync::atomic::Ordering::SeqCst)
}

/// `sync_all`, counted in test builds. Wrapping it is what makes the *ordering* — fsync happens before
/// the caller verifies, and therefore before anything is destroyed — observable to a test; there is no
/// portable way to ask the OS after the fact whether a given write reached the platter.
fn sync_durably(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(test)]
    STAGING_SYNCS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
/// skipped without being followed, exactly as [`collect_files`] and the crypto core's walk do.
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
#[derive(Debug, Clone, PartialEq, Eq)]
enum HardLinks {
    /// Exactly one name — an ordinary file.
    One,
    /// More than one name: an alias.
    Many(u64),
    /// The count could not be established (payload: why).
    Unknown(&'static str),
}

/// Unix link count: `st_nlink`, straight off `symlink_metadata` (no follow — the caller has already
/// established this entry is not a symlink).
#[cfg(unix)]
fn hard_link_count(path: &Path) -> HardLinks {
    use std::os::unix::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(md) if md.nlink() > 1 => HardLinks::Many(md.nlink()),
        Ok(_) => HardLinks::One,
        Err(_) => HardLinks::Unknown("its metadata could not be read"),
    }
}

/// Windows link count: `nNumberOfLinks` from `GetFileInformationByHandle`.
/// `std::os::windows::fs::MetadataExt::number_of_links()` is still behind the unstable
/// `windows_by_handle` feature (rust-lang/rust#63010), so this makes the same call the std wrapper would,
/// via the `windows` crate already vendored for [`crate::batch_media`]'s identity probe — whose two
/// load-bearing details are reproduced here for the same reasons (CPE-1642 finding B):
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
/// same call work for any entry kind. Any failure is [`HardLinks::Unknown`] — refused, never assumed safe.
#[cfg(windows)]
fn hard_link_count(path: &Path) -> HardLinks {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let wide = crate::batch_media::verbatim_wide(path);
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 string kept alive for the whole call. This is an
    // attributes-only open of an already-existing file (`OPEN_EXISTING`, full sharing) — no create, no
    // write, no truncate, no data access — and the handle is closed on every path before returning.
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
            Err(_) => return HardLinks::Unknown("it could not be opened to read its link count"),
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        let ok = GetFileInformationByHandle(handle, &mut info).is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return HardLinks::Unknown("the filesystem did not report its link count");
        }
        match info.nNumberOfLinks {
            0 => HardLinks::Unknown("the filesystem reported a link count of zero"),
            1 => HardLinks::One,
            n => HardLinks::Many(u64::from(n)),
        }
    }
}

/// Neither Unix nor Windows: refuse rather than guess (no such platform ships today; this keeps the
/// guard fail-closed by construction rather than by omission).
#[cfg(not(any(unix, windows)))]
fn hard_link_count(_path: &Path) -> HardLinks {
    HardLinks::Unknown("this platform cannot report a file's link count")
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
    ShredEveryFile,
    /// Overwrite only files this module can prove have exactly one name; **unlink** anything else
    /// instead of writing through it. Used by the session wipe — see [`wipe_disposition`].
    UnlinkAliasesInsteadOfOverwriting,
}

/// How the session wipe must dispose of one file, given how many names point at it. Pure, so the
/// fail-closed decision is pinned by a test rather than only by review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WipeDisposition {
    /// Exactly one name: this file is the session's own, so overwrite it before unlinking.
    Shred,
    /// More than one name — or a count that could not be read at all. **Unlink the name only.**
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

/// Shred every file under `root` (each overwrite-then-unlinked via [`secure_shred::shred_file`]), then
/// remove the now-fileless directory tree. Symlinks are skipped (never followed) — matching what the
/// crypto core captured — but are removed with the tree.
///
/// **The destructive step carries its own link check** (SEC-847 round-3 audit). The re-seal's
/// [`ensure_no_aliased_files`] walk is a check-then-USE: it runs once, at the very top, and the "use" is
/// this shredder several seconds later, after encrypt + an exclusive staging write + `sync_all` + a full
/// verifying decrypt (a real scrypt KDF, ~1s **by design**) + the rename. The attacker did not even need
/// to win a race, because the staging file appearing beside the `.cpevault` is a publicly observable
/// starting gun proving the guard has already passed: poll for it, then
/// `create_hard_link(victim, "<session>/loot.xlsx")` — a registered IPC command, unprivileged on NTFS —
/// and this loop zero-filled the victim through the alias while `lock` returned `Ok(())` and the UI said
/// "Locked".
///
/// So under [`AliasPolicy::UnlinkAliasesInsteadOfOverwriting`] the count is re-read **per file, just
/// before that file is overwritten**, and a file with another name is **unlinked rather than
/// overwritten**: a name that has another name is not ours to destroy, and unlinking one of an inode's
/// names destroys nothing. That closes the hard-link variant this round was filed for, and gives the
/// destructive step a guard of its own instead of relying on the caller's earlier walk — the discipline
/// every other destructive step in this module already follows.
///
/// **It does NOT close the class, and this comment must not say that it does (SEC-847 round-3 re-audit).**
/// An earlier draft claimed "no window at all" and "closes the integrity half completely". Both are false,
/// and the auditor reproduced a victim file being securely overwritten and unlinked 3/3 through the public
/// [`VaultRegistry::lock`] with `lock` returning `Ok(())`:
///
/// - This loop is **collect-then-shred**. [`collect_files`] walks the tree first and freezes absolute
///   paths; the loop then calls [`hard_link_count`] and `shred_file`, each of which **re-resolves the
///   whole path from scratch**. So there IS a window — two independent path resolutions plus
///   `shred_file`'s own `metadata` and up to three opens.
/// - The per-file check has no-follow semantics on the **final component only**. Every *parent* component
///   is resolved by the OS. So the attacker skips the hard link entirely: plant an innocuous real
///   subdirectory before locking (it passes both alias walks — link count 1, not a reparse point, and it
///   gets sealed into the blob), wait for the first shredded file to vanish (the same starting-gun
///   problem, one level up), then `remove_dir_all` that subdirectory and drop a **junction** in its place
///   pointing at `Documents`. The frozen path now resolves through the junction, the link count of the
///   victim reads `One`, and it is shredded.
/// - `wipe_session_dir`'s own root-is-not-a-link check is likewise a single pass before `collect_files`.
///
/// This structure is **pre-existing on `main`**, not introduced by CPE-1645, which is why that ticket
/// landed with it open rather than growing a third time. It is filed with the full reproduction as
/// **CPE-1672**. The proportionate fix is to stop collecting first — walk and shred inline, checking
/// `symlink_metadata(dir).file_type().is_symlink()` immediately before descending — which removes both the
/// frozen list and the observable starting gun. The complete fix is handle-based: open each file once
/// no-follow, read `nNumberOfLinks` from *that* handle, and write through the same handle.
fn shred_tree(root: &Path, scheme: ShredScheme, aliases: AliasPolicy) -> Result<(), VaultError> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    for file in &files {
        if aliases == AliasPolicy::UnlinkAliasesInsteadOfOverwriting
            && wipe_disposition(&hard_link_count(file)) == WipeDisposition::UnlinkOnly
        {
            // Best-effort, exactly like the shred path's own error handling below is not: a name we
            // cannot unlink is left in place and the following `remove_dir_all` reports it. Nothing is
            // written to it either way, which is the property that matters.
            let _ = std::fs::remove_file(file);
            continue;
        }
        let p = file.to_string_lossy().to_string();
        secure_shred::shred_file(&p, scheme)
            .map_err(|e| VaultError::Format(format!("shred {p}: {e}")))?;
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
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

/// Collect the regular-file paths under `dir` (recursive), skipping symlinks.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), VaultError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        let p = entry.path();
        if ft.is_dir() {
            collect_files(&p, out)?;
        } else if ft.is_file() {
            out.push(p);
        }
    }
    Ok(())
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
            eprintln!(
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
            eprintln!(
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
            eprintln!(
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
            eprintln!(
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
            eprintln!(
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
            eprintln!(
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
            eprintln!(
                "NOTE the_staging_open_is_exclusive…: no symlink privilege here, so only the regular-file \
                 and hard-link forms were verified."
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
            eprintln!(
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
            eprintln!(
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
            eprintln!(
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
            eprintln!(
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
            eprintln!(
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
            eprintln!(
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

        // The whole process shares the counter and tests run in parallel, so compare a snapshot taken
        // just before with the value observed inside our own verify — a monotonic counter can only have
        // gone UP, and our own write is guaranteed to be one of the increments.
        let before = staging_sync_count();
        let at_verify = std::cell::Cell::new(usize::MAX);
        reseal_session_with_verifier(&blob_path, &session, &pass("pw"), |p, pw| {
            at_verify.set(staging_sync_count());
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
