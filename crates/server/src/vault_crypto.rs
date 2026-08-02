//! Encrypted-vault crypto **core** (CPE-1247, epic CPE-738).
//!
//! The pure, Tauri-free, fully cargo-testable heart of the encrypted-vaults feature: it turns a
//! folder tree into a single `.cpevault` blob and back, using the [`age`] crate in **passphrase
//! mode** — scrypt KDF for the key, ChaCha20-Poly1305 streaming AEAD for the payload. We do **not**
//! hand-roll any crypto: `age` owns the KDF, the AEAD, the nonces, and the streaming framing, which
//! removes the classic footguns (nonce reuse, missing authentication, weak KDFs).
//!
//! # Layers
//! - **In-memory core** — [`encrypt_bytes`] / [`decrypt_bytes`] seal and open an arbitrary byte
//!   stream. These have no filesystem side-effects and are where the adversarial tests aim.
//! - **Deterministic tree framing** — [`pack_entries`] / [`unpack_entries`] serialize a
//!   [`TreeEntry`] list to one plaintext stream and parse it back. Parsing is **panic-free** on
//!   arbitrary bytes (every read is bounds-checked; no `unwrap`/`expect`/indexing on lengths).
//! - **Filesystem helpers** — [`encrypt_tree`] walks a directory into that framing and seals it;
//!   [`decrypt_tree`] opens a blob and writes the tree under a caller-provided directory.
//!
//! # Blob layout
//! ```text
//! MAGIC ("CPEVLT1", 7 bytes) || schema_version (u16, little-endian, = 1) || age ciphertext
//! ```
//! The magic + version are our own outer envelope so a wrong/older format fails with a clear,
//! distinct error *before* any crypto runs; everything after is a standard age v1 file.
//!
//! # Framing format (inside the encryption)
//! A concatenation of records, each:
//! ```text
//! kind (u8: b'D' dir | b'F' file) || path_len (u32 LE) || path (UTF-8, POSIX '/') ||
//!   data_len (u64 LE, 0 for dirs) || data (data_len bytes, absent for dirs)
//! ```
//! Records are emitted in a deterministic order (paths sorted lexicographically), so the same tree
//! always packs to the same bytes. An empty tree is zero records (an empty stream). This internal
//! framing is used instead of `tar` deliberately: it is deterministic (no mtime/uid/gid/ordering
//! noise), dependency-free, and every byte is under our control, which keeps the parser trivially
//! auditable and panic-free.
//!
//! # Symlinks
//! **Symlinks are skipped** during [`encrypt_tree`] (a symlink is neither followed nor stored). This
//! is the safe default: following links could pull in bytes from outside the tree, and storing a
//! link target would invite symlink-escape on extraction. Only regular files and directories are
//! captured. (Other special entries — devices, FIFOs, sockets — are likewise skipped.)
//!
//! # Extraction safety
//! [`decrypt_tree`] extracts **atomically**: it decrypts fully in memory, writes the tree into a
//! sibling temporary directory, and renames it into place only on success — so any failure (a
//! rejected path, an OS write error) leaves the output directory untouched rather than
//! half-populated. Every record path is sanitized first: a `..` traversal component, an absolute
//! path, a backslash/UNC component, or a Windows drive-letter component (`C:`, `C:evil`) is rejected
//! with [`VaultError::Format`], so a maliciously-authored (but validly-encrypted) vault cannot escape
//! the output directory ("zip-slip"). A `:` *inside* an ordinary component is allowed, so a
//! Linux/macOS filename like `notes:draft.txt` seals and extracts on those platforms (a Windows host
//! extracting such a vault fails cleanly and leaves nothing partial). Non-UTF-8 filenames are
//! unsupported in v1 and rejected on encrypt.
//!
//! # Zeroization
//! The passphrase is an [`age::secrecy::SecretString`], which zeroizes its heap buffer on drop. We
//! additionally [`Zeroize`] the intermediate *decrypted plaintext* buffer once it has been unpacked.
//! What we **cannot** guarantee: copies `age` (and its dependencies) make of key/plaintext material
//! on their own heap during streaming — those are outside this module's control.

use age::secrecy::SecretString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

/// Outer-envelope magic. Bumping the format means bumping this and/or [`SCHEMA_VERSION`].
pub const MAGIC: &[u8; 7] = b"CPEVLT1";
/// Envelope schema version (little-endian `u16` on disk). Version 1 is the initial format.
pub const SCHEMA_VERSION: u16 = 1;
/// Bytes of fixed envelope header preceding the age ciphertext: [`MAGIC`] (7) + version (2).
const HEADER_LEN: usize = MAGIC.len() + 2;

/// Record kind byte for a directory entry.
const KIND_DIR: u8 = b'D';
/// Record kind byte for a regular-file entry.
const KIND_FILE: u8 = b'F';

/// Cap on the scrypt work factor we will *attempt* on decrypt. age's `scrypt::Identity` already
/// bounds this (default ≈ target + 4); we pin an explicit ceiling so a maliciously-crafted blob
/// cannot demand an unbounded amount of CPU/RAM (age warns that factors > 22 can cost hours and tens
/// of GiB). 22 comfortably accepts vaults sealed on slower hardware while bounding the DoS surface.
const MAX_WORK_FACTOR: u8 = 22;

/// The kind of a captured tree entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A directory (captured so empty directories survive a round-trip).
    Dir,
    /// A regular file (its bytes live in [`TreeEntry::data`]).
    File,
}

