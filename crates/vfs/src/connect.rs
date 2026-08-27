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
//! Provider-supplied entry names are re-filtered through **the provider's own**
//! [`FileSystemProvider::is_safe_leaf_name`] before they become a navigable child URI, inheriting the
//! CPE-1461/1462 source-side traversal defense even if a hostile server hands back a `../escape` name.
//!
//! **CPE-1704 (round 3):** this used to hardcode `cpe_server::transfer::is_safe_name` for every backend
//! regardless of which one produced the entry — correct for local/SFTP/WebDAV/FTP (filesystem-shaped
//! paths, where `:` is a genuine Windows drive-letter/ADS hazard), but silently wrong for a backend whose
//! keyspace has different rules (S3: no drive letters, no ADS, `:` is an ordinary key byte). This is the
//! ONE code path a user's remote directory listing actually takes
//! ([`remote_dir_entries`] ← `remote_list_dir_impl` ← the `list_dir` Tauri command), so hardcoding the
//! wrong guard here silently re-dropped a legal S3 key even after `cpe-s3`'s own listing had correctly let
//! it through — CPE-1704's opening bug, surviving one layer further out than its first fix reached.
//! [`remote_dir_entries`] now asks `provider.is_safe_leaf_name(name)` — the provider's own answer — and
//! also asks `provider.list_with_filtered_count` instead of `list` so a leaf a backend's OWN listing pass
//! had to drop internally (S3's embedded-`/`/literal-`..` case) is counted too, not just what this
//! function itself drops. See [`RemoteListing`] for why that count is a real field, never mixed into the
//! entry `Vec`.

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
///
/// `is_dir` appends S3's own spelling of "this is a prefix, not an object" — a trailing `/` — so a
/// same-named file and directory (CPE-1737: an S3 object `photos` and a prefix `photos/` are
/// independent and can coexist) never build the same path. This is done here, in the shared
/// `cpe-vfs::connect` layer, so every remote backend gets it, not just S3 — the one backend that can
/// actually collide today (S3 isn't user-reachable yet: `cpe_vfs::open` has no `s3` arm, CPE-1685).
///
/// A trailing `/` on a directory path is a near-universal convention (POSIX treats `dir` and `dir/`
/// identically; WebDAV's own collection-URL convention, RFC 4918 §8.3, IS the trailing slash), so
/// applying it uniformly is expected to be safe for the three backends that ARE reachable today — but
/// it is not uniformly TRUE that every operation on every backend already trims/tolerates it, and an
/// earlier version of this comment overclaimed that it was. `stat` trims a trailing slash when
/// computing the display NAME on SFTP/FTP/WebDAV (`sftp/src/lib.rs`, `ftp/src/lib.rs`,
/// `webdav/src/lib.rs`), but `list`/`mkdir`/`delete`/`rename` send the path VERBATIM on every backend —
/// and one of those was a real, live bug: WebDAV's `delete` retry-then-escalate guard used to key off
/// the INPUT path's own spelling, so an already-slashed directory path skipped the guard entirely on a
/// 3xx response and silently reported `Ok(())` having deleted nothing (fixed alongside this comment —
/// see [`FileSystemProvider::delete`]'s WebDAV impl and
/// `delete_of_an_already_slashed_directory_that_redirects_is_reported_as_an_error_not_ok`).
/// CPE-1950: this is `pub` for ONE reason — `crates/vfs/tests/real_server_conformance.rs` calls it
/// directly instead of reimplementing it.
///
/// It used to carry its own copies (`remote`/`remote_dir`) plus a sentence here claiming that
/// `remote()` "mirrors this function so the real-server-rig E2E job actually exercises the new shape".
/// That sentence named the **wrong helper**: `remote()` is the `is_dir: false` join and never appends
/// the slash; the slashed coverage came from `remote_dir()`, which
/// `assert_slashed_directory_path_round_trips` drives through `mkdir`/`stat`/`list`/`delete` against
/// the real servers. Both landed in the same commit as this sentence (b15c9f7b, CPE-1737 #908), so
/// the claim's **conclusion was true from the day it was written** — only its pointer was wrong. (An
/// earlier draft of this comment, PR #1067, asserted the opposite and said the rig had been testing a
/// stale shape; that was incorrect and is corrected here.)
///
/// A misnamed reference is still a provenance claim nothing could check, and the fix is the same one
/// that removes the whole class: there are no copies left. The rig and production run this exact
/// function, and the compiler enforces it.
pub fn join_remote(dir: &str, name: &str, is_dir: bool) -> String {
    let base = dir.trim_end_matches('/');
    let suffix = if is_dir { "/" } else { "" };
    if base.is_empty() {
        format!("/{name}{suffix}")
    } else {
        format!("{base}/{name}{suffix}")
    }
}

