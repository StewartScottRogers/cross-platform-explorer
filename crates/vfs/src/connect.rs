//! Command-layer routing for remote locations (CPE-1511, epic CPE-1499): turn a remote URI into a live,
//! reusable [`FileSystemProvider`] and map its entries into the app's [`DirEntry`] rows, so an SFTP /
//! WebDAV location browses through the very same commands a local path does.
//!
//! This lives in `cpe-vfs` — not `cpe-server`'s `fs_route` — because it is the one place that can see
//! **both** the connection model + secret/known-hosts helpers (`cpe-server`) **and** the concrete
//! providers [`crate::open`] dispatches to (`cpe-sftp`/`cpe-webdav`). `cpe-server` deliberately cannot
//! depend on those network crates, so `fs_route` stays the pure local/remote *classifier* and this module
//! is the remote *resolver* the app calls after that classification.
//!
//! # Local is never touched
//! Nothing here runs for a local path. The app's fast-path checks [`cpe_server::fs_route::route`] first
//! and only reaches [`connected_provider`] for a remote scheme, so the plain explorer's hot path is
//! byte-for-byte unchanged (PURPOSE.md's hard constraint).
//!
//! # Pool
//! [`ProviderPool`] keeps one live provider per saved connection (keyed by its unique `name`), so a browse
//! → open-file → browse-again sequence reuses a single SFTP session instead of reconnecting per op. A
//! failed connect is never cached; [`ProviderPool::invalidate`] drops a session after an operation error
//! so the next op reconnects.
//!
//! # Security
//! Host-key verification is the SFTP provider's own TOFU: a **changed** or **revoked** key is refused at
//! connect with a distinct message that propagates out of [`connected_provider`] (never a silent connect).
//! Provider-supplied entry names are re-filtered through [`cpe_server::transfer::is_safe_name`] before they
//! become a navigable child URI, inheriting the CPE-1461/1462 source-side traversal defense even if a
//! hostile server hands back a `../escape` name.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use cpe_server::connections::{AuthMethod, Connection};
use cpe_server::known_hosts::KnownHost;
use cpe_server::location::{self, Location};
use cpe_server::model::{extension_of, DirEntry};
use cpe_server::provider::{FileSystemProvider, ProviderEntry};
use cpe_server::vault_manager::SecretAccess;

use crate::{BoxedProvider, HostKeyPolicy};

/// A connected provider shared across ops. Read ops take `&self`, write ops `&mut self`, so it is wrapped
/// in a `Mutex`; `Arc` lets the pool hand the same live session to concurrent commands.
pub type SharedProvider = Arc<Mutex<BoxedProvider>>;

/// The seam that actually opens a provider — [`VfsOpener`] in production (a real SFTP/WebDAV connect via
/// [`crate::open`]), an injectable fake in tests so the routing logic is exercised with **no network**.
/// `record_first_contact` is the app-managed `known_hosts` store path (CPE-1512): the real opener persists
/// a first-contact SFTP host key there so a later connect resolves `Trusted` instead of `Unknown` forever.
pub trait ProviderOpener: Send + Sync {
    fn open(
        &self,
        conn: &Connection,
        secret: Option<&str>,
        known_hosts: Vec<KnownHost>,
        policy: HostKeyPolicy,
        record_first_contact: Option<&Path>,
    ) -> Result<BoxedProvider, String>;
}

/// Production opener: dispatches to [`crate::open`], performing the real SFTP/WebDAV connect + host-key
/// verification (and, for SFTP, first-contact key persistence — CPE-1512).
pub struct VfsOpener;

impl ProviderOpener for VfsOpener {
    fn open(
        &self,
        conn: &Connection,
        secret: Option<&str>,
        known_hosts: Vec<KnownHost>,
        policy: HostKeyPolicy,
        record_first_contact: Option<&Path>,
    ) -> Result<BoxedProvider, String> {
        crate::open(conn, secret, known_hosts, policy, record_first_contact)
    }
}