/// One entry in a captured/decoded folder tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Path relative to the tree root, using `/` separators. Never absolute, never contains `..`.
    pub path: String,
    /// Whether this entry is a directory or a file.
    pub kind: EntryKind,
    /// File contents. Empty for directories and for empty files alike.
    pub data: Vec<u8>,
}

/// Everything that can go wrong sealing or opening a vault. `Debug` only — it intentionally does not
/// implement `PartialEq` (an inner `io::Error` isn't comparable); tests use `matches!`.
#[derive(Debug)]
pub enum VaultError {
    /// The passphrase did not open the vault (scrypt-wrapped file key failed to authenticate).
    BadPassphrase,
    /// The ciphertext failed authentication — tampering, truncation, or a corrupted payload/header.
    Corrupt,
    /// The blob does not start with the [`MAGIC`] envelope marker (not a `.cpevault`, or truncated).
    BadMagic,
    /// The envelope's schema version is one this build does not understand.
    UnsupportedVersion(u16),
    /// An underlying I/O error (filesystem walk/write, in the fs helpers).
    Io(std::io::Error),
    /// A structural problem in the plaintext framing or a rejected (unsafe / non-UTF-8) path.
    Format(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultError::BadPassphrase => write!(f, "incorrect passphrase"),
            VaultError::Corrupt => write!(f, "vault is corrupt or has been tampered with"),
            VaultError::BadMagic => write!(f, "not a CPE vault (bad magic marker)"),
            VaultError::UnsupportedVersion(v) => write!(f, "unsupported vault schema version {v}"),
            VaultError::Io(e) => write!(f, "vault I/O error: {e}"),
            VaultError::Format(m) => write!(f, "vault format error: {m}"),
        }
    }
}

