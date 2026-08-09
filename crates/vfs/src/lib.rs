//! Scheme router (epic CPE-616/CPE-1502): open the right [`FileSystemProvider`] for a saved connection.
//!
//! The connections model ([`cpe_server::connections`]) stores non-secret profiles; the actual password /
//! key-passphrase comes from the OS keychain at connect time. This crate is the seam that turns a
//! `(Connection, secret)` into a live, boxed provider — dispatching by scheme to `cpe-sftp`, `cpe-webdav`,
//! or `cpe-ftp` (CPE-1514, the first net-new Network protocol). The app calls [`open`] with the secret it
//! fetched from the keychain.

use cpe_ftp::{FtpAuth, FtpConfig, FtpProvider};
use cpe_server::connections::{AuthMethod, Connection};
use cpe_server::known_hosts::KnownHost;
use cpe_server::provider::FileSystemProvider;
use cpe_sftp::{SftpAuth, SftpConfig, SftpProvider};
use cpe_webdav::{WebdavConfig, WebdavProvider};

/// The command-layer routing seam (CPE-1511): a per-connection provider pool + `connected_provider`, so
/// a remote URI browses through the same commands a local path does. Kept in this crate because it is the
/// only place that can see both the connection model (`cpe-server`) and the concrete providers this
/// module opens.
pub mod connect;

/// Re-export so the app adapter can name the host-key policy without a direct `cpe-sftp` dependency —
/// `open`/`connect` take it, and the app only depends on `cpe-vfs`.
pub use cpe_sftp::HostKeyPolicy;

/// A live provider that is safe to move to a blocking worker thread and hold in the shared pool. Every
/// concrete backend ([`cpe_sftp::SftpProvider`], [`cpe_webdav::WebdavProvider`], [`cpe_ftp::FtpProvider`],
/// [`cpe_server::provider::LocalProvider`]) is `Send`.
pub type BoxedProvider = Box<dyn FileSystemProvider + Send>;

/// Open a live [`FileSystemProvider`] for `conn`, using `secret` (the password, or a key's passphrase)
/// fetched from the OS keychain. `known_hosts` + `policy` govern SFTP host-key verification (ignored for
/// WebDAV and FTP). Errors carry the scheme/host context.
pub fn open(
    conn: &Connection,
    secret: Option<&str>,
    known_hosts: Vec<KnownHost>,
    policy: HostKeyPolicy,
) -> Result<BoxedProvider, String> {
    match conn.scheme.as_str() {
        "sftp" | "ssh" => {
            let cfg = SftpConfig {
                host: conn.host.clone(),
                port: conn.port,
                user: conn.user.clone(),
                auth: sftp_auth_from(conn, secret)?,
            };
            Ok(Box::new(SftpProvider::connect(&cfg, known_hosts, policy)?))
        }
        "webdav" | "davs" | "dav" => {
            let mut cfg = WebdavConfig::new(webdav_base_url(conn));
            if !conn.user.is_empty() {
                cfg = cfg.with_basic_auth(&conn.user, secret.unwrap_or(""));
            }
            Ok(Box::new(WebdavProvider::connect(&cfg)))
        }
        "ftp" | "ftps" => {
            let cfg = FtpConfig {
                host: conn.host.clone(),
                port: conn.port,
                user: conn.user.clone(),
                auth: ftp_auth_from(conn, secret),
                tls: conn.scheme == "ftps",
            };
            Ok(Box::new(FtpProvider::connect(&cfg)?))
        }
        other => Err(format!("vfs: unsupported scheme '{other}'")),
    }
}

/// The FTP auth for a connection: Anonymous when the profile's user is blank or literally `"anonymous"`
/// (case-insensitive) — the common shape for a public FTP mirror/archive — else a password login using the
/// keychain secret. FTP has no key-based auth ([`AuthMethod::Key`] never applies here), so unlike
/// [`sftp_auth_from`] this doesn't need to branch on `conn.auth` at all; CPE-1514 handles Anonymous
/// directly rather than waiting on CPE-1501's broader auth-model epic (see `cpe-ftp`'s module docs).
fn ftp_auth_from(conn: &Connection, secret: Option<&str>) -> FtpAuth {
    if conn.user.is_empty() || conn.user.eq_ignore_ascii_case("anonymous") {
        FtpAuth::Anonymous
    } else {
        FtpAuth::Password(secret.unwrap_or("").to_string())
    }
}

/// Build the SFTP auth method from a connection + its secret: a password, or a private key **read from
/// `key_path`** with `secret` as its passphrase.
fn sftp_auth_from(conn: &Connection, secret: Option<&str>) -> Result<SftpAuth, String> {
    match &conn.auth {
        AuthMethod::Password => Ok(SftpAuth::Password(secret.unwrap_or("").to_string())),
        AuthMethod::Key { key_path } => {
            let pem = std::fs::read_to_string(key_path).map_err(|e| format!("{key_path}: {e}"))?;
            Ok(SftpAuth::PrivateKey { pem, passphrase: secret.map(str::to_string) })
        }
    }
}

