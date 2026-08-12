---
id: CPE-1505
title: "EPIC: Network protocol — NFSv3 (Linux/macOS; Windows deferred)"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616 / may use CPE-1500 OS-mount). Unix-native filesystem.**
> Filed 2026-08-08 (sprint PM, Network research). Dormant.

## Why + reality check
NFS is Unix-native (Linux/macOS NAS). Auth (**AUTH_SYS** — UID/GID trust, effectively anonymous) is trivially
simple, but two real limits: (a) **Windows has no reliable native NFS client** (optional, Pro/Enterprise-only,
absent on Home) → cross-platform is a stretch from day one; (b) the RPC/XDR transport adds protocol plumbing.

## Scope (v3 only)
- Linux/macOS: in-app client via **`nfs3_client`** (async NFSv3 over RPC — still maturing) implementing
  `FileSystemProvider`, OR **OS-mount** (`mount -t nfs` via CPE-1500) as the safe path — decide at activation.
- **Windows: explicitly out of scope for v1** (product decision, not a technical blocker).
- Auth: AUTH_SYS (needs CPE-1501's `Anonymous`-ish handling).
- **DEFERRED:** **NFSv4** (stateful, ACLs, ID-mapping — materially harder). Note here, no separate epic yet.

## Effort / deps / fit
Medium. Headless for the client half; Windows exclusion documented. Deps: F1–F3 (+ CPE-1500 if OS-mount path).
Lower priority than SMB (narrower audience). `nfs3_client` maturity is a flagged risk.

## Test target — real QNAP TS-133 NAS (available 2026-08-10)
QTS exposes **NFS (ports 111 + 2049)** as an enable-in-Control-Panel service, so the user's QNAP gives a real
NFSv3 export to validate against — mainly the Linux/macOS legs (the OS-mount path via CPE-1500 first, the
`nfs3_client` in-app path if/when it proves out). Windows stays out of v1 as documented. QNAP advertises NFS
over mDNS `_nfs._tcp` → feeds the discovery epic CPE-1517.