/// Build the navigable child URI for `name` under the directory `uri` points at. `is_dir` carries the
/// CPE-1737 fix through to the built URI — see [`join_remote`].
fn child_uri(uri: &str, loc: &Location, name: &str, is_dir: bool) -> String {
    format!("{}://{}{}", scheme_word(uri), authority(loc), join_remote(&loc.path, name, is_dir))
}

/// Map one provider entry (under directory `uri`) into a [`DirEntry`] the frontend renders and can
/// navigate into. Remote backends don't report a modified time or OS symlink flag, so those are
/// `None`/`false`; `hidden` follows the POSIX leading-dot convention.
///
/// `e.is_dir` is threaded into [`child_uri`] (CPE-1737) so a directory and a same-named file never
/// build an identical `path` — the collision two independent `ListObjectsV2` result sets (`<Contents>`
/// vs `<CommonPrefixes>`) can legitimately produce over S3's flat, `/`-free keyspace. See
/// `cpe_s3::provider::tests::a_name_collision_lists_as_two_rows_so_the_user_has_already_said_which_one_they_meant`.
fn dir_entry_from_provider(uri: &str, loc: &Location, e: ProviderEntry) -> DirEntry {
    let extension =
        if e.is_dir { String::new() } else { extension_of(std::path::Path::new(&e.name)) };
    DirEntry {
        path: child_uri(uri, loc, &e.name, e.is_dir),
        hidden: e.name.starts_with('.'),
        extension,
        name: e.name,
        is_dir: e.is_dir,
        size: e.size,
        modified: None,
        is_symlink: false,
    }
}

/// The result of listing a remote directory (CPE-1704): the entries safe to show, plus how many were not,
/// as a real `usize` — never a synthetic row mixed into `entries`. An earlier round of CPE-1704 tried
/// exactly that (a fake `⚠ N filtered` `ProviderEntry`) and review found it worse than the silent drop it
/// replaced: a REAL object can be named the marker's own text (nothing stops it), so the only such row a
/// user could ever see was, in fact, indistinguishable from one an attacker planted; the fake entry's
/// `is_dir`/`size` fields were dishonest; and deleting it would silently "succeed" (S3 `DELETE` of a
/// nonexistent key returns `204`). `filtered` here can't be spoofed by anything a server sends, because it
/// is computed in this process, from what this function (and the provider's own listing pass) dropped —
/// never from data that arrived over the wire.
pub struct RemoteListing {
    pub entries: Vec<DirEntry>,
    pub filtered: usize,
}

