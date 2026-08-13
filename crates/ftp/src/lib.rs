//! FTP/FTPS filesystem provider (CPE-1514, epic CPE-1502): a remote backend over FTP/FTPS that implements
//! [`cpe_server::provider::FileSystemProvider`], so the explorer can browse a plain FTP or explicit-FTPS
//! server by the same interface it uses for the local disk, SFTP, and WebDAV. This is the **first net-new
//! Network protocol** since the SFTP/WebDAV pair shipped under epic CPE-616 — it deliberately mirrors both
//! of them rather than inventing a new shape.
//!
//! Built on **`suppaftp`** (a maintained, actively-developed fork of the abandoned/vulnerable `ftp`
//! crate) with its `rustls-ring` feature — the same self-contained pure-Rust/`ring` crypto backend
//! `cpe-sftp` already chose for `russh`, so the remote-provider family stays consistent and no C build
//! tooling (NASM/cmake) is ever needed. Like `cpe-webdav`, the wire client is **synchronous** (a blocking
//! TCP control connection + per-transfer data connection), so no internal async runtime is needed here
//! either.
//!
//! # Auth: password and Anonymous
//! FTP's login is just `USER`/`PASS`, so [`FtpAuth`] covers both a normal `user`+`password` pair and
//! **Anonymous** login (RFC 1635): username `"anonymous"`, password an email-ish placeholder rather than
//! an empty string — a courtesy convention some picky servers still expect something plausible in that
//! field, even though essentially none of them validate it. Anonymous FTP is common enough for public
//! mirrors/archives that this is handled directly here rather than waiting on CPE-1501's broader
//! attended-auth-model epic; see [`ANONYMOUS_PASSWORD`].
//!
//! # Why `Mutex`, not `&mut self` state
//! Unlike WebDAV (stateless HTTP — every request is independent) FTP is a **stateful** protocol: one
//! control connection, commands issued and answered strictly in order. But
//! [`FileSystemProvider::list`]/`stat`/`read` take `&self`, matching the read side of the trait's other
//! implementors. [`FtpProvider`] resolves that with interior mutability — a `Mutex<RustlsFtpStream>` — the
//! same shape the app's own provider pool already wraps every provider in
//! (`Arc<Mutex<BoxedProvider>>`, see `cpe-vfs::connect::SharedProvider`), so this is not adding a new kind
//! of synchronization to the stack, just moving the lock one layer in.
//!
//! # Reads are streamed in fixed-size chunks
//! [`FtpProvider::read`] pulls the file over `retr_as_stream` and copies it in bounded (64 KiB) chunks
//! rather than one unbounded `read_to_end` call — never a single uncapped allocation driven directly by
//! server-controlled data. The [`FileSystemProvider::read`] contract still returns a fully-materialized
//! `Vec<u8>` (same as `cpe-sftp`/`cpe-webdav` — neither imposes a hard byte ceiling either), so a
//! whole-file cap is a broader, pre-existing trait-level question, not something specific to FTP; see the
//! [`READ_CHUNK_BYTES`] doc for the detailed rationale.
//!
//! # Traversal hardening
//! Every server-supplied `LIST` entry name is run through [`cpe_server::transfer::is_safe_name`] before it
//! becomes a [`cpe_server::provider::ProviderEntry`] (CPE-1461/1462, same defense `cpe-sftp`'s `READDIR`
//! filter and `cpe-webdav`'s PROPFIND `href` filter apply) — a hostile FTP server returning a `..` or
//! `/etc/passwd`-shaped name in its listing can never reach the local-write sink in
//! [`cpe_server::transfer::download_tree`].
//!
//! Testing runs against an in-process, hand-rolled FTP server (see the tests) — no Docker, no real FTP
//! daemon, so it runs identically on all three CI OSes.

use std::io::{Cursor, Read as _};
use std::sync::{Arc, Mutex};

use cpe_server::provider::{FileSystemProvider, ProviderEntry};
use suppaftp::{list::ListParser, rustls, types::FileType, FtpError, RustlsConnector, RustlsFtpStream};

/// How to authenticate to the FTP server.
#[derive(Debug, Clone)]
pub enum FtpAuth {
    /// A plaintext username + password (FTP has no other auth mechanism worth modeling yet).
    Password(String),
    /// Anonymous FTP (RFC 1635): the wire username is always `"anonymous"`, regardless of what
    /// [`FtpConfig::user`] holds; see the module docs and [`ANONYMOUS_PASSWORD`] for the password choice.
    Anonymous,
}

