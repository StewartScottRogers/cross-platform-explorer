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
/// WebDAV and FTP). `record_first_contact` is the app-managed `known_hosts` store path (CPE-1512): for
/// SFTP, a first-contact (`Unknown`) host key is persisted there so a later connect to the same host
/// resolves `Trusted` (or `Changed` → refused, on a key swap) — `None` skips persistence (e.g. no app
/// config dir on this platform) without failing the connect. Errors carry the scheme/host context.
pub fn open(
    conn: &Connection,
    secret: Option<&str>,
    known_hosts: Vec<KnownHost>,
    policy: HostKeyPolicy,
    record_first_contact: Option<&std::path::Path>,
) -> Result<BoxedProvider, String> {
    match conn.scheme.as_str() {
        "sftp" | "ssh" => {
            let cfg = SftpConfig {
                host: conn.host.clone(),
                port: conn.port,
                user: conn.user.clone(),
                auth: sftp_auth_from(conn, secret)?,
            };
            Ok(Box::new(SftpProvider::connect_and_record(&cfg, known_hosts, policy, record_first_contact)?))
        }
        "webdav" | "davs" | "dav" => {
            let cfg = webdav_auth_from(WebdavConfig::new(webdav_base_url(conn)), conn, secret)?;
            Ok(Box::new(WebdavProvider::connect(&cfg)))
        }
        "ftp" | "ftps" => {
            let cfg = FtpConfig {
                host: conn.host.clone(),
                port: conn.port,
                user: conn.user.clone(),
                auth: ftp_auth_from(conn, secret)?,
                tls: conn.scheme == "ftps",
            };
            Ok(Box::new(FtpProvider::connect(&cfg)?))
        }
        other => Err(format!("vfs: unsupported scheme '{other}'")),
    }
}

/// The FTP auth for a connection. [`AuthMethod::Anonymous`] is first-class as of CPE-1515 and maps
/// directly to [`FtpAuth::Anonymous`]. For [`AuthMethod::Password`]/[`AuthMethod::Key`] (FTP has no
/// key-based auth, so `Key` is treated the same as `Password` here), the CPE-1514 heuristic still applies
/// for connections saved before `Anonymous` existed: a blank or literal `"anonymous"` username
/// (case-insensitive — the common shape for a public FTP mirror/archive) is still treated as anonymous,
/// so an old saved connection keeps working unchanged. `Token`/`AccessKey` are cloud-only auth kinds FTP
/// doesn't support — a connection profile combining `ftp`/`ftps` with either is a clear configuration
/// error, not a silent fallback.
fn ftp_auth_from(conn: &Connection, secret: Option<&str>) -> Result<FtpAuth, String> {
    match &conn.auth {
        AuthMethod::Anonymous => Ok(FtpAuth::Anonymous),
        AuthMethod::Token { .. } => {
            Err("ftp: token auth is not supported by this provider — reserved for a future cloud provider".into())
        }
        AuthMethod::AccessKey { .. } => Err(
            "ftp: access-key auth is not supported by this provider — reserved for a future S3/cloud provider".into(),
        ),
        AuthMethod::Password | AuthMethod::Key { .. } => {
            if conn.user.is_empty() || conn.user.eq_ignore_ascii_case("anonymous") {
                Ok(FtpAuth::Anonymous)
            } else {
                Ok(FtpAuth::Password(secret.unwrap_or("").to_string()))
            }
        }
    }
}

/// Build the SFTP auth method from a connection + its secret: a password, or a private key **read from
/// `key_path`** with `secret` as its passphrase. [`AuthMethod::Anonymous`] has no true SFTP/SSH
/// equivalent (the protocol always authenticates a username), so it attempts a password login with an
/// **empty** password — the conventional shape for the rare anonymous/public SFTP endpoint; `conn.user`
/// still carries whatever username the profile specifies. `Token`/`AccessKey` are cloud-only auth kinds
/// SFTP doesn't support — a clear error rather than a silent, wrong connect attempt.
fn sftp_auth_from(conn: &Connection, secret: Option<&str>) -> Result<SftpAuth, String> {
    match &conn.auth {
        AuthMethod::Password => Ok(SftpAuth::Password(secret.unwrap_or("").to_string())),
        AuthMethod::Key { key_path } => {
            let pem = std::fs::read_to_string(key_path).map_err(|e| format!("{key_path}: {e}"))?;
            Ok(SftpAuth::PrivateKey { pem, passphrase: secret.map(str::to_string) })
        }
        AuthMethod::Anonymous => Ok(SftpAuth::Password(String::new())),
        AuthMethod::Token { .. } => {
            Err("sftp: token auth is not supported by this provider — reserved for a future cloud provider".into())
        }
        AuthMethod::AccessKey { .. } => Err(
            "sftp: access-key auth is not supported by this provider — reserved for a future S3/cloud provider".into(),
        ),
    }
}

