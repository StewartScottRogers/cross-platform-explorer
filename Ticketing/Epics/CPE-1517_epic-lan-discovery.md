---
id: CPE-1517
title: "EPIC: Network — LAN device & service discovery (auto-find NAS/servers: mDNS + SSDP/UPnP + SMB browse)"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
epic: CPE-616
created: 2026-08-09
closed:
---

> **ACTIVATED 2026-08-09 (Sprint, decide-and-log).** Decomposed just-in-time. Windows-native leg already
> shipped (CPE-1519, WNet). Cross-platform mDNS leg dep-vetted → **`mdns-sd`** chosen (pure-Rust, no native
> Bonjour SDK; research entry [[mdns-discovery-dependency-2026-08-09]]). First slice filed as **CPE-1523**
> (new `crates/mdns` + `discover_network_mdns` command + merge into the existing Discovered tier). SSDP/UPnP
> (v3) remains dormant. Live verification folds into the QNAP E2E (CPE-1518, 2026-08-10).
> **Network Filesharing program (parent CPE-616).** Filed 2026-08-09 after the user added a real **QNAP
> TS-133** NAS to the LAN and asked for "maximum network functionality." Dormant — activate via
> `/ticketing-epic`.

## Why (the missing discovery layer)
Today the app can *connect* to a remote you already know the address of (SFTP/WebDAV/FTP built; SMB/NFS
planned), and it *reflects* already-mapped OS shares (`net use` on Windows). What it can't do is **find**
devices on the local network — the thing every mature file manager (Finder's "Network", Windows Explorer's
"Network", GNOME Files, superfile-adjacent tools) does: show the NAS/servers on your LAN and let you connect
in one click, no IP typing. With a real NAS now on the network, this is both high-value and finally testable.

## What a NAS actually advertises (QNAP TS-133 as the reference target)
QNAP (and Synology, Samba, macOS, etc.) announce themselves over several LAN discovery mechanisms — we should
listen to the standard ones (no vendor SDK):
- **mDNS / DNS-SD (Bonjour, UDP 5353)** — service types like `_smb._tcp`, `_afpovertcp._tcp`, `_nfs._tcp`,
  `_webdav._tcp` / `_webdavs._tcp`, `_ftp._tcp`, `_sftp-ssh._tcp`, `_http._tcp`, plus QNAP's own `_qnap._tcp`.
  This alone yields host + protocol + port + friendly name.
- **SSDP / UPnP (UDP 1900)** — NAS media/DLNA + device descriptors.
- **SMB/NetBIOS browse (UDP 137 / WS-Discovery)** — Windows-world neighbourhood; lowest-value, highest-noise,
  scope carefully or defer.

## Design — use each OS's OWN discovery (mirror how we enumerate shares)
Just as share enumeration already uses the native path per OS (`net use` / `/proc/mounts` / `mount`), discovery
should lean on each platform's built-in mechanism first, then a cross-platform mDNS listener as the shared
complement:
- **Windows — use the OS's built-in network discovery (Explorer's "Network" folder).** User-requested
  (2026-08-09): "Windows File Explorer has network discovery built in — can we support this?" Yes — enumerate
  the same neighborhood via **`WNetOpenEnum`/`WNetEnumResource` (`RESOURCE_GLOBALNET`)** + `NetShareEnum`, no
  new protocol code and no new dep (the `windows` crate is already in `src-tauri`, add the
  `Win32_NetworkManagement_WNet` feature). Full slice: **CPE-1519**. This is the easiest, highest-parity
  Windows path (modern Windows discovery already rides WS-Discovery + mDNS under the hood; the QNAP shows via
  mDNS).
- **macOS — Bonjour** (`NSNetServiceBrowser` / `dns-sd`), the OS-native DNS-SD browser.
- **Linux — Avahi / mDNS** (the same DNS-SD, via Avahi where present).

## Scope (staged)
- **v1 — Windows-native (CPE-1519):** WNet enumeration → a **"Discovered on your network" tier** in the Network
  section (dedup against saved connections + `net use` shares via `isDuplicateShare`/`dedupeShares`); each
  discovered `\\server\share` → a pre-filled one-click **"＋ Add a connection"**.
- **v2 — cross-platform mDNS/DNS-SD listener** (pure-Rust; `mdns-sd`/`astro-dnssd`-class crate, vet the
  lean-core cost with the Dependency Steward): browse the file-service service-types above. Serves macOS/Linux
  natively AND catches device-advertised hosts that Explorer's chain misses on Windows (a superset complement,
  not a duplicate).
- **v3 — SSDP/UPnP** for devices that do neither.
- Discovery is **opt-in and quiet**: a bounded, cancellable listen (no continuous chatter), respecting
  [[avoid-modal-permission-popups]] and the additive-mode "plain explorer stays light" rule.
- Streams results as they arrive ([[prefer-streaming-liveness]]) — the section paints hosts as they answer.

## Effort / deps / fit
M (mDNS v1 is self-contained; a new dep to justify). Backend listener + a discovered-rows tier in the Network
section (CPE-1498/1516). Independent of the protocol epics but multiplies their value (discovery → pre-filled
connect). **Test target available 2026-08-10** (QNAP TS-133 advertises all of the above once file services are
enabled). Purpose fit: a NAS-owner's single most-wanted convenience; keep it off by default so the sidecar-free
plain explorer is untouched.

## Follow-ups noted (not scoped here)
- **rsync** (QNAP supports it, port 873) and **AFP** (QNAP supports it, port 548, but Apple-deprecated in
  favour of SMB) are separate low-priority protocol candidates — capture as their own epics only if demand is
  real; do not fold into discovery.
