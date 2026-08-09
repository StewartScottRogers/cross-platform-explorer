---
id: CPE-1514
title: "cpe-ftp provider (FTP/FTPS) — implement FileSystemProvider via suppaftp, register ftp/ftps in vfs::open"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1502
created: 2026-08-09
---
## What (first net-new Network protocol — mirrors the shipped cpe-sftp/cpe-webdav pattern)
New `crates/ftp` (`cpe-ftp`) crate implementing the existing `FileSystemProvider` trait (list/stat/read/write/
mkdir/delete/rename) over FTP/FTPS, and a `ftp`/`ftps` scheme arm in `cpe_vfs::open` (`crates/vfs`) + the
`location.rs` URI parser. Once done, an `ftp://host/path` connection browses through the same CPE-1511 command
routing that SFTP/WebDAV already use.

## How
- Crate: **`suppaftp`** (maintained fork of the abandoned/vulnerable `ftp` crate), with the `rustls-ring` TLS
  feature to match the `ring` backend `cpe-sftp` already chose (consistency + no new TLS stack). Sync API — no
  async runtime needed (like `cpe-webdav`). Study `crates/webdav/src/lib.rs` + `crates/sftp/src/lib.rs` for the
  exact provider shape (connect(config, secret, ...) → provider; the FileSystemProvider impl; bounded reads;
  streaming where applicable) and mirror it.
- Auth: **user+pass** and **Anonymous** (anonymous = user `"anonymous"`, password an email-ish placeholder or
  empty) — anonymous is common for public FTP and can be handled directly here without waiting on CPE-1501's
  broader auth-model epic; document the choice. FTPS (explicit TLS) via suppaftp's TLS feature; port 21 default.
- Register `ftp`/`ftps` in `location.rs` (`Scheme`) + `cpe_vfs::open` scheme match (currently returns
  "unsupported scheme" for anything but sftp/webdav). Path traversal: remote names flow through the same
  `is_safe_name`/guarded_join guards CPE-1511 applies at the listing layer — but ALSO ensure the provider's own
  `download_tree`/entry handling can't be fooled (mirror cpe-sftp/cpe-webdav's hardening).
- Bounded reads / resource-exhaustion conventions (never buffer a whole remote file unbounded; stream).

## Verify (HEADLESS)
- The crate's own tests: mirror how `crates/sftp`/`crates/webdav` test their providers. If suppaftp offers an
  in-process/test FTP server or a mock, use it; else use a lightweight local FTP server fixture spun up in the
  test (gated/`#[ignore]` if it needs a real server binary). At minimum: connect (mock), list a dir, stat a file,
  read a file, error on bad auth — no panic, bounded.
- **vfs routing:** a test proving `cpe_vfs::open` dispatches an `ftp://` URI to `cpe-ftp` (and `ftps://` too).
- `cargo test` (crates/ftp + crates/vfs + crates/server as touched) green; `cargo clippy --all-targets -D
  warnings` both feature modes; **commit any Cargo.lock delta incl `src-tauri/Cargo.lock`** if the app pulls the
  new crate (suppaftp is a new dep — Dependency Steward: justified, the maintained FTP crate, rustls-ring feature
  no new TLS stack; regenerate + commit all affected lockfiles). Regenerate `bindings.gen.ts` only if a
  command/specta surface changed (likely not — this is provider-internal).

## Ship
Move CPE-1514 Doing→Done, Work Log (crate shape, auth handling, scheme registration, dep+lockfile note, tests).
The Network sidebar (CPE-1513) will let a user add an `ftp://` connection with no further UI work. Effort S–M.

## Work Log (2026-08-09)