/// The WebDAV base URL for a connection: `davs` → `https`, else `http`; `host:port` + the optional path.
fn webdav_base_url(conn: &Connection) -> String {
    let scheme = if conn.scheme == "davs" { "https" } else { "http" };
    let path = conn.path.as_deref().unwrap_or("");
    format!("{scheme}://{}:{}{}", conn.host, conn.port, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(scheme: &str, auth: AuthMethod) -> Connection {
        Connection {
            name: "t".into(),
            scheme: scheme.into(),
            host: "host.example.com".into(),
            port: 2222,
            user: "me".into(),
            auth,
            path: Some("/dav".into()),
        }
    }

    #[test]
    fn an_unsupported_scheme_is_a_clear_error() {
        let err = match open(&conn("s3", AuthMethod::Password), None, vec![], HostKeyPolicy::Tofu) {
            Ok(_) => panic!("s3 must be unsupported"),
            Err(e) => e,
        };
        assert!(err.contains("unsupported scheme 's3'"), "got: {err}");
    }

    #[test]
    fn webdav_base_url_maps_scheme_host_port_path() {
        assert_eq!(webdav_base_url(&conn("webdav", AuthMethod::Password)), "http://host.example.com:2222/dav");
        assert_eq!(webdav_base_url(&conn("davs", AuthMethod::Password)), "https://host.example.com:2222/dav");
        let mut c = conn("webdav", AuthMethod::Password);
        c.path = None;
        assert_eq!(webdav_base_url(&c), "http://host.example.com:2222");
    }

    #[test]
    fn sftp_auth_from_password_and_key() {
        // Password → SftpAuth::Password.
        assert!(matches!(
            sftp_auth_from(&conn("sftp", AuthMethod::Password), Some("pw")).unwrap(),
            SftpAuth::Password(p) if p == "pw"
        ));
        // Key → reads the PEM at key_path; secret is the passphrase.
        let dir = std::env::temp_dir().join(format!("cpe-vfs-key-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let key = dir.join("id");
        std::fs::write(&key, "PEM-CONTENT").unwrap();
        let c = conn("sftp", AuthMethod::Key { key_path: key.to_string_lossy().into_owned() });
        match sftp_auth_from(&c, Some("pass")).unwrap() {
            SftpAuth::PrivateKey { pem, passphrase } => {
                assert_eq!(pem, "PEM-CONTENT");
                assert_eq!(passphrase.as_deref(), Some("pass"));
            }
            other => panic!("expected PrivateKey, got {other:?}"),
        }
        // A missing key file is a clear error.
        let missing = conn("sftp", AuthMethod::Key { key_path: "/no/such/key".into() });
        assert!(sftp_auth_from(&missing, None).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_dispatches_sftp_to_the_sftp_provider() {
        // Port 1 has nothing listening → SFTP connect fails; the error proves we routed to SFTP (not a
        // "unsupported scheme"), exercising the dispatch + config path end to end.
        let mut c = conn("sftp", AuthMethod::Password);
        c.host = "127.0.0.1".into();
        c.port = 1;
        let err = match open(&c, Some("pw"), vec![], HostKeyPolicy::Tofu) {
            Ok(_) => panic!("connect to a dead port must fail"),
            Err(e) => e,
        };
        assert!(err.starts_with("sftp"), "expected an SFTP-flavoured error, got: {err}");
    }

    #[test]
    fn open_builds_a_webdav_provider_lazily() {
        // WebdavProvider::connect is lazy (no request), so routing a webdav connection succeeds without a
        // server; a later op would surface a connection error.
        let c = conn("webdav", AuthMethod::Password);
        assert!(open(&c, Some("pw"), vec![], HostKeyPolicy::Tofu).is_ok());
    }

    #[test]
    fn open_dispatches_ftp_and_ftps_to_the_ftp_provider() {
        // Port 1 has nothing listening → FTP connect fails; the error proves we routed to FTP (not an
        // "unsupported scheme"), exercising the dispatch + config path end to end, for both scheme words.
        for scheme in ["ftp", "ftps"] {
            let mut c = conn(scheme, AuthMethod::Password);
            c.host = "127.0.0.1".into();
            c.port = 1;
            let err = match open(&c, Some("pw"), vec![], HostKeyPolicy::Tofu) {
                Ok(_) => panic!("connect to a dead port must fail ({scheme})"),
                Err(e) => e,
            };
            assert!(err.starts_with("ftp"), "expected an FTP-flavoured error for {scheme}, got: {err}");
        }
    }

    #[test]
    fn ftp_auth_from_picks_anonymous_or_password() {
        // A blank user, or literally "anonymous" (any case), means Anonymous — no keychain secret needed.
        let mut c = conn("ftp", AuthMethod::Password);
        c.user = String::new();
        assert!(matches!(ftp_auth_from(&c, None), FtpAuth::Anonymous));
        c.user = "Anonymous".into();
        assert!(matches!(ftp_auth_from(&c, Some("ignored")), FtpAuth::Anonymous));

        // A real username uses the keychain secret as the password.
        c.user = "deploy".into();
        assert!(matches!(ftp_auth_from(&c, Some("pw")), FtpAuth::Password(p) if p == "pw"));
    }
}
