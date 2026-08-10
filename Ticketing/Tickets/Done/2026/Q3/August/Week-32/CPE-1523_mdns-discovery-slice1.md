---
id: CPE-1523
title: "mDNS/DNS-SD LAN discovery — slice 1: cpe-mdns crate + discover_network_mdns command → Discovered tier"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-1517
created: 2026-08-09
---
## Why
First buildable slice of CPE-1517 (LAN discovery) — the cross-platform **mDNS/DNS-SD** complement to the
Windows-native WNet tier shipped in CPE-1519. mDNS is the ONLY discovery path on macOS/Linux, and on Windows a
superset (surfaces sftp/webdav/ftp/nfs hosts WNet's SMB-only neighborhood never sees — the first "found → one
click → actually browsing" via the live in-app VFS). Planned + dep-vetted 2026-08-09 (see research entry
[[mdns-discovery-dependency-2026-08-09]]).

## Dependency (decided)
**`mdns-sd`** (Apache-2.0 OR MIT, pure-Rust, own daemon thread, no async-runtime coupling, no native Bonjour
SDK). Chosen over `zeroconf`/`astro-dnssd` (C-bindings needing Bonjour installed on Windows) and `simple-mdns`
(less battle-tested). ~8 small transitive deps, mostly already in-tree. Self-describes as "beta" — fine for the
mainstream RFC 6762/6763 path; smoke-test against any LAN mDNS device before the QNAP.

## Scope (mirror the cpe-ftp/webdav/sftp + WNet pattern)
- **New crate `crates/mdns` (`cpe-mdns`)** — `cpe-server` path dep (for `NetShare`) + `mdns-sd`. Split like
  `net_share.rs`:
  - **Pure, unit-tested:** `map_mdns_service(service_type, hostname, port, txt_name) -> Option<NetShare>` —
    table-driven: `_smb._tcp`→smb:445, `_sftp-ssh._tcp`→sftp:22, `_webdav._tcp`→webdav:80,
    `_webdavs._tcp`→davs:443, `_ftp._tcp`→ftp:21, `_nfs._tcp`→nfs:2049; everything else (incl.
    `_afpovertcp._tcp`, `_http._tcp`, `_qnap._tcp`) → `None`. Build `path` via `net_share`'s existing
    `NetworkShare::to_url()`; `name` = TXT friendly-name → hostname fallback (like `map_discovered_share`).
    `kind:"discovered"` (uniform with the WNet tier).
  - **Impure:** `discover(timeout) -> Vec<NetShare>` — `ServiceDaemon`, `browse()` the 6 service types, poll
    non-blocking until a ~6s deadline (match WNet's `DISCOVERY_TIMEOUT`), map + dedup via `net_share::dedup_key`
    (bump it to `pub` — one-line companion change).
- **Command (`src-tauri/src/lib.rs`):** `#[tauri::command] async fn discover_network_mdns() -> Vec<NetShare>`
  = `spawn_blocking(|| cpe_mdns::discover(TIMEOUT))`. **NOT `#[cfg(windows)]`** (identical on all OSes) → normal
  `generate_handler!` entry, and **INCLUDE it in the specta typed bindings** (unlike `discover_network_windows`
  which is excluded for being per-OS). **Regenerate `bindings.gen.ts`** (`cargo run --bin export_bindings
  --features "specta-bindings sidecar-platform"`) — new command → additive drift; and **regenerate
  `src-tauri/Cargo.lock`** (new crate/dep). ⚠ Verify the ubuntu Backend drift-guard CI leg green before merge
  (batch-21 lesson, [[regen-specta-bindings-on-struct-change]]).
- **Frontend glue (`App.svelte`):** in `loadDiscovered()`, call `discover_network_windows` + the new
  `discover_network_mdns` in parallel (`Promise.all`), concat, and dedupe the combined list by path — extract a
  small PURE TS merge helper into `network.ts` (unit-tested in `network.test.ts`, mirroring `flatten_discovered`).
  Tier 3 already renders `kind:"discovered"` rows → no Sidebar.svelte change needed.
- **Two known fixes folded in:** (a) `discoveredShareToFormInput` (network.ts) currently parses only UNC
  `\\host\share`; add a `scheme://host[:port]` branch so mDNS sftp/webdav/ftp rows prefill correctly. (b) add
  `ftp` to `SUPPORTED_SCHEMES` (cpe-ftp already ships, so a discovered `_ftp._tcp` row must validate).

## Verify
- Headless: `map_mdns_service` table (all 6 + the 3 excluded → None), dedup of duplicate resolves, the TS
  merge-and-dedupe helper, the `scheme://` prefill branch — all unit-tested. `cargo build` + `clippy` (both
  feature modes) clean on all 3 CI OSes; `npm run check` + vitest green. `bindings.gen.ts` + `Cargo.lock`
  regenerated & committed.
- **Attended (QNAP, from 2026-08-10 — CPE-1518):** live resolve of the QNAP's advertised
  `_smb/_sftp-ssh/_webdav/_ftp/_nfs._tcp` records; discover → ＋Add → connect → browse for sftp/webdav; confirm
  no Windows Firewall multicast prompt blocks it (if a first-run OS prompt fires, note whether it's a one-time
  OS-level dialog outside app control — [[avoid-modal-permission-popups]]).

## Notes
M effort. Serialize after CPE-1521 (both touch `src-tauri/src/lib.rs`). New-crate chores: root + src-tauri
Cargo.lock ([[multiple-independent-cargo-locks]]), specta bindings regen, workspace member add. Live half rides
on mdns-sd's cross-platform uniformity (can't attend-verify mac/Linux on this Windows box).