**Crate shape.** New standalone `crates/ftp` (`cpe-ftp`), sibling of `cpe-sftp`/`cpe-webdav` (out of any
workspace). `FtpProvider` wraps `suppaftp::RustlsFtpStream` in a `Mutex` for interior mutability (FTP is a
stateful single-control-connection protocol, but `FileSystemProvider::list/stat/read` take `&self` — the
same shape the app's own provider pool already wraps every provider in, `Arc<Mutex<BoxedProvider>>`).
Implements all 6 `FileSystemProvider` ops: `list`/`stat` (parse `LIST` output — POSIX format tried first,
DOS as fallback, matching `suppaftp`'s own documented convention), `read` (via `retr_as_stream` +
`finalize_retr_stream`, copied in fixed 64 KiB chunks — never one unbounded `read_to_end` call), `write`
(`put_file`), `mkdir`/`delete` (`rm` then `rmdir` fallback, mirroring `cpe-sftp`'s file-then-dir delete
shape), `rename` (`RNFR`/`RNTO` under the hood).

**Auth.** `FtpAuth::{Password, Anonymous}`. Anonymous sends the wire user `"anonymous"` with an RFC
1635-style placeholder password (`ANONYMOUS_PASSWORD`, an email-shaped, `.invalid`-TLD string — never a
real credential, never logged) rather than an empty string, since some servers still expect *something*
plausible in that field even though virtually none validate it. Handled directly in this crate + `cpe-vfs`'s
dispatch (`ftp_auth_from`: a blank or literally-"anonymous" connection user → `Anonymous`, else `Password`
from the keychain secret) — not wired through the shared `cpe_server::connections::AuthMethod` enum, to
keep this ticket's diff scoped to the FTP crate + routing rather than reshaping the connection model (that
broader wiring, if the Network sidebar wants an explicit "Anonymous" auth-method radio, is follow-up work,
tracked implicitly under CPE-1501).

**FTPS.** Explicit TLS (`AUTH TLS` upgrade in place on the plaintext control channel, `into_secure` via
`suppaftp`'s `rustls-ring` feature — matching `cpe-sftp`'s `ring` backend choice), never the deprecated
implicit-TLS mode. Port 21 default for both `ftp` and `ftps` (explicit FTPS shares plain FTP's port, unlike
legacy implicit-TLS port 990). Root store: Mozilla's bundled roots (`webpki-roots`), matching `suppaftp`'s
own README FTPS example — no OS trust-store access needed, so it behaves identically everywhere. Verified
end-to-end (root store built, `into_secure` invoked, TLS ClientHello actually sent) via a negative test:
`ftps://` against a plaintext-only server fails cleanly (`Err`, not a panic) — no in-process TLS-terminating
test fixture was built (out of scope for this ticket's headless verify bar), so a full FTPS handshake isn't
exercised in CI, only that the code path runs and fails gracefully when misconfigured.

**Scheme registration.** `crates/server/src/location.rs`: added `Scheme::Ftp` (`ftp`/`ftps` both map to it,
same shape as `webdav`/`davs`→`Scheme::Webdav`); `fs_route.rs`'s `scheme_label` match updated (was
non-exhaustive without it). `connections.rs`: `default_port` returns 21 for `ftp`/`ftps`. `crates/vfs`: new
`cpe-ftp` dependency, `"ftp" | "ftps"` arm in `open()`.