/// Apply a connection's auth to a WebDAV config. [`AuthMethod::Anonymous`] (and the pre-existing
/// behaviour for `Password`/`Key` with a blank `user`) sends no `Authorization` header at all — many
/// WebDAV shares allow unauthenticated read access. `Token`/`AccessKey` are cloud-only auth kinds WebDAV
/// (as modeled here — HTTP Basic only) doesn't support — a clear error rather than silently dropping the
/// credential.
fn webdav_auth_from(cfg: WebdavConfig, conn: &Connection, secret: Option<&str>) -> Result<WebdavConfig, String> {
    match &conn.auth {
        AuthMethod::Anonymous => Ok(cfg),
        AuthMethod::Token { .. } => Err(
            "webdav: token auth is not supported by this provider — reserved for a future cloud provider".into(),
        ),
        AuthMethod::AccessKey { .. } => Err(
            "webdav: access-key auth is not supported by this provider — reserved for a future S3/cloud provider"
                .into(),
        ),
        AuthMethod::Password | AuthMethod::Key { .. } => {
            if conn.user.is_empty() {
                Ok(cfg)
            } else {
                Ok(cfg.with_basic_auth(&conn.user, secret.unwrap_or("")))
            }
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
        let err = match open(&conn("s3", AuthMethod::Password), None, vec![], HostKeyPolicy::Tofu, None) {
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
        let err = match open(&c, Some("pw"), vec![], HostKeyPolicy::Tofu, None) {
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
        assert!(open(&c, Some("pw"), vec![], HostKeyPolicy::Tofu, None).is_ok());
    }

    #[test]
    fn open_dispatches_ftp_and_ftps_to_the_ftp_provider() {
        // Port 1 has nothing listening → FTP connect fails; the error proves we routed to FTP (not an
        // "unsupported scheme"), exercising the dispatch + config path end to end, for both scheme words.
        for scheme in ["ftp", "ftps"] {
            let mut c = conn(scheme, AuthMethod::Password);
            c.host = "127.0.0.1".into();
            c.port = 1;
            let err = match open(&c, Some("pw"), vec![], HostKeyPolicy::Tofu, None) {
                Ok(_) => panic!("connect to a dead port must fail ({scheme})"),
                Err(e) => e,
            };
            assert!(err.starts_with("ftp"), "expected an FTP-flavoured error for {scheme}, got: {err}");
        }
    }

    #[test]
    fn ftp_auth_from_picks_anonymous_or_password() {
        // A blank user, or literally "anonymous" (any case), means Anonymous — no keychain secret needed.
        // This is the CPE-1514 heuristic, still honoured for a `Password`-auth profile saved before
        // `AuthMethod::Anonymous` existed.
        let mut c = conn("ftp", AuthMethod::Password);
        c.user = String::new();
        assert!(matches!(ftp_auth_from(&c, None).unwrap(), FtpAuth::Anonymous));
        c.user = "Anonymous".into();
        assert!(matches!(ftp_auth_from(&c, Some("ignored")).unwrap(), FtpAuth::Anonymous));

        // A real username uses the keychain secret as the password.
        c.user = "deploy".into();
        assert!(matches!(ftp_auth_from(&c, Some("pw")).unwrap(), FtpAuth::Password(p) if p == "pw"));
    }

    #[test]
    fn ftp_auth_from_maps_the_first_class_anonymous_variant() {
        // Even with a non-blank, non-"anonymous" username, the explicit AuthMethod::Anonymous wins.
        let mut c = conn("ftp", AuthMethod::Anonymous);
        c.user = "someone".into();
        assert!(matches!(ftp_auth_from(&c, None).unwrap(), FtpAuth::Anonymous));
    }

    #[test]
    fn ftp_auth_from_rejects_cloud_only_auth_kinds() {
        let token = conn("ftp", AuthMethod::Token { token_ref: "t".into() });
        assert!(ftp_auth_from(&token, None).unwrap_err().contains("token"));
        let ak = conn("ftp", AuthMethod::AccessKey { id: "AKIA".into(), secret_ref: "s".into() });
        assert!(ftp_auth_from(&ak, None).unwrap_err().contains("access-key"));
    }

    #[test]
    fn sftp_auth_from_maps_anonymous_and_rejects_cloud_only_auth_kinds() {
        // Anonymous → an empty-password attempt (SFTP has no true anonymous mechanism).
        assert!(matches!(
            sftp_auth_from(&conn("sftp", AuthMethod::Anonymous), None).unwrap(),
            SftpAuth::Password(p) if p.is_empty()
        ));
        let token = conn("sftp", AuthMethod::Token { token_ref: "t".into() });
        assert!(sftp_auth_from(&token, None).unwrap_err().contains("token"));
        let ak = conn("sftp", AuthMethod::AccessKey { id: "AKIA".into(), secret_ref: "s".into() });
        assert!(sftp_auth_from(&ak, None).unwrap_err().contains("access-key"));
    }

    #[test]
    fn webdav_auth_from_maps_anonymous_password_and_rejects_cloud_only_auth_kinds() {
        // Anonymous sends no Authorization header — same behaviour as the pre-existing blank-user case.
        let anon = conn("webdav", AuthMethod::Anonymous);
        let cfg = webdav_auth_from(WebdavConfig::new("http://h"), &anon, Some("ignored")).unwrap();
        assert!(cfg.user.is_none() && cfg.password.is_none());

        // Password with a real user still applies basic auth.
        let pw = conn("webdav", AuthMethod::Password);
        let cfg = webdav_auth_from(WebdavConfig::new("http://h"), &pw, Some("pw")).unwrap();
        assert_eq!(cfg.user.as_deref(), Some("me"));
        assert_eq!(cfg.password.as_deref(), Some("pw"));

        let token = conn("webdav", AuthMethod::Token { token_ref: "t".into() });
        assert!(webdav_auth_from(WebdavConfig::new("http://h"), &token, None).unwrap_err().contains("token"));
        let ak = conn("webdav", AuthMethod::AccessKey { id: "AKIA".into(), secret_ref: "s".into() });
        assert!(webdav_auth_from(WebdavConfig::new("http://h"), &ak, None).unwrap_err().contains("access-key"));
    }
}
