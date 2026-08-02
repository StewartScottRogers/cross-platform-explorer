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
//! - **Unlock / lock** — [`VaultRegistry`] decrypts a blob into a caller-provided *session directory*
//!   and remembers the unlocked (blob → session) mapping; locking drops that mapping and **securely
//!   wipes** the session directory (shred each extracted file, then remove the tree).
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
/// With `shred_original` off (the default) the plaintext is left untouched and no verification runs.
pub fn create_vault(
    folder: &Path,
    dest_blob_path: &Path,
    passphrase: &SecretString,
    opts: &CreateOpts,
) -> Result<(), VaultError> {
    create_vault_with_verifier(folder, dest_blob_path, passphrase, opts, verify_recoverable)
}

/// [`create_vault`] with the recoverability check injected, so tests can force the verify step to
/// fail and assert the plaintext original survives (the invariant is only meaningful if it is
/// falsifiable). Production always passes [`verify_recoverable`].
fn create_vault_with_verifier(
    folder: &Path,
    dest_blob_path: &Path,
    passphrase: &SecretString,
    opts: &CreateOpts,
    verify: impl Fn(&Path, &SecretString) -> Result<(), VaultError>,
) -> Result<(), VaultError> {
    if opts.shred_original {
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
        shred_tree(folder, opts.shred_scheme)?;
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

/// Decrypt the blob at `blob_path` with `passphrase` into `session_dir` (the crypto core extracts
/// atomically — a failure leaves `session_dir` untouched). Does not record any state; use
/// [`VaultRegistry::unlock`] to track the unlocked session.
pub fn unlock_to_session(
    blob_path: &Path,
    passphrase: &SecretString,
    session_dir: &Path,
) -> Result<(), VaultError> {
    let blob = std::fs::read(blob_path)?;
    vault_crypto::decrypt_tree(&blob, passphrase, session_dir)
}

/// Securely wipe an unlocked session directory: shred every extracted file, then remove the tree, so
/// the extracted plaintext does not linger. A missing directory is a no-op success.
pub fn wipe_session_dir(session_dir: &Path, scheme: ShredScheme) -> Result<(), VaultError> {
    if session_dir.exists() {
        shred_tree(session_dir, scheme)?;
    }
    Ok(())
}

/// The set of currently-unlocked vaults: `blob path → session directory`. Cheaply cloneable (an
/// `Arc` around the map) and zero-cost until a vault is unlocked, mirroring
/// [`crate::terminal_tabs::TerminalDockState`] — the shape the Tauri app manages as state.
#[derive(Clone, Default)]
pub struct VaultRegistry(Arc<Mutex<HashMap<PathBuf, PathBuf>>>);

impl VaultRegistry {
    /// Unlock `blob_path` into `session_dir` and record the mapping. If decryption fails, no state is
    /// recorded (the vault stays locked).
    ///
    /// Re-unlock safety (CPE-1249 review #1): if `blob_path` is ALREADY unlocked into a *different* session
    /// dir, that prior session dir's plaintext is securely wiped before the mapping is overwritten — so a
    /// direct/double unlock never leaves the old plaintext orphaned on disk. The new decrypt runs first, so
    /// a failed re-unlock leaves the existing unlocked state (and its plaintext) untouched.
    pub fn unlock(
        &self,
        blob_path: &Path,
        passphrase: &SecretString,
        session_dir: &Path,
    ) -> Result<(), VaultError> {
        self.unlock_with_wiper(blob_path, passphrase, session_dir, |dir| {
            wipe_session_dir(dir, SESSION_WIPE_SCHEME)
        })
    }

    /// [`unlock`](Self::unlock) with the stale-session wipe injected, so tests can assert a re-unlock wipes
    /// the prior session dir. Production always passes [`wipe_session_dir`].
    fn unlock_with_wiper(
        &self,
        blob_path: &Path,
        passphrase: &SecretString,
        session_dir: &Path,
        wipe: impl Fn(&Path) -> Result<(), VaultError>,
    ) -> Result<(), VaultError> {
        // Decrypt into the NEW session dir first — a failed decrypt must leave any existing unlocked state
        // (and its plaintext) intact, so this happens before we touch the map.
        unlock_to_session(blob_path, passphrase, session_dir)?;
        // Record the new mapping, capturing any prior session dir for the same blob. Inserting BEFORE the
        // wipe keeps the (freshly-decrypted) new session always reachable/lockable even if the wipe below
        // fails on a stubborn file — the new plaintext is never the orphan.
        let prev = {
            let mut map = self.0.lock().unwrap();
            map.insert(blob_path.to_path_buf(), session_dir.to_path_buf())
        };
        if let Some(old) = prev {
            if old != session_dir {
                wipe(&old)?; // the prior plaintext must not linger unreferenced (review #1)
            }
        }
        Ok(())
    }

    /// Lock `blob_path`: securely wipe its session directory, then drop the unlocked mapping. Locking a
    /// vault that is not unlocked is a no-op success.
    ///
    /// The mapping is dropped **only after** the wipe succeeds: if the wipe fails (a read-only extracted
    /// file, a file held open by another process), the vault stays reported unlocked so the lock is
    /// retryable and never claims "locked" while plaintext still lingers on disk.
    pub fn lock(&self, blob_path: &Path) -> Result<(), VaultError> {
        self.lock_with_wiper(blob_path, |dir| wipe_session_dir(dir, SESSION_WIPE_SCHEME))
    }

    /// [`lock`](Self::lock) with the wipe injected, so tests can force a wipe failure and assert the
    /// vault stays unlocked (retryable). Production always passes [`wipe_session_dir`].
    fn lock_with_wiper(
        &self,
        blob_path: &Path,
        wipe: impl Fn(&Path) -> Result<(), VaultError>,
    ) -> Result<(), VaultError> {
        // Read (don't remove) the session first, so a failing wipe leaves the mapping in place.
        let session = self.0.lock().unwrap().get(blob_path).cloned();
        if let Some(dir) = session {
            wipe(&dir)?; // on Err: the mapping is untouched → is_unlocked stays true → retryable.
            // Drop the mapping only if it STILL points at the dir we just wiped — guards the narrow
            // unlock-during-lock race (a concurrent re-unlock into a different session dir), so we never
            // clear a fresh mapping whose plaintext we didn't wipe.
            let mut map = self.0.lock().unwrap();
            if map.get(blob_path) == Some(&dir) {
                map.remove(blob_path);
            }
        }
        Ok(())
    }

    /// Is `blob_path` currently unlocked?
    pub fn is_unlocked(&self, blob_path: &Path) -> bool {
        self.0.lock().unwrap().contains_key(blob_path)
    }

    /// The live session directory for an unlocked `blob_path`, if any.
    pub fn session_dir(&self, blob_path: &Path) -> Option<PathBuf> {
        self.0.lock().unwrap().get(blob_path).cloned()
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

/// Shred every file under `root` (each overwrite-then-unlinked via [`secure_shred::shred_file`]), then
/// remove the now-fileless directory tree. Symlinks are skipped (never followed) — matching what the
/// crypto core captured — but are removed with the tree.
fn shred_tree(root: &Path, scheme: ShredScheme) -> Result<(), VaultError> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    for file in &files {
        let p = file.to_string_lossy().to_string();
        secure_shred::shred_file(&p, scheme)
            .map_err(|e| VaultError::Format(format!("shred {p}: {e}")))?;
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
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
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default()).unwrap();
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
        create_vault(&src, &blob_path, &pass("open sesame"), &CreateOpts::default()).unwrap();
        assert!(src.join("top.txt").exists(), "default create must not shred the original");
        assert!(is_vault(&blob_path));

        // Unlock into a session dir → the plaintext tree comes back byte-identical.
        let reg = VaultRegistry::default();
        let session = dir.path().join("session");
        reg.unlock(&blob_path, &pass("open sesame"), &session).unwrap();
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
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default()).unwrap();

        let reg = VaultRegistry::default();
        let session1 = dir.path().join("session1");
        reg.unlock(&blob_path, &pass("pw"), &session1).unwrap();
        assert!(session1.join("top.txt").exists(), "first unlock extracts plaintext");

        // Re-unlock into a DIFFERENT session dir.
        let session2 = dir.path().join("session2");
        reg.unlock(&blob_path, &pass("pw"), &session2).unwrap();

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
    fn unlock_with_wrong_passphrase_fails_and_records_no_state() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        sample_folder(&src);
        let blob_path = dir.path().join("v.cpevault");
        create_vault(&src, &blob_path, &pass("right"), &CreateOpts::default()).unwrap();

        let reg = VaultRegistry::default();
        let session = dir.path().join("session");
        let result = reg.unlock(&blob_path, &pass("wrong"), &session);
        assert!(matches!(result, Err(VaultError::BadPassphrase)), "got {result:?}");
        assert!(!reg.is_unlocked(&blob_path), "a failed unlock must not record unlocked state");
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
        let result = create_vault_with_verifier(&src, &blob_path, &pass("pw"), &opts, |_, _| {
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
        create_vault(&src, &blob_path, &pass("pw"), &opts).unwrap();
        assert!(!src.exists(), "a good verify must let the original be shredded away");
        assert!(is_vault(&blob_path));

        let reg = VaultRegistry::default();
        let session = dir.path().join("session");
        reg.unlock(&blob_path, &pass("pw"), &session).unwrap();
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
        let result = create_vault(&folder, &dest_inside, &pass("pw"), &opts);
        assert!(matches!(result, Err(VaultError::Format(_))), "must refuse a dest inside the folder: {result:?}");

        // Nothing was shredded or overwritten: plaintext AND the pre-existing blob both survive intact.
        assert_eq!(std::fs::read(folder.join("top.txt")).unwrap(), b"top secret");
        assert_eq!(std::fs::read(&dest_inside).unwrap(), b"pre-existing bytes");

        // A nested-inside dest is refused too (guard is by canonicalized path prefix, not name match).
        let nested = folder.join("sub").join("deep.cpevault");
        assert!(matches!(
            create_vault(&folder, &nested, &pass("pw"), &opts),
            Err(VaultError::Format(_))
        ));
        assert_eq!(std::fs::read(folder.join("top.txt")).unwrap(), b"top secret");

        // A sibling dest just OUTSIDE the folder (shared name prefix) is allowed and shreds cleanly.
        let sibling = dir.path().join("secret.cpevault");
        create_vault(&folder, &sibling, &pass("pw"), &opts).unwrap();
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
        let result = create_vault(&src, &blob_path, &pass("pw"), &opts);
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
        create_vault(&src, &blob_path, &pass("pw"), &opts).unwrap();

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
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default()).unwrap();

        let reg = VaultRegistry::default();
        let session = dir.path().join("session");
        reg.unlock(&blob_path, &pass("pw"), &session).unwrap();
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
        create_vault(&src, &blob_path, &pass("pw"), &CreateOpts::default()).unwrap();

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
}
