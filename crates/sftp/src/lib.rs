//! SFTP filesystem provider (epic CPE-616): a remote backend over SSH/SFTP that implements
//! [`cpe_server::provider::FileSystemProvider`], so the explorer can browse a remote host by the same
//! interface it uses for the local disk. Built on `russh` + `russh-sftp` (pure Rust, no libssh2/C).
//!
//! The async surface (russh/tokio) is deliberately isolated in this crate: the provider owns a small
//! internal tokio runtime and presents a **synchronous** provider, so the lean `cpe-server` core stays
//! std-only. Host-key verification is delegated to [`cpe_server::known_hosts`] at connect time (the
//! `check_server_key` hook), so a changed/revoked key is refused before any filesystem op — the whole
//! point of SFTP over a bare TCP transport.
//!
//! Auth is by password or an OpenSSH private key (optionally passphrase-protected). Testing runs against
//! an in-process `russh-sftp` server (see the tests) — no Docker, so it runs identically on all three CI OSes.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use cpe_server::known_hosts::{verify_host_key, HostKeyVerdict, KnownHost};
use cpe_server::provider::{FileSystemProvider, ProviderEntry};
use russh::client;
use russh::keys::{ssh_key, PrivateKey, PrivateKeyWithHashAlg};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use tokio::io::AsyncWriteExt as _;
use tokio::runtime::Runtime;

/// How to authenticate to the SSH server.
#[derive(Debug, Clone)]
pub enum SftpAuth {
    /// A plaintext password.
    Password(String),
    /// An OpenSSH-format private key (the contents of e.g. `~/.ssh/id_ed25519`), with an optional
    /// passphrase if the key is encrypted.
    PrivateKey { pem: String, passphrase: Option<String> },
}

/// How to connect to a remote SFTP host.
#[derive(Debug, Clone)]
pub struct SftpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: SftpAuth,
}

impl SftpConfig {
    /// Connect with password authentication.
    pub fn password(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self { host: host.into(), port, user: user.into(), auth: SftpAuth::Password(password.into()) }
    }

    /// Connect with an OpenSSH private key (optionally passphrase-protected).
    pub fn key(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        pem: impl Into<String>,
        passphrase: Option<String>,
    ) -> Self {
        Self { host: host.into(), port, user: user.into(), auth: SftpAuth::PrivateKey { pem: pem.into(), passphrase } }
    }
}

/// What to do when the server's host key isn't already trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Only proceed if the key is already `Trusted`. An `Unknown` (first-contact) key is refused — the
    /// caller must record it out-of-band first. The safe default for unattended use.
    Strict,
    /// Trust-on-first-use: proceed for a `Trusted` **or** `Unknown` key (the caller should persist an
    /// `Unknown` key it accepted). A `Changed` or `Revoked` key is still refused.
    Tofu,
}

/// The presented host key, decomposed into the `known_hosts` fields `(key_type, key_b64)`.
type KeyFields = (String, String);

/// The russh client handler: its sole job here is host-key verification via [`cpe_server::known_hosts`].
struct VerifyingHandler {
    known: Arc<Vec<KnownHost>>,
    host: String,
    port: u16,
    policy: HostKeyPolicy,
    /// Filled in `check_server_key` so `connect` can report the verdict + the key that was presented.
    seen: Arc<Mutex<Option<(HostKeyVerdict, KeyFields)>>>,
}

impl client::Handler for VerifyingHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, server_public_key: &ssh_key::PublicKey) -> Result<bool, Self::Error> {
        let fields = openssh_fields(server_public_key);
        let verdict = verify_host_key(&self.known, &self.host, self.port, &fields.0, &fields.1);
        *self.seen.lock().unwrap() = Some((verdict, fields));
        // Only Trusted (always) or Unknown-under-TOFU may proceed; Changed/Revoked are always refused.
        Ok(matches!(
            (verdict, self.policy),
            (HostKeyVerdict::Trusted, _) | (HostKeyVerdict::Unknown, HostKeyPolicy::Tofu)
        ))
    }
}

/// Split an OpenSSH public-key line (`"ssh-ed25519 AAAA… comment"`) into the `known_hosts` fields
/// `(key_type, key_b64)`. On any encoding error, returns empties — which can only ever cause an
/// `Unknown`/`Changed` verdict (never a false `Trusted`), so it fails safe.
fn openssh_fields(key: &ssh_key::PublicKey) -> KeyFields {
    match key.to_openssh() {
        Ok(line) => {
            let mut it = line.split_whitespace();
            let ty = it.next().unwrap_or_default().to_string();
            let b64 = it.next().unwrap_or_default().to_string();
            (ty, b64)
        }
        Err(_) => (String::new(), String::new()),
    }
}

/// Parse an OpenSSH-format private key, decrypting it with `passphrase` if it is encrypted.
fn decode_private_key(pem: &str, passphrase: Option<&str>) -> Result<PrivateKey, String> {
    let key = PrivateKey::from_openssh(pem).map_err(|e| format!("sftp: invalid private key: {e}"))?;
    if key.is_encrypted() {
        let pass = passphrase.ok_or_else(|| "sftp: private key is encrypted but no passphrase was given".to_string())?;
        key.decrypt(pass).map_err(|e| format!("sftp: wrong passphrase or undecryptable key: {e}"))
    } else {
        Ok(key)
    }
}

/// A connected SFTP session presented as a synchronous [`FileSystemProvider`]. Owns its tokio runtime;
/// dropping it tears down the connection.
pub struct SftpProvider {
    rt: Runtime,
    sftp: SftpSession,
    _handle: client::Handle<VerifyingHandler>,
    verdict: HostKeyVerdict,
    presented_key: KeyFields,
}

impl SftpProvider {
    /// Connect, verify the host key against `known` under `policy`, authenticate, and open the SFTP
    /// subsystem. Fails with a clear message if the host key is refused (before any auth is attempted).
    pub fn connect(config: &SftpConfig, known: Vec<KnownHost>, policy: HostKeyPolicy) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("sftp runtime: {e}"))?;
        let seen = Arc::new(Mutex::new(None));

        let connected = {
            let seen = seen.clone();
            rt.block_on(async move {
                let handler = VerifyingHandler {
                    known: Arc::new(known),
                    host: config.host.clone(),
                    port: config.port,
                    policy,
                    seen: seen.clone(),
                };
                let ssh_config = Arc::new(client::Config::default());
                let mut session = client::connect(ssh_config, (config.host.as_str(), config.port), handler)
                    .await
                    .map_err(|e| connect_error(&seen, e))?;
                let authed = match &config.auth {
                    SftpAuth::Password(pw) => session
                        .authenticate_password(&config.user, pw)
                        .await
                        .map_err(|e| format!("sftp auth: {e}"))?
                        .success(),
                    SftpAuth::PrivateKey { pem, passphrase } => {
                        let key = decode_private_key(pem, passphrase.as_deref())?;
                        // The RSA signature hash to negotiate (ignored for non-RSA keys like Ed25519).
                        let hash = session.best_supported_rsa_hash().await.ok().flatten().flatten();
                        session
                            .authenticate_publickey(&config.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                            .await
                            .map_err(|e| format!("sftp auth: {e}"))?
                            .success()
                    }
                };
                if !authed {
                    return Err("sftp: authentication failed".to_string());
                }
                let channel = session.channel_open_session().await.map_err(|e| format!("sftp channel: {e}"))?;
                channel.request_subsystem(true, "sftp").await.map_err(|e| format!("sftp subsystem: {e}"))?;
                let sftp = SftpSession::new(channel.into_stream()).await.map_err(|e| format!("sftp init: {e}"))?;
                Ok::<_, String>((session, sftp))
            })?
        };

        let (verdict, presented_key) =
            seen.lock().unwrap().clone().unwrap_or((HostKeyVerdict::Unknown, (String::new(), String::new())));
        Ok(SftpProvider { rt, sftp: connected.1, _handle: connected.0, verdict, presented_key })
    }

    /// The host-key verdict established at connect time.
    pub fn host_key_verdict(&self) -> HostKeyVerdict {
        self.verdict
    }

    /// The host key the server presented, as `known_hosts` fields — a TOFU caller persists this after an
    /// `Unknown` verdict.
    pub fn presented_key(&self) -> &(String, String) {
        &self.presented_key
    }

    /// Connect like [`Self::connect`], and — completing TOFU (CPE-1512) — on a first-contact (`Unknown`)
    /// verdict, persist the presented host key to the **app-managed** `known_hosts` store at `record_path`
    /// (see [`cpe_server::known_hosts::append_host_key`]; never the user's real `~/.ssh/known_hosts`). A
    /// `Trusted` verdict is already recorded (no-op, no reprompt); a `Changed`/`Revoked` verdict is refused
    /// by `connect` itself before this ever runs, so this path can never write a swapped key over a
    /// disagreeing one. A `None` `record_path` (e.g. no app config dir on this platform) simply skips
    /// persistence — the connect itself still succeeds/fails exactly as [`Self::connect`] would.
    ///
    /// Persistence failure (e.g. the app config dir is unwritable) does not fail the connect — the caller
    /// already has a working session; it just won't be remembered as Trusted next time.
    pub fn connect_and_record(
        config: &SftpConfig,
        known: Vec<KnownHost>,
        policy: HostKeyPolicy,
        record_path: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        let provider = Self::connect(config, known, policy)?;
        if provider.verdict == HostKeyVerdict::Unknown {
            if let Some(path) = record_path {
                let (key_type, key_b64) = &provider.presented_key;
                let _ = cpe_server::known_hosts::append_host_key(path, &config.host, config.port, key_type, key_b64);
            }
        }
        Ok(provider)
    }

    /// Recursively walk the tree under `root`, invoking `on_entry` for each entry; cancellable. Delegates
    /// to the provider-agnostic [`cpe_server::transfer::walk`] (shared by all backends, CPE-905).
    pub fn walk(
        &self,
        root: &str,
        cancel: &AtomicBool,
        on_entry: impl FnMut(WalkEntry),
    ) -> Result<usize, String> {
        cpe_server::transfer::walk(self, root, cancel, on_entry)
    }

    /// Download the remote tree under `remote_root` into `local_dir` (cancellable). Delegates to the
    /// provider-agnostic [`cpe_server::transfer::download_tree`].
    pub fn download_tree(
        &self,
        remote_root: &str,
        local_dir: &std::path::Path,
        cancel: &AtomicBool,
    ) -> Result<usize, String> {
        cpe_server::transfer::download_tree(self, remote_root, local_dir, cancel)
    }

    /// Upload the local tree under `local_dir` into `remote_root` (cancellable). Delegates to the
    /// provider-agnostic [`cpe_server::transfer::upload_tree`].
    pub fn upload_tree(
        &mut self,
        local_dir: &std::path::Path,
        remote_root: &str,
        cancel: &AtomicBool,
    ) -> Result<usize, String> {
        cpe_server::transfer::upload_tree(self, local_dir, remote_root, cancel)
    }
}

