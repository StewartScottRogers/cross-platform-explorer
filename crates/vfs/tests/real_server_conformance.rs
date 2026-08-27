//! Real-server conformance suite (CPE-1659, epic CPE-1498, retiring MANUAL-TEST-BURNDOWN row #13).
//!
//! Every remote provider this app ships (`crates/sftp`, `crates/webdav`, `crates/ftp`) is otherwise
//! tested only against an in-process fake server **written by the same author as the client** — so
//! those tests can only prove the client agrees with itself, never that it interoperates with a real
//! implementation. This file is the interop layer above them: it drives ONE shared [`conformance`]
//! function against real, third-party server images (OpenSSH `sftp-server` via `atmoz/sftp`, Apache
//! `mod_dav`, and `vsftpd`) started on a Docker bridge network by the CI job named
//! `Network E2E (ubuntu-latest, real servers)` (`.github/workflows/ci.yml`). Adding SMB later
//! (CPE-1504) is meant to be a new service block in that job plus one more thin test function here
//! calling the same [`conformance`] — not a new test file.
//!
//! Every test in this file is `#[ignore]`d, so `cargo test` in `crates/vfs` (part of the existing 3-OS
//! `Server crates` CI matrix) is byte-for-byte unchanged — these only run under
//! `cargo test -- --ignored` inside the Docker rig, which sets the `CPE_E2E_*` environment variables
//! read below. Running this file with none of those set fails fast with a clear message pointing at the
//! CI job, rather than silently skipping (a missing env var must never look like a pass).
//!
//! # Routing seam
//! Every connection here goes through [`cpe_vfs::open`] — the exact routing seam the app itself calls
//! after fetching a secret from the keychain (`cpe_vfs::connect::connected_provider`) — never a provider
//! constructor (`SftpProvider::connect`, `WebdavProvider::connect`, `FtpProvider::connect`) directly.
//! That is what makes this an end-to-end test of the real dispatch path, not a fourth copy of each
//! provider's own unit tests.
//!
//! # Mutations are verified off the client (CPE-1049 / CPE-1307 pattern)
//! Every `mkdir`/`write`/`rename`/`delete` assertion re-reads what happened via `std::fs`, directly
//! against the **host-mounted fixture directory** each container has bind-mounted as (part of) its
//! filesystem — the OS's own view, read by a process (this one) that is not the client under test. A
//! provider that merely "agrees with itself" (e.g. its own `list` echoing back what it just wrote,
//! without ever really landing on disk) cannot pass these assertions.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use cpe_server::connections::{AuthMethod, Connection};
use cpe_server::known_hosts;
use cpe_server::provider::FileSystemProvider;
use cpe_vfs::HostKeyPolicy;

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// Read a required env var, panicking with a message pointing at the CI job rather than a bare
/// `unwrap` panic — every test in this file needs these, and they are only ever set by the
/// `Network E2E (ubuntu-latest, real servers)` job.
fn env_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!(
            "{name} is not set — this test only runs inside the CPE-1659 Docker rig (job \
             `Network E2E (ubuntu-latest, real servers)` in .github/workflows/ci.yml); it is not \
             meant to run standalone."
        )
    })
}

