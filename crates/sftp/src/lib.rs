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
}

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
    use std::sync::atomic::{AtomicU64, Ordering};
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
            if !self.real(&path).is_dir() {
                return Err(StatusCode::NoSuchFile);
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
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(pflags.contains(OpenFlags::TRUNCATE))
                    .open(&real)
                    .map_err(io_err)?;
            } else if !real.exists() {
                return Err(StatusCode::NoSuchFile);
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

        async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
            std::fs::remove_dir_all(self.real(&path)).map_err(io_err)?;
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

    /// A `known_hosts` list trusting `(key_type, key_b64)` at `127.0.0.1:port`.
    fn known_for(port: u16, key: &KeyFields) -> Vec<KnownHost> {
        parse_known_hosts(&format!("{} {} {}", host_token("127.0.0.1", port), key.0, key.1))
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
}