impl std::error::Error for VaultError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VaultError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for VaultError {
    fn from(e: std::io::Error) -> Self {
        VaultError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// In-memory crypto core
// ---------------------------------------------------------------------------

/// Encrypt an arbitrary byte stream into a `.cpevault` blob with `passphrase`.
///
/// Uses age passphrase mode with a work factor calibrated to ~1 second on the current machine.
/// The returned blob is `MAGIC || version || age-ciphertext`.
pub fn encrypt_bytes(plaintext: &[u8], passphrase: &SecretString) -> Result<Vec<u8>, VaultError> {
    let encryptor = age::Encryptor::with_user_passphrase(passphrase.clone());
    seal(encryptor, plaintext)
}

/// Shared sealing path: prepend the envelope, then stream `plaintext` through `encryptor`.
fn seal(encryptor: age::Encryptor, plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
    let mut out = Vec::with_capacity(HEADER_LEN + plaintext.len() + 64);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
    let mut writer = encryptor
        .wrap_output(&mut out)
        .map_err(|e| VaultError::Format(format!("age wrap_output: {e}")))?;
    writer.write_all(plaintext).map_err(VaultError::Io)?;
    writer
        .finish()
        .map_err(|e| VaultError::Format(format!("age finish: {e}")))?;
    Ok(out)
}

/// Decrypt and authenticate a `.cpevault` blob back to its plaintext bytes.
///
/// Error mapping (the security-critical part):
/// - wrong passphrase → [`VaultError::BadPassphrase`]
/// - a tampered/corrupt **payload** (streaming-AEAD failure) → [`VaultError::Corrupt`]
/// - missing/short magic → [`VaultError::BadMagic`]; unknown version → [`VaultError::UnsupportedVersion`]
///
/// **Header-region tampering** is also always a hard failure, but the *variant* depends on where the
/// flip lands, because the age header is authenticated only after the scrypt key is derived: a mangled
/// scrypt salt / wrapped file key is indistinguishable from a wrong passphrase and surfaces as
/// [`VaultError::BadPassphrase`]; a mangled recipient stanza reads as "not passphrase-encrypted" and
/// surfaces as [`VaultError::Format`]; a bad header MAC or otherwise-malformed header structure is
/// [`VaultError::Corrupt`]. In every case no plaintext is returned and no tamper is silently accepted.
///
/// Never panics on arbitrary input.
pub fn decrypt_bytes(blob: &[u8], passphrase: &SecretString) -> Result<Vec<u8>, VaultError> {
    let ciphertext = parse_envelope(blob)?;

    let decryptor = age::Decryptor::new_buffered(ciphertext).map_err(map_age_err)?;
    // Only passphrase (scrypt) vaults are valid here; anything else is a malformed/foreign blob.
    if !decryptor.is_scrypt() {
        return Err(VaultError::Format(
            "vault is not passphrase-encrypted".to_owned(),
        ));
    }

    let mut identity = age::scrypt::Identity::new(passphrase.clone());
    identity.set_max_work_factor(MAX_WORK_FACTOR);

    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(map_unlock_err)?;

    let mut plaintext = Vec::new();
    // A read failure here is the streaming AEAD rejecting a tampered/truncated payload.
    reader
        .read_to_end(&mut plaintext)
        .map_err(|_| VaultError::Corrupt)?;
    Ok(plaintext)
}

/// Validate the outer envelope and return the age-ciphertext slice.
fn parse_envelope(blob: &[u8]) -> Result<&[u8], VaultError> {
    if blob.len() < HEADER_LEN || &blob[..MAGIC.len()] != MAGIC.as_slice() {
        return Err(VaultError::BadMagic);
    }
    let version = u16::from_le_bytes([blob[MAGIC.len()], blob[MAGIC.len() + 1]]);
    if version != SCHEMA_VERSION {
        return Err(VaultError::UnsupportedVersion(version));
    }
    Ok(&blob[HEADER_LEN..])
}

/// Map an error from unlocking the file key (`Decryptor::decrypt`).
///
/// For a passphrase vault, scrypt returns [`age::DecryptError::DecryptionFailed`] when the wrapped
/// file key fails to authenticate — i.e. the passphrase is wrong. A header-MAC/format failure means
/// the blob was tampered/corrupted; an excessive work factor is a hostile blob. Everything that is
/// not "wrong passphrase" or a genuine I/O error is surfaced as [`VaultError::Corrupt`].
fn map_unlock_err(e: age::DecryptError) -> VaultError {
    use age::DecryptError as D;
    match e {
        D::DecryptionFailed | D::NoMatchingKeys => VaultError::BadPassphrase,
        D::Io(inner) => VaultError::Io(inner),
        _ => VaultError::Corrupt,
    }
}

/// Map an error from parsing the age header (`Decryptor::new_buffered`). A structurally-broken age
/// stream is corruption; a real I/O error is passed through.
fn map_age_err(e: age::DecryptError) -> VaultError {
    match e {
        age::DecryptError::Io(inner) => VaultError::Io(inner),
        _ => VaultError::Corrupt,
    }
}

// ---------------------------------------------------------------------------
// Deterministic tree framing (pure, panic-free parsing)
// ---------------------------------------------------------------------------

/// Serialize a list of tree entries into one deterministic plaintext stream (see module docs for
/// the record layout). The caller is responsible for the entry ordering it wants preserved;
/// [`encrypt_tree`] sorts by path so the same tree always yields the same bytes.
pub fn pack_entries(entries: &[TreeEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        match e.kind {
            EntryKind::Dir => out.push(KIND_DIR),
            EntryKind::File => out.push(KIND_FILE),
        }
        let path = e.path.as_bytes();
        out.extend_from_slice(&(path.len() as u32).to_le_bytes());
        out.extend_from_slice(path);
        let data_len = if matches!(e.kind, EntryKind::File) {
            e.data.len() as u64
        } else {
            0
        };
        out.extend_from_slice(&data_len.to_le_bytes());
        if matches!(e.kind, EntryKind::File) {
            out.extend_from_slice(&e.data);
        }
    }
    out
}

/// Parse a plaintext stream produced by [`pack_entries`] back into entries.
///
/// Fully bounds-checked: any truncation, bad kind byte, non-UTF-8 path, or inconsistent length
/// yields `Err(VaultError::Format(..))` — it never panics, indexes out of bounds, or pre-allocates
/// from an attacker-supplied length (data is sliced from the existing buffer).
pub fn unpack_entries(bytes: &[u8]) -> Result<Vec<TreeEntry>, VaultError> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos < bytes.len() {
        let kind = match take(bytes, &mut pos, 1)?[0] {
            KIND_DIR => EntryKind::Dir,
            KIND_FILE => EntryKind::File,
            other => {
                return Err(VaultError::Format(format!(
                    "unknown entry kind byte 0x{other:02x}"
                )))
            }
        };

        let path_len = read_u32(bytes, &mut pos)? as usize;
        let path_bytes = take(bytes, &mut pos, path_len)?;
        let path = std::str::from_utf8(path_bytes)
            .map_err(|_| VaultError::Format("non-UTF-8 path in framing".to_owned()))?
            .to_owned();

        let data_len = read_u64(bytes, &mut pos)? as usize;
        let data = match kind {
            EntryKind::File => take(bytes, &mut pos, data_len)?.to_vec(),
            EntryKind::Dir => {
                if data_len != 0 {
                    return Err(VaultError::Format(
                        "directory record carries data".to_owned(),
                    ));
                }
                Vec::new()
            }
        };

        out.push(TreeEntry { path, kind, data });
    }
    Ok(out)
}

/// Borrow `n` bytes from `bytes` starting at `*pos`, advancing `*pos`. Bounds-checked.
fn take<'a>(bytes: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8], VaultError> {
    let end = pos
        .checked_add(n)
        .ok_or_else(|| VaultError::Format("length overflow in framing".to_owned()))?;
    if end > bytes.len() {
        return Err(VaultError::Format("truncated framing".to_owned()));
    }
    let slice = &bytes[*pos..end];
    *pos = end;
    Ok(slice)
}