fn env_var_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_port(name: &str, default: u16) -> u16 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Block until `host:port` accepts a TCP connection, or panic after `timeout` — the CI job already
/// waits for each container before invoking `cargo test`, but this makes each test independently
/// robust (and gives a much clearer failure than a first-request connection-refused deep inside a
/// provider's error string).
fn wait_for_tcp(host: &str, port: u16, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    let addr = format!("{host}:{port}");
    loop {
        match std::net::TcpStream::connect(&addr) {
            Ok(_) => return,
            Err(e) if Instant::now() < deadline => {
                let _ = e;
                std::thread::sleep(Duration::from_millis(300));
            }
            Err(e) => panic!("{addr} never accepted a TCP connection within {timeout:?}: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Fixture — the hostile tree the CI workflow seeds (see `.github/workflows/ci.yml`'s
// "Seed the hostile fixture tree" step). Every provider gets its OWN copy of this tree in its own
// host directory (a distinct `CPE_E2E_*_FIXTURE_DIR`) so mutating one scheme's copy can never leak
// into another scheme's `list` assertions.
// ---------------------------------------------------------------------------
mod fixture {
    pub const EMPTY: &str = "empty.bin";
    pub const BIG: &str = "big.bin";
    pub const BIG_LEN: u64 = 5 * 1024 * 1024;
    pub const NESTED: &str = "sub/dir/nested.txt";
    pub const SPACE: &str = "name with space.txt";
    pub const WEIRD: &str = "weird#name%.txt";
    pub const EMOJI: &str = "emoji-\u{1F600}-name.txt";
    pub const MIXED_EOL: &str = "mixed-eol.txt";

    /// Every top-level entry the seeded tree contains directly under its root — used by the `list`
    /// assertion. `sub` is a directory; everything else is a file.
    pub const TOP_LEVEL: &[(&str, bool)] = &[
        (EMPTY, false),
        (BIG, false),
        ("sub", true),
        (SPACE, false),
        (WEIRD, false),
        (EMOJI, false),
        (MIXED_EOL, false),
    ];
}

/// Join a leaf/relative `name` onto the provider-relative `root` as a FILE path.
///
/// CPE-1950: this **calls** `cpe_vfs::connect::join_remote` rather than reimplementing it. It used to
/// be a hand-written copy carrying the comment "mirrors `join_remote(..., is_dir: false)`" — a claim
/// that was false the day it was written, because the copy trimmed and rejoined but never handled the
/// `is_dir` slash at all, i.e. it mirrored the PRE-CPE-1737 `join_remote`. The rig therefore sent
/// OpenSSH/vsftpd/mod_dav a path shape production had stopped building, and nothing reddened. Calling
/// the real function deletes the claim instead of restating it: the compiler now enforces what the
/// comment used to assert.
fn remote(root: &str, name: &str) -> String {
    cpe_vfs::connect::join_remote(root, name, false)
}

/// The same join as [`remote`], but as a DIRECTORY path — trailing `/` included. CPE-1737 round 2:
/// before this helper existed, `remote()` was the ONLY path-builder in this suite and it never
/// appended a trailing slash, so this real-server rig never sent a slashed directory path to
/// OpenSSH/vsftpd/mod_dav, and the one real bug that shape exposed (WebDAV's `delete`
/// retry-then-escalate guard skipping entirely on an already-slashed path — see `WebdavProvider::delete`)
/// was caught only by an in-process fake-server test, not by this E2E job.
/// `assert_slashed_directory_path_round_trips` below is what actually exercises the new shape here.
fn remote_dir(root: &str, name: &str) -> String {
    cpe_vfs::connect::join_remote(root, name, true)
}

/// A short, collision-resistant tag for scratch paths this run creates — so re-running the suite (or
/// running two scheme tests back to back against server images that happen to share a mount) can never
/// collide on a leftover name from a previous run.
fn unique_tag() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!("{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// The one shared conformance function (CPE-1659's crux)
// ---------------------------------------------------------------------------

/// Run every slice-1 assertion group from the ticket against `provider`, a live connection already
/// opened through [`cpe_vfs::open`]. `root` is the provider-relative directory the fixture tree lives
/// under for THIS scheme (schemes differ in where they land a client: SFTP's `atmoz/sftp` chroots the
/// user to their home dir with the writable mount one level under it, so `root` is `"/upload"`; WebDAV
/// and FTP mount the fixture directly at the provider's own root, `"/"`). `host_dir` is the SAME
/// fixture tree's location on the CI runner's own filesystem (bind-mounted into the container), used to
/// verify every mutation independently of `provider` itself.
fn conformance(provider: &mut dyn FileSystemProvider, root: &str, host_dir: &Path) {
    assert_list_matches_seeded_set(provider, root, host_dir);
    assert_stat_resolves_a_nested_path(provider, root, host_dir);
    assert_big_binary_read_is_byte_exact(provider, root, host_dir);
    assert_write_read_round_trip_for_hostile_names(provider, root, host_dir);
    assert_mkdir_rename_delete_verified_from_host_disk(provider, root, host_dir);
    assert_slashed_directory_path_round_trips(provider, root, host_dir);
    assert_missing_path_is_a_clean_error(provider, root);
}

/// Assertion 1: `list("/")` returns the seeded set with correct `is_dir` and sizes.
///
/// Deliberately checks **presence**, not set-equality: this asserts every entry in
/// [`fixture::TOP_LEVEL`] is present with the right `is_dir`/size, but does not fail if the listing
/// contains extra entries beyond that set. That's on purpose (CPE-1673 review) — FTP and FTPS share the
/// SAME fixture directory on disk (`CPE_E2E_FTP_FIXTURE_DIR`, mounted into both the plain and TLS
/// vsftpd containers), so whichever of the two scheme's conformance runs happens to execute second in
/// this job sees BOTH schemes' writes/renames left over from the first, not just its own seeded set. A
/// set-equality assertion here would make one of FTP/FTPS's runs order-dependently flaky; presence is
/// the correct check for a fixture root that's intentionally shared across schemes.
fn assert_list_matches_seeded_set(provider: &dyn FileSystemProvider, root: &str, host_dir: &Path) {
    let entries = provider.list(root).expect("list the fixture root");
    for (name, is_dir) in fixture::TOP_LEVEL {
        let entry = entries
            .iter()
            .find(|e| e.name == *name)
            .unwrap_or_else(|| panic!("expected {name:?} in the real server's listing, got: {entries:?}"));
        assert_eq!(entry.is_dir, *is_dir, "{name}: is_dir mismatch (server: {entries:?})");
        if !is_dir {
            let want = std::fs::metadata(host_dir.join(name))
                .unwrap_or_else(|e| panic!("host fixture is missing {name:?}: {e}"))
                .len();
            assert_eq!(entry.size, want, "{name}: size reported by the real server must match the file on disk");
        }
    }
}

/// Assertion 2: `stat` resolves a nested path.
fn assert_stat_resolves_a_nested_path(provider: &dyn FileSystemProvider, root: &str, host_dir: &Path) {
    let want_len = std::fs::metadata(host_dir.join(fixture::NESTED)).expect("host fixture has the nested file").len();
    let st = provider.stat(&remote(root, fixture::NESTED)).expect("stat the nested fixture file");
    assert!(!st.is_dir, "the nested fixture path is a file");
    assert_eq!(st.size, want_len);
}

/// Assertion 3: `read` of the 5 MiB binary is byte-for-byte equal to the source (chunking + FTP
/// transfer-mode correctness — an ASCII-mode FTP server would corrupt any embedded CR/LF byte in a
/// "binary" transfer).
fn assert_big_binary_read_is_byte_exact(provider: &dyn FileSystemProvider, root: &str, host_dir: &Path) {
    let want = std::fs::read(host_dir.join(fixture::BIG)).expect("host fixture has the big binary");
    assert_eq!(want.len() as u64, fixture::BIG_LEN, "fixture setup produced the wrong size");
    let got = provider.read(&remote(root, fixture::BIG)).expect("read the 5 MiB binary");
    assert_eq!(got.len(), want.len(), "byte length mismatch reading the 5 MiB binary");
    assert!(got == want, "the 5 MiB binary must round-trip byte-for-byte through the real server");
}

/// Assertion 4: `write` -> `read` round-trip for an empty file, a name with space/`#`/`%`, a
/// non-ASCII/emoji name, and a payload mixing CRLF and lone LF — verified both through the client's own
/// `read` AND independently via the host-mounted directory (never trusting the client to grade its own
/// homework).
fn assert_write_read_round_trip_for_hostile_names(
    provider: &mut dyn FileSystemProvider,
    root: &str,
    host_dir: &Path,
) {
    let tag = unique_tag();
    let cases: Vec<(String, Vec<u8>)> = vec![
        (format!("rt-empty-{tag}.bin"), Vec::new()),
        (format!("rt name with space {tag}.txt"), b"hello space".to_vec()),
        (format!("rt#weird%{tag}.txt"), b"hello weird".to_vec()),
        (format!("rt-emoji-\u{1F600}-{tag}.txt"), "hello emoji \u{1F600}".as_bytes().to_vec()),
        (format!("rt-mixed-eol-{tag}.txt"), b"line1\r\nline2\nline3\r\n".to_vec()),
    ];

    for (name, data) in &cases {
        let path = remote(root, name);
        provider.write(&path, data).unwrap_or_else(|e| panic!("write {name:?}: {e}"));

        let via_client = provider.read(&path).unwrap_or_else(|e| panic!("read back {name:?} via the client: {e}"));
        assert_eq!(&via_client, data, "{name}: client read-back must equal what was written");

        let via_disk = std::fs::read(host_dir.join(name.as_str()))
            .unwrap_or_else(|e| panic!("{name} must exist on the host-mounted fixture disk after write: {e}"));
        assert_eq!(&via_disk, data, "{name}: host-disk bytes (read independently of the client) must match");
    }

    // Clean up so a re-run (or a later scheme sharing a mount) never sees stale scratch entries.
    for (name, _) in &cases {
        let _ = provider.delete(&remote(root, name));
    }
}

/// Assertion 5: `mkdir` / `rename` / `delete` are each verified **from the server side** — every step
/// asserts against the host-mounted directory on disk (the CPE-1049/CPE-1307 "read it back with
/// something other than the client" pattern), never only via the client's own `list`.
fn assert_mkdir_rename_delete_verified_from_host_disk(
    provider: &mut dyn FileSystemProvider,
    root: &str,
    host_dir: &Path,
) {
    let tag = unique_tag();
    let dir_name = format!("case5-{tag}");
    let file_name = "original.txt";
    let renamed_name = "renamed.txt";
    let body: &[u8] = b"case5 body";

    // mkdir — verified as a real directory on disk, independent of the client.
    provider.mkdir(&remote(root, &dir_name)).unwrap_or_else(|e| panic!("mkdir {dir_name}: {e}"));
    let host_meta = std::fs::metadata(host_dir.join(&dir_name))
        .unwrap_or_else(|e| panic!("mkdir did not create a real directory on the host disk: {e}"));
    assert!(host_meta.is_dir(), "{dir_name} exists on disk but is not a directory");

    // write a file inside it, then rename the FILE — verified from disk at each step.
    let orig_remote = remote(root, &format!("{dir_name}/{file_name}"));
    provider.write(&orig_remote, body).unwrap_or_else(|e| panic!("write {orig_remote}: {e}"));
    assert_eq!(
        std::fs::read(host_dir.join(&dir_name).join(file_name)).expect("original file on disk after write"),
        body
    );

    let renamed_remote = remote(root, &format!("{dir_name}/{renamed_name}"));
    provider.rename(&orig_remote, &renamed_remote).unwrap_or_else(|e| panic!("rename {orig_remote}: {e}"));
    assert!(
        std::fs::metadata(host_dir.join(&dir_name).join(file_name)).is_err(),
        "the OLD name must be gone from disk after rename"
    );
    assert_eq!(
        std::fs::read(host_dir.join(&dir_name).join(renamed_name)).expect("renamed file on disk after rename"),
        body,
        "the NEW name must exist on disk with the same bytes after rename"
    );

    // delete the file, then the now-empty directory — verified from disk at each step.
    provider.delete(&renamed_remote).unwrap_or_else(|e| panic!("delete {renamed_remote}: {e}"));
    assert!(
        std::fs::metadata(host_dir.join(&dir_name).join(renamed_name)).is_err(),
        "the renamed file must be gone from disk after delete"
    );

    provider.delete(&remote(root, &dir_name)).unwrap_or_else(|e| panic!("delete (rmdir) {dir_name}: {e}"));
    assert!(
        std::fs::metadata(host_dir.join(&dir_name)).is_err(),
        "the now-empty directory must be gone from disk after delete"
    );
}

/// Assertion 6 (CPE-1737 round 2): `mkdir`/`stat`/`list`/`delete` all accept a directory path built
/// WITH its trailing slash (`remote_dir`, mirroring the shape a real listing row now hands back for a
/// directory — CPE-1737 round 1), verified from the HOST disk at the end, not just via the client's own
/// say-so. This is exactly the shape the in-process WebDAV fake could not have caught the `delete`
/// retry-guard bug through (an already-slashed path used to skip the guard and silently report success
/// having deleted nothing) — only a real server proves the fix end-to-end.
fn assert_slashed_directory_path_round_trips(
    provider: &mut dyn FileSystemProvider,
    root: &str,
    host_dir: &Path,
) {
    let tag = unique_tag();
    let dir_name = format!("slashed-dir-{tag}");
    let slashed = remote_dir(root, &dir_name);

    provider.mkdir(&slashed).unwrap_or_else(|e| panic!("mkdir {slashed} (trailing-slash form): {e}"));
    let host_meta = std::fs::metadata(host_dir.join(&dir_name)).unwrap_or_else(|e| {
        panic!("mkdir (trailing-slash form) did not create a real directory on the host disk: {e}")
    });
    assert!(host_meta.is_dir(), "{dir_name} exists on disk but is not a directory");

    let st = provider.stat(&slashed).unwrap_or_else(|e| panic!("stat {slashed} (trailing-slash form): {e}"));
    assert!(st.is_dir, "stat of the trailing-slash form must still report a directory");

    provider.list(&slashed).unwrap_or_else(|e| panic!("list {slashed} (trailing-slash form): {e}"));

    provider.delete(&slashed).unwrap_or_else(|e| panic!("delete {slashed} (trailing-slash form): {e}"));
    assert!(
        std::fs::metadata(host_dir.join(&dir_name)).is_err(),
        "{dir_name} must be gone from the host disk after a trailing-slash delete — a delete that \
         silently no-ops on this shape (CPE-1737 round 2's WebDAV bug) would leave it behind"
    );
}

/// Assertion 7 (error shape): `read`/`stat` of a missing path returns `Err` — not a panic, not an
/// empty success.
fn assert_missing_path_is_a_clean_error(provider: &dyn FileSystemProvider, root: &str) {
    let missing = remote(root, &format!("definitely-does-not-exist-{}.bin", unique_tag()));
    assert!(provider.read(&missing).is_err(), "reading a missing path must be Err, not a silent success");
    assert!(provider.stat(&missing).is_err(), "stat of a missing path must be Err, not a silent success");
}

// ---------------------------------------------------------------------------
// SFTP — real OpenSSH `sftp-server` (atmoz/sftp)
// ---------------------------------------------------------------------------

fn sftp_connection(name: &str) -> Connection {
    Connection {
        name: name.to_string(),
        scheme: "sftp".into(),
        host: env_var("CPE_E2E_SFTP_HOST"),
        port: env_port("CPE_E2E_SFTP_PORT", 22),
        user: env_var("CPE_E2E_SFTP_USER"),
        auth: AuthMethod::Password,
        path: None,
    }
}

fn sftp_root() -> String {
    env_var_or("CPE_E2E_SFTP_ROOT", "/upload")
}

#[test]
#[ignore = "needs the CPE-1659 docker rig — see .github/workflows/ci.yml"]
fn sftp_conformance_against_real_openssh() {
    let host = env_var("CPE_E2E_SFTP_HOST");
    let port = env_port("CPE_E2E_SFTP_PORT", 22);
    wait_for_tcp(&host, port, Duration::from_secs(30));

    let conn = sftp_connection("e2e-sftp");
    let secret = env_var("CPE_E2E_SFTP_PASS");
    let host_dir = PathBuf::from(env_var("CPE_E2E_SFTP_FIXTURE_DIR"));

    // Trust the server's key outright for the plain conformance run (Tofu, no persistence needed) —
    // the TOFU *mechanism itself* gets its own dedicated tests below, against the same real sshd.
    let mut provider = cpe_vfs::open(&conn, Some(&secret), vec![], HostKeyPolicy::Tofu, None)
        .expect("open() must route sftp:// through cpe-sftp and connect to the real OpenSSH server");

    conformance(&mut *provider, &sftp_root(), &host_dir);
}

/// Assertion 7, part 1: SFTP host-key TOFU against a REAL sshd (CPE-1512 has never once been exercised against
/// OpenSSH): first contact records the container's real host key to a temp known_hosts file, and a
/// reconnect using that recorded key resolves `Trusted` — with NO Tofu leniency needed the second time
/// (policy `Strict`), proving the record actually established trust rather than just Tofu tolerating an
/// unknown key twice.
///
/// CI runs this BEFORE recreating the SFTP container with a fresh host key, then runs
/// [`sftp_host_key_tofu_changed_key_is_refused_against_real_sshd`] after the recreate — see the
/// `Network E2E` job's "SFTP host-key TOFU" steps.
#[test]
#[ignore = "needs the CPE-1659 docker rig — see .github/workflows/ci.yml"]
fn sftp_host_key_tofu_first_contact_then_trusts_against_real_sshd() {
    let host = env_var("CPE_E2E_SFTP_HOST");
    let port = env_port("CPE_E2E_SFTP_PORT", 22);
    wait_for_tcp(&host, port, Duration::from_secs(30));

    let store = PathBuf::from(env_var("CPE_E2E_SFTP_KNOWN_HOSTS_STORE"));
    let conn = sftp_connection("e2e-sftp-tofu");
    let secret = env_var("CPE_E2E_SFTP_PASS");

    let known = known_hosts::load_known_hosts(&store);
    assert!(known.is_empty(), "the known_hosts store must start empty for this test to prove first contact");

    // First contact: Tofu policy accepts the Unknown key and connect_and_record persists it.
    cpe_vfs::open(&conn, Some(&secret), known, HostKeyPolicy::Tofu, Some(&store))
        .expect("first-contact TOFU connect must succeed against the real OpenSSH server");
    let recorded = known_hosts::load_known_hosts(&store);
    assert!(!recorded.is_empty(), "the presented host key must have been persisted to the known_hosts store");

    // Reconnect with the recorded key under Strict policy — must resolve Trusted (no TOFU leniency).
    cpe_vfs::open(&conn, Some(&secret), recorded, HostKeyPolicy::Strict, Some(&store)).unwrap_or_else(|e| {
        panic!("a previously-recorded key must resolve Trusted against the SAME real sshd under Strict policy: {e}")
    });
}

/// Assertion 7, part 2: after CI recreates the SFTP container at the SAME address with a FRESH (regenerated)
/// host key, a reconnect using the known_hosts store from the previous test must resolve `Changed` and
/// be REFUSED — proving CPE-1512's changed-key defense against a real OpenSSH server, not just the
/// in-process fake `russh-sftp` server `crates/sftp`'s own tests use.
#[test]
#[ignore = "needs the CPE-1659 docker rig — see .github/workflows/ci.yml"]
fn sftp_host_key_tofu_changed_key_is_refused_against_real_sshd() {
    let host = env_var("CPE_E2E_SFTP_HOST");
    let port = env_port("CPE_E2E_SFTP_PORT", 22);
    wait_for_tcp(&host, port, Duration::from_secs(30));

    let store = PathBuf::from(env_var("CPE_E2E_SFTP_KNOWN_HOSTS_STORE"));
    let known = known_hosts::load_known_hosts(&store);
    assert!(
        !known.is_empty(),
        "expected the previous test's recorded key in {store:?} — the CI job must run the TOFU steps \
         in order (first-contact test, then recreate the container, then this test)"
    );

    let conn = sftp_connection("e2e-sftp-tofu");
    let secret = env_var("CPE_E2E_SFTP_PASS");
    let err = match cpe_vfs::open(&conn, Some(&secret), known, HostKeyPolicy::Strict, Some(&store)) {
        Ok(_) => panic!("a REGENERATED host key must be refused, not silently accepted"),
        Err(e) => e,
    };
    assert!(err.contains("CHANGED"), "expected the distinct changed-host-key refusal, got: {err}");
}

// ---------------------------------------------------------------------------
// WebDAV — real Apache mod_dav
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs the CPE-1659 docker rig — see .github/workflows/ci.yml"]
fn webdav_conformance_against_real_apache_moddav() {
    let host = env_var("CPE_E2E_WEBDAV_HOST");
    let port = env_port("CPE_E2E_WEBDAV_PORT", 80);
    wait_for_tcp(&host, port, Duration::from_secs(30));

    let conn = Connection {
        name: "e2e-webdav".into(),
        scheme: "webdav".into(),
        host: host.clone(),
        port,
        user: env_var("CPE_E2E_WEBDAV_USER"),
        auth: AuthMethod::Password,
        path: None,
    };
    let secret = env_var("CPE_E2E_WEBDAV_PASS");
    let host_dir = PathBuf::from(env_var("CPE_E2E_WEBDAV_FIXTURE_DIR"));

    let mut provider = cpe_vfs::open(&conn, Some(&secret), vec![], HostKeyPolicy::Tofu, None)
        .expect("open() must route webdav:// through cpe-webdav and connect to the real Apache mod_dav server");

    conformance(&mut *provider, &env_var_or("CPE_E2E_WEBDAV_ROOT", "/"), &host_dir);
}

// ---------------------------------------------------------------------------
// FTP / FTPS — real vsftpd (QTS's own FTP daemon)
// ---------------------------------------------------------------------------

fn ftp_connection(name: &str, scheme: &str) -> Connection {
    Connection {
        name: name.to_string(),
        scheme: scheme.to_string(),
        host: env_var("CPE_E2E_FTP_HOST"),
        port: env_port("CPE_E2E_FTP_PORT", 21),
        user: env_var("CPE_E2E_FTP_USER"),
        auth: AuthMethod::Password,
        path: None,
    }
}

#[test]
#[ignore = "needs the CPE-1659 docker rig — see .github/workflows/ci.yml"]
fn ftp_conformance_against_real_vsftpd() {
    let host = env_var("CPE_E2E_FTP_HOST");
    let port = env_port("CPE_E2E_FTP_PORT", 21);
    wait_for_tcp(&host, port, Duration::from_secs(30));

    let conn = ftp_connection("e2e-ftp", "ftp");
    let secret = env_var("CPE_E2E_FTP_PASS");
    let host_dir = PathBuf::from(env_var("CPE_E2E_FTP_FIXTURE_DIR"));

    let mut provider = cpe_vfs::open(&conn, Some(&secret), vec![], HostKeyPolicy::Tofu, None)
        .expect("open() must route ftp:// through cpe-ftp and connect to the real vsftpd server");

    conformance(&mut *provider, &env_var_or("CPE_E2E_FTP_ROOT", "/"), &host_dir);
}

/// Assertion 8 (FTPS): explicit `AUTH TLS` negotiates against vsftpd's real certificate. A full
/// conformance pass over the `ftps://` scheme can only succeed if the TLS upgrade (and every
/// subsequent data-channel TLS negotiation for LIST/RETR/STOR) genuinely works against vsftpd's real
/// cert — the in-process FTP fake `crates/ftp` tests against has no TLS story to get wrong in the
/// first place.
#[test]
#[ignore = "needs the CPE-1659 docker rig — see .github/workflows/ci.yml"]
fn ftps_conformance_against_real_vsftpd_with_tls() {
    let host = env_var("CPE_E2E_FTP_HOST");
    let port = env_port("CPE_E2E_FTP_PORT", 21);
    wait_for_tcp(&host, port, Duration::from_secs(30));

    let conn = ftp_connection("e2e-ftps", "ftps");
    let secret = env_var("CPE_E2E_FTP_PASS");
    let host_dir = PathBuf::from(env_var("CPE_E2E_FTP_FIXTURE_DIR"));

    let mut provider = cpe_vfs::open(&conn, Some(&secret), vec![], HostKeyPolicy::Tofu, None)
        .expect("open() must route ftps:// through cpe-ftp with AUTH TLS and connect to the real vsftpd server");

    conformance(&mut *provider, &env_var_or("CPE_E2E_FTP_ROOT", "/"), &host_dir);
}
