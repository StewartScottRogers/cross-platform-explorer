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
                        // CPE-1726, the sibling primitive (CPE-1719's failure shape, checked here rather
                        // than only `rename`): `fs::write` **follows** a link at the final component and
                        // writes *through* it, so a STOR onto a symlink clobbers the link's target rather
                        // than the link. Left as-is for the same measured reason as RNTO below — this is
                        // `#[cfg(test)]`-only, and a real FTP daemon follows the link too — but recorded
                        // so the next sweep does not have to rediscover which of the two shapes this is.
                        let _ = std::fs::write(&path, &buf);
                    }
                    send(&mut ctrl, "226 Transfer complete\r\n");
                }
                "DELE" => match std::fs::remove_file(real_path(&root, &arg)) {
                    Ok(()) => send(&mut ctrl, "250 Deleted\r\n"),
                    Err(_) => send(&mut ctrl, "550 Delete failed\r\n"),
                },
                // CPE-1731: `remove_dir`, **not** `remove_dir_all`. RFC 959 §4.1.3 defines `RMD` as
                // "remove the directory" and every real daemon (vsftpd, ProFTPD, pure-ftpd, IIS)
                // answers `550` with `ENOTEMPTY` when it is not empty — deleting the subtree instead is
                // behaviour no server this client will ever meet has. A test double that quietly
                // succeeds where the wire says "directory not empty" lets a client test pass against a
                // fiction, which is the same "model the wire, do not defend against it" rule CPE-1726
                // used to leave the `RNTO` rename unguarded, applied in the other direction.
                //
                // Measured rather than assumed (both CI platforms): `remove_dir` on a non-empty
                // directory is `Err` with `raw_os_error 145`/`ERROR_DIR_NOT_EMPTY` on Windows and `39`/
                // `ENOTEMPTY` on Linux, and the directory and its contents are untouched afterwards.
                "RMD" => match std::fs::remove_dir(real_path(&root, &arg)) {
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
                        // **CPE-1731: compare the RESOLVED destination against the served root — do not
                        // enumerate spellings.** `RNTO` with no argument at all was answered
                        // `250 Renamed`, in a crate CPE-1726 declared *structurally immune* to that
                        // defect on the grounds that FTP takes both paths from one resolver rather than
                        // from a second header. True of the source; false of the destination —
                        // `real_path` maps an empty path *and* `/` to the root, so the rig expressed the
                        // identical shape the WebDAV half had just been fixed for.
                        //
                        // The property is shared, so the implementation is too:
                        // `cpe_server::fsutil::same_place` is the one CPE-1726 arrived at after five
                        // rounds of denylists, and its doc carries the whole argument (why it
                        // canonicalizes when both sides resolve, why the lexical fallback pops `..`, and
                        // the proof that the pop errs safe). Read it before changing this line.
                        //
                        // **Reused after re-measuring, not inherited** — inheriting a claim across these
                        // three crates is what produced this ticket. CPE-1731's probe ran *this* rig's
                        // `real_path` over all three escape families on Windows and Linux; the one
                        // difference that matters is that a wire path is `/`-separated even on Windows,
                        // so a resolved destination is mixed-separator (`…\cpe-ftp-srv-0\sub/..`).
                        // `Path::components()` and `canonicalize` both accept that, and every
                        // root-resolving row still compares equal. Full findings on `same_place`.
                        //
                        // **There is deliberately no separate "argument is empty" branch**, tempting as
                        // a `501 Syntax error in parameters` would be for wire fidelity. An empty
                        // argument is a *member* of the family this guard closes, and a second check
                        // that catches it first would keep the headline regression row
                        // (`RNTO` with no argument) green with this comparison deleted — masking exactly
                        // the guard the row exists to prove. One guard, so neutralising it goes red.
                        //
                        // **The SOURCE side is deliberately NOT guarded, and the gap is real:**
                        // `RNFR /` + `RNTO /elsewhere` still renames the served root itself into a
                        // subdirectory. Out of scope by choice, not by oversight — this ticket's subject
                        // is the destination, `cpe-webdav` has the identical asymmetry recorded at its
                        // own MOVE (`DELETE /` removes the root outright there), and a source guard
                        // needs the containment check CPE-1730 is opening. Recorded here so the next
                        // sweep reads it as a decision rather than as an absence nobody noticed.
                        let dest = real_path(&root, &arg);
                        if cpe_server::fsutil::same_place(&dest, &root) {
                            // RFC 959 §4.2: `553 Requested action not taken. File name not allowed.` —
                            // the reply for a destination name a server refuses to accept, and a `5xx`
                            // so `suppaftp` surfaces it as an error rather than a rename that happened.
                            send(&mut ctrl, "553 Rename destination is the served root\r\n");
                            continue;
                        }
                        // CPE-1726 re-took CPE-1710's classification against a **measurement** instead of
                        // a category ("it is a protocol server" is a category). DELIBERATELY UNGUARDED —
                        // do not wrap this in `cpe_server::fsutil::rename_into_slot`; the measurement is:
                        //
                        // 1. This entire FTP server is `#[cfg(test)]`. `cpe-ftp` ships a *client*
                        //    ([`FtpProvider`]) and no server, so this line is not compiled into the app.
                        //    The "remote client" supplying `arg` is a test in this same file, over
                        //    loopback, against a per-test temp root this rig created and seeded itself.
                        //    There is no third party whose files sit at the destination — the premise the
                        //    ticket weighed ("the client is not the person whose files are there") is
                        //    simply absent here, and that absence is what decides it.
                        //    **Bounded precisely (PR #902 review):** "no user's files at the
                        //    destination" holds because no user drives this rig, NOT because the
                        //    destination is confined — `real_path` is a bare join with no containment
                        //    check, so a `..`-shaped path would resolve outside the temp root. Nothing
                        //    sends one today; CPE-1730 tracks closing it. If that ever changes before
                        //    CPE-1730 lands, this reason expires with it.
                        // 2. That premise is pinned rather than trusted:
                        //    `cpe_1726_every_destructive_filesystem_call_is_confined_to_the_test_rig`
                        //    goes red the moment this line (or any sibling destructive primitive) moves
                        //    above the `#[cfg(test)]` marker, so promoting the rig to production forces
                        //    the decision to be re-taken rather than silently inherited.
                        // 3. A test double must model the wire, not defend against it. A real FTP daemon's
                        //    RNTO renames *onto* the destination; hardening the rig would make the client
                        //    tests pass against a server unlike any the app will ever meet.
                        //
                        // What `fs::rename` does to a link at the destination is pinned, not assumed, by
                        // `cpe_1726_rename_onto_a_link_never_writes_through_it`.
                        #[allow(clippy::disallowed_methods)]
                        match std::fs::rename(real_path(&root, &from), &dest) {
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
    /// The `#[allow(clippy::disallowed_methods)]` on
    /// `RNTO` argues that the unguarded rename is safe *because the whole server is a `#[cfg(test)]`
    /// test double* — no shipped code, no third-party files at the destination. That is a measurement,
    /// not a category, and this test is what keeps it a measurement: if the rig (or any single
    /// destructive call in it) is ever promoted above the `#[cfg(test)]` marker, this goes red and the
    /// decision has to be re-taken rather than inherited from a comment written when it was still true.
    ///
    /// A source scan rather than a type-level trick because the property *is* textual — "no destructive
    /// `std::fs` call exists in the compiled-into-the-app half of this file" is exactly what a reviewer
    /// or a future sweep would check by hand, and this makes CI check it on every commit instead.
    /// `\r` is stripped first: the working tree is CRLF on Windows and LF on the Linux/macOS runners,
    /// and a needle containing `\n` would silently match nothing on one of them — a guard that cannot
    /// fail on half the matrix is the failure this ticket family exists to stop.
    ///
    /// # What this scan does NOT catch, stated because an earlier draft overstated it
    /// The needles are **fully `std::fs::`-qualified**, and that is load-bearing in one direction (it is
    /// what keeps a wire op on the remote from being reported as a local write) and a gap in the other:
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
            "[CPE-1726] a destructive `std::fs` call now exists in the SHIPPED half of cpe-ftp:\n{}\n\n\
             CPE-1726 left the `RNTO` rename unguarded on one measured premise: this crate ships a \
             client and its FTP *server* is a `#[cfg(test)]` test double, so no user's files are ever \
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
    /// rig's `RNTO`. Both legs assert on the slot and on the victim's bytes and **never on the returned
    /// `Result`** — every bug in this family (CPE-1710/1716/1719) returned `Ok` while destroying
    /// something, so the return value is the one witness that has never been reliable.
    ///
    /// The property being pinned is the one that separates `rename` from `write`: **`fs::rename` does
    /// not follow the final component**, so it replaces the link and leaves the link's target alone,
    /// whereas `fs::write` at the same slot would write *through* it and clobber the target. That is
    /// the whole reason the two need different fixes, and it is asserted here rather than trusted.
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
        let (port, root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");

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
                "[CPE-1726] SKIPPED the LIVE-link leg of cpe-ftp's RNTO test: could not create a file \
                 symlink at {}. Rust's `symlink_file` passes ALLOW_UNPRIVILEGED_CREATE and normally \
                 succeeds unelevated on Windows too, so this is an unusual environment rather than the \
                 ordinary Windows case — under CI this is a hard red, not this notice. What is NOT \
                 covered on this run is leg 1's assertions specifically: the live victim's bytes and \
                 the slot's final contents. The DANGLING leg below still runs and still covers the \
                 write-through property.",
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
                    "RNTO reported success, so the slot must now hold the moved file's bytes; it holds \
                     something else (is_symlink = {link_now:?})"
                );
            } else {
                assert_eq!(
                    link_now.ok(),
                    Some(true),
                    "RNTO reported failure ({r:?}), so it must have left the link alone — a failed \
                     rename that still destroyed the destination is the worst of both outcomes"
                );
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1731 — the destination that resolves to the served root, and the empty-only verb
    // ---------------------------------------------------------------------------------------------

    /// Log in over a **raw** control connection and drive one `RNFR`/`RNTO` pair, returning the reply
    /// to the `RNTO`.
    ///
    /// Raw because `suppaftp`'s `rename` takes two `&str`s and always emits `RNTO <arg>` — it cannot
    /// express the case the bug was reported in, `RNTO` with **no argument at all**. `to: None` sends
    /// the bare verb; `Some(s)` sends `RNTO s`.
    ///
    /// Every step `expect`s or asserts, so a harness that fails to connect, log in or get its `RNFR`
    /// accepted is a loud panic rather than an empty string that would satisfy a "the tree survived"
    /// assertion by accident.
    fn rnfr_rnto(port: u16, from: &str, to: Option<&str>) -> String {
        let mut sock = TcpStream::connect(("127.0.0.1", port)).expect("connect to the rig");
        // libtest has no per-test timeout, so a rig that never answers would hang the suite rather
        // than fail it. Bound every read.
        sock.set_read_timeout(Some(std::time::Duration::from_secs(10))).expect("set a read timeout");
        let mut reader = BufReader::new(sock.try_clone().expect("clone the control socket"));
        let mut reply = |w: &mut TcpStream, send: Option<&str>| -> String {
            if let Some(s) = send {
                w.write_all(format!("{s}\r\n").as_bytes()).expect("send a control command");
            }
            let mut line = String::new();
            reader.read_line(&mut line).expect("read a control reply");
            line
        };

        assert!(reply(&mut sock, None).starts_with("220"), "the rig must greet with 220");
        assert!(reply(&mut sock, Some("USER user")).starts_with("331"), "USER must ask for a password");
        assert!(reply(&mut sock, Some("PASS pw")).starts_with("230"), "PASS must log us in");
        assert!(
            reply(&mut sock, Some(&format!("RNFR {from}"))).starts_with("350"),
            "RNFR must be accepted, or the RNTO below tests nothing"
        );
        let rnto = match to {
            Some(t) => reply(&mut sock, Some(&format!("RNTO {t}"))),
            None => reply(&mut sock, Some("RNTO")),
        };
        let _ = reply(&mut sock, Some("QUIT"));
        rnto
    }

    /// CPE-1731 acceptance: an `RNTO` whose destination **resolves to the served root** is refused.
    ///
    /// The defect this replaces answered `250 Renamed` to an `RNTO` carrying no argument — verbatim the
    /// WebDAV defect CPE-1726 had just fixed, in the crate that PR declared *structurally immune* to it.
    ///
    /// # What the rows are, and what they are not
    /// **Regression pins, not a specification.** CPE-1726 shipped a table like this three times and the
    /// UAT falsified each one on the code that carried it; the fix is the resolved comparison in
    /// `handle_control`, and these record which spellings have actually been observed escaping a
    /// previous round. The families are here as the cheapest available evidence that the comparison
    /// closes *families* rather than members:
    /// 1. `.`-and-`/` spellings (what CPE-1726's round-3 denylist let through);
    /// 2. `..` landing **on** the root (what its round-4 lexical comparison let through);
    /// 3. spellings the *filesystem* calls equal — Windows-only, in
    ///    `cpe_1731_an_rnto_naming_the_root_by_another_spelling_is_refused` below, because on Linux they
    ///    are genuinely different places *and* unreachable through `real_path`'s leading-`/` trim.
    ///
    /// # The source is a column, not a function of the expected status
    /// Each row carries its own `RNFR` argument. CPE-1726's equivalent table first derived the source
    /// from the row's expected status (`if want == "400" {…}`), which worked only by coincidence — a
    /// proxy standing in for a property, in the one file whose subject is that substitution. Here the
    /// root-resolution rows send `RNFR /` because **that is the shape the bug was reported in**, and it
    /// is the only source that reproduces it: with the guard deleted and a *file* source, the rig
    /// answers `550` (renaming a file onto an existing non-empty directory fails) — a red produced by
    /// an errno rather than by the defect. With `RNFR /` it answers `250 Renamed` for a rename that did
    /// nothing, which is the defect verbatim. Being saved by an errno is not the same as being guarded.
    ///
    /// # The last row is a positive control, and it is load-bearing
    /// Without it, a harness that silently failed to drive the rig at all would satisfy every refusal
    /// row. The control asserts `readme.txt`'s **bytes** arrive at the new name and the old name is
    /// gone — so the refusals above are measured against a connection that demonstrably can rename.
    ///
    /// # What the filesystem assertions can and cannot catch here — stated rather than implied
    /// They **cannot fail today**, and pretending otherwise would be the vacuous-assertion trap this
    /// sprint keeps finding. A destination that resolves to the root cannot destroy anything: renaming
    /// the root onto itself is a no-op, and renaming a file onto the populated root fails. So the
    /// observable defect is the *reply* — a success reported for a rename that never happened, which a
    /// client will believe and then delete its source. The tree assertions are kept as the cheap thing
    /// that goes red if a future change ever makes this shape destructive, not as this test's evidence.
    #[test]
    fn cpe_1731_an_rnto_whose_destination_resolves_to_the_served_root_is_refused() {
        // (RNFR argument, RNTO argument as sent on the wire, expected reply prefix, why)
        let cases: &[(&str, Option<&str>, &str, &str)] = &[
            ("/", None, "553", "no argument at all — the shape the ticket was filed on"),
            ("/", Some("/"), "553", "a bare `/` — `real_path` maps it to the root"),
            ("/", Some("//"), "553", "two slashes — survived CPE-1726's round-2 pre-trim filter"),
            ("/", Some("///"), "553", "three slashes — same evasion"),
            ("/", Some("."), "553", "a bare `.`"),
            // Family 1 — the four the round-3 UAT measured returning success against a literal
            // denylist of `""` and `"."`. Each trims to something that is neither literal.
            ("/", Some("/."), "553", "`/.` — trims to `.`"),
            ("/", Some("/./"), "553", "`/./` — trims to `./`, neither denied literal"),
            ("/", Some("/.//"), "553", "`/.//` — a CurDir component then an empty one"),
            ("/", Some("//./"), "553", "`//./` — leading empty component before the dot"),
            ("/", Some("/./."), "553", "`/./.` — two CurDir components, no trailing slash"),
            ("/", Some("//."), "553", "`//.` — slashes then a dot"),
            // Family 2 — `..` landing ON the root rather than escaping it. Round 4 preserved `..` on
            // the theory that popping it strayed into CPE-1730's containment scope; these escape
            // nothing, need no knowledge of the server, and resolve to the served root.
            ("/", Some("/nonexistent/.."), "553", "`..` popping a name that never existed"),
            ("/", Some("/sub/.."), "553", "`..` popping a real subdirectory"),
            ("/", Some("/./sub/../."), "553", "`..` and `.` mixed, still the root"),
            // The positive control (see the doc above).
            ("/readme.txt", Some("/renamed.txt"), "250", "an ordinary destination must still be renamed"),
        ];

        for (source, dest, want, why) in cases {
            let (port, root) = exact_server();
            let reply = rnfr_rnto(port, source, *dest);

            if *want == "250" {
                assert_eq!(
                    std::fs::read(root.join("renamed.txt")).ok().as_deref(),
                    Some(FILE_BODY),
                    "[{why}] the control row must actually move the file's bytes, or every refusal \
                     row above is measured against a rig that renames nothing (reply {reply:?})"
                );
                assert!(
                    !root.join(FILE_NAME).exists(),
                    "[{why}] and the source name must be gone (reply {reply:?})"
                );
            } else {
                assert_eq!(
                    std::fs::read(root.join(FILE_NAME)).ok().as_deref(),
                    Some(FILE_BODY),
                    "[{why}] the served tree must be intact after a refusal (reply {reply:?})"
                );
                assert!(
                    root.join(DIR_NAME).join("nested.txt").is_file(),
                    "[{why}] and so must its subdirectory (reply {reply:?})"
                );
            }
            assert!(
                reply.starts_with(want),
                "[{why}] expected a {want} reply. A destination that resolves to the served root is a \
                 refusal, not a rename the server reports as done. Got {reply:?}"
            );
        }
    }

    /// Family 3: the served root spelled in a way **only the filesystem** knows is the same place.
    ///
    /// Windows matches names case-insensitively and strips trailing dots, while `PathBuf` equality
    /// compares `Component::Normal` byte-wise — so this is the row `normalise_lexically` alone cannot
    /// answer, and the reason `fsutil::same_place` consults `canonicalize`. Removing that half turns
    /// exactly this test red and leaves the table above green (measured; see the PR body).
    ///
    /// **Windows-only, and measured rather than assumed.** `real_path` trims the leading `/` before
    /// joining, so on Linux an absolute destination becomes a *relative* one and lands inside the root
    /// (`/tmp/<root>` resolved to `<root>/tmp/<root>`, `same_place = false` — measured under WSL). On a
    /// case-sensitive filesystem these spellings are genuinely different places anyway. Two independent
    /// reasons there is nothing to catch there; on Windows a `C:\…` destination survives the trim and
    /// `Path::join` discards the base, so it arrives as the spelling itself.
    #[cfg(windows)]
    #[test]
    fn cpe_1731_an_rnto_naming_the_root_by_another_spelling_is_refused() {
        for (spell, why) in [
            ("upper-case", "Windows matches names case-insensitively"),
            ("trailing dot", "Windows strips a trailing dot during path processing"),
        ] {
            let (port, root) = exact_server();
            let literal = root.to_string_lossy().to_string();
            let dest = if spell == "upper-case" { literal.to_uppercase() } else { format!("{literal}.") };

            // `RNFR /` for the same reason the table above uses it: it is the source that reproduces
            // the defect (a `250` for a rename that did nothing) rather than one an errno stops.
            let reply = rnfr_rnto(port, "/", Some(&dest));

            assert_eq!(
                std::fs::read(root.join(FILE_NAME)).ok().as_deref(),
                Some(FILE_BODY),
                "[{spell}] the served tree must survive a rename onto a different spelling of the root \
                 ({why}); reply {reply:?}"
            );
            assert!(
                reply.starts_with("553"),
                "[{spell}] {why}, so this destination IS the served root and must be refused. Byte-wise \
                 path equality does not know that, which is why the check consults the filesystem. Got \
                 {reply:?}"
            );
        }
    }

    /// CPE-1731 acceptance: `RMD` is the **empty-directory** verb (RFC 959 §4.1.3), so a non-empty
    /// directory is refused and its contents survive.
    ///
    /// The rig implemented it with `remove_dir_all`, which deleted the tree and answered `250 Removed`
    /// — behaviour no real daemon has, and the mirror image of CPE-1726's thesis (which had WebDAV as
    /// the crate that got its verb semantics wrong; WebDAV's `DELETE` is correctly recursive).
    ///
    /// **Asserted on the filesystem, and on the exact file the tree seeder created** — not on the reply
    /// and not on a bare `!exists()`. A negative assertion guarded by a filename typed twice passes
    /// vacuously the moment the two drift, so `nested.txt`'s **bytes** are what is checked, and the
    /// positive control below proves `RMD` still works on the case it is defined for. Unlike the rename
    /// family above, these assertions can and do fail: with `remove_dir_all` restored, the tree is
    /// genuinely gone.
    #[test]
    fn cpe_1731_rmd_refuses_a_non_empty_directory_and_leaves_it_intact() {
        let (port, root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");

        // `delete` tries DELE first (fails — it is a directory), then RMD. The seeded `sub` holds
        // `nested.txt`, so RMD must refuse.
        let r = provider.delete(&format!("/{DIR_NAME}"));
        assert!(
            root.join(DIR_NAME).is_dir(),
            "the non-empty directory itself must survive an empty-only verb (delete reported {r:?})"
        );
        assert_eq!(
            std::fs::read(root.join(DIR_NAME).join("nested.txt")).ok().as_deref(),
            Some(&b"deep"[..]),
            "and its contents must still be there, byte for byte — `RMD` is not a recursive delete \
             (delete reported {r:?})"
        );
        assert!(r.is_err(), "a refusal the client reads as success is the CPE-1726 failure shape");

        // Positive control: `RMD` on the directory it IS defined for still works, so the assertions
        // above are not measuring a verb that simply stopped functioning.
        provider.mkdir("/emptydir").expect("mkdir");
        provider.delete("/emptydir").expect("RMD must still remove an EMPTY directory");
        assert!(!root.join("emptydir").exists(), "the empty directory must actually be gone");
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