## Work Log
- 2026-08-09: **Headless slice landed** (PR pending). New crate `crates/mdns` (`cpe-mdns`), mirroring the
  `cpe-ftp`/`cpe-webdav` shape: `cpe-server` path dep (for `NetShare`) + `mdns-sd = "0.20"` (resolved 0.20.3).
  Pure `map_mdns_service(service_type, hostname, port, txt_name) -> Option<NetShare>` implements the full
  6-entry table (`_smb._tcp.local.`→smb:445, `_sftp-ssh._tcp.local.`→sftp:22, `_webdav._tcp.local.`→webdav:80,
  `_webdavs._tcp.local.`→davs:443, `_ftp._tcp.local.`→ftp:21, `_nfs._tcp.local.`→nfs:2049; everything else,
  including `_afpovertcp._tcp.local.`/`_http._tcp.local.`/`_qnap._tcp.local.`, → `None`), folds a non-default
  port into the host (omitted when it's the scheme's own default, matching `Connection::location()`'s
  convention), falls back a `0` port to the scheme default, trims the DNS record's trailing `.`, and prefers a
  TXT friendly-name over the hostname. Impure `discover(timeout)` drives a real `mdns_sd::ServiceDaemon`,
  browsing all 6 types and polling their receivers (round-robin `try_recv` + a short sleep between passes, per
  the ticket's "poll non-blocking until deadline" spec) until the deadline, mapping + deduping via
  `net_share::dedup_key` (bumped `pub`, one-line companion change). Never panics: a daemon that fails to start,
  a type that fails to browse, or any per-event glitch just yields fewer rows. 17 unit tests (all 6 mapped
  types, all 3 named exclusions + one more unrecognized type, name fallback incl. blank-TXT, port defaulting
  incl. the `port: 0` guard, hostname trim/empty-guard, and a `discover()` smoke test proving it returns
  promptly and never panics with no reachable daemon).
  **Decide-and-log #1 — no workspace exists:** the ticket's "add it to the workspace members in the root
  Cargo.toml" doesn't apply — this repo has **no root `Cargo.toml`/workspace at all**; every provider crate
  (`cpe-ftp`/`cpe-webdav`/`cpe-sftp`/`cpe-vfs`/…) is standalone with its own `Cargo.lock`, and `crates/ftp`'s
  own header comment says so explicitly. Built `crates/mdns` the same way (standalone, own `Cargo.lock`); "root
  + src-tauri Cargo.lock" in Notes above narrows to just `src-tauri/Cargo.lock` (updated) plus the new
  `crates/mdns/Cargo.lock` (created) — there is no root lockfile to update.
  **Decide-and-log #2 — `NetworkShare`/`ShareProtocol` didn't cover webdav/davs:** the ticket says build `path`
  via `net_share`'s existing `NetworkShare::to_url()`, but `ShareProtocol` only had `Smb`/`Nfs`/`Ftp`/`Sftp` —
  no `Webdav`/`Davs` — so it couldn't actually represent 2 of the table's 6 schemes. Extended `ShareProtocol`
  with `Webdav`/`Davs` (scheme words already used elsewhere for saved connections, `connections.rs`'s
  `default_port`/`location.rs`'s `remote_scheme`) and `parse_share` to accept `webdav://`/`davs://` for
  symmetry, with new unit tests (`parses_webdav_and_davs_urls`, `round_trips_davs_via_to_url`) — self-contained
  to `net_share.rs` (grepped: `ShareProtocol`/`parse_share` have no callers outside that file), so this is a
  net-new capability, not a behavior change to anything existing.
  Command `discover_network_mdns()` in `src-tauri/src/lib.rs`: `async fn` + `spawn_blocking`, 6s timeout
  (matching `DISCOVERY_TIMEOUT`), **not** `#[cfg(windows)]`-gated, registered in `generate_handler!` AND
  **included** in the specta `collect_commands!` export (unlike `discover_network_windows`, which stays
  excluded — see the updated comment in `lib.rs`) since mDNS behavior is identical on every OS.
  `bindings.gen.ts` regenerated (`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`
  in `src-tauri/`) — purely additive (`discoverNetworkMdns()`), verified clean via `git diff --exit-code` after
  staging. Both `src-tauri/Cargo.lock` (updated: `cpe-mdns` + `mdns-sd`/`flume`/`if-addrs`/`socket-pktinfo`
  transitive deps) and the new `crates/mdns/Cargo.lock` are committed.
  Frontend: `network.ts` gained `mergeDiscovered(windows, mdns)` (pure, path-deduped via the existing
  `shareDedupKey`, WNet rows win a duplicate) and a `scheme://host[:port]` branch in
  `discoveredShareToFormInput` (alongside the existing UNC branch) so an mDNS row pre-fills its real
  scheme/host/port with the row's own label as the connection name. `SUPPORTED_SCHEMES` gained `"ftp"`
  (cpe-ftp already ships); found and fixed a latent bug this surfaced — `DEFAULT_PORTS` had no `ftp` entry, so
  an ftp connection with a blank port field would have built with port `0` — added `ftp: 21`. Fixed a
  now-stale existing test (`rejects an unsupported scheme` asserted `ftp` was rejected; retargeted to `s3`) and
  added a new one proving `ftp` now builds a valid connection. `App.svelte`'s `loadDiscovered()` now runs
  `discover_network_windows` (raw `invoke`, unchanged — still excluded from typed bindings) and
  `commands.discoverNetworkMdns()` (typed) via `Promise.all`, each independently degrading to `[]` on failure,
  then merges via `mergeDiscovered`. 22 new/updated tests in `network.test.ts` (mDNS scheme-authority parsing
  incl. port/name-fallback, `mergeDiscovered` incl. cross-tier dedup and within-tier dedup, the `ftp`
  accept/reject test fix). Docs (`src/docs/31-network.md`) rewrote the "Discovered on your network" section:
  dropped the "(Windows)" qualifier, explained the two parallel scans (WNet Windows-only + mDNS
  cross-platform/superset), the pre-fill behavior for mDNS rows including the NFS "informational only, no
  client yet" caveat, and the per-scan independent-degradation + one-time-OS-multicast-prompt caveats.
  **Verify, all green:** `cargo build` + `cargo clippy --all-targets -- -D warnings` clean for `crates/mdns`,
  `crates/server`, and `src-tauri` in both feature modes (default and `--features sidecar-platform`);
  `cargo test --lib` 17/17 (`crates/mdns`) + 33/33 `net_share` tests (`crates/server`, incl. new
  webdav/davs coverage); `npm run check` 0 errors; full `npx vitest run` 233 files / 2641 tests green (includes
  `network.test.ts`'s 51 tests and confirms nothing else regressed, incl. `sectionDocs.test.ts`).
  **Still owed** (per the ticket's Verify section): the attended live-LAN resolve against the QNAP (from
  2026-08-10, CPE-1518) — discover → ＋Add → connect → browse for sftp/webdav, and confirming no Windows
  Firewall multicast prompt blocks it — plus the user's visual sign-off. Both require the built app + a real
  LAN device and couldn't be done headlessly in this pass.
