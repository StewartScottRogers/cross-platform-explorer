---
id: CPE-1512
title: "Persist first-contact SFTP host keys to an app-managed known_hosts (complete TOFU)"
type: Bug
status: Done
priority: Medium
component: Backend
tags: [ready, security]
epic: CPE-616
parent: CPE-1499
created: 2026-08-08
---
## Vector (found by CPE-1511's adversarial review)
CPE-1511 wired SFTP/WebDAV browsing with `HostKeyPolicy::Tofu`. It reads the user's real `~/.ssh/known_hosts`
(so a host the user already pinned via OpenSSH gets genuine CHANGED-key MITM protection), and a CHANGED/REVOKED
key refuses loudly. BUT there is **no `save_known_hosts`/persist path** (`known_hosts.rs` only loads/parses/
verifies), and `presented_host_key` is never read by non-test code. So a **first-contact** host (not already in
`~/.ssh/known_hosts`) is accepted under Tofu and **never written back** → effectively **trust-on-every-use** for
hosts only the app ever touches: a later MITM presenting a different key is again just `Unknown` → silently
accepted; the CHANGED path can never fire for app-only hosts.

## Not a regression, but a real completeness gap
`origin/main` had no remote at all, and CHANGED protection IS real for OpenSSH-pinned hosts — so this is not a
regression and CPE-1511's own AC ("changed key refuses loudly") holds for pinned hosts. But TOFU is incomplete
without persistence.