/// One entry yielded by [`SftpProvider::walk`] — the shared type from `cpe-server`.
pub use cpe_server::transfer::WalkEntry;

/// Connect an [`SftpProvider`] from a parsed [`cpe_server::location::Location`] (must be `Sftp`) plus an
/// auth method — the bridge from a user-typed `sftp://user@host[:port]/path` to a live provider. Port
/// defaults to 22; a username is required (SFTP has no anonymous mode).
pub fn connect_location(
    loc: &cpe_server::location::Location,
    auth: SftpAuth,
    known: Vec<KnownHost>,
    policy: HostKeyPolicy,
) -> Result<SftpProvider, String> {
    use cpe_server::location::Scheme;
    if loc.scheme != Scheme::Sftp {
        return Err(format!("sftp: not an SFTP location (scheme {:?})", loc.scheme));
    }
    let host = loc.host.as_deref().ok_or("sftp: location has no host")?;
    let user = loc.user.as_deref().ok_or("sftp: location has no user (use sftp://user@host/…)")?;
    let config = SftpConfig {
        host: host.to_string(),
        port: loc.port.unwrap_or(22),
        user: user.to_string(),
        auth,
    };
    SftpProvider::connect(&config, known, policy)
}

/// Turn a failed `connect` into a legible error, upgrading a host-key refusal into a specific message
/// (the raw russh error for a rejected key is opaque).
fn connect_error(seen: &Mutex<Option<(HostKeyVerdict, KeyFields)>>, err: russh::Error) -> String {
    match seen.lock().unwrap().as_ref().map(|(v, _)| *v) {
        Some(HostKeyVerdict::Changed) => "sftp: host key CHANGED — refused (possible man-in-the-middle)".into(),
        Some(HostKeyVerdict::Revoked) => "sftp: host key is REVOKED — refused".into(),
        Some(HostKeyVerdict::Unknown) => "sftp: unknown host key — refused (not in known_hosts)".into(),
        _ => format!("sftp connect: {err}"),
    }
}