/// The placeholder "password" sent for [`FtpAuth::Anonymous`] logins — the RFC 1635-suggested shape (an
/// email-like string), not a real credential. Never logged (errors below never echo it). A dot-`invalid`
/// TLD is used deliberately so this can never resolve to (or be mistaken for) a real mailbox.
pub const ANONYMOUS_PASSWORD: &str = "anonymous@cross-platform-explorer.invalid";

/// How to connect to a remote FTP/FTPS host.
#[derive(Debug, Clone)]
pub struct FtpConfig {
    pub host: String,
    pub port: u16,
    /// The wire username for [`FtpAuth::Password`]; ignored (always `"anonymous"` on the wire) for
    /// [`FtpAuth::Anonymous`].
    pub user: String,
    pub auth: FtpAuth,
    /// Explicit FTPS: send `AUTH TLS` on the plaintext control channel and upgrade it in place before
    /// login (never the deprecated *implicit*-TLS mode, which listens on a separate, non-standard port).
    /// `false` is plain, unencrypted FTP. Port 21 is the default for both — explicit FTPS negotiates TLS
    /// on the same port plain FTP uses, unlike the legacy implicit-TLS port 990.
    pub tls: bool,
}

impl FtpConfig {
    /// A plain (or, with [`with_tls`](Self::with_tls), FTPS) connection with username + password auth.
    pub fn password(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self { host: host.into(), port, user: user.into(), auth: FtpAuth::Password(password.into()), tls: false }
    }

    /// A plain (or, with [`with_tls`](Self::with_tls), FTPS) anonymous connection.
    pub fn anonymous(host: impl Into<String>, port: u16) -> Self {
        Self { host: host.into(), port, user: "anonymous".to_string(), auth: FtpAuth::Anonymous, tls: false }
    }

    /// Enable explicit FTPS (`AUTH TLS`) for this config.
    pub fn with_tls(mut self) -> Self {
        self.tls = true;
        self
    }
}

/// A connected FTP/FTPS session presented as a synchronous [`FileSystemProvider`]. Dropping it closes the
/// control connection.
pub struct FtpProvider {
    // See the module docs ("Why `Mutex`, not `&mut self` state") for why interior mutability is needed
    // even though several trait methods only need `&self`.
    session: Mutex<RustlsFtpStream>,
}

/// Bytes copied per `read` iteration of the data-connection stream (module docs: "Reads are streamed in
/// fixed-size chunks"). 64 KiB is the same order of magnitude as a typical OS socket-buffer/page-cache
/// read size — large enough that the per-call overhead of many small reads doesn't dominate, small enough
/// that no single `Read::read` call can itself demand an outsized allocation regardless of how much data a
/// hostile or runaway server tries to push down the data connection in one go.
const READ_CHUNK_BYTES: usize = 64 * 1024;

impl FtpProvider {
    /// Connect, optionally upgrade to FTPS (`AUTH TLS`), and log in (password or Anonymous). Fails with a
    /// clear message at whichever step goes wrong — a refused TLS upgrade or a rejected login never leaves
    /// a half-open, unauthenticated session behind.
    pub fn connect(config: &FtpConfig) -> Result<Self, String> {
        let addr = format!("{}:{}", config.host, config.port);
        let stream =
            RustlsFtpStream::connect(&addr).map_err(|e| format!("ftp: connect to {addr}: {e}"))?;
        let mut stream = if config.tls {
            // suppaftp only pulls in a root-cert source as a *dev*-dependency (see its own README FTPS
            // example) — a real caller supplies its own. Mozilla's bundled roots (`webpki-roots`) match
            // what that example uses and need no OS trust-store access, so this behaves identically on
            // every CI OS and every user machine.
            let mut root_store =
                rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            // CI/test-only (CPE-1659, hardened by CPE-1673 with the `e2e-extra-ca` feature gate below):
            // trust one extra self-signed root IF (and only if) the harness set
            // `CPE_E2E_FTPS_EXTRA_CA_PEM_FILE` — see `extra_test_root`'s doc comment. A no-op for every
            // real app run — and on a normal (non-`e2e-extra-ca`) build, the whole hook is compiled out
            // (the `not(feature)` stub below does nothing), not merely inert.
            extra_test_root(&mut root_store);
            let tls_config =
                rustls::ClientConfig::builder().with_root_certificates(root_store).with_no_client_auth();
            stream
                .into_secure(RustlsConnector::from(Arc::new(tls_config)), &config.host)
                .map_err(|e| format!("ftps: TLS upgrade to {}: {e}", config.host))?
        } else {
            stream
        };
        let (user, pass): (&str, &str) = match &config.auth {
            FtpAuth::Password(p) => (config.user.as_str(), p.as_str()),
            FtpAuth::Anonymous => ("anonymous", ANONYMOUS_PASSWORD),
        };
        stream.login(user, pass).map_err(|e| format!("ftp: login: {e}"))?;
        // Explicit `TYPE I` (CPE-1659): `suppaftp` never sets a transfer type on its own — `retr_as_stream`
        // and `put_file` just issue RETR/STOR over whatever type the server currently has, and RFC 959's
        // default representation type at connection is ASCII, not binary. Kept explicit on principle (an
        // RFC-conformant ASCII-mode daemon could otherwise silently rewrite line endings through a binary
        // payload) even though CPE-1659's own negative-control experiment (forcing ASCII deliberately,
        // then reverting it here) found this particular vsftpd build/config does NOT actually translate
        // on the wire — a real, confirmed finding, not a guess: see the Work Log. The in-process fake FTP
        // server ignores TYPE entirely regardless (`"CWD" | "TYPE" | "OPTS" => 200 OK`), so neither mode
        // is visible to `cargo test -p cpe-ftp` either way.
        stream.transfer_type(FileType::Binary).map_err(|e| format!("ftp: TYPE I: {e}"))?;
        Ok(FtpProvider { session: Mutex::new(stream) })
    }
}