## Fix
After a successful **first-contact (`Unknown`)** SFTP connect, persist the presented host key to an
**app-managed** known_hosts store (NOT the user's `~/.ssh/known_hosts` — never silently mutate that), so
subsequent connects to that host resolve to `Trusted` (or `Changed` on a key swap → loud refuse). Add
`save_known_hosts`/an append to `known_hosts.rs`, wire the SFTP connect path to record `presented_host_key` on
first contact, and merge the app store with `~/.ssh/known_hosts` at verify time (user's pins win). Headless-
testable: first connect records the key; a second connect with the SAME key → Trusted (no reprompt); a second
connect with a DIFFERENT key → Changed → refused. Surface a first-contact "trust this host?" affordance in the
Network UI later (CPE-1498) — for now recording-on-first-use is the baseline (document the UX choice).

## Notes
Epic CPE-616 / Network program. Security-completeness. Filed from CPE-1511 review.

## Work Log (2026-08-09)

Implemented end to end, headless, no new dependencies.

**`crates/server/src/known_hosts.rs`** — TOFU persistence + merge, hand-written `known_hosts` line
format (same shape `parse_known_hosts` already reads):
- `save_known_hosts(path, entries)` — serializes entries back to `known_hosts` lines; writes to a
  sibling `.tmp` file and renames into place (atomic on all three target OSes — no half-written store on
  a crash mid-write).
- `append_host_key(path, host, port, key_type, key_b64)` — read-modify-write; a no-op if an entry for the
  exact same `(host, port, key_type, key_b64)` already exists (no duplicate entries on re-record); a
  malformed pre-existing line is skipped by the existing `parse_known_hosts`, never a panic.
- `default_app_known_hosts_path()` — the **app-managed** store: `%APPDATA%\cross-platform-explorer\known_hosts`
  on Windows, `$XDG_CONFIG_HOME` or `~/.config/cross-platform-explorer/known_hosts` elsewhere (sits next
  to `connections.json`, mirroring `connections::default_connections_path`'s resolution). Deliberately
  distinct from `default_known_hosts_path()` (the user's real `~/.ssh/known_hosts`) — this module never
  writes to that file.
- `load_merged_known_hosts(app_path, ssh_path)` — loads + merges both stores for a verify call. **The
  user's `~/.ssh` entries always win**: an app-store entry whose host-token + key-type is already covered
  by an `ssh_path` entry is dropped from the merge (regardless of whether the key material agrees), so a
  stale/tampered app-store entry can never override a genuine OpenSSH pin.
- 12 new tests (round-trip, first-contact record → Trusted/Changed, no-dup, port/host disambiguation,
  malformed-line safety, merge precedence, app-only entries, both-files-missing).

**`crates/sftp/src/lib.rs`** — `SftpProvider::connect_and_record(config, known, policy, record_path)`:
calls `connect` as before, and on an `Unknown` verdict only, appends the presented key to the
app-managed store at `record_path` (`None` skips persistence, e.g. no app config dir on this platform,
without failing the connect). `Trusted` is already recorded (no re-record/reprompt); `Changed`/`Revoked`
are refused by `connect` itself before this ever runs, so a swapped key can never get auto-trusted here.
Persistence failure is swallowed (`let _ =`) — a working session must not fail just because the store
couldn't be written. 3 new tests against the crate's existing in-process SSH/SFTP test server: full
first-contact → record → same-key-Trusted sequence (asserted under `Strict`, proving the record itself
established trust, not TOFU leniency) + no-dup on re-record; a swapped key refused as `Changed` with the
store left untouched (no auto-trust); `record_path: None` behaves exactly like plain `connect`.

**`crates/vfs/src/lib.rs`** / **`crates/vfs/src/connect.rs`** — wiring: `open()` and
`ProviderOpener::open`/`VfsOpener`/`connected_provider` all gained a `record_first_contact: Option<&Path>`
parameter, threaded down to `SftpProvider::connect_and_record` (WebDAV/FTP ignore it, unchanged). Added a
seam test in `connect.rs` (`FakeOpener` now captures the `record_first_contact` it was called with) proving
`connected_provider` forwards the app-managed store path unchanged down to the opener — the real recording
behaviour is exercised end-to-end against a live server in `cpe-sftp`'s own tests, not reachable through
this trait-object boundary.

**`src-tauri/src/lib.rs`** (`remote_provider_for`) — the app entry point: loads both
`default_app_known_hosts_path()` and `default_known_hosts_path()`, merges them via
`load_merged_known_hosts` (falling back gracefully if either resolves to `None` on some platform), and
passes the app store path through to `connected_provider` as `record_first_contact`. This is the only
production call site; the whole first-contact → persist → Trusted-next-time loop is now live under
`HostKeyPolicy::Tofu`.

**Tests**: `cargo test` — cpe-server 20/20 known_hosts tests pass; cpe-sftp 17/17 pass (incl. the 3 new
ones); cpe-vfs 16/16 pass (incl. the new seam test); full workspace `cargo build --all-targets` (default +
`--features sidecar-platform`) compiles clean. `cargo clippy --all-targets -D warnings` clean on
cpe-server (default + `--features index`), cpe-sftp, cpe-vfs, and the src-tauri app (default +
`--features sidecar-platform`).

**Cargo.lock**: `crates/sftp/Cargo.lock` was regenerated and committed — pre-existing drift unrelated to
this ticket (it hadn't been touched since the original CPE-899 SFTP-provider commit, while
`crates/server/Cargo.toml` gained many dependencies since; building this crate resynced it). No new
dependency was added by this ticket. `crates/server/Cargo.lock`, `crates/vfs/Cargo.lock`, and
`src-tauri/Cargo.lock` had no delta.

**Reviewer should scrutinize**: (1) `~/.ssh/known_hosts` is never written to anywhere in this diff — only
read (`load_known_hosts`); persistence only ever targets `default_app_known_hosts_path()`. (2) A `Changed`
verdict is never auto-trusted/overwritten — `connect_and_record` only records on `Unknown`, and `connect`
itself already refuses `Changed`/`Revoked` before the recording branch is reached. (3) No duplicate
entries on repeated first-contact/re-record — `append_host_key`'s exact-match check. (4) No panic on a
malformed existing store line — reuses `parse_known_hosts`'s already-tolerant parsing.
