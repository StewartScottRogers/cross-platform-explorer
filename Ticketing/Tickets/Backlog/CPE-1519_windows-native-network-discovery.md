---
id: CPE-1519
title: "Windows-native network discovery (WNetEnumResource / Explorer's Network folder parity)"
type: Feature
status: Backlog
priority: High
component: Multiple
tags: [ready]
epic: CPE-1517
created: 2026-08-09
---
## Why (user, 2026-08-09)
"Windows File Explorer has network discovery built in — can we support this?" Yes, and it's the cheapest,
highest-parity discovery slice on Windows: instead of running our own protocol listener, enumerate the **same
network neighborhood Explorer shows**, via the OS. This is the Windows backend of the discovery epic CPE-1517
(macOS = Bonjour, Linux = Avahi/mDNS handled separately).

## Background (researched 2026-08-09)
- Explorer's **Network** folder is populated by the **WNet** provider chain / **Function Discovery** (WSD +
  SSDP providers). The legacy **NetBIOS Computer Browser** path is dead — SMB1 is off by default since Win10
  1511/1709 — so modern discovery rides **WS-Discovery + mDNS**. (The user's **QNAP advertises via mDNS**, so
  it shows up; a bare Samba box would need `wsdd`. Our enumeration inherits exactly Explorer's reach — and its
  limits.)
- Programmatic entry point: **`WNetOpenEnum(RESOURCE_GLOBALNET, RESOURCETYPE_DISK, …)` → loop
  `WNetEnumResource`**, recursing into container `NETRESOURCE`s (workgroup/domain → server → share). For a
  specific known server, **`NetShareEnum`** lists its shares. This is precisely what Sysinternals ShareEnum and
  Explorer use.
- **No new dependency:** the `windows` crate (v0.56) is already in `src-tauri/Cargo.toml` — just add the
  `Win32_NetworkManagement_WNet` feature (+ `Win32_NetworkManagement_NetManagement` for `NetShareEnum`).

## Scope
- New Tauri command `discover_network_windows()` (`#[cfg(windows)]`), **async + `spawn_blocking`, and
  time-bounded** — WNet enumeration does live network I/O and can hang; bound it like the existing
  `run_bounded_capture`/`enumerate_os_shares` pattern ([[async-all-blocking-commands]]). **Skip unreadable
  containers/servers** rather than failing the whole walk (mirror `list_dir`'s skip-on-error guarantee).
- Walk `RESOURCE_GLOBALNET` recursively; collect servers + their disk shares as `NetShare`-shaped rows
  (reuse/extend `cpe_server::net_share::NetShare` so the frontend tier is uniform with today's `net use` rows).
- Surface as a **"Discovered on your network" tier** in the Network section (CPE-1498/1516), **deduped** against
  saved connections and already-mapped `net use` shares (reuse `dedupeShares`/`isDuplicateShare`). Each
  discovered `\\server\share` → a one-click **pre-filled "＋ Add a connection"** (scheme `smb`, host, path).
- On Windows, a discovered UNC (`\\server\share`) is **immediately browsable via `LocalProvider`** with no SMB
  client (ties into CPE-1504's Windows-native leg) — so discovery is useful the moment it lands, before any
  in-app SMB engine exists.
- Non-Windows: this command is a no-op/absent; those OSes get discovery via CPE-1517's mDNS/Avahi backend.

## Honesty / caveats to encode
- Only returns what **Windows itself has already discovered** — requires the "Network discovery" setting on and
  the device advertising (WSD/mDNS). It is **parity with Explorer, including Explorer's gaps** (e.g. wsdd-less
  Samba). Don't oversell it as universal discovery; the cross-platform mDNS listener (CPE-1517) is the
  device-advertised complement that can even catch things Explorer's chain misses.

## Verify
- Rust unit tests for the `NETRESOURCE` → `NetShare` mapping + the container-recursion/skip logic (pure part);
  the WNet call itself is integration-tested manually against the LAN.
- **Attended, with the QNAP (CPE-1518 / from 2026-08-10):** the QNAP appears in the Discovered tier, dedupes
  against a manually-added connection, and one-click connect pre-fills correctly. Confirm a wedged/slow server
  can't hang the app (the time-bound fires).
- Docs (`src/docs/31-network.md`, CPE-579): note the Discovered tier + the "needs Network discovery on" caveat.

## Notes
Windows-native, no new crate, real parity with what the user already sees in Explorer. Child of CPE-1517;
complements CPE-1504 (SMB) and CPE-1500 (OS-mount). Test target [[qnap-nas-test-target]].