/// A per-connection pool of live providers, keyed by the connection's unique `name`. Open once, reuse
/// across ops; a failed connect is never cached, and [`invalidate`](Self::invalidate) drops a session so
/// the next op reconnects.
#[derive(Default)]
pub struct ProviderPool {
    cache: Mutex<HashMap<String, SharedProvider>>,
}

impl ProviderPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the cached provider for `key`, or open one via `make` and cache it. The connect runs
    /// **without** holding the cache lock (a slow connect must not block other connections); if another
    /// thread cached one meanwhile, that one wins and this freshly-opened session is dropped (closed).
    fn get_or_open(
        &self,
        key: &str,
        make: impl FnOnce() -> Result<BoxedProvider, String>,
    ) -> Result<SharedProvider, String> {
        if let Some(existing) = self.cache.lock().unwrap().get(key).cloned() {
            return Ok(existing);
        }
        // Open outside the lock; only a successful connect is inserted.
        let provider: SharedProvider = Arc::new(Mutex::new(make()?));
        let mut cache = self.cache.lock().unwrap();
        Ok(cache.entry(key.to_string()).or_insert(provider).clone())
    }

    /// Drop the cached session for `key` (e.g. after an operation error), so the next op reconnects.
    pub fn invalidate(&self, key: &str) {
        self.cache.lock().unwrap().remove(key);
    }

    /// Number of live cached sessions (test/observability helper).
    pub fn len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Find the saved [`Connection`] a remote `uri` navigates to, matching on scheme + host (and user/port
/// when the URI specifies them). Returns `None` for a local path or when no profile matches — the caller
/// turns that into a clear "no saved connection" error.
pub fn find_connection(uri: &str, conns: &[Connection]) -> Option<Connection> {
    let loc = location::parse(uri);
    if loc.is_local() {
        return None;
    }
    conns
        .iter()
        .find(|c| {
            let cloc = location::parse(&c.location());
            cloc.scheme == loc.scheme
                && cloc.host == loc.host
                && (loc.user.is_none() || loc.user == cloc.user)
                && (loc.port.is_none() || loc.port == cloc.port)
        })
        .cloned()
}

/// Resolve a remote `uri` to a live, pooled provider (CPE-1511's crux). Loads the matching saved
/// connection, fetches its secret through the keychain seam, and opens (or reuses) a provider via
/// `opener`. `record_first_contact` is the app-managed `known_hosts` store path forwarded to the opener
/// (CPE-1512) — `None` if this platform has no app config dir, which only skips persistence, never the
/// connect itself.
///
/// Errors, never panics:
/// - no saved connection matches the URI → clear `Err`;
/// - a password connection with no stored secret → clear `Err` (never an empty-password connect attempt);
/// - a changed/revoked host key → the opener's distinct refusal message propagates out (TOFU, no silent
///   connect);
/// - a connect failure is not cached, so a later retry can succeed.
#[allow(clippy::too_many_arguments)]
pub fn connected_provider(
    pool: &ProviderPool,
    opener: &dyn ProviderOpener,
    access: &dyn SecretAccess,
    conns: &[Connection],
    known_hosts: &[KnownHost],
    policy: HostKeyPolicy,
    uri: &str,
    record_first_contact: Option<&Path>,
) -> Result<SharedProvider, String> {
    let conn = find_connection(uri, conns)
        .ok_or_else(|| format!("no saved connection matches '{uri}'"))?;
    let known = known_hosts.to_vec();
    let key = conn.name.clone();
    pool.get_or_open(&key, move || {
        let secret = cpe_server::secret_store::secret_for(access, &conn.name)?;
        // Password auth with no stored secret is an error — attempting a blank-password connect would be
        // both wrong and a lockout risk. A key's passphrase (`AuthMethod::Key`) may legitimately be absent
        // (an unencrypted key), so `None` is allowed there.
        if matches!(conn.auth, AuthMethod::Password) && secret.is_none() {
            return Err(format!(
                "no saved secret for connection '{}' — set its password first",
                conn.name
            ));
        }
        opener.open(&conn, secret.as_deref(), known, policy, record_first_contact)
    })
}

/// The scheme *word* as written in the URI (`sftp`/`ssh`/`webdav`/`davs`/…), preserved verbatim so a child
/// URI round-trips through the same scheme the user navigated with (`location::Scheme` alone would lose
/// `davs` vs `webdav`).
fn scheme_word(uri: &str) -> &str {
    uri.split("://").next().unwrap_or("")
}

/// The `[user@]host[:port]` authority for a parsed remote location.
fn authority(loc: &Location) -> String {
    let mut s = String::new();
    if let Some(u) = &loc.user {
        s.push_str(u);
        s.push('@');
    }
    if let Some(h) = &loc.host {
        s.push_str(h);
    }
    if let Some(p) = loc.port {
        s.push_str(&format!(":{p}"));
    }
    s
}

/// Join a safe leaf `name` onto the remote directory path `dir` (forward-slash, provider convention).
fn join_remote(dir: &str, name: &str) -> String {
    let base = dir.trim_end_matches('/');
    if base.is_empty() {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    }
}

/// Build the navigable child URI for `name` under the directory `uri` points at.
fn child_uri(uri: &str, loc: &Location, name: &str) -> String {
    format!("{}://{}{}", scheme_word(uri), authority(loc), join_remote(&loc.path, name))
}

/// Map one provider entry (under directory `uri`) into a [`DirEntry`] the frontend renders and can
/// navigate into. Remote backends don't report a modified time or OS symlink flag, so those are
/// `None`/`false`; `hidden` follows the POSIX leading-dot convention.
fn dir_entry_from_provider(uri: &str, loc: &Location, e: ProviderEntry) -> DirEntry {
    let extension =
        if e.is_dir { String::new() } else { extension_of(std::path::Path::new(&e.name)) };
    DirEntry {
        path: child_uri(uri, loc, &e.name),
        hidden: e.name.starts_with('.'),
        extension,
        name: e.name,
        is_dir: e.is_dir,
        size: e.size,
        modified: None,
        is_symlink: false,
    }
}

/// List the immediate children of the remote directory `uri` as [`DirEntry`] rows. Provider-supplied names
/// that aren't a safe single path segment ([`cpe_server::transfer::is_safe_name`]) are dropped, inheriting
/// the CPE-1461 source-side traversal defense so a hostile server can't inject a `..`/separator name.
pub fn remote_dir_entries(
    provider: &dyn FileSystemProvider,
    uri: &str,
) -> Result<Vec<DirEntry>, String> {
    let loc = location::parse(uri);
    let entries = provider.list(&loc.path)?;
    Ok(entries
        .into_iter()
        .filter(|e| cpe_server::transfer::is_safe_name(&e.name))
        .map(|e| dir_entry_from_provider(uri, &loc, e))
        .collect())
}

/// Stat the remote path `uri` into a single [`DirEntry`].
pub fn remote_stat(provider: &dyn FileSystemProvider, uri: &str) -> Result<DirEntry, String> {
    let loc = location::parse(uri);
    let e = provider.stat(&loc.path)?;
    // `stat` targets the path itself, so the row's URI is the input URI (not a joined child).
    let extension =
        if e.is_dir { String::new() } else { extension_of(std::path::Path::new(&e.name)) };
    Ok(DirEntry {
        path: uri.to_string(),
        hidden: e.name.starts_with('.'),
        extension,
        name: e.name,
        is_dir: e.is_dir,
        size: e.size,
        modified: None,
        is_symlink: false,
    })
}

/// Read the whole remote file `uri`.
pub fn remote_read(provider: &dyn FileSystemProvider, uri: &str) -> Result<Vec<u8>, String> {
    let loc = location::parse(uri);
    provider.read(&loc.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpe_server::provider::FakeProvider;
    use std::collections::HashMap as Map;

    /// Extract the error string from a `Result` whose `Ok` type (a boxed provider) isn't `Debug`, so
    /// `expect_err`/`unwrap_err` can't be used directly.
    fn err_of<T>(r: Result<T, String>) -> String {
        match r {
            Ok(_) => panic!("expected an error"),
            Err(e) => e,
        }
    }

    /// In-memory keychain fake, mirroring `secret_store`'s test fake.
    #[derive(Default)]
    struct MemAccess {
        map: Mutex<Map<(String, String), String>>,
    }
    impl SecretAccess for MemAccess {
        fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
            self.map.lock().unwrap().insert((service.into(), account.into()), secret.into());
            Ok(())
        }
        fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
            Ok(self.map.lock().unwrap().get(&(service.into(), account.into())).cloned())
        }
        fn delete(&self, service: &str, account: &str) -> Result<(), String> {
            self.map.lock().unwrap().remove(&(service.into(), account.into()));
            Ok(())
        }
    }

    /// An opener that hands back a pre-seeded [`FakeProvider`] instead of connecting, so the whole routing
    /// path is testable with no server. Records how many times it opened (to prove the pool reuses) and
    /// the `record_first_contact` path it was last called with (CPE-1512 seam: proves `connected_provider`
    /// actually forwards the app-managed known_hosts store path down to the opener, since the real SFTP
    /// recording behaviour itself lives — and is exercised against a real in-process server — in
    /// `cpe-sftp`'s own tests, not reachable through this trait-object boundary).
    struct FakeOpener {
        opens: Mutex<usize>,
        last_record_path: Mutex<Option<std::path::PathBuf>>,
        // A factory so each open produces a fresh provider (with the seeded tree).
        seed: Box<dyn Fn() -> FakeProvider + Send + Sync>,
    }
    impl FakeOpener {
        fn new(seed: impl Fn() -> FakeProvider + Send + Sync + 'static) -> Self {
            Self { opens: Mutex::new(0), last_record_path: Mutex::new(None), seed: Box::new(seed) }
        }
    }
    impl ProviderOpener for FakeOpener {
        fn open(
            &self,
            _conn: &Connection,
            _secret: Option<&str>,
            _known: Vec<KnownHost>,
            _policy: HostKeyPolicy,
            record_first_contact: Option<&Path>,
        ) -> Result<BoxedProvider, String> {
            *self.opens.lock().unwrap() += 1;
            *self.last_record_path.lock().unwrap() = record_first_contact.map(Path::to_path_buf);
            Ok(Box::new((self.seed)()))
        }
    }

    /// An opener that always refuses with the SFTP crate's distinct changed-key message.
    struct ChangedKeyOpener;
    impl ProviderOpener for ChangedKeyOpener {
        fn open(
            &self,
            _c: &Connection,
            _s: Option<&str>,
            _k: Vec<KnownHost>,
            _p: HostKeyPolicy,
            _record_first_contact: Option<&Path>,
        ) -> Result<BoxedProvider, String> {
            Err("sftp: host key CHANGED — refused (possible man-in-the-middle)".to_string())
        }
    }

    fn sftp_conn() -> Connection {
        Connection {
            name: "prod".into(),
            scheme: "sftp".into(),
            host: "host.example.com".into(),
            port: 2222,
            user: "me".into(),
            auth: AuthMethod::Password,
            path: Some("/srv".into()),
        }
    }

    fn seeded() -> FakeProvider {
        let mut fp = FakeProvider::new();
        fp.mkdir("/srv/sub").unwrap();
        fp.write("/srv/readme.txt", b"hello").unwrap();
        fp.write("/srv/.hidden", b"x").unwrap();
        fp
    }

    fn uri() -> &'static str {
        "sftp://me@host.example.com:2222/srv"
    }

    #[test]
    fn find_connection_matches_scheme_host_user_port() {
        let conns = vec![sftp_conn()];
        assert_eq!(find_connection(uri(), &conns).unwrap().name, "prod");
        // A local path never matches.
        assert!(find_connection(r"C:\Users\me", &conns).is_none());
        // A different host doesn't match.
        assert!(find_connection("sftp://me@other.example.com:2222/srv", &conns).is_none());
    }

    #[test]
    fn connected_provider_routes_list_stat_read_to_the_provider() {
        let pool = ProviderPool::new();
        let opener = FakeOpener::new(seeded);
        let access = MemAccess::default();
        cpe_server::secret_store::set_secret(&access, "prod", "pw").unwrap();
        let conns = vec![sftp_conn()];

        let store = std::path::Path::new("/fake/app-config/known_hosts");
        let provider = connected_provider(
            &pool,
            &opener,
            &access,
            &conns,
            &[],
            HostKeyPolicy::Tofu,
            uri(),
            Some(store),
        )
        .expect("resolves");

        // CPE-1512 seam: connected_provider forwards the app-managed known_hosts store path down to the
        // opener unchanged (the real recording behaviour, exercised against a live in-process SFTP server,
        // lives in `cpe-sftp`'s own tests — this trait-object boundary can only prove the wiring/plumbing).
        assert_eq!(opener.last_record_path.lock().unwrap().as_deref(), Some(store));

        // list → maps to DirEntry rows with navigable child URIs; the hostile-looking hidden file is kept
        // (it's a safe name), directories sort in.
        let rows = remote_dir_entries(&**provider.lock().unwrap(), uri()).unwrap();
        let mut names: Vec<_> = rows.iter().map(|r| r.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec![".hidden", "readme.txt", "sub"]);
        let readme = rows.iter().find(|r| r.name == "readme.txt").unwrap();
        assert_eq!(readme.path, "sftp://me@host.example.com:2222/srv/readme.txt");
        assert_eq!(readme.extension, "txt");
        assert!(!readme.is_dir);
        assert!(rows.iter().find(|r| r.name == ".hidden").unwrap().hidden);
        assert!(rows.iter().find(|r| r.name == "sub").unwrap().is_dir);

        // stat + read route to the same provider.
        let st = remote_stat(&**provider.lock().unwrap(), "sftp://me@host.example.com:2222/srv/readme.txt").unwrap();
        assert_eq!(st.size, 5);
        let bytes = remote_read(&**provider.lock().unwrap(), "sftp://me@host.example.com:2222/srv/readme.txt").unwrap();
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn pool_reuses_a_connection_across_ops() {
        let pool = ProviderPool::new();
        let opener = FakeOpener::new(seeded);
        let access = MemAccess::default();
        cpe_server::secret_store::set_secret(&access, "prod", "pw").unwrap();
        let conns = vec![sftp_conn()];

        for _ in 0..3 {
            connected_provider(&pool, &opener, &access, &conns, &[], HostKeyPolicy::Tofu, uri(), None).unwrap();
        }
        assert_eq!(*opener.opens.lock().unwrap(), 1, "opened once, reused thereafter");
        assert_eq!(pool.len(), 1);

        // After invalidation the next op reconnects.
        pool.invalidate("prod");
        assert!(pool.is_empty());
        connected_provider(&pool, &opener, &access, &conns, &[], HostKeyPolicy::Tofu, uri(), None).unwrap();
        assert_eq!(*opener.opens.lock().unwrap(), 2);
    }

    #[test]
    fn missing_connection_is_a_clear_error() {
        let pool = ProviderPool::new();
        let opener = FakeOpener::new(seeded);
        let access = MemAccess::default();
        let err = err_of(connected_provider(&pool, &opener, &access, &[], &[], HostKeyPolicy::Tofu, uri(), None));
        assert!(err.contains("no saved connection"), "got: {err}");
    }

    #[test]
    fn missing_password_secret_is_a_clear_error_and_not_cached() {
        let pool = ProviderPool::new();
        let opener = FakeOpener::new(seeded);
        let access = MemAccess::default(); // no secret stored
        let conns = vec![sftp_conn()];
        let err = err_of(connected_provider(&pool, &opener, &access, &conns, &[], HostKeyPolicy::Tofu, uri(), None));
        assert!(err.contains("no saved secret"), "got: {err}");
        assert!(pool.is_empty(), "a failed resolve must not cache a session");
        assert_eq!(*opener.opens.lock().unwrap(), 0, "never attempted a blank-password connect");
    }

    #[test]
    fn a_key_connection_may_connect_without_a_passphrase() {
        // An unencrypted key has no passphrase; `None` must be allowed (not the password error).
        let pool = ProviderPool::new();
        let opener = FakeOpener::new(seeded);
        let access = MemAccess::default();
        let mut conn = sftp_conn();
        conn.auth = AuthMethod::Key { key_path: "/home/me/.ssh/id".into() };
        let conns = vec![conn];
        assert!(connected_provider(&pool, &opener, &access, &conns, &[], HostKeyPolicy::Tofu, uri(), None).is_ok());
    }

    #[test]
    fn a_changed_host_key_refuses_loudly_and_is_not_cached() {
        let pool = ProviderPool::new();
        let access = MemAccess::default();
        cpe_server::secret_store::set_secret(&access, "prod", "pw").unwrap();
        let conns = vec![sftp_conn()];
        let err = err_of(connected_provider(&pool, &ChangedKeyOpener, &access, &conns, &[], HostKeyPolicy::Tofu, uri(), None));
        assert!(err.contains("CHANGED"), "distinct changed-key error, got: {err}");
        assert!(pool.is_empty(), "a refused connect is never cached");
    }

    #[test]
    fn remote_dir_entries_drops_a_hostile_traversal_name() {
        // A provider that hands back a `..`-style name must have it filtered before it becomes a child URI.
        struct Hostile;
        impl FileSystemProvider for Hostile {
            fn list(&self, _p: &str) -> Result<Vec<ProviderEntry>, String> {
                Ok(vec![
                    ProviderEntry { name: "..".into(), is_dir: true, size: 0 },
                    ProviderEntry { name: "../escape".into(), is_dir: false, size: 1 },
                    ProviderEntry { name: "ok.txt".into(), is_dir: false, size: 2 },
                ])
            }
            fn stat(&self, _p: &str) -> Result<ProviderEntry, String> { unreachable!() }
            fn read(&self, _p: &str) -> Result<Vec<u8>, String> { unreachable!() }
            fn write(&mut self, _p: &str, _d: &[u8]) -> Result<(), String> { unreachable!() }
            fn mkdir(&mut self, _p: &str) -> Result<(), String> { unreachable!() }
            fn delete(&mut self, _p: &str) -> Result<(), String> { unreachable!() }
            fn rename(&mut self, _f: &str, _t: &str) -> Result<(), String> { unreachable!() }
        }
        let rows = remote_dir_entries(&Hostile, uri()).unwrap();
        let names: Vec<_> = rows.iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, vec!["ok.txt"], "only the safe leaf survives");
    }

    #[test]
    fn child_uri_preserves_scheme_word_and_authority() {
        let loc = location::parse("davs://u@h:8443/dav");
        assert_eq!(child_uri("davs://u@h:8443/dav", &loc, "f.txt"), "davs://u@h:8443/dav/f.txt");
        // Root path joins without a doubled slash.
        let root = location::parse("sftp://h/");
        assert_eq!(child_uri("sftp://h/", &root, "x"), "sftp://h/x");
    }
}
