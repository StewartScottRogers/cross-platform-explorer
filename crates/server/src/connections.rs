//! Saved remote-connection profiles (CPE-683, epic CPE-616): the persisted list behind the "Connections"
//! sidebar — add/edit/remove a remote (host, port, user, auth *method*), navigable like a location.
//!
//! **Secrets are never stored here.** A profile records only non-secret metadata (host/port/user, and for
//! key auth the *path* to a private key file); the actual password or key passphrase lives in the OS
//! keychain, keyed by the connection, and is fetched at connect time. This module is the pure, headless
//! data + persistence layer (JSON on disk); the sidebar UI and the keychain integration are attended.

#![allow(dead_code)] // consumed by the connections UI + command layer (CPE-683 GUI half); compiled + tested now.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How a saved connection authenticates. **No secret material** — a password's value, a key's
/// passphrase, a bearer token, and an S3 secret access key all live in the OS keychain (CPE-1510), never
/// in this enum or in `connections.json`.
///
/// Additive since CPE-1515: `Anonymous`/`Token`/`AccessKey` were added alongside the original
/// `Password`/`Key`. Because this enum is serialized internally-tagged (`#[serde(tag = "kind")]`), adding
/// variants is backward-compatible by construction — an old `connections.json` that only ever wrote
/// `{"kind":"password"}` / `{"kind":"key",...}` still deserializes unchanged (see
/// `connections.rs`'s tests for the explicit proof).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthMethod {
    /// Password auth; the password itself is stored in the keychain, keyed by the connection.
    Password,
    /// Public-key auth using the private key at `key_path` (an optional passphrase lives in the keychain).
    Key { key_path: String },
    /// No credentials at all — anonymous/public access (e.g. a public FTP mirror or an unauthenticated
    /// share). First-class as of CPE-1515; `cpe-vfs`'s FTP auth mapping previously inferred this purely
    /// from a blank/`"anonymous"` username (CPE-1514) and still honours that heuristic for connections
    /// saved before this variant existed — new connections should set `Anonymous` explicitly.
    Anonymous,
    /// An opaque bearer/OAuth token, for a future cloud provider that authenticates that way. `token_ref`
    /// is **not** the token itself — it's a non-secret reference/label; the real token lives in the OS
    /// keychain (CPE-1510), fetched at connect time the same way a password is (keyed by the connection's
    /// `name`). No provider consumes this yet — it's plumbing ahead of a future cloud provider.
    Token { token_ref: String },
    /// S3-style access-key auth (SigV4). `id` is the access key ID — not secret, safe to store here like
    /// `user` is. `secret_ref` is a non-secret reference/label for the secret access key, which itself
    /// lives in the OS keychain (CPE-1510) and is never written here. No provider consumes this yet — it
    /// unblocks CPE-1503 (S3).
    AccessKey { id: String, secret_ref: String },
}

/// A saved remote connection profile. `name` is the stable display name / identity (unique in a store).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Connection {
    pub name: String,
    /// Scheme, e.g. `"sftp"` (matches [`crate::location::Scheme`] lower-cased).
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
    /// Optional initial remote path to open (defaults to the server's home/root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Connection {
    /// The `scheme://user@host[:port]/path` location string this profile navigates to.
    pub fn location(&self) -> String {
        let port = if self.port == default_port(&self.scheme) { String::new() } else { format!(":{}", self.port) };
        let path = self.path.as_deref().unwrap_or("/");
        format!("{}://{}@{}{}{}", self.scheme, self.user, self.host, port, path)
    }
}

/// The default port for a scheme (so a profile on the default port omits it from its location string).
fn default_port(scheme: &str) -> u16 {
    match scheme {
        "sftp" | "ssh" => 22,
        "smb" => 445,
        "webdav" => 80,
        "davs" => 443,
        // Explicit FTPS negotiates TLS on the same port as plain FTP (unlike legacy implicit-TLS port
        // 990), so both share port 21.
        "ftp" | "ftps" => 21,
        _ => 0,
    }
}

/// Insert or replace a connection by `name` (names are unique), returning the updated list. Editing a
/// connection is just an upsert with the same name.
pub fn upsert(mut list: Vec<Connection>, conn: Connection) -> Vec<Connection> {
    if let Some(existing) = list.iter_mut().find(|c| c.name == conn.name) {
        *existing = conn;
    } else {
        list.push(conn);
    }
    list
}

/// Remove the connection named `name`, returning the updated list (a no-op if absent).
pub fn remove(mut list: Vec<Connection>, name: &str) -> Vec<Connection> {
    list.retain(|c| c.name != name);
    list
}

