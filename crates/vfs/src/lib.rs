//! Scheme router (epic CPE-616): open the right [`FileSystemProvider`] for a saved connection.
//!
//! The connections model ([`cpe_server::connections`]) stores non-secret profiles; the actual password /
//! key-passphrase comes from the OS keychain at connect time. This crate is the seam that turns a
//! `(Connection, secret)` into a live, boxed provider — dispatching by scheme to `cpe-sftp` or
//! `cpe-webdav`. The app calls [`open`] with the secret it fetched from the keychain.

use cpe_server::connections::{AuthMethod, Connection};
use cpe_server::known_hosts::KnownHost;
use cpe_server::provider::FileSystemProvider;
use cpe_sftp::{HostKeyPolicy, SftpAuth, SftpConfig, SftpProvider};
use cpe_webdav::{WebdavConfig, WebdavProvider};

/// Open a live [`FileSystemProvider`] for `conn`, using `secret` (the password, or a key's passphrase)
/// fetched from the OS keychain. `known_hosts` + `policy` govern SFTP host-key verification (ignored for
/// WebDAV). Errors carry the scheme/host context.
pub fn open(
    conn: &Connection,
    secret: Option<&str>,
    known_hosts: Vec<KnownHost>,
    policy: HostKeyPolicy,
) -> Result<Box<dyn FileSystemProvider>, String> {
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
        other => Err(format!("vfs: unsupported scheme '{other}'")),
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
}