impl FileSystemProvider for SftpProvider {
    fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
        self.rt.block_on(async {
            let dir = self.sftp.read_dir(path).await.map_err(|e| format!("{path}: {e}"))?;
            Ok(dir
                // Source-side path-traversal defense (CPE-1461): the READDIR filename is server-supplied
                // and russh-sftp only strips exact `.`/`..`. Drop any name that isn't a safe single
                // segment (contains `/`/`\`, is `.`/`..`, or carries a drive/root prefix) so it can never
                // reach the local-write sink in `transfer::download_tree`.
                .filter(|entry| cpe_server::transfer::is_safe_name(&entry.file_name()))
                .map(|entry| {
                    let is_dir = entry.file_type().is_dir();
                    ProviderEntry {
                        name: entry.file_name(),
                        is_dir,
                        size: if is_dir { 0 } else { entry.metadata().len() },
                    }
                })
                .collect())
        })
    }

    fn stat(&self, path: &str) -> Result<ProviderEntry, String> {
        self.rt.block_on(async {
            let meta = self.sftp.metadata(path).await.map_err(|e| format!("{path}: {e}"))?;
            let is_dir = meta.file_type().is_dir();
            let name = path.rsplit('/').find(|s| !s.is_empty()).unwrap_or(path).to_string();
            Ok(ProviderEntry { name, is_dir, size: if is_dir { 0 } else { meta.len() } })
        })
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        self.rt.block_on(async { self.sftp.read(path).await.map_err(|e| format!("{path}: {e}")) })
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        // Create-or-overwrite semantics (the convenience `SftpSession::write` opens WRITE-only and fails
        // if the file doesn't exist), so a provider write behaves like a local one.
        self.rt.block_on(async {
            let mut file = self
                .sftp
                .open_with_flags(path, OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE)
                .await
                .map_err(|e| format!("{path}: {e}"))?;
            file.write_all(data).await.map_err(|e| format!("{path}: {e}"))?;
            file.shutdown().await.map_err(|e| format!("{path}: {e}"))?;
            Ok(())
        })
    }

    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        self.rt.block_on(async { self.sftp.create_dir(path).await.map_err(|e| format!("{path}: {e}")) })
    }

    fn delete(&mut self, path: &str) -> Result<(), String> {
        self.rt.block_on(async {
            // A path can be a file or a dir; try file removal first, then directory.
            match self.sftp.remove_file(path).await {
                Ok(()) => Ok(()),
                Err(_) => self.sftp.remove_dir(path).await.map_err(|e| format!("{path}: {e}")),
            }
        })
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.rt.block_on(async { self.sftp.rename(from, to).await.map_err(|e| format!("{from} -> {to}: {e}")) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpe_server::known_hosts::{host_token, parse_known_hosts};
    use russh::keys::{Algorithm, PrivateKey};
    use russh::server::{Auth, Msg, Server as _, Session};
    use russh::{Channel, ChannelId};
    use russh_sftp::protocol::{
        Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
    };
    use std::collections::{HashMap, HashSet};
    use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    // --- A real filesystem-backed in-process SFTP server, rooted at a temp dir. It maps SFTP ops onto
    // `std::fs`, so the provider's full surface — list/stat/read AND write/mkdir/delete — round-trips
    // against actual files over a real SSH handshake. Reads/writes are offset-based (open+seek+op per
    // call), so no open-file table is needed; only dir-read state is tracked (to return EOF). ---

    const FILE_NAME: &str = "readme.txt";
    const FILE_BODY: &[u8] = b"hello world"; // 11 bytes
    const DIR_NAME: &str = "sub";

    fn ok_status(id: u32) -> Status {
        Status { id, status_code: StatusCode::Ok, error_message: String::new(), language_tag: String::new() }
    }
    fn io_err(e: std::io::Error) -> StatusCode {
        match e.kind() {
            std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
            std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
            _ => StatusCode::Failure,
        }
    }
    fn attrs_of(meta: &std::fs::Metadata) -> FileAttributes {
        let mut a = FileAttributes::default();
        if meta.is_dir() {
            a.set_dir(true);
        } else {
            a.set_regular(true);
            a.size = Some(meta.len());
        }
        a
    }

    struct FsSftp {
        root: PathBuf,
        dirs_read: HashSet<String>, // dir handles whose entries were already returned (→ EOF next)
    }

    impl FsSftp {
        fn new(root: PathBuf) -> Self {
            Self { root, dirs_read: HashSet::new() }
        }
        /// Map an SFTP path (server-absolute, `/`-rooted) to a real path under `root`.
        fn real(&self, sftp_path: &str) -> PathBuf {
            let rel = sftp_path.trim_start_matches('/');
            if rel.is_empty() || rel == "." {
                self.root.clone()
            } else {
                self.root.join(rel)
            }
        }
    }

    impl russh_sftp::server::Handler for FsSftp {
        type Error = StatusCode;
        fn unimplemented(&self) -> Self::Error {
            StatusCode::OpUnsupported
        }

        async fn init(&mut self, _v: u32, _e: HashMap<String, String>) -> Result<Version, Self::Error> {
            Ok(Version::new())
        }

        async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
            let resolved = if path == "." { "/".to_string() } else { path };
            Ok(Name { id, files: vec![File::dummy(resolved)] })
        }

        async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
            // CPE-1692: `!is_dir()` collapsed a `stat` FAILURE (missing, permission-denied, …) and a
            // successful stat of the WRONG type into the same `NoSuchFile` — the fixture's own `stat`
            // handler above already routes a genuine stat failure through `io_err` (which distinguishes
            // `NoSuchFile` from `PermissionDenied` from a generic `Failure`); this was the one caller
            // that didn't. `metadata()` + `io_err` here matches that existing convention: a real stat
            // failure gets its own real cause, and only a *successful* stat of a non-directory is the
            // (still real, still not an absence) type mismatch.
            let meta = std::fs::metadata(self.real(&path)).map_err(io_err)?;
            if !meta.is_dir() {
                return Err(StatusCode::Failure);
            }
            self.dirs_read.remove(&path);
            Ok(Handle { id, handle: path })
        }

        async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
            if !self.dirs_read.insert(handle.clone()) {
                return Err(StatusCode::Eof); // already returned this dir's entries
            }
            let mut files = Vec::new();
            for entry in std::fs::read_dir(self.real(&handle)).map_err(io_err)?.flatten() {
                let Ok(meta) = entry.metadata() else { continue };
                files.push(File::new(entry.file_name().to_string_lossy().to_string(), attrs_of(&meta)));
            }
            Ok(Name { id, files })
        }

        async fn close(&mut self, id: u32, _handle: String) -> Result<Status, Self::Error> {
            Ok(ok_status(id))
        }

        async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
            let meta = std::fs::metadata(self.real(&path)).map_err(io_err)?;
            Ok(Attrs { id, attrs: attrs_of(&meta) })
        }

        async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
            let meta = std::fs::symlink_metadata(self.real(&path)).map_err(io_err)?;
            Ok(Attrs { id, attrs: attrs_of(&meta) })
        }

        async fn open(
            &mut self,
            id: u32,
            filename: String,
            pflags: OpenFlags,
            _attrs: FileAttributes,
        ) -> Result<Handle, Self::Error> {
            let real = self.real(&filename);
            if pflags.contains(OpenFlags::CREATE) {
                // Create (and truncate if asked) up front; the seek-based write() ops fill it in.
                //
                // CPE-1726, the sibling primitive (CPE-1719's failure shape, checked here rather than
                // only `rename`): this is the write path, and `OpenOptions::create(true)` **follows** a
                // link at the final component — an SFTP `open` onto a symlink truncates and rewrites the
                // link's *target*, not the link. `create_new(true)` is the opener that refuses instead
                // (`cpe_server::fsutil::stage_exclusive`), but that is the wrong semantics for a server:
                // OpenSSH's sftp-server follows the link too. Left as-is for the same measured reason as
                // `rename` below — this is `#[cfg(test)]`-only — but recorded so the next sweep does not
                // have to rediscover which of the two shapes this is.
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(pflags.contains(OpenFlags::TRUNCATE))
                    .open(&real)
                    .map_err(io_err)?;
            } else {
                // CPE-1692: `!real.exists()` swallowed a stat failure (permission-denied, a dead network
                // mount, …) into the same `NoSuchFile` a genuine absence gets. `try_exists()` is the
                // right primitive here — only existence is in question, not the entry's type — and its
                // `Err` leg still goes through `io_err` for the same classification `stat`/`opendir` use.
                match real.try_exists() {
                    Ok(true) => {}
                    Ok(false) => return Err(StatusCode::NoSuchFile),
                    Err(e) => return Err(io_err(e)),
                }
            }
            Ok(Handle { id, handle: filename })
        }

        async fn read(&mut self, id: u32, handle: String, offset: u64, len: u32) -> Result<Data, Self::Error> {
            let mut f = std::fs::File::open(self.real(&handle)).map_err(io_err)?;
            f.seek(SeekFrom::Start(offset)).map_err(io_err)?;
            let mut buf = vec![0u8; len as usize];
            let n = f.read(&mut buf).map_err(io_err)?;
            if n == 0 {
                return Err(StatusCode::Eof);
            }
            buf.truncate(n);
            Ok(Data { id, data: buf })
        }

        async fn write(&mut self, id: u32, handle: String, offset: u64, data: Vec<u8>) -> Result<Status, Self::Error> {
            let mut f = std::fs::OpenOptions::new().write(true).open(self.real(&handle)).map_err(io_err)?;
            f.seek(SeekFrom::Start(offset)).map_err(io_err)?;
            f.write_all(&data).map_err(io_err)?;
            Ok(ok_status(id))
        }

        async fn mkdir(&mut self, id: u32, path: String, _attrs: FileAttributes) -> Result<Status, Self::Error> {
            std::fs::create_dir_all(self.real(&path)).map_err(io_err)?;
            Ok(ok_status(id))
        }

        async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
            std::fs::remove_file(self.real(&filename)).map_err(io_err)?;
            Ok(ok_status(id))
        }

        /// CPE-1731: `remove_dir`, **not** `remove_dir_all`. `SSH_FXP_RMDIR` (RFC 4251 draft-filexfer
        /// §7.3, and OpenSSH's `sftp-server.c` `process_rmdir`) is `rmdir(2)` — it removes an *empty*
        /// directory and fails otherwise. Deleting the subtree instead is behaviour no server this
        /// client will ever meet has, and a test double that quietly succeeds where the wire says
        /// "directory not empty" lets a client test pass against a fiction.
        ///
        /// The error code follows from [`io_err`] rather than from a new branch: `ENOTEMPTY` is neither
        /// `NotFound` nor `PermissionDenied`, so it maps to `StatusCode::Failure` — which is what
        /// OpenSSH returns for it under protocol 3 (`SSH_FX_FAILURE`; the finer `SSH_FX_DIR_NOT_EMPTY`
        /// only exists from version 6). Measured on both CI platforms: `remove_dir` on a non-empty
        /// directory is `Err` with `raw_os_error 145`/`ERROR_DIR_NOT_EMPTY` on Windows and `39`/
        /// `ENOTEMPTY` on Linux, `ErrorKind::DirectoryNotEmpty` on each, and the directory and its
        /// contents are untouched afterwards. (`ErrorKind::DirectoryNotEmpty` is deliberately not named
        /// in the code: it stabilised in Rust 1.83 and this crate declares `rust-version = "1.77.2"`.)
        ///
        /// `SftpProvider::delete` — the **shipped** half — already classifies dir-vs-file and issues
        /// `remove_file` or `rmdir`, so a client deleting a populated directory now gets the same
        /// refusal from this rig that a real server would give it.
        async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
            std::fs::remove_dir(self.real(&path)).map_err(io_err)?;
            Ok(ok_status(id))
        }

        /// **CPE-1731: compare the RESOLVED destination against the served root — do not enumerate
        /// spellings.** `rename("/", "")` and `rename("/", ".")` were answered `Ok(())`, in a crate
        /// CPE-1726 declared *structurally immune* to that defect on the grounds that SFTP takes both
        /// paths from one resolver in one message rather than from a second header. True of the source;
        /// false of the destination — [`FsSftp::real`] maps empty *and* `"."` *and* `"/"` to the served
        /// root, so the rig expressed the identical shape the WebDAV half had just been fixed for.
        ///
        /// The property is shared, so the implementation is too: [`cpe_server::fsutil::same_place`] is
        /// the one CPE-1726 arrived at after five rounds of denylists, and its doc carries the whole
        /// argument (why it canonicalizes when both sides resolve, why the lexical fallback pops `..`,
        /// and the proof that the pop errs safe). Read it before changing this line.
        ///
        /// **Reused after re-measuring, not inherited** — inheriting a claim across these three crates
        /// is what produced this ticket. CPE-1731's probe ran *this* rig's `real` over all three escape
        /// families on Windows and Linux; the one difference that matters is that a wire path is
        /// `/`-separated even on Windows, so a resolved destination is mixed-separator
        /// (`…\cpe-sftp-srv-0\sub/..`). `Path::components()` and `canonicalize` both accept that, and
        /// every root-resolving row still compares equal. Full findings on `same_place`.
        ///
        /// The reply is `SSH_FX_FAILURE`, which is what OpenSSH's `sftp-server` returns for a rename it
        /// will not perform. **No separate "the path is empty" branch**, tempting as one is: an empty
        /// destination is a *member* of the family this guard closes, and a second check catching it
        /// first would keep the headline regression row green with this comparison deleted — masking
        /// exactly the guard the row exists to prove.
        ///
        /// **The SOURCE side is deliberately NOT guarded, and the gap is real:** `rename("/", "/x")`
        /// still moves the served root into a subdirectory. Out of scope by choice, not by oversight —
        /// this ticket's subject is the destination, `cpe-webdav` has the identical asymmetry recorded
        /// at its own MOVE, and a source guard needs the containment check CPE-1730 is opening.
        /// Recorded here so the next sweep reads it as a decision rather than as an absence nobody
        /// noticed.
        async fn rename(&mut self, id: u32, oldpath: String, newpath: String) -> Result<Status, Self::Error> {
            let dest = self.real(&newpath);
            if cpe_server::fsutil::same_place(&dest, &self.root) {
                return Err(StatusCode::Failure);
            }
            // CPE-1726 re-took CPE-1710's classification against a **measurement** instead of a category
            // ("it is a protocol server" is a category). DELIBERATELY UNGUARDED — do not wrap this in
            // `cpe_server::fsutil::rename_into_slot`; the measurement is:
            //
            // 1. This entire SFTP server is `#[cfg(test)]`. `cpe-sftp` ships a *client*
            //    ([`SftpProvider`]) and no server, so this line is not compiled into the app. The
            //    "remote client" supplying `newpath` is a test in this same file, over loopback,
            //    against a per-test temp root this rig created and seeded itself. There is no third
            //    party whose files sit at the destination — the premise the ticket weighed ("a user
            //    running the SFTP server to share a folder did not agree to have their symlinks
            //    replaced by whoever connects") describes a server this repo does not ship, and that
            //    absence is what decides it.
            //    **Bounded precisely (PR #902 review):** "no user's files at the destination" holds
            //    because no user drives this rig, NOT because the destination is confined — `real()`
            //    is a bare join with no containment check, so a `..`-shaped path would resolve
            //    outside the temp root. Nothing sends one today; CPE-1730 tracks closing it. If that
            //    ever changes before CPE-1730 lands, this reason expires with it.
            // 2. That premise is pinned rather than trusted:
            //    `cpe_1726_every_destructive_filesystem_call_is_confined_to_the_test_rig` goes red the
            //    moment this line (or any sibling destructive primitive) moves above the `#[cfg(test)]`
            //    marker, so promoting the rig to production forces the decision to be re-taken rather
            //    than silently inherited.
            // 3. A test double must model the wire, not defend against it. Hardening the rig would make
            //    the client tests pass against a server unlike the OpenSSH one the app will actually
            //    meet.
            //
            // What `fs::rename` does to a link at the destination is pinned, not assumed, by
            // `cpe_1726_rename_onto_a_link_never_writes_through_it`.
            #[allow(clippy::disallowed_methods)]
            std::fs::rename(self.real(&oldpath), &dest).map_err(io_err)?;
            Ok(ok_status(id))
        }
    }

    // The SSH layer: accept any password (or, if configured, only a specific public key), then hand the
    // `sftp` subsystem to the canned handler.
    #[derive(Clone)]
    struct TestServer {
        root: PathBuf,
        accept_pubkey: Option<ssh_key::PublicKey>,
    }

    impl russh::server::Server for TestServer {
        type Handler = SshSession;
        fn new_client(&mut self, _: Option<SocketAddr>) -> SshSession {
            SshSession { channel: None, root: self.root.clone(), accept_pubkey: self.accept_pubkey.clone() }
        }
    }

    struct SshSession {
        channel: Option<Channel<Msg>>,
        root: PathBuf,
        accept_pubkey: Option<ssh_key::PublicKey>,
    }

    impl russh::server::Handler for SshSession {
        type Error = russh::Error;

        async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
            Ok(Auth::Accept)
        }

        async fn auth_publickey(&mut self, _user: &str, key: &ssh_key::PublicKey) -> Result<Auth, Self::Error> {
            Ok(match &self.accept_pubkey {
                Some(expected) if key == expected => Auth::Accept,
                _ => Auth::reject(),
            })
        }

        async fn channel_open_session(&mut self, channel: Channel<Msg>, _s: &mut Session) -> Result<bool, Self::Error> {
            self.channel = Some(channel);
            Ok(true)
        }

        async fn subsystem_request(&mut self, id: ChannelId, name: &str, session: &mut Session) -> Result<(), Self::Error> {
            if name == "sftp" {
                let channel = self.channel.take().expect("channel opened before subsystem");
                session.channel_success(id)?;
                // The handler is called inline on the session's message loop, so it must NOT block on the
                // SFTP I/O (that loop is what pumps the channel data the SFTP server reads/writes) — spawn
                // it and return immediately.
                tokio::spawn(russh_sftp::server::run(channel.into_stream(), FsSftp::new(self.root.clone())));
            } else {
                session.channel_failure(id)?;
            }
            Ok(())
        }
    }

    /// Spawn the canned server on an ephemeral loopback port (its own thread + runtime), returning the
    /// address and the host public key as `known_hosts` fields `(key_type, key_b64)`. If `accept_pubkey`
    /// is set, the server only accepts publickey auth with that exact key (else it accepts any password).
    fn spawn_server_with(accept_pubkey: Option<ssh_key::PublicKey>) -> (SocketAddr, KeyFields) {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).expect("gen host key");
        let pub_fields = openssh_fields(key.public_key());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        // `tokio::net::TcpListener::from_std` requires the socket already be non-blocking on Unix (Windows
        // is lenient) — without this the server thread panics on Linux/macOS.
        listener.set_nonblocking(true).unwrap();

        // Seed a temp root: one file `readme.txt` ("hello world") + one empty dir `sub` (the OS reaper
        // cleans temp; each server gets a unique dir).
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("cpe-sftp-srv-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(root.join(DIR_NAME)).unwrap();
        std::fs::write(root.join(FILE_NAME), FILE_BODY).unwrap();
        std::fs::write(root.join(DIR_NAME).join("nested.txt"), b"deep").unwrap();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let config = Arc::new(russh::server::Config { keys: vec![key], ..Default::default() });
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                // run_on_socket drives the full accept + per-connection session lifecycle.
                let _ = TestServer { root, accept_pubkey }.run_on_socket(config, &listener).await;
            });
        });
        (addr, pub_fields)
    }

    fn spawn_server() -> (SocketAddr, KeyFields) {
        spawn_server_with(None)
    }

    /// Like [`spawn_server`] but also returns the server's on-disk root, so a test can seed extra
    /// (possibly hostile-named) files into it before listing, or construct an on-disk permission
    /// condition (CPE-1692). Accepts any password. Seeds the same `readme.txt` + `sub/nested.txt` as the
    /// other spawners. Not `#[cfg(unix)]` despite having a Unix-only first caller (a backslash filename
    /// isn't creatable on Windows) — nothing in this function is Unix-specific, and CPE-1692's
    /// permission-denied test needs it on every OS.
    fn spawn_server_returning_root() -> (SocketAddr, KeyFields, PathBuf) {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).expect("gen host key");
        let pub_fields = openssh_fields(key.public_key());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();

        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("cpe-sftp-srvroot-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(root.join(DIR_NAME)).unwrap();
        std::fs::write(root.join(FILE_NAME), FILE_BODY).unwrap();
        std::fs::write(root.join(DIR_NAME).join("nested.txt"), b"deep").unwrap();

        let root_ret = root.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let config = Arc::new(russh::server::Config { keys: vec![key], ..Default::default() });
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let _ = TestServer { root, accept_pubkey: None }.run_on_socket(config, &listener).await;
            });
        });
        (addr, pub_fields, root_ret)
    }

    /// A `known_hosts` list trusting `(key_type, key_b64)` at `127.0.0.1:port`.
    fn known_for(port: u16, key: &KeyFields) -> Vec<KnownHost> {
        parse_known_hosts(&format!("{} {} {}", host_token("127.0.0.1", port), key.0, key.1))
    }

    /// A fresh scratch **path** for an app-managed known_hosts store used by a test (the file itself
    /// starts absent — `connect_and_record`/`load_known_hosts` both treat that as empty). Unique per test
    /// run so parallel `cargo test` runs never collide.
    fn scratch_known_hosts_path(tag: &str) -> std::path::PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("cpe-sftp-kh-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("known_hosts")
    }

    // CPE-1512: connect_and_record completes TOFU — a first-contact connect persists the presented key to
    // the app-managed store, a later connect with the SAME key resolves Trusted against that store (no
    // reprompt, no re-record/duplicate), and a later connect with a DIFFERENT key for the same host is
    // refused as Changed (never silently auto-trusted).
    #[test]
    fn connect_and_record_persists_first_contact_then_trusts_the_same_key() {
        let (addr, hostkey) = spawn_server();
        let store = scratch_known_hosts_path("first-contact");
        assert!(!store.exists(), "store starts absent, like a fresh app install");
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");

        // First contact: no known_hosts entry anywhere → Unknown, accepted under Tofu, and recorded.
        let first = SftpProvider::connect_and_record(&cfg, vec![], HostKeyPolicy::Tofu, Some(&store))
            .expect("TOFU should accept a first-contact host");
        assert_eq!(first.host_key_verdict(), HostKeyVerdict::Unknown);
        assert!(store.exists(), "the presented key must be written to the app-managed store");
        let recorded = cpe_server::known_hosts::load_known_hosts(&store);
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].key_type, hostkey.0);
        assert_eq!(recorded[0].key_b64, hostkey.1);

        // A second connect loads the now-recorded store: SAME key → Trusted, and even under Strict (no
        // TOFU needed anymore) — proving the record actually established trust, not just Tofu leniency.
        let known = cpe_server::known_hosts::load_known_hosts(&store);
        let second = SftpProvider::connect_and_record(&cfg, known, HostKeyPolicy::Strict, Some(&store))
            .expect("a recorded key should now be Trusted under Strict");
        assert_eq!(second.host_key_verdict(), HostKeyVerdict::Trusted);

        // Re-recording (Trusted, not Unknown) must not duplicate the entry.
        assert_eq!(cpe_server::known_hosts::load_known_hosts(&store).len(), 1, "no duplicate on re-record");

        let _ = std::fs::remove_dir_all(store.parent().unwrap());
    }

    #[test]
    fn connect_and_record_refuses_a_swapped_key_without_auto_trusting() {
        let (addr, _real_hostkey) = spawn_server();
        let store = scratch_known_hosts_path("swapped-key");
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");

        // Simulate an app store that already recorded a DIFFERENT key for this host (e.g. from a prior,
        // now-replaced server) — a real MITM (or legitimate rekey) presents the server's actual key, which
        // won't match.
        cpe_server::known_hosts::append_host_key(
            &store,
            "127.0.0.1",
            addr.port(),
            "ssh-ed25519",
            "AAAAAstalekeythatdoesnotmatch",
        )
        .unwrap();
        let known = cpe_server::known_hosts::load_known_hosts(&store);

        let err = match SftpProvider::connect_and_record(&cfg, known, HostKeyPolicy::Tofu, Some(&store)) {
            Ok(_) => panic!("a changed host key must be refused, even under Tofu"),
            Err(e) => e,
        };
        assert!(err.contains("CHANGED"), "expected a changed-key refusal, got: {err}");

        // The store must be untouched — no auto-trust of the swapped key, still just the one stale entry.
        let after = cpe_server::known_hosts::load_known_hosts(&store);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].key_b64, "AAAAAstalekeythatdoesnotmatch");

        let _ = std::fs::remove_dir_all(store.parent().unwrap());
    }

    #[test]
    fn connect_and_record_with_no_record_path_behaves_like_plain_connect() {
        // A platform with no app config dir (record_path = None) must not fail the connect — persistence
        // is simply skipped.
        let (addr, hostkey) = spawn_server();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider = SftpProvider::connect_and_record(&cfg, vec![], HostKeyPolicy::Tofu, None)
            .expect("TOFU should still accept with no record_path");
        assert_eq!(provider.host_key_verdict(), HostKeyVerdict::Unknown);
        assert_eq!(provider.presented_key(), &hostkey);
    }

    // Full happy path over a real in-process SSH/SFTP handshake: host-key verification (Trusted) →
    // list → stat → read, plus a TOFU accept of an unknown host.
    #[test]
    fn connects_to_a_trusted_host_then_lists_stats_and_reads() {
        let (addr, hostkey) = spawn_server();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider = SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict)
            .expect("connect to a trusted host should succeed");
        assert_eq!(provider.host_key_verdict(), HostKeyVerdict::Trusted);

        let mut entries = provider.list("/").expect("list");
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].name.as_str(), entries[0].is_dir), (FILE_NAME, false));
        assert_eq!(entries[0].size, FILE_BODY.len() as u64);
        assert_eq!((entries[1].name.as_str(), entries[1].is_dir), (DIR_NAME, true));

        assert!(!provider.stat(&format!("/{FILE_NAME}")).unwrap().is_dir);
        assert_eq!(provider.read(&format!("/{FILE_NAME}")).unwrap(), FILE_BODY);

        // TOFU accepts an unknown host and surfaces its key so a caller could persist it.
        let tofu = SftpProvider::connect(&cfg, vec![], HostKeyPolicy::Tofu).expect("TOFU should accept");
        assert_eq!(tofu.host_key_verdict(), HostKeyVerdict::Unknown);
        assert_eq!(tofu.presented_key(), &hostkey);
    }

    #[test]
    fn writes_mkdirs_lists_and_deletes_round_trip() {
        let (addr, hostkey) = spawn_server();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let mut provider = SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict)
            .expect("connect");

        // Write a new file, read it back verbatim.
        provider.write("/notes.txt", b"remote write works").expect("write");
        assert_eq!(provider.read("/notes.txt").unwrap(), b"remote write works");

        // Make a directory; stat sees it as a dir.
        provider.mkdir("/newdir").expect("mkdir");
        assert!(provider.stat("/newdir").unwrap().is_dir);

        // Both new entries appear in the listing (alongside the seeded readme.txt + sub).
        let names: Vec<String> = provider.list("/").unwrap().into_iter().map(|e| e.name).collect();
        assert!(names.contains(&"notes.txt".to_string()), "got {names:?}");
        assert!(names.contains(&"newdir".to_string()), "got {names:?}");

        // Delete the file, then the dir — both gone afterwards.
        provider.delete("/notes.txt").expect("delete file");
        assert!(provider.stat("/notes.txt").is_err(), "file should be gone");
        provider.delete("/newdir").expect("delete dir");
        assert!(provider.stat("/newdir").is_err(), "dir should be gone");
    }

    #[test]
    fn walk_recurses_the_remote_tree() {
        let (addr, hostkey) = spawn_server();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();
        let cancel = AtomicBool::new(false);
        let mut paths = Vec::new();
        let n = provider.walk("/", &cancel, |e| paths.push((e.path, e.is_dir))).unwrap();
        paths.sort();
        assert_eq!(n, 3, "readme.txt + sub + sub/nested.txt; got {paths:?}");
        assert!(paths.contains(&("/readme.txt".to_string(), false)));
        assert!(paths.contains(&("/sub".to_string(), true)));
        assert!(paths.contains(&("/sub/nested.txt".to_string(), false)));
    }

    #[test]
    fn download_tree_recreates_the_remote_files_locally() {
        let (addr, hostkey) = spawn_server();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();

        let out = std::env::temp_dir().join(format!("cpe-sftp-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let cancel = AtomicBool::new(false);
        let files = provider.download_tree("/", &out, &cancel).expect("download");

        assert_eq!(files, 2, "readme.txt + sub/nested.txt");
        assert_eq!(std::fs::read(out.join("readme.txt")).unwrap(), FILE_BODY);
        assert_eq!(std::fs::read(out.join("sub").join("nested.txt")).unwrap(), b"deep");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn upload_tree_recreates_local_files_on_the_remote() {
        let (addr, hostkey) = spawn_server();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let mut provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();

        // Build a local tree: a.txt + inner/b.txt.
        let src = std::env::temp_dir().join(format!("cpe-sftp-up-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("a.txt"), b"alpha").unwrap();
        std::fs::write(src.join("inner").join("b.txt"), b"bravo").unwrap();

        let cancel = AtomicBool::new(false);
        let files = provider.upload_tree(&src, "/up", &cancel).expect("upload");
        assert_eq!(files, 2);

        // Read them back over SFTP to confirm they landed with the right structure + content.
        assert_eq!(provider.read("/up/a.txt").unwrap(), b"alpha");
        assert_eq!(provider.read("/up/inner/b.txt").unwrap(), b"bravo");
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn walk_stops_promptly_when_cancelled() {
        let (addr, hostkey) = spawn_server();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();
        let cancel = AtomicBool::new(false);
        let mut count = 0;
        // Cancel from inside the callback after the very first entry → the walk stops immediately.
        let visited = provider
            .walk("/", &cancel, |_| {
                count += 1;
                cancel.store(true, Ordering::Relaxed);
            })
            .unwrap();
        assert_eq!((visited, count), (1, 1), "should stop right after the first entry");
    }

    #[test]
    fn rename_moves_a_file_over_sftp() {
        let (addr, hostkey) = spawn_server();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let mut provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();
        provider.rename("/readme.txt", "/renamed.txt").expect("rename");
        assert_eq!(provider.read("/renamed.txt").unwrap(), FILE_BODY);
        assert!(provider.read("/readme.txt").is_err(), "old path should be gone");
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1726 — the unguarded `fs::rename` on a remote-supplied path, decided by measurement
    // ---------------------------------------------------------------------------------------------

    /// Every `std::fs` primitive that can destroy something, as the literal source text a sweep would
    /// grep for. `fs::copy` and `File::create` are in the list even though this crate has neither: the
    /// point of a guard is to catch the one that gets added later, and CPE-1719 was missed precisely
    /// because the sweep looked for `rename` while the bug was a `write`.
    const CPE_1726_DESTRUCTIVE_CALLS: &[&str] = &[
        "std::fs::rename(",
        "std::fs::write(",
        "std::fs::copy(",
        "std::fs::remove_file(",
        "std::fs::remove_dir(",
        "std::fs::remove_dir_all(",
        "std::fs::File::create(",
        "std::fs::OpenOptions",
    ];

    /// **Catches verbatim promotion of the current rig — not a general audit of the shipped half.**
    /// (An earlier draft called this "the guard that carries CPE-1726's decision", which the UAT
    /// showed is too strong: written in this file's own prevailing style — `use std::fs;` at the top,
    /// then `fs::write(..)` — **seven** of the eight primitives slip past this scan, and clippy's
    /// CPE-1710 ban catches only one of them, `rename`. See the scope section below.)
    /// The `#[allow(clippy::disallowed_methods)]` on the
    /// rig's `rename` argues that the unguarded rename is safe *because the whole server is a
    /// `#[cfg(test)]` test double* — no shipped code, no third-party files at the destination. That is a
    /// measurement, not a category, and this test is what keeps it a measurement: if the rig (or any
    /// single destructive call in it) is ever promoted above the `#[cfg(test)]` marker, this goes red
    /// and the decision has to be re-taken rather than inherited from a comment written when it was
    /// still true.
    ///
    /// Note the shipped half of this crate *does* delete things — `SftpProvider::delete` issues an SFTP
    /// `remove_file` — but over the wire, on the **remote**, which is the app asking a real server to do
    /// something rather than a local destructive primitive. The needles are `std::fs::`-qualified
    /// precisely so that distinction survives: `self.sftp.remove_file(..)` is not a local write and must
    /// not be reported as one.
    ///
    /// `\r` is stripped first: the working tree is CRLF on Windows and LF on the Linux/macOS runners,
    /// and a needle containing `\n` would silently match nothing on one of them — a guard that cannot
    /// fail on half the matrix is the failure this ticket family exists to stop.
    ///
    /// # What this scan does NOT catch, stated because an earlier draft overstated it
    /// The `std::fs::` qualification is load-bearing in one direction (it is what keeps
    /// `self.sftp.remove_file` above from being reported as a local write) and a gap in the other:
    /// `use std::fs;` followed by `fs::write(..)` or `fs::remove_dir_all(..)` in the shipped half passes
    /// this scan untouched — and unqualified `fs::` is this repo's prevailing style. Of the eight
    /// needles only `fs::rename` has a second line of defence, CPE-1710's `clippy.toml` ban, which does
    /// catch the unqualified spelling. So this is a **backstop against the specific regression CPE-1726
    /// reasoned about** — the rig being promoted out of `#[cfg(test)]`, which moves these exact
    /// fully-qualified lines — not a general audit of the shipped half, and it should not be cited as
    /// one. Widening it to unqualified `fs::` is a fine follow-up; leaving the gap unstated is not.
    #[test]
    fn cpe_1726_every_destructive_filesystem_call_is_confined_to_the_test_rig() {
        let src = include_str!("lib.rs").replace('\r', "");
        let marker = "\n#[cfg(test)]\nmod tests {";
        let rig_starts = src.find(marker).expect(
            "[CPE-1726] this guard finds the test rig by its exact `#[cfg(test)] / mod tests {` header \
             at column 0. It is missing, so the scan below has no boundary to test against and would \
             pass vacuously. Fix the needle to match the new header — never delete the guard.",
        );
        let mut leaked = Vec::new();
        for needle in CPE_1726_DESTRUCTIVE_CALLS {
            let mut from = 0;
            while let Some(hit) = src[from..].find(needle) {
                let at = from + hit;
                if at < rig_starts {
                    leaked.push(format!("  line {}: {needle}", src[..at].matches('\n').count() + 1));
                }
                from = at + needle.len();
            }
        }
        assert!(
            leaked.is_empty(),
            "[CPE-1726] a destructive `std::fs` call now exists in the SHIPPED half of cpe-sftp:\n{}\n\n\
             CPE-1726 left the rig's `rename` unguarded on one measured premise: this crate ships a \
             client and its SFTP *server* is a `#[cfg(test)]` test double, so no user's files are ever \
             at a destination it is handed. The call(s) above are outside that rig, so the premise no \
             longer holds for them and the decision must be re-taken, not inherited:\n\
             - renaming onto a slot a user or a remote named → `cpe_server::fsutil::rename_into_slot`;\n\
             - editing a file that may be a link → `cpe_server::fsutil::replace_file_contents`;\n\
             - claiming a new name → `cpe_server::fsutil::stage_exclusive`.\n\
             Moving the line back inside the rig is also a fix. Deleting this assertion is not.",
            leaked.join("\n")
        );
    }

    /// CPE-1726 acceptance: what actually happens when a **symlink** sits at the destination of the
    /// rig's SFTP `rename`. Both legs assert on the slot and on the victim's bytes and **never on the
    /// returned `Result`** — every bug in this family (CPE-1710/1716/1719) returned `Ok` while
    /// destroying something, so the return value is the one witness that has never been reliable.
    ///
    /// The property being pinned is the one that separates `rename` from `write`: **`fs::rename` does
    /// not follow the final component**, so it replaces the link and leaves the link's target alone,
    /// whereas the `open`+`write` path's `OpenOptions::create(true)` at the same slot would truncate
    /// *through* it and clobber the target. That is the whole reason the two need different fixes, and
    /// it is asserted rather than trusted.
    ///
    /// # Platform staging — and the CPE-1716 claim this deliberately does *not* repeat
    /// An earlier draft of this comment said a live file symlink "cannot be staged on an unprivileged
    /// Windows runner at all". **That is too broad, and `main` already carries the corrected form** (see
    /// `src-tauri/src/lib.rs`, from PR #899's review) — writing the broad version here would have
    /// regressed a correction back into the tree. What is true is narrower and mechanism-specific:
    /// `std::os::windows::fs::symlink_file` passes `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE` and
    /// succeeds unelevated, while PowerShell's `New-Item -ItemType SymbolicLink` does **not** pass that
    /// flag and fails with "Administrator privilege required". A check run through PowerShell therefore
    /// says nothing about what Rust can stage. (A junction really is directory-only and a hard link
    /// really is `is_symlink() == false`; neither substitutes for a live *file* link. That part stands —
    /// it is the "so Windows cannot do this at all" conclusion that does not follow.)
    ///
    /// Measured on this ticket's own CI run (`31800562558`, job `94767293360`, windows-latest): all three
    /// copies of this test recorded `ok` with **zero** `[CPE-1726] SKIPPED` lines, against a control in
    /// the same job — `[CPE-1692] SKIPPED sftp opendir…` — proving the absence means "did not skip"
    /// rather than "notices do not reach this log".
    ///
    /// So `supported_here` is **`true`, not `cfg!(unix)`**. Under `cfg!(unix)` a Windows runner that
    /// later lost the capability would turn leg 1 into a *silent skip* instead of a red — precisely the
    /// CPE-1717 failure this family exists to stop, and a real regression risk given the capability is
    /// demonstrably present today. It costs a contributor nothing: `staging_is_strict()` follows `$CI`,
    /// so an unusual local box still gets the loud skip. Leg 2 runs everywhere regardless, via
    /// `make_dangling_link`'s privilege-free junction fallback.
    #[test]
    fn cpe_1726_rename_onto_a_link_never_writes_through_it() {
        let (addr, hostkey, root) = spawn_server_returning_root();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let mut provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();

        // ── Leg 1: a LIVE link at the destination, pointing at a victim with known bytes.
        let victim = root.join("victim.txt");
        std::fs::write(&victim, b"victim bytes").unwrap();
        let slot = root.join("slot.txt");
        #[cfg(windows)]
        let staged = std::os::windows::fs::symlink_file(&victim, &slot).is_ok();
        #[cfg(unix)]
        let staged = std::os::unix::fs::symlink(&victim, &slot).is_ok();
        if cpe_server::fsutil::require_staged("live_file_symlink", true, staged) {
            provider.write("/live-src.txt", b"source bytes").expect("seed the rename source");
            let r = provider.rename("/live-src.txt", "/slot.txt");
            assert_eq!(
                std::fs::read(&victim).unwrap(),
                b"victim bytes",
                "the link's TARGET must be untouched — `fs::rename` does not follow the final \
                 component, so a write-through here would mean the rig had stopped using `rename` \
                 (rename reported {r:?})"
            );
            assert!(
                !std::fs::symlink_metadata(&slot).unwrap().file_type().is_symlink(),
                "and the link itself must be GONE, replaced by the moved file: that is the silent \
                 destruction CPE-1726 weighed and deliberately accepted for a `#[cfg(test)]` rig \
                 (rename reported {r:?})"
            );
            assert_eq!(std::fs::read(&slot).unwrap(), b"source bytes", "rename reported {r:?}");
        } else {
            cpe_server::skip_notice!(
                "[CPE-1726] SKIPPED the LIVE-link leg of cpe-sftp's rename test: could not create a \
                 file symlink at {}. Rust's `symlink_file` passes ALLOW_UNPRIVILEGED_CREATE and \
                 normally succeeds unelevated on Windows too, so this is an unusual environment rather \
                 than the ordinary Windows case — under CI this is a hard red, not this notice. What \
                 is NOT covered on this run is leg 1's assertions specifically: the live victim's \
                 bytes and the slot's final contents. The DANGLING leg below still runs and still \
                 covers the write-through property.",
                slot.display()
            );
            let _ = std::fs::remove_file(&slot);
        }

        // ── Leg 2: a DANGLING link. Runs on every platform (junction fallback), and it is the leg that
        // proves the write-through property without needing a live target: if the rig ever wrote
        // *through* the link instead of replacing it, the link's non-existent target would spring into
        // existence. It never may.
        let dangling = root.join("dangling.txt");
        if cpe_server::fsutil::make_dangling_link(&dangling) {
            let never = root.join("dangling.txt-target-that-does-not-exist");
            provider.write("/dangling-src.txt", b"dangling source").expect("seed the rename source");
            let r = provider.rename("/dangling-src.txt", "/dangling.txt");
            assert!(
                !matches!(never.try_exists(), Ok(true)),
                "the dangling link's target must NEVER be created: it existing means the rig wrote \
                 THROUGH the link (the CPE-1719 shape) instead of replacing it (rename reported {r:?})"
            );
            // Outcome consistency, so a rig that reports success without moving anything is red rather
            // than green. Not an assertion *on* the `Result` — it is an assertion on the slot, selected
            // by what the rig claimed.
            let link_now = std::fs::symlink_metadata(&dangling).map(|m| m.file_type().is_symlink());
            if r.is_ok() {
                assert_eq!(
                    std::fs::read(&dangling).ok().as_deref(),
                    Some(&b"dangling source"[..]),
                    "rename reported success, so the slot must now hold the moved file's bytes; it \
                     holds something else (is_symlink = {link_now:?})"
                );
            } else {
                assert_eq!(
                    link_now.ok(),
                    Some(true),
                    "rename reported failure ({r:?}), so it must have left the link alone — a failed \
                     rename that still destroyed the destination is the worst of both outcomes"
                );
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1731 — the destination that resolves to the served root, and the empty-only verb
    // ---------------------------------------------------------------------------------------------

    /// CPE-1731 acceptance: a `rename` whose destination **resolves to the served root** is refused.
    ///
    /// The defect this replaces answered `Ok(())` to `rename("/", "")` and `rename("/", ".")` — the
    /// WebDAV defect CPE-1726 had just fixed, in the crate that PR declared *structurally immune* to it
    /// on the grounds that SFTP takes both paths from one resolver in one message. True of the source;
    /// false of the destination, since [`FsSftp::real`] maps empty, `"."` and `"/"` all to the root.
    ///
    /// # What the rows are, and what they are not
    /// **Regression pins, not a specification.** CPE-1726 shipped a table like this three times and the
    /// UAT falsified each one on the code that carried it; the fix is the resolved comparison in
    /// `Handler::rename`, and these record which spellings have actually been observed escaping a
    /// previous round. Families 1 and 2 are here as the cheapest available evidence that the comparison
    /// closes *families* rather than members; family 3 (spellings only the filesystem calls equal) is
    /// Windows-only and lives in the test below.
    ///
    /// # The source is a column, not a function of the expected outcome
    /// Each row carries its own source path. CPE-1726's equivalent table first derived the source from
    /// the row's expected status, which worked only by coincidence — a proxy standing in for a
    /// property, in the one file whose subject is that substitution. Here the root-resolution rows
    /// rename **`/`** because that is the shape the bug was reported in, and it is the only source that
    /// reproduces it: with the guard deleted and a *file* source, `fs::rename` fails anyway (a file
    /// cannot replace the populated root) — a red produced by an errno rather than by the defect. From
    /// `/` it returns `Ok(())` for a rename that did nothing, which is the defect verbatim. Being saved
    /// by an errno is not the same as being guarded.
    ///
    /// # The last row is a positive control, and it is load-bearing
    /// Without it, a client that silently failed to drive the rig would satisfy every refusal row (the
    /// tree stays intact when nothing happens). The control asserts `readme.txt`'s **bytes** arrive at
    /// the new name — so the refusals are measured against a session that demonstrably can rename.
    ///
    /// # What the filesystem assertions can and cannot catch here — stated rather than implied
    /// They **cannot fail today**, and pretending otherwise would be the vacuous-assertion trap this
    /// sprint keeps finding. A destination that resolves to the root cannot destroy anything: renaming
    /// the root onto itself is a no-op, and renaming a file onto the populated root fails. So the
    /// observable defect is the *reply* — a success reported for a rename that never happened, which a
    /// client will believe and then delete its source. The tree assertions are kept as the cheap thing
    /// that goes red if a future change ever makes this shape destructive, not as this test's evidence.
    #[test]
    fn cpe_1731_a_rename_whose_destination_resolves_to_the_served_root_is_refused() {
        // (source path, destination as sent on the wire, must it be refused?, why)
        let cases: &[(&str, &str, bool, &str)] = &[
            ("/", "", true, "an empty destination — the shape the ticket was filed on"),
            ("/", ".", true, "a bare `.` — `real` maps it to the root explicitly"),
            ("/", "/", true, "a bare `/` — trims to empty"),
            ("/", "//", true, "two slashes — survived CPE-1726's round-2 pre-trim filter"),
            ("/", "///", true, "three slashes — same evasion"),
            // Family 1 — the spellings a literal denylist of `""` and `"."` let through.
            ("/", "/.", true, "`/.` — trims to `.`"),
            ("/", "/./", true, "`/./` — trims to `./`, neither denied literal"),
            ("/", "/.//", true, "`/.//` — a CurDir component then an empty one"),
            ("/", "//./", true, "`//./` — leading empty component before the dot"),
            ("/", "/./.", true, "`/./.` — two CurDir components, no trailing slash"),
            ("/", "//.", true, "`//.` — slashes then a dot"),
            // Family 2 — `..` landing ON the root rather than escaping it.
            ("/", "/nonexistent/..", true, "`..` popping a name that never existed"),
            ("/", "/sub/..", true, "`..` popping a real subdirectory"),
            ("/", "/./sub/../.", true, "`..` and `.` mixed, still the root"),
            // The positive control (see the doc above).
            ("/readme.txt", "/renamed.txt", false, "an ordinary destination must still be renamed"),
        ];

        for (source, dest, refuse, why) in cases {
            let (addr, hostkey, root) = spawn_server_returning_root();
            let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
            let mut provider =
                SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict)
                    .expect("connect");

            let r = provider.rename(source, dest);

            if *refuse {
                assert_eq!(
                    std::fs::read(root.join(FILE_NAME)).ok().as_deref(),
                    Some(FILE_BODY),
                    "[{why}] the served tree must be intact after a refusal (rename reported {r:?})"
                );
                assert!(
                    root.join(DIR_NAME).join("nested.txt").is_file(),
                    "[{why}] and the rest of the served tree must be intact (rename reported {r:?})"
                );
                // `is_err()` alone is **not enough here, and the Linux measurement is why.** With the
                // `..` pop neutralised, `/nonexistent/..` stops being caught by the guard and is
                // stopped instead by `fs::rename`'s `ENOENT` — which `io_err` maps to `NoSuchFile`,
                // still an `Err`, so a bare `is_err()` would stay green through a broken guard on
                // exactly the row that guard exists for. Being saved by an errno is not the same as
                // being guarded, and this is where the two are told apart.
                //
                // The two strings are **measured, not guessed** — `SftpProvider::rename` formats
                // `"{from} -> {to}: {e}"` around russh-sftp's `Display`, which yields
                // `"/ -> : Failure: Failure"` for the refusal and
                // `"…: No such file: No such file"` for an `ENOENT`. If a dependency bump changes that
                // wording this assertion goes red with the strings in the message, which is the right
                // way for it to break.
                let msg = r.as_ref().err().cloned().unwrap_or_default();
                assert!(
                    r.is_err() && msg.contains("Failure") && !msg.contains("No such file"),
                    "[{why}] a destination that resolves to the served root is a refusal \
                     (SSH_FX_FAILURE), not a rename the server reports as done and not an incidental \
                     ENOENT. Got {r:?}"
                );
            } else {
                assert_eq!(
                    std::fs::read(root.join("renamed.txt")).ok().as_deref(),
                    Some(FILE_BODY),
                    "[{why}] the control row must actually move the file's bytes, or every refusal \
                     row above is measured against a rig that renames nothing (rename reported {r:?})"
                );
                assert!(
                    !root.join(FILE_NAME).exists(),
                    "[{why}] and the source name must be gone (rename reported {r:?})"
                );
            }
        }
    }

    /// Family 3: the served root spelled in a way **only the filesystem** knows is the same place.
    ///
    /// Windows matches names case-insensitively and strips trailing dots, while `PathBuf` equality
    /// compares `Component::Normal` byte-wise — so this is the row `normalise_lexically` alone cannot
    /// answer, and the reason `fsutil::same_place` consults `canonicalize`. Removing that half turns
    /// exactly this test red and leaves the table above green (measured; see the PR body).
    ///
    /// **Windows-only, and measured rather than assumed.** [`FsSftp::real`] trims the leading `/` before
    /// joining, so on Linux an absolute destination becomes a *relative* one and lands inside the root
    /// (`/tmp/<root>` resolved to `<root>/tmp/<root>`, `same_place = false` — measured under WSL). On a
    /// case-sensitive filesystem these spellings are genuinely different places anyway. Two independent
    /// reasons there is nothing to catch there; on Windows a `C:\…` destination survives the trim and
    /// `Path::join` discards the base, so it arrives as the spelling itself.
    #[cfg(windows)]
    #[test]
    fn cpe_1731_a_rename_naming_the_root_by_another_spelling_is_refused() {
        for (spell, why) in [
            ("upper-case", "Windows matches names case-insensitively"),
            ("trailing dot", "Windows strips a trailing dot during path processing"),
        ] {
            let (addr, hostkey, root) = spawn_server_returning_root();
            let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
            let mut provider =
                SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict)
                    .expect("connect");
            let literal = root.to_string_lossy().to_string();
            let dest = if spell == "upper-case" { literal.to_uppercase() } else { format!("{literal}.") };

            // Source `/` for the same reason the table above uses it: it is the source that reproduces
            // the defect (an `Ok(())` for a rename that did nothing) rather than one an errno stops.
            let r = provider.rename("/", &dest);

            assert_eq!(
                std::fs::read(root.join(FILE_NAME)).ok().as_deref(),
                Some(FILE_BODY),
                "[{spell}] the served tree must survive a rename onto a different spelling of the root \
                 ({why}); rename reported {r:?}"
            );
            let msg = r.as_ref().err().cloned().unwrap_or_default();
            assert!(
                r.is_err() && msg.contains("Failure") && !msg.contains("No such file"),
                "[{spell}] {why}, so this destination IS the served root and must be refused \
                 (SSH_FX_FAILURE, not an incidental ENOENT — see the table test's note). Byte-wise \
                 path equality does not know that, which is why the check consults the filesystem. \
                 Got {r:?}"
            );
        }
    }

    /// CPE-1731 acceptance: `SSH_FXP_RMDIR` is the **empty-directory** verb, so a non-empty directory
    /// is refused and its contents survive.
    ///
    /// The rig implemented it with `remove_dir_all`, which deleted the tree and answered `Ok(())` —
    /// behaviour no real server has, and the mirror image of CPE-1726's thesis (which had WebDAV as the
    /// crate that got its verb semantics wrong; WebDAV's `DELETE` is correctly recursive).
    ///
    /// **Asserted on the filesystem, and on the exact file the seeder created** — not on the returned
    /// `Result` and not on a bare `!exists()`. A negative assertion guarded by a filename typed twice
    /// passes vacuously the moment the two drift, so `nested.txt`'s **bytes** are what is checked, and
    /// the positive control proves `rmdir` still works on the case it is defined for.
    ///
    /// `SftpProvider::delete` tries `remove_file` first and falls back to `remove_dir`, so this drives
    /// the same path a user deleting a populated remote folder would.
    #[test]
    fn cpe_1731_rmdir_refuses_a_non_empty_directory_and_leaves_it_intact() {
        let (addr, hostkey, root) = spawn_server_returning_root();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let mut provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict)
                .expect("connect");

        let r = provider.delete(&format!("/{DIR_NAME}"));
        assert!(
            root.join(DIR_NAME).is_dir(),
            "the non-empty directory itself must survive an empty-only verb (delete reported {r:?})"
        );
        assert_eq!(
            std::fs::read(root.join(DIR_NAME).join("nested.txt")).ok().as_deref(),
            Some(&b"deep"[..]),
            "and its contents must still be there, byte for byte — `rmdir` is not a recursive delete \
             (delete reported {r:?})"
        );
        assert!(r.is_err(), "a refusal the client reads as success is the CPE-1726 failure shape");

        // Positive control: `rmdir` on the directory it IS defined for still works, so the assertions
        // above are not measuring a verb that simply stopped functioning.
        provider.mkdir("/emptydir").expect("mkdir");
        provider.delete("/emptydir").expect("rmdir must still remove an EMPTY directory");
        assert!(!root.join("emptydir").exists(), "the empty directory must actually be gone");
    }

    #[test]
    fn a_changed_host_key_is_refused() {
        let (addr, _hostkey) = spawn_server();
        // Same host+type, DIFFERENT key material → Changed → connection must be refused.
        let wrong = ("ssh-ed25519".to_string(), "AAAAthisisnottherealkey".to_string());
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let err = match SftpProvider::connect(&cfg, known_for(addr.port(), &wrong), HostKeyPolicy::Strict) {
            Ok(_) => panic!("a changed host key must be refused"),
            Err(e) => e,
        };
        assert!(err.contains("CHANGED"), "expected a MITM/changed-key error, got: {err}");
    }

    #[test]
    fn an_unknown_host_is_refused_under_strict() {
        // No known_hosts entry → Unknown → Strict refuses at the handshake (before any SFTP op).
        let (addr, _hostkey) = spawn_server();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let err = match SftpProvider::connect(&cfg, vec![], HostKeyPolicy::Strict) {
            Ok(_) => panic!("an unknown host must be refused under Strict"),
            Err(e) => e,
        };
        assert!(err.contains("unknown host key"), "got: {err}");
    }

    /// A fresh OpenSSH Ed25519 keypair: (public key, private-key PEM string).
    fn client_keypair() -> (ssh_key::PublicKey, String) {
        let key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        let pem = key.to_openssh(ssh_key::LineEnding::LF).unwrap().to_string();
        (key.public_key().clone(), pem)
    }

    #[test]
    fn authenticates_with_an_ssh_key_then_lists() {
        // The server accepts only this client public key; the provider auths with the matching private key.
        let (client_pub, pem) = client_keypair();
        let (addr, hostkey) = spawn_server_with(Some(client_pub));
        let cfg = SftpConfig::key("127.0.0.1", addr.port(), "user", pem, None);
        let provider = SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict)
            .expect("key auth should succeed");
        assert_eq!(provider.host_key_verdict(), HostKeyVerdict::Trusted);
        assert_eq!(provider.list("/").expect("list").len(), 2);
    }

    #[test]
    fn a_wrong_ssh_key_is_rejected() {
        // Server accepts one key; the provider offers a different one → auth fails (after the host key,
        // which is still Trusted, was verified).
        let (accepted_pub, _accepted_pem) = client_keypair();
        let (_wrong_pub, wrong_pem) = client_keypair();
        let (addr, hostkey) = spawn_server_with(Some(accepted_pub));
        let cfg = SftpConfig::key("127.0.0.1", addr.port(), "user", wrong_pem, None);
        let err = match SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict) {
            Ok(_) => panic!("a wrong key must be rejected"),
            Err(e) => e,
        };
        assert!(err.contains("authentication failed"), "got: {err}");
    }

    #[test]
    fn connect_location_bridges_an_sftp_url_to_a_provider() {
        let (addr, hostkey) = spawn_server();
        let url = format!("sftp://user@127.0.0.1:{}/", addr.port());
        let loc = cpe_server::location::parse(&url);
        let provider = connect_location(
            &loc,
            SftpAuth::Password("pw".into()),
            known_for(addr.port(), &hostkey),
            HostKeyPolicy::Strict,
        )
        .expect("connect_location should succeed for a valid sftp URL");
        assert_eq!(provider.list("/").expect("list").len(), 2);
    }

    #[test]
    fn connect_location_rejects_a_non_sftp_or_userless_location() {
        let err_of = |loc: &cpe_server::location::Location| match connect_location(
            loc,
            SftpAuth::Password("p".into()),
            vec![],
            HostKeyPolicy::Tofu,
        ) {
            Ok(_) => panic!("expected an error"),
            Err(e) => e,
        };
        // A local path is not an SFTP location.
        assert!(err_of(&cpe_server::location::parse("/home/x")).contains("not an SFTP location"));
        // An sftp URL with no user is refused (before any connection).
        assert!(err_of(&cpe_server::location::parse("sftp://host.example.com/path")).contains("no user"));
    }

    #[test]
    fn an_invalid_private_key_is_a_clear_error() {
        let (addr, hostkey) = spawn_server_with(Some(client_keypair().0));
        let cfg = SftpConfig::key("127.0.0.1", addr.port(), "user", "not a real key", None);
        let err = match SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict) {
            Ok(_) => panic!("an invalid key must error"),
            Err(e) => e,
        };
        assert!(err.contains("invalid private key"), "got: {err}");
    }

    /// CPE-1461 source-side defense: `list` must drop a READDIR entry whose server-supplied filename is a
    /// traversal/separator/drive name (via `cpe_server::transfer::is_safe_name`), so it can never reach
    /// the local-write sink in `download_tree`. Gated to Unix: a filename containing a literal backslash
    /// isn't creatable on Windows, and the filter itself is platform-independent.
    #[cfg(unix)]
    #[test]
    fn list_filters_out_a_path_traversal_readdir_name() {
        let (addr, hostkey, root) = spawn_server_returning_root();
        // Seed a hostile-named regular file directly on the server root (a filename russh-sftp would
        // forward verbatim). A backslash-bearing name is `is_safe_name`-unsafe and Unix-creatable.
        std::fs::write(root.join(r"evil\..\..\escape"), b"pwn").unwrap();

        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();
        let names: Vec<String> = provider.list("/").expect("list").into_iter().map(|e| e.name).collect();

        assert!(!names.iter().any(|n| n.contains('\\')), "hostile backslash name leaked through: {names:?}");
        // The legitimate entries still come through (no over-rejection).
        assert!(names.contains(&FILE_NAME.to_string()), "legit file was dropped: {names:?}");
        assert!(names.contains(&DIR_NAME.to_string()), "legit dir was dropped: {names:?}");
    }

    /// The deterministic half of the CPE-1692 guard for this fixture (runs on every OS/account, no
    /// privilege needed) — same role as `cpe_server::dispatch::classify_path_error`'s own unit tests.
    /// `stat`/`readdir` already routed every stat failure through `io_err`; `opendir`/`open` were the two
    /// call sites that didn't (they used `!is_dir()`/`!exists()` instead, folding a permission-denied or
    /// otherwise-unstattable entry into the same `NoSuchFile` a genuine absence gets). This pins the
    /// taxonomy `io_err` itself already encoded, which the wiring fix below now reaches from all four
    /// handlers uniformly.
    #[test]
    fn io_err_maps_kinds_to_distinct_status_codes() {
        assert_eq!(io_err(std::io::Error::from(std::io::ErrorKind::NotFound)), StatusCode::NoSuchFile);
        assert_eq!(
            io_err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            StatusCode::PermissionDenied
        );
        for kind in [std::io::ErrorKind::Other, std::io::ErrorKind::TimedOut, std::io::ErrorKind::BrokenPipe] {
            assert_eq!(
                io_err(std::io::Error::from(kind)),
                StatusCode::Failure,
                "{kind:?} must not be reported as NoSuchFile (an absence) or as an unrelated code"
            );
        }
    }

    /// A second, fully deterministic (no privilege needed) half of the `opendir` guard specifically:
    /// `!is_dir()` folded a SUCCESSFUL stat of the WRONG type (a real file, not a directory) into the
    /// same `NoSuchFile` a genuine absence gets — unlike `open`'s `!exists()`, which doesn't care about
    /// type and so never had this particular confusion, `opendir`'s old check conflated "gone" with
    /// "present but not a directory". This needs no OS permission trick (a file that just isn't a
    /// directory is constructible everywhere, unprivileged), so it runs for real on every OS/account
    /// and can be broken-and-confirmed-red without depending on the platform-limited leg below.
    #[test]
    fn list_on_a_file_path_is_not_reported_as_missing() {
        let (addr, hostkey, root) = spawn_server_returning_root();
        std::fs::write(root.join("plain.txt"), b"just a file").unwrap();

        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();

        let err = provider.list("/plain.txt").unwrap_err();
        assert!(
            !err.to_lowercase().contains("no such file"),
            "a real file (wrong type for opendir, not an absence) must not be reported as missing: {err}"
        );
    }

    /// The end-to-end half of the `opendir` guard, driving the real wire protocol —
    /// `provider.list`, the production entry point a Tauri command ultimately calls — rather than
    /// calling the fixture's handler methods directly (Evidence Rules: verify through the channel that
    /// will actually carry the message). `opendir` calls `std::fs::metadata`, so the permission
    /// condition needs a PARENT-directory traversal deny (mirrors `cpe_server::fsutil::deny_dir_traversal`
    /// by hand, since this crate doesn't depend on that test-only helper) — genuinely Unix-only, per the
    /// PR #874 review's measurement on `deny_dir_traversal`'s doc comment; this leg only runs for real on
    /// Unix (non-root). `open`'s own guard is separate, below, and uses a different mechanism because it
    /// calls `try_exists`, not `metadata`.
    #[test]
    fn opendir_over_a_permission_denied_directory_is_not_reported_as_missing_over_the_wire() {
        let (addr, hostkey, root) = spawn_server_returning_root();

        let gp = root.join("gp");
        std::fs::create_dir_all(gp.join("real_dir")).unwrap();
        std::fs::write(gp.join("real_dir").join("inner.txt"), b"secret").unwrap();

        // Armed before the deny so cleanup runs on every exit path — mirrors split_join.rs's `Restore`
        // pattern (Evidence Rules: a red run must never leave debris).
        struct Restore(PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                #[cfg(windows)]
                {
                    if let Ok(user) = std::env::var("USERNAME") {
                        let _ = std::process::Command::new("icacls")
                            .arg(&self.0)
                            .arg("/remove:d")
                            .arg(&user)
                            .output();
                    }
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
                }
            }
        }
        let _restore = Restore(gp.clone());

        #[cfg(windows)]
        {
            if let Ok(user) = std::env::var("USERNAME") {
                if !user.is_empty() {
                    let _ = std::process::Command::new("icacls")
                        .arg(&gp)
                        .arg("/deny")
                        .arg(format!("{user}:(RX)"))
                        .output();
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&gp, std::fs::Permissions::from_mode(0o000)).unwrap();
        }

        // CPE-1717: `supported_here = cfg!(unix)` — this is the traversal-deny mechanism, which
        // `cpe_server::fsutil::deny_dir_traversal` documents as genuinely Unix-only, so Windows keeps
        // its notice-only skip while a Unix runner that stops honouring mode bits goes red under CI.
        let denied = cpe_server::fsutil::require_staged(
            "sftp opendir traversal deny",
            cfg!(unix),
            std::fs::metadata(gp.join("real_dir"))
                .is_err_and(|e| e.kind() != std::io::ErrorKind::NotFound),
        );
        if !denied {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1692] SKIPPED sftp opendir permission-denied leg: could not deny traversal on {} \
                 on this machine (elevated/root, or a filesystem ignoring ACLs/mode bits, or simply \
                 Windows — this mechanism is genuinely Unix-only). The remaining assertions do NOT cover \
                 CPE-1692 for crates/sftp opendir.",
                gp.display()
            );
            return;
        }

        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();

        // opendir path: `list` -> `read_dir` -> SSH_FXP_OPENDIR under the hood.
        let dir_err = provider.list("/gp/real_dir").unwrap_err();
        assert!(
            !dir_err.to_lowercase().contains("no such file"),
            "a permission-denied directory must not be reported as missing over the wire: {dir_err}"
        );
        // `_restore` cleans up on the way out, panic or not.
    }

    /// The end-to-end half of the `open` guard — the ticket's stated highest-visibility site
    /// (`StatusCode::NoSuchFile` traveling over the real wire — though see the PR body for the
    /// correction that this fixture is a test-only stand-in, not a real shipped SFTP server). `open`
    /// (non-`CREATE`) calls `real.try_exists()`, not `metadata`, so this needs `deny_stat_of`'s mechanism
    /// (mirrored here by hand): a deny placed directly ON the target itself on Windows (measured, PR
    /// #874 review, to be refused by `try_exists` even though `fs::metadata` on the same target still
    /// succeeds — see `cpe_server::fsutil::deny_stat_of`'s doc comment), on the target's PARENT on Unix.
    /// Runs for REAL on both platforms now — this is also the site F3 of that review flagged as having
    /// no wiring-level guard-neutralisation evidence at all; see the PR body's mutation table.
    #[test]
    fn open_over_a_permission_denied_file_is_not_reported_as_missing_over_the_wire() {
        let (addr, hostkey, root) = spawn_server_returning_root();

        let gp = root.join("gp");
        std::fs::create_dir_all(&gp).unwrap();
        let real_file = gp.join("real_file.txt");
        std::fs::write(&real_file, b"secret file").unwrap();

        // Armed before the deny so cleanup runs on every exit path — mirrors split_join.rs's `Restore`
        // pattern (Evidence Rules: a red run must never leave debris).
        struct Restore { target: PathBuf, parent: PathBuf }
        impl Drop for Restore {
            fn drop(&mut self) {
                #[cfg(windows)]
                {
                    let _ = &self.parent; // Windows denies `target` itself; `parent` untouched there.
                    if let Ok(user) = std::env::var("USERNAME") {
                        let _ = std::process::Command::new("icacls")
                            .arg(&self.target)
                            .arg("/remove:d")
                            .arg(&user)
                            .output();
                    }
                }
                #[cfg(unix)]
                {
                    let _ = &self.target; // Unix denies `parent`; `target` itself untouched there.
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&self.parent, std::fs::Permissions::from_mode(0o700));
                }
            }
        }
        let _restore = Restore { target: real_file.clone(), parent: gp.clone() };

        #[cfg(windows)]
        {
            if let Ok(user) = std::env::var("USERNAME") {
                if !user.is_empty() {
                    let _ = std::process::Command::new("icacls")
                        .arg(&real_file)
                        .arg("/deny")
                        .arg(format!("{user}:(F)"))
                        .output();
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&gp, std::fs::Permissions::from_mode(0o000)).unwrap();
        }

        // CPE-1717: `supported_here = true` — the target deny is `(F)` on Windows and a parent `chmod`
        // on Unix, both of which make `try_exists()` fail on every platform CI runs, so a failure here
        // means the runner changed and must be red rather than a notice in a green log.
        let denied = cpe_server::fsutil::require_staged(
            "sftp open target deny",
            true,
            real_file.try_exists().is_err(),
        );
        if !denied {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1692] SKIPPED sftp open permission-denied leg: could not deny stat of {} on this \
                 machine (elevated/root, or a filesystem ignoring ACLs/mode bits). The remaining \
                 assertions do NOT cover CPE-1692 for crates/sftp open.",
                real_file.display()
            );
            return;
        }

        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();

        // open path: `read` -> SSH_FXP_OPEN under the hood.
        let file_err = provider.read("/gp/real_file.txt").unwrap_err();
        assert!(
            !file_err.to_lowercase().contains("no such file"),
            "a permission-denied file must not be reported as missing over the wire: {file_err}"
        );
        // `_restore` cleans up on the way out, panic or not.
    }

    /// A second, SURGICAL half of the `open` guard, complementing the wire-level test above. Measured
    /// (PR #874 review follow-up): the wire-level test above CANNOT discriminate a wiring regression on
    /// its own — `provider.read` chains `open` with a SEPARATE, genuinely-real `std::fs::File::open` for
    /// the actual read data (see the fixture's `read` handler), and a target-level deny ACE blocks THAT
    /// real file-open too (Full Control deny refuses actual data access, not just the existence probe).
    /// So reverting `open`'s own check to `!real.exists()` (which never fails under this deny on
    /// Windows — F1) still ends up producing `PermissionDenied` over the wire, just via `read`'s
    /// independent real failure instead of `open`'s guard — the two code paths are indistinguishable
    /// from the client's observed message alone, which is exactly what made the wire-level test above
    /// pass vacuously against a reverted `open` when first tried. Calling `FsSftp::open` directly (not
    /// through the wire, and not followed by any read) isolates `open`'s OWN check: fixed code errors
    /// with `PermissionDenied` at `open` time; broken code returns `Ok(Handle)` (success) at `open` time,
    /// unambiguously.
    #[tokio::test]
    async fn open_handler_itself_reports_the_real_cause_for_a_permission_denied_file_not_missing() {
        use russh_sftp::server::Handler;

        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("cpe-sftp-openh-{}-{}", std::process::id(), n));
        let gp = root.join("gp");
        std::fs::create_dir_all(&gp).unwrap();
        let real_file = gp.join("real_file.txt");
        std::fs::write(&real_file, b"secret file").unwrap();

        struct Restore { target: PathBuf, parent: PathBuf, root: PathBuf }
        impl Drop for Restore {
            fn drop(&mut self) {
                #[cfg(windows)]
                {
                    let _ = &self.parent;
                    if let Ok(user) = std::env::var("USERNAME") {
                        let _ = std::process::Command::new("icacls")
                            .arg(&self.target)
                            .arg("/remove:d")
                            .arg(&user)
                            .output();
                    }
                }
                #[cfg(unix)]
                {
                    let _ = &self.target;
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&self.parent, std::fs::Permissions::from_mode(0o700));
                }
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }
        let _restore = Restore { target: real_file.clone(), parent: gp.clone(), root: root.clone() };

        #[cfg(windows)]
        {
            if let Ok(user) = std::env::var("USERNAME") {
                if !user.is_empty() {
                    let _ = std::process::Command::new("icacls")
                        .arg(&real_file)
                        .arg("/deny")
                        .arg(format!("{user}:(F)"))
                        .output();
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&gp, std::fs::Permissions::from_mode(0o000)).unwrap();
        }

        // CPE-1717: `supported_here = true`, as on the `open` leg above.
        let denied = cpe_server::fsutil::require_staged(
            "sftp open-handler target deny",
            true,
            real_file.try_exists().is_err(),
        );
        if !denied {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1692] SKIPPED sftp open-handler permission-denied leg: could not deny stat of {} \
                 on this machine (elevated/root, or a filesystem ignoring ACLs/mode bits). The remaining \
                 assertions do NOT cover CPE-1692 for crates/sftp open's own guard in isolation.",
                real_file.display()
            );
            return;
        }

        let mut fixture = FsSftp::new(root.clone());
        let result = fixture.open(1, "/gp/real_file.txt".to_string(), OpenFlags::READ, FileAttributes::default()).await;
        match result {
            Err(StatusCode::NoSuchFile) => {
                panic!("a permission-denied file must not be reported as missing (open's own guard)")
            }
            Ok(_) => panic!(
                "expected `open` to refuse a permission-denied target on its own — the guard was not \
                 exercised at all (this is the exact vacuous-pass shape a wiring regression produces)"
            ),
            Err(_) => {} // any other status (PermissionDenied here) correctly names the real cause
        }
        // `_restore` cleans up on the way out, panic or not.
    }

    /// F7 (PR #874 review): the honest case for BOTH `opendir` and `open`, pinned at the real wire
    /// protocol — a genuinely missing directory/file must still come back `NoSuchFile`, distinguishably,
    /// so the fix above doesn't accidentally make every stat failure look like "we don't know".
    #[test]
    fn opendir_and_open_on_a_genuinely_missing_path_still_say_no_such_file() {
        let (addr, hostkey, _root) = spawn_server_returning_root();
        let cfg = SftpConfig::password("127.0.0.1", addr.port(), "user", "pw");
        let provider =
            SftpProvider::connect(&cfg, known_for(addr.port(), &hostkey), HostKeyPolicy::Strict).unwrap();

        let dir_err = provider.list("/truly/does-not-exist").unwrap_err();
        assert!(
            dir_err.to_lowercase().contains("no such file"),
            "a real absence must still say so over the wire: {dir_err}"
        );

        let file_err = provider.read("/truly-missing.txt").unwrap_err();
        assert!(
            file_err.to_lowercase().contains("no such file"),
            "a real absence must still say so over the wire: {file_err}"
        );
    }
}