/// Load the saved connections from `path`. A missing/unreadable/garbage file yields an **empty** list,
/// never an error — a corrupt store must not brick the sidebar.
pub fn load_connections(path: &Path) -> Vec<Connection> {
    match std::fs::read_to_string(path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Persist `conns` to `path` (pretty JSON), creating parent directories. No secrets are written.
pub fn save_connections(path: &Path, conns: &[Connection]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(conns).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("{}: {e}", path.display()))
}

/// The default `connections.json` path: the per-OS user config dir + `cross-platform-explorer/`.
/// Windows `%APPDATA%`; else `$XDG_CONFIG_HOME` or `$HOME/.config`.
pub fn default_connections_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("cross-platform-explorer").join("connections.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Connection {
        Connection {
            name: "prod".into(),
            scheme: "sftp".into(),
            host: "host.example.com".into(),
            port: 22,
            user: "deploy".into(),
            auth: AuthMethod::Key { key_path: "/home/me/.ssh/id_ed25519".into() },
            path: Some("/var/www".into()),
        }
    }

    #[test]
    fn upsert_adds_then_edits_in_place_by_name() {
        let list = upsert(Vec::new(), sample());
        assert_eq!(list.len(), 1);
        // Same name → replace (edit), not duplicate.
        let mut edited = sample();
        edited.host = "new.example.com".into();
        let list = upsert(list, edited);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].host, "new.example.com");
        // Different name → appended.
        let mut other = sample();
        other.name = "staging".into();
        let list = upsert(list, other);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn remove_drops_by_name() {
        let list = upsert(Vec::new(), sample());
        assert!(remove(list.clone(), "prod").is_empty());
        assert_eq!(remove(list, "absent").len(), 1, "removing a missing name is a no-op");
    }

    #[test]
    fn location_string_round_trips_via_the_uri_parser() {
        // A profile's location() must parse back to the same authority via crate::location.
        let conn = sample();
        let loc = crate::location::parse(&conn.location());
        assert_eq!(loc.scheme, crate::location::Scheme::Sftp);
        assert_eq!(loc.user.as_deref(), Some("deploy"));
        assert_eq!(loc.host.as_deref(), Some("host.example.com"));
        assert_eq!(loc.port, None, "default port 22 is omitted");
        assert_eq!(loc.path, "/var/www");
        // A non-default port is included in the location string and round-trips.
        let mut c2 = sample();
        c2.port = 2222;
        assert!(c2.location().contains(":2222"));
        assert_eq!(crate::location::parse(&c2.location()).port, Some(2222));
    }

    #[test]
    fn save_then_load_round_trips_without_secrets() {
        let dir = std::env::temp_dir().join(format!("cpe-conns-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("connections.json");
        let list = upsert(upsert(Vec::new(), sample()), Connection { name: "pw".into(), auth: AuthMethod::Password, ..sample() });
        save_connections(&path, &list).unwrap();

        // The on-disk JSON never contains a password/secret — only metadata + a key *path*.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("id_ed25519") && !raw.to_lowercase().contains("password\":\""));

        let back = load_connections(&path);
        assert_eq!(back, list);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auth_method_round_trips_every_variant_through_serde() {
        for auth in [
            AuthMethod::Password,
            AuthMethod::Key { key_path: "/home/me/.ssh/id".into() },
            AuthMethod::Anonymous,
            AuthMethod::Token { token_ref: "tok-1".into() },
            AuthMethod::AccessKey { id: "AKIAEXAMPLE".into(), secret_ref: "sec-1".into() },
        ] {
            let json = serde_json::to_string(&auth).unwrap();
            let back: AuthMethod = serde_json::from_str(&json).unwrap();
            assert_eq!(back, auth, "round-trip mismatch for {json}");
        }
    }

    #[test]
    fn an_old_connections_json_with_only_password_and_key_still_deserializes() {
        // The frozen v1 on-disk shape (pre-CPE-1515): only "password"/"key" `kind`s ever existed. A
        // binary that now knows about Anonymous/Token/AccessKey must still load an existing user's
        // connections.json byte-for-byte unchanged — the explicit back-compat proof this ticket exists
        // to deliver. Internally-tagged enums (`#[serde(tag = "kind")]`) are additive by construction:
        // new variants don't change how old tags parse.
        let dir = std::env::temp_dir().join(format!("cpe-conns-oldfmt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("connections.json");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            &path,
            r#"[
                {"name":"prod","scheme":"sftp","host":"h.example.com","port":22,"user":"me","auth":{"kind":"password"}},
                {"name":"keyed","scheme":"sftp","host":"h2.example.com","port":22,"user":"me2","auth":{"kind":"key","key_path":"/home/me/.ssh/id_ed25519"}}
            ]"#,
        )
        .unwrap();

        let conns = load_connections(&path);
        assert_eq!(conns.len(), 2, "an old-format store must not be dropped as garbage");
        assert_eq!(conns[0].name, "prod");
        assert_eq!(conns[0].auth, AuthMethod::Password);
        assert_eq!(conns[1].name, "keyed");
        assert_eq!(conns[1].auth, AuthMethod::Key { key_path: "/home/me/.ssh/id_ed25519".into() });

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_of_a_missing_or_garbage_file_is_empty_not_an_error() {
        assert!(load_connections(Path::new("/no/such/cpe/connections.json")).is_empty());
        let dir = std::env::temp_dir().join(format!("cpe-conns-bad-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("connections.json");
        std::fs::write(&path, "{ not valid json").unwrap();
        assert!(load_connections(&path).is_empty(), "a corrupt store must not brick the sidebar");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