/// List the immediate children of the remote directory `uri` as [`DirEntry`] rows. Provider-supplied names
/// that aren't safe by **the provider's own rule** ([`FileSystemProvider::is_safe_leaf_name`] — defaults to
/// [`cpe_server::transfer::is_safe_name`], overridden by a backend whose keyspace has different rules, e.g.
/// `cpe-s3`) are dropped, inheriting the CPE-1461 source-side traversal defense so a hostile server can't
/// inject a `..`/separator name — while a backend like S3 that legitimately allows `:` is no longer
/// re-refused by the wrong, hardcoded guard (CPE-1704).
///
/// Uses [`FileSystemProvider::list_with_filtered_count`] rather than `list` so a leaf the PROVIDER's own
/// listing pass already had to drop internally (S3's embedded-`/`/literal-`..` case, dropped before an
/// entry could even be constructed) is folded into [`RemoteListing::filtered`] too, not just what this
/// function's own `is_safe_leaf_name` pass drops.
pub fn remote_dir_entries(
    provider: &dyn FileSystemProvider,
    uri: &str,
) -> Result<RemoteListing, String> {
    let loc = location::parse(uri);
    let (raw_entries, mut filtered) = provider.list_with_filtered_count(&loc.path)?;
    let mut entries = Vec::with_capacity(raw_entries.len());
    for e in raw_entries {
        if provider.is_safe_leaf_name(&e.name) {
            entries.push(dir_entry_from_provider(uri, &loc, e));
        } else {
            filtered += 1;
        }
    }
    Ok(RemoteListing { entries, filtered })
}

/// Normalise `uri`'s trailing slash to match `is_dir` — a directory's URI always ends `/`, a file's
/// never does. `remote_stat` (below) uses this so the `path` it returns follows the SAME convention
/// [`dir_entry_from_provider`]'s listing rows use (CPE-1737), regardless of which spelling the caller
/// used to reach this exact path (a favourite, a typed address, a listing row — non-blocking cleanup
/// from CPE-1737 round 2's review; see `src/lib/paths.ts`'s `canonicalPath` for the frontend's mirror of
/// the same idea, going the other direction).
fn with_dir_suffix(uri: &str, is_dir: bool) -> String {
    let base = uri.trim_end_matches('/');
    if is_dir { format!("{base}/") } else { base.to_string() }
}