/// CI/test-only escape hatch (CPE-1659, gated behind the `e2e-extra-ca` Cargo feature since CPE-1673):
/// the env var naming a PEM file with ONE extra certificate to trust for FTPS, read by
/// [`extra_test_root`]. Used ONLY by the real-server E2E rig (`crates/vfs/tests/real_server_conformance.rs`)
/// to validate against a throwaway container certificate that no public CA could ever sign for a
/// private, ephemeral Docker IP — a real public CA (Let's Encrypt et al.) fundamentally cannot issue for
/// an address like that, so this is the only way to prove the FTPS handshake genuinely works against
/// vsftpd's real certificate through the SAME `cpe_vfs::open` seam the app itself uses, rather than
/// bypassing it with a second, test-only connect path. On a normal build (feature off — every real app
/// run, including the shipped binary) this whole hook doesn't exist in the compiled code at all, not
/// merely an unset env var: the production trust store is unconditionally
/// `webpki_roots::TLS_SERVER_ROOTS`. Never surfaced in the `Connection` model or any UI; this is not a
/// general "trust a custom CA" feature.
#[cfg(feature = "e2e-extra-ca")]
const EXTRA_TEST_CA_ENV: &str = "CPE_E2E_FTPS_EXTRA_CA_PEM_FILE";

/// Add the certificate named by [`EXTRA_TEST_CA_ENV`] (if the env var is set and the file parses) to
/// `store`. Any failure (var unset, file missing, bad PEM) is silently ignored — this must never turn
/// into its own confusing error; the *real* signal is whichever TLS/handshake error the caller already
/// surfaces when the certificate genuinely isn't trusted.
#[cfg(feature = "e2e-extra-ca")]
fn extra_test_root(store: &mut rustls::RootCertStore) {
    let Ok(path) = std::env::var(EXTRA_TEST_CA_ENV) else { return };
    let Ok(pem) = std::fs::read_to_string(&path) else { return };
    if let Some(der) = decode_pem_certificate(&pem) {
        let _ = store.add(rustls::pki_types::CertificateDer::from(der));
    }
}

/// No-op stub for a normal (non-`e2e-extra-ca`) build: the real hook above doesn't exist in this binary
/// at all, so the call site can stay unconditional (`&mut store` — hence still `mut` — without an
/// `unused_mut` warning either way).
#[cfg(not(feature = "e2e-extra-ca"))]
fn extra_test_root(_store: &mut rustls::RootCertStore) {}

/// Minimal single-certificate PEM -> DER decode: strip the `-----BEGIN/END CERTIFICATE-----` marker
/// lines and base64-decode the rest. Hand-rolled (no new dependency) since this is a narrow CI/test-only
/// need (see [`extra_test_root`]), not a general PEM-bundle parser.
#[cfg(feature = "e2e-extra-ca")]
fn decode_pem_certificate(pem: &str) -> Option<Vec<u8>> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    base64_decode_standard(&body)
}

