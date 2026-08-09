---
id: CPE-1517
title: "EPIC: Network — LAN device & service discovery (auto-find NAS/servers: mDNS + SSDP/UPnP + SMB browse)"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
epic: CPE-616
created: 2026-08-09
closed:
---
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

## Scope (staged; mDNS first)
- **v1 — mDNS/DNS-SD listener** (pure-Rust; `mdns-sd` or `astro-dnssd`-class crate, vet the lean-core cost with
  the Dependency Steward): passively browse the file-service service-types above, surface discovered hosts as a
  **"Discovered on your network" tier** in the Network section (dedup against saved connections + OS shares,
  reusing `isDuplicateShare`/`dedupeShares`). Each discovered host maps its advertised service → a pre-filled
  **"＋ Add a connection"** (protocol + host + port already chosen) so one click connects.
- **v2 — SSDP/UPnP** for devices that don't do mDNS.
- **v3 (DEFER unless demanded)** — SMB/NetBIOS/WS-Discovery browse.
- Discovery is **opt-in and quiet**: a bounded, cancellable background listen (no continuous chatter),
  respecting [[avoid-modal-permission-popups]] and the additive-mode "plain explorer stays light" rule.
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
