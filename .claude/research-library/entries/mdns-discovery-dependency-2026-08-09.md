# Research — mDNS/DNS-SD crate choice for LAN discovery (CPE-1517), 2026-08-09

**Question:** which Rust crate for cross-platform mDNS/DNS-SD LAN discovery (the complement to the shipped
Windows-native WNet tier), and is it lean enough to adopt?

## Decision: **`mdns-sd`** (adopt)
Pure-Rust, owns its UDP sockets + a background daemon thread, **no async-runtime coupling** (ideal for
`tauri::async_runtime::spawn_blocking`, mirrors the WNet command pattern). Apache-2.0 OR MIT. v0.20.3
(2026-07-26), 2M+ downloads, actively maintained. ~8 small transitive deps (`fastrand`, `flume`, `if-addrs`,
`log`, `mio`, `serde`, `socket2`, `socket-pktinfo`) — most already resolved transitively (via tauri/notify).
Materially lighter than deps already shipped (pdfium/DICOM). Self-describes "beta" — fine for the mainstream
RFC 6762/6763 path; smoke-test vs any LAN mDNS device (printer/Chromecast) before the QNAP.

## Rejected
- **`zeroconf` / `astro-dnssd`** — C-bindings to Bonjour/Avahi; **require the Bonjour SDK installed on Windows**
  (absent on stock machines) → breaks "ships self-contained." Disqualifying for the majority OS.
- **`simple-mdns`** — pure-Rust, fine, but far less battle-tested than `mdns-sd`; second choice only.

## Slice-1 shape (→ CPE-1523)
New `crates/mdns` (`cpe-mdns`) like cpe-ftp/webdav/sftp: pure `map_mdns_service(service_type,host,port,txt)→
Option<NetShare>` (table: `_smb._tcp`→smb:445, `_sftp-ssh._tcp`→sftp:22, `_webdav._tcp`→webdav:80,
`_webdavs._tcp`→davs:443, `_ftp._tcp`→ftp:21, `_nfs._tcp`→nfs:2049; `_afpovertcp`/`_http`/`_qnap`→None) +
impure `discover(timeout)→Vec<NetShare>` (browse the 6 types, ~6s bound, dedup via `net_share::dedup_key`→pub).
Command `discover_network_mdns` (cross-platform, INCLUDED in specta bindings, unlike the per-OS WNet one).
Frontend: `App.svelte loadDiscovered()` runs WNet+mDNS in parallel, merges+dedupes (pure TS helper). Reuses the
existing tier-3 UI (zero Sidebar change). Two folded fixes: extend `discoveredShareToFormInput` for `scheme://`
paths; add `ftp` to `SUPPORTED_SCHEMES` (cpe-ftp ships). New-crate chores: root+src-tauri Cargo.lock, bindings
regen, workspace member. Verify ubuntu drift-guard CI leg before merge.

## Headless-testable vs QNAP-attended
Headless: the mapping table, dedup, TS merge helper, scheme:// prefill, 3-OS compile/clippy. Attended (QNAP,
CPE-1518): live resolve of the QNAP's advertised records + discover→add→connect→browse + Windows-Firewall
multicast-prompt check.

## Sources
- crates.io: mdns-sd, zeroconf, astro-dnssd, simple-mdns (versions/downloads/licenses as of 2026-08-09).
- In-repo patterns: `crates/{ftp,webdav,sftp}`, `crates/server/src/net_share.rs`, `src-tauri/src/lib.rs`
  (`discover_network_windows`). Related: [[qnap-nas-test-target]], [[network-filesharing-program-2026-08-08]].