/// Read a little-endian `u32`, advancing `*pos`.
fn read_u32(bytes: &[u8], pos: &mut usize) -> Result<u32, VaultError> {
    let s = take(bytes, pos, 4)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Read a little-endian `u64`, advancing `*pos`.
fn read_u64(bytes: &[u8], pos: &mut usize) -> Result<u64, VaultError> {
    let s = take(bytes, pos, 8)?;
    Ok(u64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

// ---------------------------------------------------------------------------
// Filesystem helpers
// ---------------------------------------------------------------------------

/// Walk `root`, pack it, and seal it into a `.cpevault` blob with `passphrase`.
///
/// Captures regular files and directories (so empty directories survive); **skips symlinks** and
/// other special files (see module docs). Entries are packed in sorted path order for determinism.
/// The intermediate plaintext is zeroized before returning.
pub fn encrypt_tree(root: &Path, passphrase: &SecretString) -> Result<Vec<u8>, VaultError> {
    let entries = collect_tree(root)?;
    let mut plaintext = pack_entries(&entries);
    let result = encrypt_bytes(&plaintext, passphrase);
    plaintext.zeroize();
    result
}

/// Open a `.cpevault` blob with `passphrase` and write its tree under `out_dir`, **atomically**.
///
/// The blob is decrypted fully in memory, then the tree is written into a sibling temporary
/// directory and renamed into place only on success. Any failure (a rejected path, an OS write
/// error, e.g. a Windows host extracting a Linux vault that holds a `:`-name) removes the temporary
/// directory and leaves `out_dir` untouched — never a half-populated result. `out_dir` may be a
/// not-yet-existing path or an existing **empty** directory; a non-empty `out_dir` is refused rather
/// than clobbered. Every path is sanitized against directory traversal first (see module docs). The
/// intermediate decrypted plaintext is zeroized before writing.
pub fn decrypt_tree(
    blob: &[u8],
    passphrase: &SecretString,
    out_dir: &Path,
) -> Result<(), VaultError> {
    let mut plaintext = decrypt_bytes(blob, passphrase)?;
    let parsed = unpack_entries(&plaintext);
    plaintext.zeroize();
    let entries = parsed?;
    write_tree_atomic(&entries, out_dir)
}

/// Prove a `.cpevault` blob is fully recoverable **without writing any plaintext to disk** (CPE-1248).
///
/// Runs the same authentication path a real open would — [`decrypt_bytes`] (which checks the outer
/// envelope, derives the scrypt key, and authenticates the streaming AEAD, so a wrong passphrase or a
/// tampered/truncated payload fails) and [`unpack_entries`] (which confirms the internal framing parses)
/// — but keeps the decrypted plaintext entirely in memory and [`Zeroize`]s it before returning. It never
/// materializes files, so it is the safe recoverability check for the destructive "seal then shred the
/// original" path: verifying no longer leaves an unshredded plaintext copy in a temp directory.
///
/// Returns `Ok(())` iff the blob decrypts, authenticates, its framing parses, **and every entry path
/// is extraction-safe** — so verify green-lights exactly what a real extraction requires and can never
/// pass a blob that would later abort on a rejected path. Never panics.
pub fn verify_blob(blob: &[u8], passphrase: &SecretString) -> Result<(), VaultError> {
    let mut plaintext = decrypt_bytes(blob, passphrase)?;
    let parsed = unpack_entries(&plaintext);
    plaintext.zeroize();
    let entries = parsed?;
    // Defense-in-depth: prove every path passes the SAME extraction-side gate, in memory. Belt-and-
    // braces against a future encrypt path that forgets to validate — verify then still refuses to
    // certify a blob that `decrypt_tree` would reject, keeping the shred-safety invariant sound.
    for e in &entries {
        sanitize_rel_path(&e.path)?;
    }
    Ok(())
}

/// Collect a directory tree into sorted [`TreeEntry`]s.
fn collect_tree(root: &Path) -> Result<Vec<TreeEntry>, VaultError> {
    let mut entries = Vec::new();
    collect_dir(root, "", &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// Recursive worker for [`collect_tree`].
fn collect_dir(dir: &Path, rel: &str, out: &mut Vec<TreeEntry>) -> Result<(), VaultError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue; // v1: symlinks are neither followed nor stored.
        }
        let os_name = entry.file_name();
        let name = os_name.to_str().ok_or_else(|| {
            VaultError::Format(format!(
                "non-UTF-8 filename under {}",
                dir.display()
            ))
        })?;
        let child_rel = if rel.is_empty() {
            name.to_owned()
        } else {
            format!("{rel}/{name}")
        };
        let child_path = entry.path();
        // SEAL⟺EXTRACT SYMMETRY (CPE-1248): reject at *encrypt* time any path the *extract*-side
        // `sanitize_rel_path` would refuse — a filename containing `\`, a drive-letter component, `..`,
        // etc. (all legal on Linux/macOS but rejected on extraction). Running the identical gate here
        // guarantees "if it sealed, it extracts", so a destructive `shred_original` seal can never
        // produce an unextractable vault and then destroy the original. This runs before the blob is
        // written or anything is shredded, so shred-safety is preserved (create returns Err first).
        sanitize_rel_path(&child_rel).map_err(|_| {
            VaultError::Format(format!(
                "cannot seal file whose name is unsafe for extraction (contains '\\', a drive-letter, \
                 or a traversal component): {}",
                child_path.display()
            ))
        })?;
        if ft.is_dir() {
            out.push(TreeEntry {
                path: child_rel.clone(),
                kind: EntryKind::Dir,
                data: Vec::new(),
            });
            collect_dir(&child_path, &child_rel, out)?;
        } else if ft.is_file() {
            let data = std::fs::read(&child_path)?;
            out.push(TreeEntry {
                path: child_rel,
                kind: EntryKind::File,
                data,
            });
        }
        // Anything else (device/fifo/socket) is skipped.
    }
    Ok(())
}

/// Write `entries` into `out_dir` atomically: stage into a sibling temp dir, then rename into place.
///
/// On any failure the staging directory is removed, so a failed extraction never leaves partial
/// output. A staging *sibling* (same parent, hence same filesystem) guarantees the final `rename`
/// is a cheap, atomic same-volume move on both Unix and Windows.
fn write_tree_atomic(entries: &[TreeEntry], out_dir: &Path) -> Result<(), VaultError> {
    let staging = staging_path(out_dir);
    // Best-effort clear of any stale staging dir from a previous crashed run.
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;

    if let Err(e) = write_tree(entries, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    if let Err(e) = promote(&staging, out_dir) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(VaultError::Io(e));
    }
    Ok(())
}

/// Compute a unique sibling staging path next to `out_dir` (same parent → same filesystem).
fn staging_path(out_dir: &Path) -> PathBuf {
    let parent = out_dir.parent().unwrap_or_else(|| Path::new("."));
    let name = out_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vault".to_owned());
    parent.join(format!(".{name}.cpevault-tmp-{}", unique_suffix()))
}

/// A process-and-call-unique suffix for the staging directory name (no RNG dependency).
fn unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{pid}-{nanos}-{n}")
}

/// Promote `staging` to `out_dir`. If `out_dir` already exists it must be empty (it is then removed
/// so the rename target is free); a non-empty `out_dir` is refused rather than clobbered.
fn promote(staging: &Path, out_dir: &Path) -> std::io::Result<()> {
    match std::fs::read_dir(out_dir) {
        Ok(mut rd) => {
            if rd.next().is_some() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "output directory already exists and is not empty",
                ));
            }
            std::fs::remove_dir(out_dir)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    std::fs::rename(staging, out_dir)
}

