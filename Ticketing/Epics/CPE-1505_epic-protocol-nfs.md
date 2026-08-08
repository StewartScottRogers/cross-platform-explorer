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
> Filed 2026-08-08 (workshift PM, Network research). Dormant.

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
