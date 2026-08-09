---
id: CPE-1523
title: "mDNS/DNS-SD LAN discovery — slice 1: cpe-mdns crate + discover_network_mdns command → Discovered tier"
type: Feature
status: Backlog
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