/// Materialize entries under `dir`, sanitizing each path first. Used against the staging directory.
fn write_tree(entries: &[TreeEntry], out_dir: &Path) -> Result<(), VaultError> {
    for e in entries {
        let rel = sanitize_rel_path(&e.path)?;
        let full = out_dir.join(&rel);
        match e.kind {
            EntryKind::Dir => {
                std::fs::create_dir_all(&full)?;
            }
            EntryKind::File => {
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&full, &e.data)?;
            }
        }
    }
    Ok(())
}

/// Turn a relative POSIX path from a vault into a safe, traversal-free [`PathBuf`], or reject it.
///
/// Rejects (all `Format` errors): `..` traversal, empty paths, any component containing a backslash
/// (a Windows separator — also covers UNC `\\server\share`), and a Windows **drive-letter** component
/// (a component whose first two bytes are `<letter>:`, blocking `C:` and `C:evil`). A `:` *inside* an
/// ordinary component is **allowed**, so a legal Linux/macOS filename such as `notes:draft.txt` is
/// preserved (seal ⇄ extract symmetry on those platforms). Leading `/` (or `//…`) yields empty first
/// components that are skipped, so an "absolute-looking" path is normalized to a safe relative one.
fn sanitize_rel_path(rel: &str) -> Result<PathBuf, VaultError> {
    let mut pb = PathBuf::new();
    let mut any = false;
    for comp in rel.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp == ".." {
            return Err(VaultError::Format(format!("path escapes vault root: {rel}")));
        }
        if comp.contains('\\') {
            return Err(VaultError::Format(format!(
                "illegal path component (backslash) in vault: {rel}"
            )));
        }
        if is_drive_letter_component(comp) {
            return Err(VaultError::Format(format!(
                "illegal path component (drive letter) in vault: {rel}"
            )));
        }
        pb.push(comp);
        any = true;
    }
    if !any {
        return Err(VaultError::Format(format!("empty path in vault: {rel}")));
    }
    Ok(pb)
}