**Traversal hardening.** Every `LIST` entry name is filtered through `cpe_server::transfer::is_safe_name`
before becoming a `ProviderEntry` (same CPE-1461/1462 defense `cpe-sftp`'s READDIR filter and
`cpe-webdav`'s PROPFIND filter apply) — a hostile server returning `..` or a `/`-embedded name in its
listing is dropped before it can reach the local-write sink in `download_tree`. Covered by a dedicated test
using a synthetic server-side LIST response (cross-platform-safe — no OS-specific "can this filename even
be created" constraint, unlike the SFTP sibling test which is Unix-gated for that reason).

**Dependency + lockfiles.** New dep: `suppaftp` 10.0.1 (`default-features = false`, `rustls-ring` feature —
maintained, actively-developed fork of the abandoned/vulnerable `ftp` crate; the `ring` backend adds no new
TLS stack and matches `cpe-sftp`), plus `webpki-roots` 1.x (already present in the tree at 1.0.8/0.26.11
transitively — this adds a direct edge onto an already-resolved crate, not a new major dependency). `rustls`
itself is NOT a direct dependency: `cpe-ftp` builds the FTPS `ClientConfig` through `suppaftp`'s own
re-export (`suppaftp::rustls`) so the type handed to `into_secure` is guaranteed to match the exact version
`suppaftp` compiled against. Lockfiles regenerated + committed: `crates/ftp/Cargo.lock` (new),
`crates/vfs/Cargo.lock` (adds the `suppaftp`/`rustls`/`chrono`/etc. subtree — the standalone crate's own
lock didn't have it yet, unlike the big app), `src-tauri/Cargo.lock` (regenerated via `cargo check`, NOT
`cargo generate-lockfile` — the latter bumped ~990 unrelated packages to "latest compatible", a huge diff
that had nothing to do with this ticket; `cargo check` instead added exactly the ~46 lines this ticket's new
dependency edge needs: the `cpe-ftp`/`suppaftp`/`lazy-regex` package entries, reusing the app's
already-resolved `webpki-roots 1.0.8` rather than pulling a duplicate version).

**Tests.** `crates/ftp`: 8 tests against an in-process, hand-rolled FTP server (raw TCP, PASV-mode data
connections, no Docker/real daemon — runs identically on all 3 CI OSes) — connect/list/stat/read, full
write/mkdir/rename/delete round trip, wrong-password rejection, anonymous login, hostile-name filtering,
and the FTPS-against-plaintext-server negative case. Found and fixed a genuine intermittent race in the
test server itself during development (a not-found `RETR`'s error path dropped the PASV listener without
ever accepting the connection a racing client might already be mid-connect to — fixed by only consuming the
listener once the file is confirmed to exist; stress-tested 20x clean after the fix, 0 failures). `crates/vfs`:
+2 tests (`open` dispatches both `ftp://` and `ftps://` to the FTP provider; `ftp_auth_from` picks
Anonymous vs Password correctly). `crates/server`: +2 in `location.rs` (ftp/ftps parse to `Scheme::Ftp`),
`fs_route.rs`'s existing remote-scheme test extended with both words.

**Results:** `cargo test` — cpe-ftp 8/8, cpe-vfs 16/16, cpe-server full lib 1799/1799, all green.
`cargo clippy --all-targets -D warnings` clean on cpe-ftp, cpe-vfs, cpe-server, and `src-tauri` (the app has
no feature flags gating this code path — it's wired unconditionally through `cpe-vfs::open`, so there's only
one relevant "mode" here, unlike `cpe-server`'s optional `index`/`pdf-thumb`/etc. features). `src-tauri
cargo check` confirms the whole app still builds with `cpe-ftp` wired in transitively.
`.github/workflows/ci.yml` updated: new "ftp — clippy + test" step + rust-cache workspace entry, in the
sftp→webdav→ftp→vfs dependency order.

**For the Reviewer to scrutinize:** (1) the FTPS TLS path has no in-process handshake test, only a
fails-cleanly-against-plaintext negative test — a real FTPS server round-trip has never actually run in CI;
(2) `read()`'s trait contract still returns a fully-materialized `Vec<u8>` (same as `cpe-sftp`/`cpe-webdav`
— neither imposes a hard byte cap either), so "bounded" here means chunked-copy-loop, not a size ceiling;
(3) Anonymous auth is a heuristic in `cpe-vfs` (blank/`"anonymous"` username), not a first-class
`AuthMethod::Anonymous` variant in the shared connections model — the Network sidebar has no explicit
"connect anonymously" toggle yet; (4) the traversal-name test seeds a synthetic LIST response rather than
real hostile-named files (deliberate, for cross-platform safety, but worth confirming it faithfully
represents what `parse_list_line` + `is_safe_name` see from a real hostile server).