/// Stat the remote path `uri` into a single [`DirEntry`].
pub fn remote_stat(provider: &dyn FileSystemProvider, uri: &str) -> Result<DirEntry, String> {
    let loc = location::parse(uri);
    let e = provider.stat(&loc.path)?;
    // `stat` targets the path itself, so the row's URI is (a normalised form of) the input URI, not a
    // joined child.
    let extension =
        if e.is_dir { String::new() } else { extension_of(std::path::Path::new(&e.name)) };
    Ok(DirEntry {
        path: with_dir_suffix(uri, e.is_dir),
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
        let listing = remote_dir_entries(&**provider.lock().unwrap(), uri()).unwrap();
        assert_eq!(listing.filtered, 0, "nothing unsafe in this fixture — the count must stay honest at 0");
        let rows = listing.entries;
        let mut names: Vec<_> = rows.iter().map(|r| r.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec![".hidden", "readme.txt", "sub"]);
        let readme = rows.iter().find(|r| r.name == "readme.txt").unwrap();
        assert_eq!(readme.path, "sftp://me@host.example.com:2222/srv/readme.txt");
        assert_eq!(readme.extension, "txt");
        assert!(!readme.is_dir);
        assert!(rows.iter().find(|r| r.name == ".hidden").unwrap().hidden);
        let sub = rows.iter().find(|r| r.name == "sub").unwrap();
        assert!(sub.is_dir);
        // CPE-1737 round 2 (review finding): nothing here pinned a DIRECTORY row's own path through the
        // real `remote_dir_entries` channel — only the file row above was ever asserted, so a directory
        // path regression here was invisible to this test even though it exercises the exact function
        // (`dir_entry_from_provider`) both rows are built by.
        assert_eq!(sub.path, "sftp://me@host.example.com:2222/srv/sub/");

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
        let listing = remote_dir_entries(&Hostile, uri()).unwrap();
        let names: Vec<_> = listing.entries.iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, vec!["ok.txt"], "only the safe leaf survives");
        assert_eq!(listing.filtered, 2, "the two unsafe entries must be counted, not just dropped with no trace");
    }

    /// CPE-1704 (round 3), Evidence Rule 2 — proven through the REAL channel, not the provider boundary.
    /// A minimal test double standing in for `cpe-s3`'s `S3Provider` (this crate does not, and per
    /// CPE-1685 should not yet, depend on `cpe-s3` — that wiring is a separate, still-blocked ticket): it
    /// overrides `is_safe_leaf_name` exactly the way `S3Provider` does, to prove `remote_dir_entries` asks
    /// the PROVIDER'S own rule rather than the hardcoded filesystem guard that silently re-dropped
    /// `colon:name.txt` in an earlier round of this fix.
    #[test]
    fn remote_dir_entries_keeps_a_colon_name_when_the_provider_says_its_own_rule_allows_it() {
        struct ColonFriendly;
        impl FileSystemProvider for ColonFriendly {
            fn list(&self, _p: &str) -> Result<Vec<ProviderEntry>, String> {
                Ok(vec![
                    ProviderEntry { name: "colon:name.txt".into(), is_dir: false, size: 4 },
                    ProviderEntry { name: "x:y".into(), is_dir: false, size: 1 },
                    ProviderEntry { name: "..evil.txt".into(), is_dir: false, size: 1 },
                    // Still genuinely unsafe under ANY provider's rule — must still be dropped, proving
                    // the override narrows the rule rather than disabling filtering outright.
                    ProviderEntry { name: "../escape".into(), is_dir: false, size: 1 },
                ])
            }
            fn stat(&self, _p: &str) -> Result<ProviderEntry, String> { unreachable!() }
            fn read(&self, _p: &str) -> Result<Vec<u8>, String> { unreachable!() }
            fn write(&mut self, _p: &str, _d: &[u8]) -> Result<(), String> { unreachable!() }
            fn mkdir(&mut self, _p: &str) -> Result<(), String> { unreachable!() }
            fn delete(&mut self, _p: &str) -> Result<(), String> { unreachable!() }
            fn rename(&mut self, _f: &str, _t: &str) -> Result<(), String> { unreachable!() }
            fn is_safe_leaf_name(&self, name: &str) -> bool {
                // The same shape `cpe-s3::provider::is_safe_s3_leaf` uses: no drive-letter/ADS rule, but
                // still no embedded/leading separator and no literal ".."/"." segment.
                !name.is_empty()
                    && name != ".."
                    && name != "."
                    && !name.contains('/')
                    && !name.contains('\\')
                    && !name.chars().any(|c| c.is_control())
            }
        }

        // First, prove the OLD behaviour really would have dropped these — the regression this test exists
        // to catch. If this assertion ever fails, the shared default silently changed underneath this test.
        assert!(!cpe_server::transfer::is_safe_name("colon:name.txt"));
        assert!(!cpe_server::transfer::is_safe_name("x:y"));
        assert!(!cpe_server::transfer::is_safe_name("..evil.txt"));

        let listing = remote_dir_entries(&ColonFriendly, uri()).unwrap();
        let mut names: Vec<_> = listing.entries.iter().map(|r| r.name.clone()).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["..evil.txt".to_string(), "colon:name.txt".to_string(), "x:y".to_string()],
            "every colon/ADS-hardening shape this provider's own rule accepts must reach a real DirEntry"
        );
        assert_eq!(listing.filtered, 1, "the one genuinely unsafe '../escape' entry must still be counted");
    }

    #[test]
    fn child_uri_preserves_scheme_word_and_authority() {
        let loc = location::parse("davs://u@h:8443/dav");
        assert_eq!(child_uri("davs://u@h:8443/dav", &loc, "f.txt", false), "davs://u@h:8443/dav/f.txt");
        // Root path joins without a doubled slash.
        let root = location::parse("sftp://h/");
        assert_eq!(child_uri("sftp://h/", &root, "x", false), "sftp://h/x");
    }

    /// CPE-1737: a directory child's URI carries a trailing `/` — S3's own spelling for "this is a
    /// prefix, not an object" — so a same-named file and folder never build the same path.
    #[test]
    fn child_uri_marks_a_directory_child_with_a_trailing_slash() {
        let loc = location::parse("s3://bucket/");
        assert_eq!(child_uri("s3://bucket/", &loc, "photos", true), "s3://bucket/photos/");
        assert_eq!(child_uri("s3://bucket/", &loc, "photos", false), "s3://bucket/photos");
    }

    /// **The bug this ticket exists to close.** An S3 bucket can hold an object named `photos` AND
    /// objects under the prefix `photos/` — keys are just strings, and `ListObjectsV2` returns the two
    /// completely independently (`<Contents>` vs `<CommonPrefixes>`; see
    /// `cpe_s3::provider::tests::a_name_collision_lists_as_two_rows_so_the_user_has_already_said_which_one_they_meant`).
    /// `dir_entry_from_provider` used to build `path` from `e.name` alone, discarding `is_dir`, so the
    /// object row and the prefix row collided on an IDENTICAL `path`.
    ///
    /// That is not a display oddity: this repo is on `"svelte": "^4"`, and every one of the (at least)
    /// three components keyed on `entry.path` (`FileList.svelte`, `FolderBrowser.svelte`,
    /// `DropStackPanel.svelte`) uses a keyed `{#each}`, which **throws** on a duplicate key rather than
    /// rendering the second row — so the failure mode is the WHOLE listing failing to render, not just
    /// the colliding pair.
    #[test]
    fn a_name_collision_never_produces_two_rows_with_the_same_path() {
        struct Colliding;
        impl FileSystemProvider for Colliding {
            fn list(&self, _p: &str) -> Result<Vec<ProviderEntry>, String> {
                // Exactly the shape `S3Provider::list` returns over the keyspace
                // `["photos", "photos/a.jpg", "photos/b.jpg"]` — see the S3 test named above.
                Ok(vec![
                    ProviderEntry { name: "photos".into(), is_dir: false, size: 4 },
                    ProviderEntry { name: "photos".into(), is_dir: true, size: 0 },
                ])
            }
            fn stat(&self, _p: &str) -> Result<ProviderEntry, String> {
                unreachable!()
            }
            fn read(&self, _p: &str) -> Result<Vec<u8>, String> {
                unreachable!()
            }
            fn write(&mut self, _p: &str, _d: &[u8]) -> Result<(), String> {
                unreachable!()
            }
            fn mkdir(&mut self, _p: &str) -> Result<(), String> {
                unreachable!()
            }
            fn delete(&mut self, _p: &str) -> Result<(), String> {
                unreachable!()
            }
            fn rename(&mut self, _f: &str, _t: &str) -> Result<(), String> {
                unreachable!()
            }
        }

        let listing = remote_dir_entries(&Colliding, uri()).expect("listing the colliding keyspace");
        assert_eq!(listing.entries.len(), 2, "both rows must survive — this is not a filtering concern");

        let paths: Vec<&str> = listing.entries.iter().map(|e| e.path.as_str()).collect();
        assert_ne!(
            paths[0], paths[1],
            "the object row and the prefix row share a path ({:?}) — this is exactly the Svelte \
             keyed-{{#each}} hazard CPE-1737 exists to close: two rows with an identical key throw \
             rather than render",
            paths[0]
        );

        // The distinguishing bit must land on the directory row specifically, and be S3's own spelling
        // of "this is a prefix" — a trailing '/' — per CPE-1737's option 1.
        let dir_row = listing.entries.iter().find(|e| e.is_dir).expect("a directory row");
        let file_row = listing.entries.iter().find(|e| !e.is_dir).expect("a file row");
        assert!(
            dir_row.path.ends_with('/'),
            "the prefix row's path must carry the trailing '/' that marks it a directory, got {:?}",
            dir_row.path
        );
        assert!(
            !file_row.path.ends_with('/'),
            "the object row's path must stay bare (unchanged shape), got {:?}",
            file_row.path
        );
    }

    /// CPE-1748 — traces the distinguishing bit the WHOLE way through this module, not just at the two
    /// endpoints the tests above already cover separately (`child_uri` builds it; a fake `list` shows it
    /// survives to a `DirEntry`). This proves that when a directory child's OWN `DirEntry.path` (built by
    /// `child_uri`, trailing `/` and all) later comes back in as the `uri` argument to `remote_stat`/
    /// `remote_read` — a favourite, a re-navigation, or CPE-1499's still-to-come remote-open wiring — the
    /// trailing `/` reaches the literal string handed to `provider.stat`/`provider.read` UNCHANGED. No
    /// intermediate step in `remote_stat`/`remote_read` (nor `location::parse`, which both route through)
    /// may quietly normalise it away before the provider ever sees it.
    ///
    /// What a real provider then DOES with that distinction is the provider's own responsibility — this
    /// crate deliberately does not depend on `cpe-s3` (CPE-1685), so `cpe-s3::provider`'s own tests
    /// (`stat_on_the_colliding_keyspace_...`, `read_on_the_colliding_keyspace_...`) cover that half.
    #[test]
    fn the_trailing_slash_that_marks_a_directory_reaches_the_provider_unchanged_for_stat_and_read() {
        /// Records the exact path string `stat`/`read` were called with — the point here is what
        /// REACHES the provider, not what a real backend would do with it.
        struct Recording {
            seen: Mutex<Vec<String>>,
        }
        impl FileSystemProvider for Recording {
            fn list(&self, _p: &str) -> Result<Vec<ProviderEntry>, String> {
                unreachable!()
            }
            fn stat(&self, p: &str) -> Result<ProviderEntry, String> {
                self.seen.lock().unwrap().push(p.to_string());
                Ok(ProviderEntry { name: "photos".into(), is_dir: p.ends_with('/'), size: 0 })
            }
            fn read(&self, p: &str) -> Result<Vec<u8>, String> {
                self.seen.lock().unwrap().push(p.to_string());
                Ok(b"x".to_vec())
            }
            fn write(&mut self, _p: &str, _d: &[u8]) -> Result<(), String> {
                unreachable!()
            }
            fn mkdir(&mut self, _p: &str) -> Result<(), String> {
                unreachable!()
            }
            fn delete(&mut self, _p: &str) -> Result<(), String> {
                unreachable!()
            }
            fn rename(&mut self, _f: &str, _t: &str) -> Result<(), String> {
                unreachable!()
            }
        }

        let recording = Recording { seen: Mutex::new(Vec::new()) };
        let loc = location::parse(uri());

        // The exact two child URIs `dir_entry_from_provider` would build for a name collision — "photos"
        // the object, "photos" the prefix — built through the SAME `child_uri` function a real listing
        // uses, so this is a round trip from a real `DirEntry.path`, not a hand-typed lookalike string.
        let file_uri = child_uri(uri(), &loc, "photos", false);
        let dir_uri = child_uri(uri(), &loc, "photos", true);
        assert_ne!(file_uri, dir_uri, "sanity: the two child URIs must differ (CPE-1737)");

        remote_stat(&recording, &file_uri).unwrap();
        remote_stat(&recording, &dir_uri).unwrap();
        remote_read(&recording, &file_uri).unwrap();
        // Reading a directory has no sane byte answer in production (a real provider like `cpe-s3` now
        // refuses it) — this test only cares which STRING reached the provider, not what it did with it.
        remote_read(&recording, &dir_uri).unwrap();

        let seen = recording.seen.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![
                "/srv/photos".to_string(),
                "/srv/photos/".to_string(),
                "/srv/photos".to_string(),
                "/srv/photos/".to_string(),
            ],
            "the object path and the directory path must reach `provider.stat`/`provider.read` as \
             DIFFERENT strings in every case — losing the trailing '/' anywhere in this module is exactly \
             the CPE-1737 collision reopened for stat/read"
        );
    }
}