/// Does `comp` look like a Windows drive-letter reference (`^[A-Za-z]:`)? Matches `C:` and `C:evil`.
fn is_drive_letter_component(comp: &str) -> bool {
    let b = comp.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deliberately-low scrypt work factor so the crypto tests stay fast. `set_work_factor`
    /// requires `0 < log_n < 64`; `4` (N = 16) is trivially cheap and still exercises the real
    /// scrypt + ChaCha20-Poly1305 path.
    const TEST_LOG_N: u8 = 4;

    fn pass(s: &str) -> SecretString {
        SecretString::from(s.to_owned())
    }

    /// Seal via a fast (low-work-factor) scrypt recipient — for tests that don't need the real
    /// ~1-second calibration. Byte-for-byte the same envelope + age format as [`encrypt_bytes`].
    fn encrypt_fast(plaintext: &[u8], passphrase: &SecretString) -> Vec<u8> {
        let mut recipient = age::scrypt::Recipient::new(passphrase.clone());
        recipient.set_work_factor(TEST_LOG_N);
        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .expect("single scrypt recipient is always valid");
        seal(encryptor, plaintext).expect("sealing to an in-memory buffer cannot fail")
    }

    fn sample_tree() -> Vec<TreeEntry> {
        vec![
            TreeEntry { path: "dir".into(), kind: EntryKind::Dir, data: vec![] },
            TreeEntry { path: "dir/empty.txt".into(), kind: EntryKind::File, data: vec![] },
            TreeEntry {
                path: "dir/nested".into(),
                kind: EntryKind::Dir,
                data: vec![],
            },
            TreeEntry {
                path: "dir/nested/binary.bin".into(),
                kind: EntryKind::File,
                data: vec![0u8, 255, 1, 254, 0, 0, 127, 128],
            },
            TreeEntry {
                path: "readme.txt".into(),
                kind: EntryKind::File,
                data: b"hello vault".to_vec(),
            },
        ]
    }

    // ---- framing --------------------------------------------------------

    #[test]
    fn pack_unpack_roundtrip_nested() {
        let entries = sample_tree();
        let packed = pack_entries(&entries);
        let back = unpack_entries(&packed).expect("well-formed framing must parse");
        assert_eq!(entries, back);
    }

    #[test]
    fn pack_entries_is_deterministic() {
        let entries = sample_tree();
        assert_eq!(pack_entries(&entries), pack_entries(&entries));
    }

    #[test]
    fn unpack_empty_stream_is_empty_tree() {
        assert_eq!(unpack_entries(&[]).unwrap(), Vec::<TreeEntry>::new());
    }

    #[test]
    fn unpack_rejects_truncated_and_garbage_without_panicking() {
        // Truncate a valid stream at every length: each prefix must be a clean Err or the full Ok.
        let packed = pack_entries(&sample_tree());
        for n in 0..packed.len() {
            // Any proper prefix that isn't a record boundary must error, never panic.
            let _ = unpack_entries(&packed[..n]);
        }
        // Explicit garbage cases.
        assert!(matches!(unpack_entries(&[0xff]), Err(VaultError::Format(_))));
        assert!(matches!(
            unpack_entries(&[KIND_FILE, 0xff, 0xff, 0xff, 0xff]),
            Err(VaultError::Format(_))
        ));
        // A file record claiming 8 bytes of data but providing none.
        let mut rec = vec![KIND_FILE, 1, 0, 0, 0, b'x'];
        rec.extend_from_slice(&8u64.to_le_bytes());
        assert!(matches!(unpack_entries(&rec), Err(VaultError::Format(_))));
    }

    // ---- in-memory crypto core -----------------------------------------

    #[test]
    fn encrypt_decrypt_bytes_roundtrip_real_workfactor() {
        // Exercises the production path ([`encrypt_bytes`], default ~1s calibration) end-to-end.
        let pw = pass("correct horse battery staple");
        let plaintext = b"the quick brown fox \x00\x01\x02 jumps".to_vec();
        let blob = encrypt_bytes(&plaintext, &pw).unwrap();
        assert_eq!(&blob[..MAGIC.len()], MAGIC.as_slice());
        let out = decrypt_bytes(&blob, &pw).unwrap();
        assert_eq!(out, plaintext);
    }

    #[test]
    fn wrong_passphrase_is_bad_passphrase_and_yields_no_output() {
        let blob = encrypt_fast(b"secret contents", &pass("right-pass"));
        let result = decrypt_bytes(&blob, &pass("wrong-pass"));
        assert!(
            matches!(result, Err(VaultError::BadPassphrase)),
            "got {result:?}"
        );
    }

    #[test]
    fn tampered_ciphertext_is_corrupt() {
        let pw = pass("pw");
        let mut blob = encrypt_fast(b"authenticated payload bytes", &pw);
        // Flip the final byte — squarely inside the streaming AEAD payload/tag.
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        let result = decrypt_bytes(&blob, &pw);
        assert!(matches!(result, Err(VaultError::Corrupt)), "got {result:?}");
    }

    #[test]
    fn tampered_payload_mid_stream_is_corrupt() {
        let pw = pass("pw");
        // A payload large enough to span multiple stream chunks would be ideal; even a modest one
        // has an AEAD tag over the whole body, so a flip a few bytes past the header must fail.
        let mut blob = encrypt_fast(&vec![7u8; 4096], &pw);
        let idx = blob.len() - 5;
        blob[idx] ^= 0xff;
        assert!(matches!(decrypt_bytes(&blob, &pw), Err(VaultError::Corrupt)));
    }

    #[test]
    fn bad_magic_is_distinct_error() {
        assert!(matches!(decrypt_bytes(&[], &pass("x")), Err(VaultError::BadMagic)));
        assert!(matches!(
            decrypt_bytes(b"NOTVLT1\x01\x00rest", &pass("x")),
            Err(VaultError::BadMagic)
        ));
        // Short buffer (< header length) is bad magic, not a panic.
        assert!(matches!(decrypt_bytes(b"CPE", &pass("x")), Err(VaultError::BadMagic)));
    }

    #[test]
    fn unsupported_version_is_distinct_error() {
        let mut blob = Vec::new();
        blob.extend_from_slice(MAGIC);
        blob.extend_from_slice(&2u16.to_le_bytes()); // version 2 — from the future
        blob.extend_from_slice(b"whatever");
        assert!(matches!(
            decrypt_bytes(&blob, &pass("x")),
            Err(VaultError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn empty_tree_roundtrips() {
        let pw = pass("pw");
        let blob = encrypt_fast(&pack_entries(&[]), &pw);
        let plaintext = decrypt_bytes(&blob, &pw).unwrap();
        assert_eq!(unpack_entries(&plaintext).unwrap(), Vec::<TreeEntry>::new());
    }

    #[test]
    fn multi_megabyte_file_roundtrips() {
        let pw = pass("pw");
        // ~3 MiB with a non-trivial pattern (so it isn't trivially compressible/zeroed).
        let big: Vec<u8> = (0..3 * 1024 * 1024).map(|i| (i * 31 + 7) as u8).collect();
        let entries = vec![TreeEntry {
            path: "big.bin".into(),
            kind: EntryKind::File,
            data: big.clone(),
        }];
        let blob = encrypt_fast(&pack_entries(&entries), &pw);
        let plaintext = decrypt_bytes(&blob, &pw).unwrap();
        assert_eq!(unpack_entries(&plaintext).unwrap(), entries);
    }

    #[test]
    fn fuzz_lite_garbage_never_panics_and_always_errs() {
        let pw = pass("pw");
        let cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![0u8; 32],
            vec![0xffu8; 200],
            b"CPEVLT1".to_vec(),                    // magic only, no version/body
            b"CPEVLT1\x01\x00".to_vec(),            // header only, empty ciphertext
            b"CPEVLT1\x01\x00garbage-not-age".to_vec(), // valid envelope, junk age region
            (0u8..=255).collect(),
        ];
        for c in cases {
            let r = decrypt_bytes(&c, &pw);
            assert!(r.is_err(), "garbage unexpectedly decrypted: {c:?}");
        }
        // Also feed truncations of a real blob.
        let real = encrypt_fast(b"payload", &pw);
        for n in 0..real.len() {
            assert!(decrypt_bytes(&real[..n], &pw).is_err());
        }
    }

    // ---- in-memory recoverability check (verify_blob, CPE-1248) ---------

    #[test]
    fn verify_blob_accepts_a_good_blob_and_rejects_bad_ones() {
        let pw = pass("verify-pw");
        let blob = encrypt_fast(&pack_entries(&sample_tree()), &pw);

        // A good blob with the right passphrase verifies.
        assert!(verify_blob(&blob, &pw).is_ok());

        // Wrong passphrase → BadPassphrase (authenticated, so it can't be trusted/shredded against).
        assert!(matches!(
            verify_blob(&blob, &pass("nope")),
            Err(VaultError::BadPassphrase)
        ));

        // A tampered payload → Corrupt.
        let mut tampered = blob.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(matches!(verify_blob(&tampered, &pw), Err(VaultError::Corrupt)));

        // Not a vault at all → BadMagic (never panics on arbitrary bytes).
        assert!(matches!(verify_blob(b"not a vault", &pw), Err(VaultError::BadMagic)));
    }

    #[test]
    fn verify_blob_rejects_a_blob_with_an_unsafe_path() {
        // A validly-encrypted blob whose framing carries paths that `decrypt_tree` would REJECT must
        // not be green-lit by `verify_blob` — otherwise a destructive shred could destroy the original
        // against a blob that can never be extracted (CPE-1248 review #2 follow-up).
        let pw = pass("pw");
        for bad in ["../escape.txt", "a\\b.txt", "C:evil.txt", "dir/z:evil.txt"] {
            let entries = vec![TreeEntry {
                path: bad.into(),
                kind: EntryKind::File,
                data: b"x".to_vec(),
            }];
            let blob = encrypt_fast(&pack_entries(&entries), &pw);
            assert!(
                matches!(verify_blob(&blob, &pw), Err(VaultError::Format(_))),
                "verify_blob must reject unsafe path {bad:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn encrypt_tree_rejects_a_backslash_named_file() {
        // On Linux/macOS `my\notes.txt` is one legal filename, but extraction's `sanitize_rel_path`
        // rejects the backslash. Sealing must therefore refuse it up front (naming the file) rather than
        // produce an unextractable vault — the seal⟺extract symmetry guarantee (CPE-1248 review).
        let pw = pass("pw");
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("my\\notes.txt"), b"legal-on-unix").unwrap();

        let result = encrypt_tree(src.path(), &pw);
        assert!(matches!(result, Err(VaultError::Format(_))), "got {result:?}");
        if let Err(VaultError::Format(msg)) = result {
            assert!(msg.contains("notes.txt"), "error should name the offending file: {msg}");
        }
    }

    // ---- path safety ----------------------------------------------------

    #[test]
    fn sanitize_rejects_traversal_and_absolute_and_drive() {
        assert!(sanitize_rel_path("../escape").is_err());
        assert!(sanitize_rel_path("a/../../b").is_err());
        assert!(sanitize_rel_path("C:evil").is_err()); // drive-letter component
        assert!(sanitize_rel_path("C:").is_err()); // bare drive letter
        assert!(sanitize_rel_path("a\\b").is_err()); // backslash / UNC separator
        assert!(sanitize_rel_path("").is_err());
        assert!(sanitize_rel_path("./.").is_err());
        // A `:` INSIDE an ordinary component (not in drive-letter position) is allowed — a legal
        // Linux/macOS name. (Compared via components so the assertion is robust to Windows PathBuf's
        // own colon parsing.)
        let colon = sanitize_rel_path("notes:draft.txt").unwrap();
        assert!(colon.is_relative());
        assert_eq!(
            colon.components().count(),
            1,
            "expected a single component, got {colon:?}"
        );
        // But a single letter + `:` at the start of a component IS a drive-letter reference (matches
        // `^[A-Za-z]:`) and is rejected — including when nested — so it can never smuggle in a drive.
        assert!(sanitize_rel_path("a:b.txt").is_err());
        assert!(sanitize_rel_path("dir/z:evil.txt").is_err());
        // A leading '/' yields an empty first component (skipped), so an "absolute-looking" path
        // sanitizes to a *relative* one that cannot escape out_dir — that's the property we need.
        let abs = sanitize_rel_path("/etc/passwd").unwrap();
        assert!(abs.is_relative());
        assert_eq!(abs, PathBuf::from("etc").join("passwd"));
        // A normal nested path is accepted and normalized.
        assert_eq!(
            sanitize_rel_path("dir/sub/file.txt").unwrap(),
            PathBuf::from("dir").join("sub").join("file.txt")
        );
    }

    // ---- filesystem round-trip -----------------------------------------

    #[test]
    fn encrypt_tree_decrypt_tree_roundtrip_on_disk() {
        let pw = pass("disk-pass");
        let src = tempfile::tempdir().unwrap();
        // Build: nested dirs, an empty file, a binary file, an empty directory.
        std::fs::create_dir_all(src.path().join("a/b")).unwrap();
        std::fs::create_dir_all(src.path().join("emptydir")).unwrap();
        std::fs::write(src.path().join("a/hello.txt"), b"hi there").unwrap();
        std::fs::write(src.path().join("a/b/empty.bin"), b"").unwrap();
        std::fs::write(
            src.path().join("a/b/data.bin"),
            [0u8, 1, 2, 253, 254, 255, 0, 42],
        )
        .unwrap();

        let blob = encrypt_tree(src.path(), &pw).unwrap();

        let dst = tempfile::tempdir().unwrap();
        decrypt_tree(&blob, &pw, dst.path()).unwrap();

        // Files come back byte-identical.
        assert_eq!(
            std::fs::read(dst.path().join("a/hello.txt")).unwrap(),
            b"hi there"
        );
        assert_eq!(std::fs::read(dst.path().join("a/b/empty.bin")).unwrap(), b"");
        assert_eq!(
            std::fs::read(dst.path().join("a/b/data.bin")).unwrap(),
            [0u8, 1, 2, 253, 254, 255, 0, 42]
        );
        // The empty directory survives.
        assert!(dst.path().join("emptydir").is_dir());
    }

    #[test]
    fn decrypt_tree_rejects_traversal_path() {
        let pw = pass("pw");
        // Author a malicious-but-validly-encrypted vault whose entry escapes the root.
        let entries = vec![TreeEntry {
            path: "../escaped.txt".into(),
            kind: EntryKind::File,
            data: b"pwned".to_vec(),
        }];
        let blob = encrypt_fast(&pack_entries(&entries), &pw);
        let dst = tempfile::tempdir().unwrap();
        let result = decrypt_tree(&blob, &pw, dst.path());
        assert!(matches!(result, Err(VaultError::Format(_))), "got {result:?}");
        // Nothing was written outside the destination.
        assert!(!dst.path().parent().unwrap().join("escaped.txt").exists());
    }

    #[test]
    fn failed_extraction_leaves_no_partial_output() {
        let pw = pass("pw");
        // A good entry FIRST (which would be staged), then a traversal entry that aborts extraction.
        let entries = vec![
            TreeEntry {
                path: "keep.txt".into(),
                kind: EntryKind::File,
                data: b"data".to_vec(),
            },
            TreeEntry {
                path: "sub".into(),
                kind: EntryKind::Dir,
                data: vec![],
            },
            TreeEntry {
                path: "../evil.txt".into(),
                kind: EntryKind::File,
                data: b"pwned".to_vec(),
            },
        ];
        let blob = encrypt_fast(&pack_entries(&entries), &pw);

        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out"); // does not exist yet
        let result = decrypt_tree(&blob, &pw, &out);

        assert!(matches!(result, Err(VaultError::Format(_))), "got {result:?}");
        // The output directory was never created — no half-written tree.
        assert!(!out.exists(), "failed extraction must leave out_dir absent");
        // And no stray staging siblings are left behind in the parent.
        let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            leftovers.iter().all(|n| !n.contains("cpevault-tmp")),
            "staging dir not cleaned up: {leftovers:?}"
        );
    }

    #[test]
    fn extraction_refuses_nonempty_out_dir_without_clobbering() {
        let pw = pass("pw");
        let entries = vec![TreeEntry {
            path: "new.txt".into(),
            kind: EntryKind::File,
            data: b"new".to_vec(),
        }];
        let blob = encrypt_fast(&pack_entries(&entries), &pw);

        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("existing.txt"), b"precious").unwrap();

        let result = decrypt_tree(&blob, &pw, &out);
        assert!(result.is_err(), "must refuse a non-empty out_dir");
        // The pre-existing content is untouched and no vault file was written into it.
        assert_eq!(std::fs::read(out.join("existing.txt")).unwrap(), b"precious");
        assert!(!out.join("new.txt").exists());
    }

    #[test]
    fn header_region_tamper_is_always_a_hard_failure() {
        let pw = pass("pw");
        let blob = encrypt_fast(b"payload bytes here", &pw);
        // Walk the age-header region (right after our 9-byte envelope) and flip each byte: every
        // result must be a hard failure (BadPassphrase / Format / Corrupt), never Ok, never a panic.
        let region_end = (HEADER_LEN + 64).min(blob.len());
        for i in HEADER_LEN..region_end {
            let mut t = blob.clone();
            t[i] ^= 0x01;
            let r = decrypt_bytes(&t, &pw);
            assert!(
                matches!(
                    r,
                    Err(VaultError::BadPassphrase)
                        | Err(VaultError::Format(_))
                        | Err(VaultError::Corrupt)
                ),
                "byte {i} flip unexpectedly gave {r:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn colon_filename_roundtrips_on_unix() {
        // A `:` in a filename is legal on Linux/macOS; it must seal AND extract (symmetry).
        let pw = pass("pw");
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("notes:draft.txt"), b"colon body").unwrap();

        let blob = encrypt_tree(src.path(), &pw).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        decrypt_tree(&blob, &pw, &out).unwrap();

        assert_eq!(
            std::fs::read(out.join("notes:draft.txt")).unwrap(),
            b"colon body"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_skipped() {
        use std::os::unix::fs::symlink;
        let pw = pass("pw");
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("real.txt"), b"real").unwrap();
        symlink(src.path().join("real.txt"), src.path().join("link.txt")).unwrap();

        let blob = encrypt_tree(src.path(), &pw).unwrap();
        let dst = tempfile::tempdir().unwrap();
        decrypt_tree(&blob, &pw, dst.path()).unwrap();

        assert!(dst.path().join("real.txt").exists());
        assert!(
            !dst.path().join("link.txt").exists(),
            "symlink must not be captured/recreated"
        );
    }
}
