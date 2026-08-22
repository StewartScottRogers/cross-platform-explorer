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
    use cpe_server::fsutil::{scratch_dir, ScratchDir};
    use std::io::{BufRead, BufReader, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

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

    /// The rig's refusal line for a request path that resolves **outside** the served root (CPE-1730).
    ///
    /// `550` rather than `553`, and the choice is load-bearing rather than cosmetic. `553` is
    /// **verb-scoped, not exclusive to this rig's confinement guards** — CPE-1731's root-destination
    /// guard uses it (`"553 Rename destination is the served root\r\n"` below), and CPE-1742's `STOR`
    /// missing-parent guard uses it too (RFC 959 §5.4 puts `553` in `STOR`'s own reply set, `550` in
    /// none of it). CPE-1731's tests assert only the reply *prefix* (`reply.starts_with("553")`), which
    /// would be a masking risk if some *other* `553` reply could reach those same assertions — but it
    /// cannot: those rows are driven exclusively through `rnfr_rnto`/`raw_commands`, which send only
    /// `RNFR`/`RNTO` and never open a data connection, so a `STOR`-only `553` line is unreachable from
    /// them. Verified rather than assumed: `crates/ftp/src/lib.rs`'s `STOR` arm cannot run inside a
    /// connection that never sends `STOR`. A *different* code here is still worth keeping, though — it
    /// keeps CPE-1730's own rows reddening on `550` specifically, not merely on "some 553 arrived from
    /// somewhere," which is the tighter assertion CPE-1731's argument below wants anyway.
    ///
    /// **It is still a refusal no errno can spell**, which is the property `553` had by construction and
    /// the reason it was chosen there. This rig's other `550` replies (`Rename failed`, `Delete failed`,
    /// `Remove failed`, `Create failed`, `Could not get size`, `Failed to open file`) are fixed
    /// server-authored strings; **no `io::Error` text ever reaches this wire**, so the full line is
    /// reachable from the confinement guard and nowhere else. The tests therefore assert the whole line,
    /// never the bare code. And nothing caller-supplied is interpolated into it — a path echoed into a
    /// refusal is how an earlier assertion in this family became forgeable by a caller who simply named
    /// their file after it.
    const CPE_1730_ESCAPED_ROOT_REFUSAL: &str = "550 Path escapes the served root\r\n";

    /// The rig's refusal line for `RNFR` naming the served root itself (CPE-1731's recorded gap).
    ///
    /// CPE-1731 guarded the rename **destination** and wrote down, at the call site, that the source was
    /// deliberately left open: `RNFR /` + `RNTO /elsewhere` still moved the whole served root into a
    /// subdirectory and answered `250 Renamed`. That gap is closed here because it is the half
    /// containment alone cannot close — the root **is** contained in itself, by design, so
    /// [`cpe_server::fsutil::confined_to`] allows it and [`cpe_server::fsutil::same_place`] is what
    /// refuses it. Containment is not equality in either direction: one guard admits the root, the other
    /// rejects it, and the rename site needs both answers.
    ///
    /// `550` for the same reason as above — never `553`, which would mask CPE-1731's rows, all of which
    /// send `RNFR /`.
    const CPE_1730_ROOT_SOURCE_REFUSAL: &str = "550 Rename source is the served root\r\n";

    /// Map a client-supplied, `/`-rooted FTP path onto the real filesystem path under `root` — or
    /// **`None`** when it resolves outside the served root (CPE-1730).
    ///
    /// This used to be a bare `root.join(rel)`, which is not a confinement at all: `Path::join` discards
    /// the base when handed an absolute path, `..` walks out of the tree, and a symlinked intermediate
    /// directory leaves it without either. Everything under the escaped path was then a live target for
    /// this rig's `fs::write`, `remove_file`, `remove_dir`, `fs::rename` and `create_dir` — and the rig
    /// runs under `cargo test` on a developer's own checkout, so the blast radius is a working tree.
    ///
    /// The property, and why it is asked rather than enumerated, is on
    /// [`cpe_server::fsutil::confined_to`]; read it before changing this. In particular it is **not**
    /// [`cpe_server::fsutil::contained_under`], which the ticket expected to be reusable and which fails
    /// *open* on a path that does not exist yet — i.e. on every `STOR` target, `MKD` name and rename
    /// destination this rig has.
    ///
    /// **Fallible on purpose, rather than silently clamping into the root.** A resolver that quietly
    /// rewrote an escaping path to something inside would answer a request the client did not make, and
    /// the caller could not tell a clamped path from an honest one. `None` forces every call site to say
    /// what it does about a refusal, and the compiler makes a new call site say it too.
    fn real_path(root: &Path, ftp_path: &str) -> Option<PathBuf> {
        let joined = joined_path(root, ftp_path);
        cpe_server::fsutil::confined_to(&joined, root).then_some(joined)
    }

    /// The **unconfined** join — the whole of what `real_path` used to be, kept as its own function for
    /// the one caller that needs the joined path even when it escapes.
    ///
    /// That caller is `RNTO`, which must ask CPE-1731's "does this resolve *to* the root?" question
    /// **before** CPE-1730's "does it stay *inside* the root?" one. The ordering is not cosmetic: on
    /// Linux `canonicalize` fails with `NotFound` for `<root>/nonexistent/..` (measured — on Windows it
    /// succeeds and yields the root), so confinement refuses that spelling while `same_place` calls it
    /// the root. It is a **regression row of CPE-1731's**, and it must keep being answered by
    /// CPE-1731's guard with CPE-1731's `553`, or that ticket's evidence quietly moves to this ticket's
    /// guard and its neutralisation test stops proving anything.
    ///
    /// Nothing else may use this. Escaping *into* it is the defect; the name is the reminder.
    fn joined_path(root: &Path, ftp_path: &str) -> PathBuf {
        let rel = ftp_path.trim_start_matches('/');
        if rel.is_empty() { root.to_path_buf() } else { root.join(rel) }
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
                    // CPE-1730: resolve BEFORE taking the PASV listener, for the same reason `RETR`
                    // below does — a real server answers a request it is about to refuse on the control
                    // channel alone, and does not open a data connection for it.
                    let listed = real_path(&root, if arg.is_empty() { "/" } else { &arg });
                    if listed.is_none() && arg != "/hostile-test" {
                        send(&mut ctrl, CPE_1730_ESCAPED_ROOT_REFUSAL);
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
                        } else if let Some(dir) = listed {
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
                    let Some(retr_path) = real_path(&root, &arg) else {
                        send(&mut ctrl, CPE_1730_ESCAPED_ROOT_REFUSAL);
                        continue;
                    };
                    match std::fs::read(retr_path) {
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
                    // CPE-1730: resolved before the listener is taken, so an escaping upload never opens
                    // a data connection and — the point of the ticket — never reaches `fs::write` on a
                    // path outside the served root.
                    let Some(path) = real_path(&root, &arg) else {
                        send(&mut ctrl, CPE_1730_ESCAPED_ROOT_REFUSAL);
                        continue;
                    };
                    // CPE-1742: `STOR` has no directory-creating semantics — RFC 959 §5.4's reply
                    // sequence for STOR is `125`/`150` → `226`/`250`, or a negative completion of
                    // `532`/`450`/`452`/`553`. **`550` is not among them** (that code belongs to
                    // `DELE`/`RMD`/`MKD`/`PWD`/`CWD`/`CDUP`/`SMNT` — the acceptance criteria's `550` was
                    // an unverified guess, corrected here by the spec text plus a real daemon). A real
                    // server checks the destination *before* ever opening the data connection: measured
                    // against the actual vsftpd this repo's own CI runs STOR through (`fauria/vsftpd`,
                    // see `.github/workflows/ci.yml` / CPE-1659) — the PR-985 review independently
                    // confirmed this against vsftpd's own source (`ftpcodes.h`'s `FTP_UPLOADFAIL = 553`,
                    // sent from `postlogin.c` ~40 lines before the data connection would open) — it
                    // answers exactly `553 Could not create file.` for a STOR whose parent directory does
                    // not exist, and never sends `150` first. So the check runs here, before the PASV
                    // listener is taken — the same shape as the CPE-1730 comment above: a refused upload
                    // must not open a data connection either. The `create_dir_all` this rig used to run
                    // inside the data-connection block below is gone: it invented a parent chain no real
                    // daemon creates. `cpe_server::transfer::upload_tree` (CPE-1741) does not depend on
                    // it — it already creates the chain itself via explicit `MKD`s before any `STOR` runs.
                    //
                    // `Path::is_dir()`, not `Path::exists()`: the former is `false` both when the parent
                    // is missing AND when it exists but is a **file** (`STOR /at-root.txt/child.txt`
                    // where `at-root.txt` is an ordinary file) — a real daemon refuses that shape too,
                    // for the same reason (no directory to create the entry in), and the latter would
                    // silently let it through to the `fs::write` below, which fails but — before this
                    // guard existed — had its error swallowed and still answered `226`. Covered by
                    // `stor_refuses_a_missing_parent_and_still_works_for_one_that_exists` below, which
                    // exercises both shapes.
                    //
                    // No `parent.as_os_str().is_empty()` guard: `real_path` → `joined_path` (below)
                    // always yields `root` itself or `root.join(rel)`, both absolute, so `.parent()` here
                    // is never `Some` of an empty path — an earlier draft carried that check as if it
                    // were load-bearing; it was dead weight and reads as more defensive than the code
                    // actually is, so it is gone.
                    if let Some(parent) = path.parent() {
                        if !parent.is_dir() {
                            send(&mut ctrl, "553 Could not create file.\r\n");
                            continue;
                        }
                    }
                    // Adjacent to CPE-1742 but a distinct condition, found in the same review pass: STOR
                    // onto a path that is ITSELF an existing directory. `fs::write` on a directory fails
                    // (`EISDIR`/`ERROR_ACCESS_DENIED` depending on OS) and, before this check existed,
                    // that failure was swallowed by the `let _ = std::fs::write(...)` below and answered
                    // `226 Transfer complete` anyway regardless — success reported for a write that never
                    // happened, the exact "silent success on a real refusal" shape this ticket family
                    // exists to close, one case over from the one CPE-1742 was filed for. Refused with
                    // the same `553` line, and it is the SAME reply vsftpd sends here too, not merely a
                    // reasonable guess: `postlogin.c`'s `handle_upload_common` opens the destination
                    // (`sysutil.c`'s `vsf_sysutil_create_or_open_file`, `open(p_filename, O_CREAT |
                    // O_WRONLY | O_NONBLOCK, mode)`) and has exactly ONE failure branch for it —
                    // `if (vsf_sysutil_retval_is_error(new_file_fd)) { vsf_cmdio_write(p_sess,
                    // FTP_UPLOADFAIL, "Could not create file."); return; }` — reached before
                    // `get_remote_transfer_fd` (i.e. before any `150`). POSIX `open(2)` returns `ENOENT`
                    // for a missing path prefix, `ENOTDIR` for a prefix component that is a plain file,
                    // and `EISDIR` for `O_WRONLY` on a path that is itself a directory — vsftpd's single
                    // undifferentiated branch treats all three identically, so this case has the same
                    // wire text as the missing-parent one above, not weaker evidence for it.
                    if path.is_dir() {
                        send(&mut ctrl, "553 Could not create file.\r\n");
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
                "DELE" => match real_path(&root, &arg) {
                    None => send(&mut ctrl, CPE_1730_ESCAPED_ROOT_REFUSAL),
                    Some(p) => match std::fs::remove_file(p) {
                        Ok(()) => send(&mut ctrl, "250 Deleted\r\n"),
                        Err(_) => send(&mut ctrl, "550 Delete failed\r\n"),
                    },
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
                "RMD" => match real_path(&root, &arg) {
                    None => send(&mut ctrl, CPE_1730_ESCAPED_ROOT_REFUSAL),
                    Some(p) => match std::fs::remove_dir(p) {
                        Ok(()) => send(&mut ctrl, "250 Removed\r\n"),
                        Err(_) => send(&mut ctrl, "550 Remove failed\r\n"),
                    },
                },
                // CPE-1731: `create_dir`, **not** `create_dir_all` — the mirror of the `RMD` fix above,
                // and found by the reviewer applying this PR's own argument to the verb sitting four
                // lines from it. RFC 959 §4.1.3 `MKD` is `mkdir(2)`: it creates **one** directory, and a
                // real daemon answers `550` both for a missing parent and for a name that already
                // exists. `create_dir_all` succeeded in both — inventing the parent chain in the first,
                // and reporting `257 "…" created` for a directory it did not create in the second. A
                // test double that succeeds where the wire refuses lets a client test pass against
                // behaviour no real server has, which is the whole reason `RMD` changed.
                //
                // Cost measured before taking the fix, when the suites stood at 13 and 28 tests: with
                // `create_dir`, `cpe-ftp` was 13/13 and `cpe-sftp` 28/28 green, so nothing in either
                // suite depended on the recursion or on `create_dir_all`'s idempotence. (They are 14
                // and 29 now — this fix brought its own tests. The figures are left as measured rather
                // than silently updated to today's, since the point they make is about what the suite
                // looked like *before* the change.)
                //
                // **The `create_dir_all` in `STOR` above was deliberately left alone for a while
                // (CPE-1741), and the first draft of this note gave a reason that measurement
                // falsified.** It claimed `upload_tree`'s round-trip needed it; removing the call left
                // this crate green, and `cpe-ftp` had no `upload_tree` test at all — the reason was
                // invented, which is the exact substitution this ticket family exists to stop, so it was
                // recorded rather than quietly swapped for a better one. **Re-measured at the time**
                // after the reviewer pointed out the first figure (13/13) had gone stale: still
                // **14/14 green** with the call removed, so the "nothing here depends on it" claim was
                // that round's measurement and not an inherited one.
                //
                // The real reason was shape, not cost: `STOR` has no directory-creating semantics to
                // model at all — a real daemon fails outright when the parent is missing — so fixing it
                // was not "make the primitive match the verb" but "remove a capability the wire never
                // had", which changes what a *client* must do before uploading and needed its own
                // client-side change and tests. **CPE-1742 has now done exactly that** — see the `STOR`
                // arm above, which refuses a missing parent with `553` (not `550`; see that arm's comment
                // for why) before ever opening the data connection. `upload_tree` (CPE-1741) was already
                // unaffected: it creates the parent chain itself via explicit `MKD`s, never relying on
                // `STOR` to invent anything, so nothing on the production side changed.
                "MKD" => match real_path(&root, &arg) {
                    None => send(&mut ctrl, CPE_1730_ESCAPED_ROOT_REFUSAL),
                    Some(p) => match std::fs::create_dir(p) {
                        Ok(()) => send(&mut ctrl, &format!("257 \"{arg}\" created\r\n")),
                        Err(_) => send(&mut ctrl, "550 Create failed\r\n"),
                    },
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
                        // **CPE-1730 closes the source gap CPE-1731 recorded here.** That note read:
                        // *"`RNFR /` + `RNTO /elsewhere` still renames the served root itself into a
                        // subdirectory … a source guard needs the containment check CPE-1730 is
                        // opening."* Both halves now exist, below — and the containment check turned out
                        // **not** to be what closes it. The root is *contained in itself* by design (a
                        // resolver must map `/` somewhere for `LIST` to work), so `confined_to` allows
                        // `RNFR /`; it is `same_place` on the **source** that refuses it. Containment is
                        // not equality, and the rename site is where that distinction is paid for: it
                        // asks both questions, of both paths, and gets four different answers.
                        //
                        // **Order matters and is measured.** The `same_place` destination check runs
                        // FIRST, on the *unconfined* join, because `<root>/nonexistent/..` — one of
                        // CPE-1731's own regression rows — canonicalises to `Err(NotFound)` on Linux and
                        // to the root on Windows. Confinement refuses it; `same_place` calls it the root.
                        // Whichever guard runs first owns the row, and it must stay CPE-1731's, or its
                        // neutralisation test goes green through this ticket's guard instead.
                        let dest_joined = joined_path(&root, &arg);
                        if cpe_server::fsutil::same_place(&dest_joined, &root) {
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
                        //    destination is confined. **CPE-1730 has since confined it** — `real_path`
                        //    now runs its join through `cpe_server::fsutil::confined_to` and returns
                        //    `None` for a path that resolves outside the served root, so both paths
                        //    reaching this `rename` are inside the temp root. That is a second,
                        //    independent reason rather than a replacement for reason 1: confinement is
                        //    not atomic with the `rename` (see `confined_to`'s TOCTOU note), so it does
                        //    not by itself make this line safe for a real server.
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
                        let Some(dest) = real_path(&root, &arg) else {
                            send(&mut ctrl, CPE_1730_ESCAPED_ROOT_REFUSAL);
                            continue;
                        };
                        // The source, both questions. `same_place` first: `RNFR /` is *contained* (the
                        // root is inside itself), so confinement alone would let the served root be
                        // renamed away — the gap CPE-1731 wrote down and this ticket was opened to close.
                        if cpe_server::fsutil::same_place(&joined_path(&root, &from), &root) {
                            send(&mut ctrl, CPE_1730_ROOT_SOURCE_REFUSAL);
                            continue;
                        }
                        let Some(src) = real_path(&root, &from) else {
                            send(&mut ctrl, CPE_1730_ESCAPED_ROOT_REFUSAL);
                            continue;
                        };
                        #[allow(clippy::disallowed_methods)]
                        match std::fs::rename(src, &dest) {
                            Ok(()) => send(&mut ctrl, "250 Renamed\r\n"),
                            Err(_) => send(&mut ctrl, "550 Rename failed\r\n"),
                        }
                    }
                    None => send(&mut ctrl, "503 RNFR required first\r\n"),
                },
                "SIZE" => match real_path(&root, &arg) {
                    None => send(&mut ctrl, CPE_1730_ESCAPED_ROOT_REFUSAL),
                    Some(p) => match std::fs::metadata(p) {
                        Ok(m) if m.is_file() => send(&mut ctrl, &format!("213 {}\r\n", m.len())),
                        _ => send(&mut ctrl, "550 Could not get size\r\n"),
                    },
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
    ///
    /// **CPE-1782:** `root` is a [`cpe_server::fsutil::ScratchDir`] guard, not a bare `PathBuf` — it used
    /// to be thrown away as a plain path, leaked unconditionally on every one of this function's ~20 call
    /// sites (the same shape `cpe-net`'s `start_server`/`start_streaming_server` had before this ticket).
    /// Returned to the **caller** rather than armed inside this function: the server below runs on a
    /// DETACHED thread with no join, so a guard armed here would delete the server's data directory while
    /// it is still serving. Keep it bound (`let (port, _root) = exact_server();`), not discarded, for as
    /// long as the server needs to keep serving — the same shape CPE-1693 chose for `cpe-webdav`'s and
    /// `cpe-s3`'s detached-thread fixture spawners.
    fn spawn_ftp_server(creds: Creds) -> (u16, ScratchDir) {
        let root = scratch_dir("cpe-ftp-srv");
        let root_path = root.to_path_buf();
        std::fs::create_dir_all(root_path.join(DIR_NAME)).unwrap();
        std::fs::write(root_path.join(FILE_NAME), FILE_BODY).unwrap();
        std::fs::write(root_path.join(DIR_NAME).join("nested.txt"), b"deep").unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { continue };
                let root = root_path.clone();
                let creds = creds.clone();
                std::thread::spawn(move || handle_control(conn, root, creds));
            }
        });
        (port, root)
    }

    fn exact_server() -> (u16, ScratchDir) {
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

    /// CPE-1742's acceptance criteria, plus the two shapes the PR-985 review found missing: `STOR` to a
    /// missing parent must be refused **and must not create anything**; `STOR` to a parent that exists
    /// but is a **file, not a directory**, must be refused too (`Path::is_dir()` catches both — see the
    /// guard's own comment for why `Path::exists()` would not); `STOR` onto a path that is **itself an
    /// existing directory** must be refused; and `STOR` to a genuinely existing parent must still work.
    ///
    /// # The refusal side is checked against the FILESYSTEM, not the client
    ///
    /// **Two different reads, deliberately.** The wire-text assertions read `FtpProvider::write`'s own
    /// error, which is fine for pinning the reply line. But whether the directory actually got created
    /// is asserted against `root.path()` — the rig's own real temp directory, read independently with
    /// plain `std::fs`, per this repo's own convention (`crates/vfs/tests/real_server_conformance.rs`'s
    /// "read it back with something other than the client under test", cited there at its
    /// `assert_mkdir_rename_delete_verified_from_host_disk`). `FtpProvider::stat` is *not* that
    /// independent read: it is `list(parent)` + `parse_list_line`, which deliberately **skips**
    /// unparsable rows (`crates/ftp/src/lib.rs`'s `list`, filtering through `parse_list_line`) — so a
    /// garbled `LIST` line would make `stat` report "not found" even if the directory genuinely exists,
    /// letting this assertion pass for the wrong reason. An earlier draft of this test used
    /// `provider.stat(..).is_err()` for exactly that; PR-985 review caught it.
    ///
    /// # The positive-control side
    ///
    /// Without it, a rig whose `STOR` refuses *everything* (a resolver stub returning `None`, say) would
    /// also pass every refusal row above — the same "vacuous guard" trap CPE-1730's tests document.
    #[test]
    fn stor_refuses_a_missing_parent_and_still_works_for_one_that_exists() {
        let (port, root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");

        // ── Refusal 1: `/nosuchdir` was never created by MKD, so `nosuchdir/file.txt`'s parent is
        // missing. Before CPE-1742 this rig invented `/nosuchdir` via `create_dir_all` and the write
        // silently succeeded — no real FTP daemon does that.
        let err = provider
            .write("/nosuchdir/file.txt", b"should never land")
            .expect_err("STOR into a missing parent must be refused, not silently succeed");
        assert!(
            err.contains("553") && err.contains("Could not create file"),
            "the refusal must be STOR's own `553` line (RFC 959 §5.4), not a `550` borrowed from \
             DELE/RMD/MKD or a generic errno message: {err}"
        );
        assert!(
            !root.path().join("nosuchdir").exists(),
            "the parent chain must NOT be invented — `/nosuchdir` must not exist on the rig's own \
             filesystem after the refusal (checked independently of the client under test)"
        );

        // ── The positive control: an existing parent (the root itself, and a directory made via MKD)
        // must still accept a STOR, bytes and all — no over-rejection.
        provider.write("/at-root.txt", b"root parent exists").expect("STOR to the root must still work");
        assert_eq!(provider.read("/at-root.txt").unwrap(), b"root parent exists");

        provider.mkdir("/madedir").expect("seed an existing parent via MKD");
        provider
            .write("/madedir/inside.txt", b"nested parent exists")
            .expect("STOR into an MKD'd directory must still work");
        assert_eq!(provider.read("/madedir/inside.txt").unwrap(), b"nested parent exists");

        // ── Refusal 2: the parent EXISTS, but as a plain file (`/at-root.txt`, written above) — the
        // shape a `Path::exists()` guard would miss but `Path::is_dir()` catches. No real daemon can
        // create an entry inside a file.
        let err = provider
            .write("/at-root.txt/child.txt", b"should never land either")
            .expect_err("STOR whose parent exists but is a plain file must be refused");
        assert!(
            err.contains("553") && err.contains("Could not create file"),
            "must be refused with the same `553 Could not create file` line: {err}"
        );
        assert!(
            !root.path().join("at-root.txt").is_dir(),
            "`/at-root.txt` must remain the plain file it was — refusing must not turn it into (or \
             replace it with) a directory"
        );

        // ── Refusal 3: the target path is ITSELF an existing directory (`/madedir`, made above via
        // MKD). Before this review round, `fs::write` on a directory failed silently and the rig still
        // answered `226 Transfer complete` — success reported for a write that never happened.
        let err = provider
            .write("/madedir", b"should never land either")
            .expect_err("STOR onto an existing directory must be refused, not silently swallowed");
        assert!(
            err.contains("553") && err.contains("Could not create file"),
            "must be refused on the control channel with the same `553 Could not create file` line, \
             not silently accepted while doing nothing: {err}"
        );
        assert!(
            root.path().join("madedir").is_dir(),
            "`/madedir` must remain a directory — the refused STOR must not have damaged it"
        );
        assert_eq!(
            provider.read("/madedir/inside.txt").unwrap(),
            b"nested parent exists",
            "and its contents from the positive control above must be untouched"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1741 — `cpe_server::transfer::upload_tree` against a remote base/parent/nested-dir that
    // already exists, over the real FTP wire (this rig's honest, non-idempotent `create_dir` MKD arm,
    // per CPE-1731). `FtpProvider` has no `upload_tree` convenience wrapper (unlike `cpe-sftp`), so
    // these call the shared `cpe_server::transfer::upload_tree` directly against the connected
    // provider — the same thing `cpe-vfs`'s callers do.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn upload_tree_succeeds_when_the_remote_base_directory_already_exists() {
        // `/sub` is seeded by `spawn_ftp_server` (with `nested.txt` inside already) — uploading into
        // it must not trip this rig's honest, non-idempotent MKD's 550 refusal.
        let (port, _root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");

        let src = scratch_dir("cpe-ftp-up-exists"); // armed before any assertion
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("a.txt"), b"alpha").unwrap();
        std::fs::write(src.join("inner").join("b.txt"), b"bravo").unwrap();

        let cancel = AtomicBool::new(false);
        let files = cpe_server::transfer::upload_tree(&mut provider, &src, &format!("/{DIR_NAME}"), &cancel)
            .expect("uploading into an already-existing remote base must succeed (CPE-1741)");
        assert_eq!(files, 2);
        assert_eq!(provider.read(&format!("/{DIR_NAME}/a.txt")).unwrap(), b"alpha");
        assert_eq!(provider.read(&format!("/{DIR_NAME}/inner/b.txt")).unwrap(), b"bravo");
        // The pre-existing seeded content is untouched.
        assert_eq!(provider.read(&format!("/{DIR_NAME}/nested.txt")).unwrap(), b"deep");
    }

    #[test]
    fn upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist() {
        let (port, _root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");

        let src = scratch_dir("cpe-ftp-up-multilevel"); // armed before any assertion
        std::fs::write(src.join("z.txt"), b"zed").unwrap();
        // Only a successful MKD chain can produce a directory with no file inside it. Before CPE-1742,
        // this rig's STOR ran `create_dir_all(parent)`, which would have invented a non-empty
        // directory's parents without ever exercising MKD — so an empty directory is what proves MKD
        // ran rather than STOR's now-removed invention.
        std::fs::create_dir(src.join("empty")).unwrap();

        // Neither "/new" nor "/new/deep" exists on the served root yet — a bare (non-recursive) MKD
        // "/new/deep" would fail 550.
        let cancel = AtomicBool::new(false);
        let files = cpe_server::transfer::upload_tree(&mut provider, &src, "/new/deep", &cancel)
            .expect("a multi-level remote_root with missing parents must succeed (CPE-1741)");
        assert_eq!(files, 1);
        assert_eq!(provider.read("/new/deep/z.txt").unwrap(), b"zed");
        assert!(
            provider.stat("/new/deep/empty").unwrap().is_dir,
            "the MKD chain, not STOR's create_dir_all, must have made these"
        );
    }

    #[test]
    fn upload_tree_succeeds_when_a_nested_directory_already_exists_remotely() {
        // The partial-re-upload case: a directory INSIDE the tree (not just the base) already exists
        // remotely — transfer.rs:761's own CPE-1741 shape, shadowed until the base-level (744) fix
        // was in place.
        let (port, _root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");

        let src = scratch_dir("cpe-ftp-up-partial"); // armed before any assertion
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("inner").join("c.txt"), b"charlie").unwrap();

        provider.mkdir("/up2").expect("seed the base");
        provider.mkdir("/up2/inner").expect("seed: 'inner' already exists remotely before the upload");

        let cancel = AtomicBool::new(false);
        let files = cpe_server::transfer::upload_tree(&mut provider, &src, "/up2", &cancel)
            .expect("re-uploading over an already-existing nested dir must succeed (CPE-1741)");
        assert_eq!(files, 1);
        assert_eq!(provider.read("/up2/inner/c.txt").unwrap(), b"charlie");
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

    /// CPE-1731 (reviewer's find): `MKD` is `mkdir(2)`, so it creates **one** directory — a missing
    /// parent is refused rather than invented, and an existing name is refused rather than reported
    /// created.
    ///
    /// Same argument as `cpe_1731_rmd_refuses_a_non_empty_directory_and_leaves_it_intact`, applied to
    /// the verb four lines away from it in `handle_control`. It was found by the reviewer noticing that
    /// this PR wrote the argument down and then walked past the sibling.
    ///
    /// # The missing-parent row is the one with real filesystem evidence, and it is not vacuous
    /// `!root.join("nope").exists()` is a **negative** assertion, which passes for free if the path is
    /// wrong — the trap a reviewer found elsewhere in this sprint. So the same path string is
    /// independently established as creatable: after `MKD /nope` succeeds, `MKD /nope/deeper` must
    /// succeed too. That proves the earlier refusal was about the absent parent and not about a name
    /// the rig could never have created, and it proves `real_path` maps this string where the test
    /// thinks it does.
    ///
    /// # The already-exists row's filesystem assertion cannot fail today, and says so
    /// `create_dir_all` on an existing directory destroys nothing, so the observable defect there is
    /// the *reply*: `257 "…" created` for a directory the server did not create. The contents check is
    /// kept as the cheap thing that goes red if that ever stops being true.
    #[test]
    fn cpe_1731_mkd_creates_one_directory_and_refuses_a_missing_parent_or_an_existing_name() {
        let (port, root) = exact_server();
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");

        // ── A missing parent is refused, and NOT invented.
        let r = provider.mkdir("/nope/deeper");
        assert!(
            !root.join("nope").exists(),
            "`MKD` must not invent the parent chain — RFC 959 gives it one directory (mkdir reported \
             {r:?})"
        );
        assert!(r.is_err(), "and the client must be told so, not handed a 257 for a chain it created");

        // ── …and the very same path is creatable once its parent exists, so the negative above was
        // about the missing parent rather than about an unmappable name.
        provider.mkdir("/nope").expect("MKD must create one directory");
        assert!(root.join("nope").is_dir(), "the parent must now exist on disk");
        provider.mkdir("/nope/deeper").expect("MKD must succeed once the parent is there");
        assert!(
            root.join("nope").join("deeper").is_dir(),
            "and the child must land exactly where the refused call named"
        );

        // ── An existing name is refused rather than reported created.
        let r = provider.mkdir(&format!("/{DIR_NAME}"));
        assert_eq!(
            std::fs::read(root.join(DIR_NAME).join("nested.txt")).ok().as_deref(),
            Some(&b"deep"[..]),
            "the existing directory's contents must be untouched (mkdir reported {r:?})"
        );
        assert!(
            r.is_err(),
            "a real daemon answers 550 for a name that already exists; reporting `257 \"…\" created` \
             for a directory it did not create is the fiction this ticket is about"
        );
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

    // ---------------------------------------------------------------------------------------------
    // CPE-1730 — the request path that resolves OUTSIDE the served root
    // ---------------------------------------------------------------------------------------------

    /// Log in over a raw control connection, send `cmds` in order, and return the reply to the last one.
    ///
    /// Raw because the assertion that matters is the **exact refusal line**, and `FtpProvider` wraps the
    /// server's reply in `format!("{path}: {e}")` — a string built partly from the caller's own path, so
    /// a `contains` over it is forgeable by naming a file after the refusal. That is not hypothetical:
    /// CPE-1731's UAT did exactly that to its sibling assertion. On the control channel the rig's line is
    /// the whole reply and nothing else can put text there.
    ///
    /// Only control-only verbs may be sent through this (`DELE`/`RMD`/`MKD`/`SIZE`/`RNFR`/`RNTO`). A data
    /// verb would leave the rig blocked in `listener.accept()` waiting for a connection this helper never
    /// makes — and it would do so *only when the guard is neutralised*, turning a red into a hang.
    fn raw_commands(port: u16, cmds: &[&str]) -> String {
        let mut sock = TcpStream::connect(("127.0.0.1", port)).expect("connect to the rig");
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
        let mut last = String::new();
        for c in cmds {
            last = reply(&mut sock, Some(c));
        }
        let _ = reply(&mut sock, Some("QUIT"));
        last
    }

    const VICTIM_BYTES: &[u8] = b"bytes belonging to somebody outside the served root";

    /// A directory **outside** the served root, holding a victim file with known bytes, plus the wire
    /// spelling that reaches it from the root by `..`. Returns `(victim_dir, victim_file, wire_prefix)`.
    ///
    /// Deliberately a real sibling of the rig's own temp root rather than a fabricated string: the point
    /// of the ticket is that `root.join("../…")` lands on somebody's real files, so the test has to put
    /// real bytes there and read them back afterwards.
    ///
    /// **CPE-1782:** `victim_dir` is a [`ScratchDir`] guard (via [`ScratchDir::adopt`], since this
    /// directory's name is derived from the caller's root rather than `scratch_dir`'s own
    /// `<prefix>-<pid>-<seq>` scheme) — the same second-level-helper fix as `cpe-sftp`'s sibling. Both
    /// callers below run many assertions between this call and their old trailing `remove_dir_all`, which
    /// never ran on a failing assertion; the guard now covers that path.
    fn seed_victim_outside(root: &Path) -> (ScratchDir, PathBuf, String) {
        let name = format!(
            "{}-cpe-1730-victim",
            root.file_name().expect("the rig root has a name").to_string_lossy()
        );
        let dir = root.parent().expect("the rig root has a parent").join(&name);
        std::fs::create_dir_all(&dir).expect("create the victim directory");
        let file = dir.join("victim.txt");
        std::fs::write(&file, VICTIM_BYTES).expect("seed the victim file");
        (ScratchDir::adopt(dir), file, format!("/../{name}"))
    }

    /// CPE-1730 acceptance: a request path resolving **outside** the served root is refused, and the
    /// file outside is still there with its bytes.
    ///
    /// # The escape shapes, enumerated
    ///
    /// Naming only `..` would imply the other two are handled, and they are different mechanisms:
    /// 1. **`..`-shaped** — `root.join("../x")` walks out of the tree. Reachable on every platform, and
    ///    what this test drives.
    /// 2. **absolute** — `Path::join` *discards* the base when handed an absolute path, so the
    ///    destination replaces the root outright with no `..` anywhere. Windows-only through this rig
    ///    (`real_path` trims the leading `/`, so on POSIX an absolute wire path becomes relative and
    ///    lands *inside* the root — measured, and the reason CPE-1731's family-3 test is
    ///    `#[cfg(windows)]` too); covered by `cpe_1730_an_absolute_request_path_cannot_replace_the_root`.
    /// 3. **through a symlinked intermediate directory** — needs neither `..` nor an absolute path, and
    ///    is invisible to any textual check. The leg at the end of this test seeds the link with
    ///    `std::fs` (nothing on this wire lets a client create one) and then drives an ordinary-looking
    ///    path through it.
    ///
    /// # Every leg asserts the filesystem BEFORE it looks at the reply
    ///
    /// The bug's shape is *success reported for an escape that happened*, so an assertion that unwraps
    /// the outcome first is unreachable in exactly the failing case. Each leg therefore reads the victim
    /// back first and mentions the outcome only inside the message.
    ///
    /// # And the reply assertion pins the WHOLE line, not the code
    ///
    /// `550` alone would be satisfied by six other replies this rig sends (`Delete failed`,
    /// `Remove failed`, …), each of which an ordinary errno produces — the "saved by an errno" trap that
    /// has bitten this ticket family three times. The full line is emitted from one constant and no
    /// `io::Error` text ever reaches this wire, so it is reachable from the confinement guard alone.
    #[test]
    fn cpe_1730_a_request_path_that_escapes_the_served_root_is_refused() {
        let (port, root) = exact_server();
        let (victim_dir, victim, out) = seed_victim_outside(&root);
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");
        let refusal = CPE_1730_ESCAPED_ROOT_REFUSAL.trim_end_matches("\r\n");

        // ── STOR: the write that would clobber somebody else's file. Driven through the provider (it
        // performs the PASV dance), so a neutralised guard fails the byte assertion rather than hanging.
        let outcome = provider.write(&format!("{out}/victim.txt"), b"PWNED");
        assert_eq!(
            std::fs::read(&victim).ok().as_deref(),
            Some(VICTIM_BYTES),
            "STOR to {out}/victim.txt must NOT reach a file outside the served root (outcome was \
             {outcome:?})"
        );

        // ── RETR: the read that would leak it back out.
        let leaked = provider.read(&format!("{out}/victim.txt"));
        assert_ne!(
            leaked.as_deref().ok(),
            Some(VICTIM_BYTES),
            "RETR must not serve a file outside the served root"
        );

        // ── DELE / RMD / MKD / SIZE: control-only, so the exact refusal line is asserted too.
        let dele = raw_commands(port, &[&format!("DELE {out}/victim.txt")]);
        assert!(victim.is_file(), "DELE must not delete outside the served root (reply {dele:?})");
        assert_eq!(dele.trim_end(), refusal, "DELE's refusal must be the confinement guard's own line");

        let rmd = raw_commands(port, &[&format!("RMD {out}")]);
        assert!(victim_dir.is_dir(), "RMD must not remove a directory outside the served root (reply {rmd:?})");
        assert_eq!(rmd.trim_end(), refusal, "RMD's refusal must be the confinement guard's own line");

        let mkd = raw_commands(port, &[&format!("MKD {out}/planted")]);
        assert!(!victim_dir.join("planted").exists(), "MKD must not create outside the root (reply {mkd:?})");
        assert_eq!(mkd.trim_end(), refusal, "MKD's refusal must be the confinement guard's own line");

        let size = raw_commands(port, &[&format!("SIZE {out}/victim.txt")]);
        assert_eq!(size.trim_end(), refusal, "SIZE must not stat outside the served root");

        // ── RNTO: the shape CPE-1731's UAT measured moving the served root out of the served area and
        // answering `250 Renamed`. The destination escapes; the source is an ordinary file.
        let rnto = raw_commands(port, &[&format!("RNFR /{FILE_NAME}"), &format!("RNTO {out}/victim.txt")]);
        assert_eq!(
            std::fs::read(&victim).ok().as_deref(),
            Some(VICTIM_BYTES),
            "RNTO must not rename onto a file outside the served root (reply {rnto:?})"
        );
        assert!(root.join(FILE_NAME).is_file(), "…and the source must be left alone (reply {rnto:?})");
        assert_eq!(rnto.trim_end(), refusal, "RNTO's refusal must be the confinement guard's own line");

        // ── RNFR: the SOURCE side, which CPE-1731 left open and recorded. An escaping source would move
        // somebody else's file *into* the served tree.
        let rnfr = raw_commands(port, &[&format!("RNFR {out}/victim.txt"), "RNTO /stolen.txt"]);
        assert!(victim.is_file(), "RNFR must not move a file INTO the served root from outside (reply {rnfr:?})");
        assert!(!root.join("stolen.txt").exists(), "…and nothing may appear at the destination (reply {rnfr:?})");
        assert_eq!(rnfr.trim_end(), refusal, "the escaping source's refusal must be the guard's own line");

        // ── Escape family 3: a path THROUGH a symlinked subdirectory. No `..`, nothing absolute — an
        // ordinary-looking `/outlink/victim.txt`. Seeded with `std::fs` because no verb on this wire
        // creates a link; that is why the shape is latent rather than live, and why it is still the one
        // a textual guard cannot see.
        let link = root.join("outlink");
        if cpe_server::fsutil::make_dir_link(&victim_dir, &link) {
            let through = provider.write("/outlink/victim.txt", b"PWNED VIA LINK");
            assert_eq!(
                std::fs::read(&victim).ok().as_deref(),
                Some(VICTIM_BYTES),
                "a STOR through a SYMLINKED subdirectory leaves the served tree with no `..` and no \
                 absolute component — the shape no textual check can see (outcome was {through:?})"
            );
            let dele_through = raw_commands(port, &["DELE /outlink/victim.txt"]);
            assert!(victim.is_file(), "…and neither may DELE (reply {dele_through:?})");
            assert_eq!(dele_through.trim_end(), refusal, "…refused by the confinement guard, by its own line");
        } else {
            cpe_server::skip_notice!(
                "[CPE-1730] SKIPPED the symlinked-subdirectory leg of cpe-ftp's confinement test: could \
                 not create a directory link at {}. What is NOT covered on this run is escape family 3 \
                 (a path through a symlinked intermediate directory); family 1 (`..`) above still ran.",
                link.display()
            );
        }

        // ── The positive control. Without it every refusal above is satisfied by a rig that does
        // nothing at all — including one whose resolver returns `None` for every path.
        provider.write("/ordinary.txt", b"ordinary").expect("an ordinary write must still succeed");
        assert_eq!(std::fs::read(root.join("ordinary.txt")).unwrap(), b"ordinary");
        assert_eq!(provider.read(&format!("/{FILE_NAME}")).expect("an ordinary read"), FILE_BODY);
    }

    /// Escape family 2: an **absolute** request path, which replaces the served root outright because
    /// `Path::join` discards its base — no `..` involved anywhere.
    ///
    /// **Windows-only, measured rather than assumed** (this ticket's probe, and CPE-1731's before it):
    /// `real_path` trims the leading `/` first, so on POSIX `/tmp/x/victim.txt` becomes the *relative*
    /// `tmp/x/victim.txt` and lands **inside** the root — measured in Docker `rust:1-slim`:
    /// `resolver("/tmp/…/sibling/victim.txt") -> "/tmp/…/root/tmp/…/sibling/victim.txt"`. There is
    /// nothing to catch there, so a cross-platform version of this test would assert a refusal that no
    /// platform-independent defect produces. On Windows a `C:\…` path survives the trim intact.
    #[cfg(windows)]
    #[test]
    fn cpe_1730_an_absolute_request_path_cannot_replace_the_root() {
        let (port, root) = exact_server();
        let (_victim_dir, victim, _) = seed_victim_outside(&root);
        let cfg = FtpConfig::password("127.0.0.1", port, "user", "pw");
        let mut provider = FtpProvider::connect(&cfg).expect("connect");
        let absolute = victim.display().to_string();
        assert!(
            Path::new(&absolute).is_absolute() && !absolute.starts_with('/'),
            "the fixture must be an absolute path that survives `trim_start_matches('/')`, or this \
             test drives family 1 by accident: {absolute}"
        );

        let outcome = provider.write(&absolute, b"PWNED ABSOLUTELY");
        assert_eq!(
            std::fs::read(&victim).ok().as_deref(),
            Some(VICTIM_BYTES),
            "an absolute STOR path must not replace the served root (outcome was {outcome:?})"
        );
        let dele = raw_commands(port, &[&format!("DELE {absolute}")]);
        assert!(victim.is_file(), "…and neither may an absolute DELE (reply {dele:?})");
        assert_eq!(
            dele.trim_end(),
            CPE_1730_ESCAPED_ROOT_REFUSAL.trim_end_matches("\r\n"),
            "the refusal must be the confinement guard's own line, not an errno's 550"
        );
    }

    /// The gap CPE-1731 wrote down at its own guard and could not close: `RNFR /` + `RNTO /elsewhere`
    /// renamed **the served root itself** into a subdirectory and answered `250 Renamed`.
    ///
    /// It is here, in the containment ticket, because CPE-1731's note said a source guard "needs the
    /// containment check CPE-1730 is opening" — and the interesting finding is that **containment is not
    /// what closes it**. The root is contained in itself by design (a resolver must map `/` somewhere, or
    /// `LIST /` cannot work), so `confined_to` says yes; `same_place` on the *source* is what says no.
    /// The note's expectation was reasonable and wrong, which is worth more written down than quietly
    /// corrected.
    ///
    /// # The recorded gap was not actually destructive, and the neutralisation run is what showed it
    ///
    /// CPE-1731's note says this shape "still renames the served root itself into a subdirectory". With
    /// the source guard neutralised, it does not — **measured, Windows**:
    ///
    /// ```text
    /// RNFR / + RNTO /moved-root, source guard neutralised:
    ///   reply = "550 Rename failed"          ← an errno, not the guard
    ///   the served root and its contents: intact
    /// ```
    ///
    /// Every destination the *confinement* guard still allows is inside the root, and no filesystem
    /// renames a directory into its own subtree. So the tree assertions below **cannot fail today** —
    /// stated rather than implied, because a test whose filesystem assertions are vacuous and whose
    /// author has not noticed is the failure mode this ticket family keeps rediscovering. What carries
    /// this test is the **exact refusal line**: with the guard gone the rig answers `550 Rename failed`,
    /// which is byte-for-byte what a `rename` errno produces, and an assertion on the code alone would
    /// have stayed green. That is the "saved by an errno" trap in its purest form, and it is why the
    /// guard is still worth having: it turns a refusal that happens to be an OS accident into one the
    /// server states.
    #[test]
    fn cpe_1730_an_rnfr_naming_the_served_root_cannot_move_it_away() {
        let (port, root) = exact_server();
        let reply = raw_commands(port, &["RNFR /", "RNTO /moved-root"]);
        assert!(
            root.join(FILE_NAME).is_file() && root.join(DIR_NAME).join("nested.txt").is_file(),
            "the served root must still be the served root, with its contents (reply {reply:?})"
        );
        assert!(
            !root.join("moved-root").exists(),
            "…and nothing may have been created at the destination (reply {reply:?})"
        );
        assert_eq!(
            reply.trim_end(),
            CPE_1730_ROOT_SOURCE_REFUSAL.trim_end_matches("\r\n"),
            "the refusal must name the source guard specifically. `553` would mean CPE-1731's \
             destination guard answered, and a bare `550` would mean an errno did"
        );
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