/// Minimal standard-alphabet base64 decoder (RFC 4648, `=`-padded), hand-rolled to avoid a new
/// dependency for the one CI/test-only PEM decode above — not used anywhere else in this crate.
#[cfg(feature = "e2e-extra-ca")]
fn base64_decode_standard(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let clean: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.is_empty() || clean.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks_exact(4) {
        let mut vals = [0u8; 4];
        let mut pad = 0u8;
        for (i, &b) in chunk.iter().enumerate() {
            if b == b'=' {
                pad += 1;
            } else {
                vals[i] = ALPHABET.iter().position(|&a| a == b)? as u8;
            }
        }
        let n = ((vals[0] as u32) << 18) | ((vals[1] as u32) << 12) | ((vals[2] as u32) << 6) | (vals[3] as u32);
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Turn a failed `suppaftp` call into a legible, path-prefixed message.
fn ftp_err(path: &str, e: FtpError) -> String {
    format!("{path}: {e}")
}

/// Parse one raw `LIST` line into a [`suppaftp::list::File`], trying the common POSIX (`ls -l`) format
/// first, then the DOS format some Windows/IIS FTP servers use — the same two-format fallback suppaftp's
/// own docs recommend, since a bare `LIST` gives no reliable way to know which one a given server speaks.
/// An unparsable line (a server-specific "total N" summary row, a blank line, garbage) is skipped rather
/// than failing the whole listing — the same skip-on-error ethos `list_dir` and the sibling providers use.
fn parse_list_line(line: &str) -> Option<suppaftp::list::File> {
    ListParser::parse_posix(line).or_else(|_| ListParser::parse_dos(line)).ok()
}

/// Connect an [`FtpProvider`] from a parsed [`cpe_server::location::Location`] (must be `Ftp`) plus an auth
/// method — the bridge from a user-typed `ftp://[user@]host[:port]/path` (or `ftps://…`) to a live
/// provider. Port defaults to 21 for both plain FTP and explicit FTPS.
pub fn connect_location(
    loc: &cpe_server::location::Location,
    auth: FtpAuth,
    tls: bool,
) -> Result<FtpProvider, String> {
    use cpe_server::location::Scheme;
    if loc.scheme != Scheme::Ftp {
        return Err(format!("ftp: not an FTP location (scheme {:?})", loc.scheme));
    }
    let host = loc.host.as_deref().ok_or("ftp: location has no host")?;
    // Unlike SFTP, FTP allows a bare `ftp://host/path` (no `user@`) — the effective user then depends on
    // `auth` (Anonymous ignores it entirely, on the wire); default to "anonymous" so a userless URI paired
    // with `FtpAuth::Anonymous` is coherent even before the connection model fills in a real user.
    let user = loc.user.as_deref().unwrap_or("anonymous");
    let config =
        FtpConfig { host: host.to_string(), port: loc.port.unwrap_or(21), user: user.to_string(), auth, tls };
    FtpProvider::connect(&config)
}

impl FileSystemProvider for FtpProvider {
    fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
        let mut ftp = self.session.lock().unwrap();
        let lines = ftp.list(Some(path)).map_err(|e| ftp_err(path, e))?;
        Ok(lines
            .iter()
            .filter_map(|line| parse_list_line(line))
            .map(|f| (f.name().to_string(), f.is_directory(), f.size() as u64))
            // Source-side path-traversal defense (CPE-1461): the LIST filename is server-supplied and
            // some servers include literal `.`/`..` rows or (a hostile one) a name carrying a separator.
            // Drop anything that isn't a safe single leaf before it can ever reach the local-write sink in
            // `cpe_server::transfer::download_tree`.
            .filter(|(name, _, _)| cpe_server::transfer::is_safe_name(name))
            .map(|(name, is_dir, size)| ProviderEntry { name, is_dir, size: if is_dir { 0 } else { size } })
            .collect())
    }

    fn stat(&self, path: &str) -> Result<ProviderEntry, String> {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() {
            // The server root is always a directory; there is no listing to derive it from.
            return Ok(ProviderEntry { name: "/".to_string(), is_dir: true, size: 0 });
        }
        let name = trimmed.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or(trimmed).to_string();
        let parent = match trimmed.rfind('/') {
            Some(0) => "/",
            Some(i) => &trimmed[..i],
            None => "",
        };
        // FTP has no universal single-path STAT that every server supports (SIZE typically errors on a
        // directory, and MLST/MLSD aren't guaranteed present) — the portable way to learn is-dir + size is
        // to list the parent and find the matching leaf, same as `list` above (so it inherits the same
        // parsing + traversal filtering for free).
        self.list(parent)?.into_iter().find(|e| e.name == name).ok_or_else(|| format!("{path}: not found"))
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let mut ftp = self.session.lock().unwrap();
        let mut stream = ftp.retr_as_stream(path).map_err(|e| ftp_err(path, e))?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; READ_CHUNK_BYTES];
        loop {
            let n = stream.read(&mut chunk).map_err(|e| format!("{path}: {e}"))?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        ftp.finalize_retr_stream(stream).map_err(|e| ftp_err(path, e))?;
        Ok(buf)
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        let mut ftp = self.session.lock().unwrap();
        let mut reader = Cursor::new(data);
        ftp.put_file(path, &mut reader).map_err(|e| ftp_err(path, e))?;
        Ok(())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        self.session.lock().unwrap().mkdir(path).map_err(|e| ftp_err(path, e))?;
        Ok(())
    }

    fn delete(&mut self, path: &str) -> Result<(), String> {
        let mut ftp = self.session.lock().unwrap();
        // A path can be a file or a dir; try file removal first, then directory (mirrors cpe-sftp's
        // `delete`, since FTP also has separate file/dir removal commands — DELE vs RMD).
        match ftp.rm(path) {
            Ok(_) => Ok(()),
            Err(_) => ftp.rmdir(path).map_err(|e| ftp_err(path, e)).map(|_| ()),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.session.lock().unwrap().rename(from, to).map_err(|e| ftp_err(from, e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const FILE_NAME: &str = "readme.txt";
    const FILE_BODY: &[u8] = b"hello ftp"; // 9 bytes
    const DIR_NAME: &str = "sub";

    /// Credential policy the fake server enforces.
    #[derive(Clone)]
    enum Creds {
        Exact { user: &'static str, pass: &'static str },
        /// Real anonymous-FTP servers accept the `anonymous` user with *any* password (courtesy-only
        /// field) — mirrored here so the anonymous-login test exercises that shape.
        AnyPasswordForAnonymous,
    }

    /// Map a client-supplied, `/`-rooted FTP path onto the real filesystem path under `root`.
    fn real_path(root: &Path, ftp_path: &str) -> PathBuf {
        let rel = ftp_path.trim_start_matches('/');
        if rel.is_empty() {
            root.to_path_buf()
        } else {
            root.join(rel)
        }
    }

    /// One POSIX (`ls -l`) format LIST line, matching what `ListParser::parse_posix` expects.
    fn list_line(name: &str, is_dir: bool, size: u64) -> String {
        let perm = if is_dir { "drwxr-xr-x" } else { "-rw-r--r--" };
        format!("{perm} 1 user group {size:>10} Jan  1 00:00 {name}\r\n")
    }

    fn send(w: &mut TcpStream, s: &str) {
        let _ = w.write_all(s.as_bytes());
    }

    /// Handle one control connection to completion (until QUIT or the client disconnects), including any
    /// number of PASV-mode data-connection transfers it drives. A hand-rolled subset of RFC 959 — just
    /// enough of the control-channel state machine (USER/PASS/PWD/CWD/TYPE/PASV/LIST/RETR/STOR/DELE/RMD/
    /// MKD/RNFR/RNTO/SIZE/FEAT/QUIT) for `suppaftp`'s sync client to drive a full round-trip against, with
    /// no external FTP daemon or Docker — so this test runs identically on all three CI OSes.
    fn handle_control(mut ctrl: TcpStream, root: PathBuf, creds: Creds) {
        send(&mut ctrl, "220 cpe-ftp test server ready\r\n");
        let mut reader = BufReader::new(ctrl.try_clone().expect("clone control socket"));
        let mut user = String::new();
        let mut logged_in = false;
        let mut rename_from: Option<String> = None;
        let mut pasv: Option<TcpListener> = None;

        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break; // client closed the control connection
            }
            let line = line.trim_end_matches(['\r', '\n']);
            let (cmd, arg) = match line.split_once(' ') {
                Some((c, a)) => (c.to_ascii_uppercase(), a.to_string()),
                None => (line.to_ascii_uppercase(), String::new()),
            };
            match cmd.as_str() {
                "USER" => {
                    user = arg;
                    send(&mut ctrl, "331 Password required\r\n");
                }
                "PASS" => {
                    let ok = match &creds {
                        Creds::Exact { user: eu, pass: ep } => user == *eu && arg == *ep,
                        Creds::AnyPasswordForAnonymous => user == "anonymous",
                    };
                    logged_in = ok;
                    send(&mut ctrl, if ok { "230 Login successful\r\n" } else { "530 Login incorrect\r\n" });
                }
                "PWD" => send(&mut ctrl, "257 \"/\"\r\n"),
                "CWD" | "TYPE" | "OPTS" => send(&mut ctrl, "200 OK\r\n"),
                "FEAT" => send(&mut ctrl, "211 no features\r\n"),
                "PASV" => {
                    let listener = TcpListener::bind("127.0.0.1:0").expect("bind data listener");
                    let p = listener.local_addr().unwrap().port();
                    pasv = Some(listener);
                    send(&mut ctrl, &format!("227 Entering Passive Mode (127,0,0,1,{},{})\r\n", p >> 8, p & 0xFF));
                }
                "LIST" => {
                    if !logged_in {
                        send(&mut ctrl, "530 Not logged in\r\n");
                        continue;
                    }
                    let Some(listener) = pasv.take() else {
                        send(&mut ctrl, "425 Use PASV first\r\n");
                        continue;
                    };
                    send(&mut ctrl, "150 Opening data connection\r\n");
                    if let Ok((mut data, _)) = listener.accept() {
                        if arg == "/hostile-test" {
                            // A synthetic listing (CPE-1461 traversal-name test) — no real files needed,
                            // and no OS-specific "can this filename even be created" constraint. Mixes a
                            // safe leaf in with hostile ones so the test also proves no over-rejection.
                            let _ = data.write_all(list_line("..", true, 0).as_bytes());
                            let _ = data.write_all(list_line("../escape.txt", false, 3).as_bytes());
                            let _ = data.write_all(list_line("sub/nested-evil.txt", false, 3).as_bytes());
                            let _ = data.write_all(list_line("good.txt", false, 4).as_bytes());
                        } else {
                            let dir = real_path(&root, if arg.is_empty() { "/" } else { &arg });
                            if let Ok(rd) = std::fs::read_dir(&dir) {
                                for e in rd.flatten() {
                                    if let Ok(meta) = e.metadata() {
                                        let line = list_line(&e.file_name().to_string_lossy(), meta.is_dir(), meta.len());
                                        let _ = data.write_all(line.as_bytes());
                                    }
                                }
                            }
                        }
                    }
                    send(&mut ctrl, "226 Transfer complete\r\n");
                }
                "RETR" => {
                    if !logged_in {
                        send(&mut ctrl, "530 Not logged in\r\n");
                        continue;
                    }
                    // Check the file BEFORE consuming the PASV listener (only `take()` it once we know a
                    // data transfer will actually happen): a real server never opens/accepts a data
                    // connection for a request it's about to answer with a bare control-channel error, and
                    // dropping an unaccepted listener here left a window where a client that raced ahead
                    // and began connecting before the negative reply arrived could end up half-connected to
                    // a socket that then vanishes — a source of exactly the kind of hard-to-reproduce
                    // control/data desync flakiness this fixed (empirically: the round-trip test failed
                    // intermittently before this change, always with the failing run's `DELE` on a still-
                    // logged-in, still-valid file spuriously erroring — never after it).
                    match std::fs::read(real_path(&root, &arg)) {
                        Ok(data) => {
                            let Some(listener) = pasv.take() else {
                                send(&mut ctrl, "425 Use PASV first\r\n");
                                continue;
                            };
                            send(&mut ctrl, "150 Opening data connection\r\n");
                            if let Ok((mut d, _)) = listener.accept() {
                                let _ = d.write_all(&data);
                            }
                            send(&mut ctrl, "226 Transfer complete\r\n");
                        }
                        Err(_) => send(&mut ctrl, "550 Failed to open file\r\n"),
                    }
                }
                "STOR" => {
                    if !logged_in {
                        send(&mut ctrl, "530 Not logged in\r\n");
                        continue;
                    }
                    let Some(listener) = pasv.take() else {
                        send(&mut ctrl, "425 Use PASV first\r\n");
                        continue;
                    };
                    send(&mut ctrl, "150 Opening data connection\r\n");
                    if let Ok((mut d, _)) = listener.accept() {
                        let mut buf = Vec::new();
                        let _ = d.read_to_end(&mut buf);
                        let path = real_path(&root, &arg);
                        if let Some(p) = path.parent() {
                            let _ = std::fs::create_dir_all(p);
                        }
                        let _ = std::fs::write(&path, &buf);
                    }
                    send(&mut ctrl, "226 Transfer complete\r\n");
                }
                "DELE" => match std::fs::remove_file(real_path(&root, &arg)) {
                    Ok(()) => send(&mut ctrl, "250 Deleted\r\n"),
                    Err(_) => send(&mut ctrl, "550 Delete failed\r\n"),
                },
                "RMD" => match std::fs::remove_dir_all(real_path(&root, &arg)) {
                    Ok(()) => send(&mut ctrl, "250 Removed\r\n"),
                    Err(_) => send(&mut ctrl, "550 Remove failed\r\n"),
                },
                "MKD" => match std::fs::create_dir_all(real_path(&root, &arg)) {
                    Ok(()) => send(&mut ctrl, &format!("257 \"{arg}\" created\r\n")),
                    Err(_) => send(&mut ctrl, "550 Create failed\r\n"),
                },
                "RNFR" => {
                    rename_from = Some(arg);
                    send(&mut ctrl, "350 Ready for RNTO\r\n");
                }
                "RNTO" => match rename_from.take() {
                    Some(from) => {
                        // CPE-1710: this is an FTP server implementing RNFR/RNTO against its own sandbox
                        // root — the wire protocol's rename semantics, not an app-side destination guard.
                        // A test rig; the client, not this crate, decides what may be replaced.
                        #[allow(clippy::disallowed_methods)]
                        match std::fs::rename(real_path(&root, &from), real_path(&root, &arg)) {
                            Ok(()) => send(&mut ctrl, "250 Renamed\r\n"),
                            Err(_) => send(&mut ctrl, "550 Rename failed\r\n"),
                        }
                    }
                    None => send(&mut ctrl, "503 RNFR required first\r\n"),
                },
                "SIZE" => match std::fs::metadata(real_path(&root, &arg)) {
                    Ok(m) if m.is_file() => send(&mut ctrl, &format!("213 {}\r\n", m.len())),
                    _ => send(&mut ctrl, "550 Could not get size\r\n"),
                },
                "QUIT" => {
                    send(&mut ctrl, "221 Bye\r\n");
                    break;
                }
                _ => send(&mut ctrl, "502 Not implemented\r\n"),
            }
        }
    }

    /// Spawn the in-process fake FTP server on an ephemeral loopback port; returns `(port, root)`. Seeds a
    /// temp root: `readme.txt` ("hello ftp") + `sub/nested.txt`.
    fn spawn_ftp_server(creds: Creds) -> (u16, PathBuf) {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("cpe-ftp-srv-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(DIR_NAME)).unwrap();
        std::fs::write(root.join(FILE_NAME), FILE_BODY).unwrap();
        std::fs::write(root.join(DIR_NAME).join("nested.txt"), b"deep").unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let root_ret = root.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { continue };
                let root = root.clone();
                let creds = creds.clone();
                std::thread::spawn(move || handle_control(conn, root, creds));
            }
        });
        (port, root_ret)
    }

    fn exact_server() -> (u16, PathBuf) {
        spawn_ftp_server(Creds::Exact { user: "user", pass: "pw" })
    }

    #[test]
    fn connects_lists_stats_and_reads_over_ftp() {
        let (port, _root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let provider = FtpProvider::connect(&cfg).expect("connect");

        let mut entries = provider.list("/").expect("list");
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2, "got {entries:?}");
        assert_eq!((entries[0].name.as_str(), entries[0].is_dir), (FILE_NAME, false));
        assert_eq!(entries[0].size, FILE_BODY.len() as u64);
        assert_eq!((entries[1].name.as_str(), entries[1].is_dir), (DIR_NAME, true));

        let st = provider.stat(&format!("/{FILE_NAME}")).expect("stat file");
        assert!(!st.is_dir);
        assert_eq!(st.size, FILE_BODY.len() as u64);
        assert!(provider.stat(&format!("/{DIR_NAME}")).unwrap().is_dir);

        assert_eq!(provider.read(&format!("/{FILE_NAME}")).unwrap(), FILE_BODY);
    }

    #[test]
    fn writes_mkdirs_deletes_and_renames_round_trip() {
        let (port, _root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");

        provider.write("/notes.txt", b"remote write works").expect("write");
        assert_eq!(provider.read("/notes.txt").unwrap(), b"remote write works");

        provider.mkdir("/newdir").expect("mkdir");
        assert!(provider.stat("/newdir").unwrap().is_dir);

        provider.rename("/notes.txt", "/renamed.txt").expect("rename");
        assert_eq!(provider.read("/renamed.txt").unwrap(), b"remote write works");
        assert!(provider.read("/notes.txt").is_err(), "old path should be gone");

        provider.delete("/renamed.txt").expect("delete file");
        assert!(provider.stat("/renamed.txt").is_err(), "file should be gone");
        provider.delete("/newdir").expect("delete dir");
        assert!(provider.stat("/newdir").is_err(), "dir should be gone");
    }

    #[test]
    fn a_wrong_password_is_a_clear_error() {
        let (port, _root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "wrong-password");
        let err = match FtpProvider::connect(&cfg) {
            Ok(_) => panic!("a wrong password must be rejected"),
            Err(e) => e,
        };
        assert!(err.contains("login"), "expected a login-flavoured error, got: {err}");
    }

    #[test]
    fn anonymous_login_succeeds_and_browses() {
        let (port, _root) = spawn_ftp_server(Creds::AnyPasswordForAnonymous);
        let cfg = FtpConfig::anonymous("127.0.0.1", port);
        let provider = FtpProvider::connect(&cfg).expect("anonymous connect should succeed");
        assert_eq!(provider.list("/").expect("list").len(), 2);
    }

    #[test]
    fn list_filters_out_hostile_traversal_names() {
        // CPE-1461 source-side defense: a hostile LIST response naming `..`, a `../`-prefixed leaf, or a
        // `/`-embedded leaf must be dropped — never surfaced as a provider entry that could reach the
        // local-write sink in `download_tree`. The legit sibling in the same listing still comes through
        // (no over-rejection). See the `/hostile-test` synthetic branch in `handle_control` above.
        let (port, _root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let provider = FtpProvider::connect(&cfg).expect("connect");
        let names: Vec<String> = provider.list("/hostile-test").expect("list").into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["good.txt"], "only the safe leaf should survive; got {names:?}");
    }

    #[test]
    fn connect_location_bridges_an_ftp_url_to_a_provider() {
        let (port, _root) = exact_server();
        let url = format!("ftp://user@127.0.0.1:{port}/");
        let loc = cpe_server::location::parse(&url);
        let provider = connect_location(&loc, FtpAuth::Password("pw".into()), false)
            .expect("connect_location should succeed for a valid ftp URL");
        assert_eq!(provider.list("/").expect("list").len(), 2);
    }

    #[test]
    fn connect_location_rejects_a_non_ftp_location() {
        let err = match connect_location(&cpe_server::location::parse("/home/x"), FtpAuth::Anonymous, false) {
            Ok(_) => panic!("expected an error"),
            Err(e) => e,
        };
        assert!(err.contains("not an FTP location"), "got: {err}");
    }

    #[test]
    fn ftps_upgrade_against_a_plain_server_fails_cleanly_not_a_panic() {
        // No in-process TLS-terminating fixture here (out of scope for this ticket's headless verify —
        // see the module docs); instead this proves the FTPS code path is actually exercised end-to-end
        // (root store built, `into_secure` called, TLS ClientHello sent) and fails gracefully — a clear
        // `Err`, never a panic — when the peer isn't a TLS server at all, which is exactly what a
        // misconfigured `ftps://` connection looks like from the client's side.
        let (port, _root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw").with_tls();
        let err = match FtpProvider::connect(&cfg) {
            Ok(_) => panic!("a TLS upgrade against a plaintext server must fail"),
            Err(e) => e,
        };
        assert!(err.contains("TLS upgrade"), "expected a TLS-upgrade-flavoured error, got: {err}");
    }

    // CPE-1659 (feature-gated behind `e2e-extra-ca` since CPE-1673): self-tests for the hand-rolled
    // base64/PEM decoder `extra_test_root` uses to load the real-server E2E rig's throwaway FTPS trust
    // anchor. Pure logic, no network — proves the decoder itself is correct independent of whether the
    // env var is ever set in a given run.
    #[test]
    #[cfg(feature = "e2e-extra-ca")]
    fn base64_decode_standard_matches_known_vectors() {
        assert_eq!(base64_decode_standard("").unwrap_or_default(), Vec::<u8>::new());
        assert_eq!(base64_decode_standard("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode_standard("aGVsbG8h").unwrap(), b"hello!");
        assert_eq!(base64_decode_standard("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(base64_decode_standard("Zg==").unwrap(), b"f");
        // Whitespace/newlines (as a real PEM body wraps at 64 cols) must be tolerated.
        assert_eq!(base64_decode_standard("aGVs\nbG8=\n").unwrap(), b"hello");
        // Malformed input (not a multiple of 4, or an invalid character) is a clean `None`, never a panic.
        assert!(base64_decode_standard("abc").is_none());
        assert!(base64_decode_standard("!!!!").is_none());
    }

    #[test]
    #[cfg(feature = "e2e-extra-ca")]
    fn decode_pem_certificate_strips_markers_and_decodes_the_body() {
        let pem = "-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----\n";
        assert_eq!(decode_pem_certificate(pem).unwrap(), b"hello");
    }

    #[test]
    #[cfg(feature = "e2e-extra-ca")]
    fn extra_test_root_is_a_no_op_when_the_env_var_is_unset() {
        // Guard against test-order env-var leakage from any other test in this binary.
        std::env::remove_var(EXTRA_TEST_CA_ENV);
        let mut store = rustls::RootCertStore::empty();
        extra_test_root(&mut store);
        assert!(store.is_empty(), "no env var set — the store must be untouched");
    }
}
