---
id: CPE-1504
title: "EPIC: Network protocol — SMB/CIFS (NTLM v1, OS-mount fallback on non-Windows)"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616 / uses CPE-1500 OS-mount). Most-requested protocol, biggest
> crate risk.** Filed 2026-08-08 (sprint PM, Network research). Dormant.

## Why + the honest risk (read before scoping)
SMB is the #1 user request (Windows shares / Samba / NAS) — but **no mature, cross-platform, pure-Rust SMB
client exists as of Aug 2026.** Options are all flawed: `pavao` (libsmbclient C binding, LGPL system dep,
**doesn't exist on Windows**), `smb2`/`smb`(afiffon) (pure-Rust but very new, pre-1.0, single-maintainer; `smb`
has no read pipelining → slow). So this epic is deliberately staged conservatively.

## Scope (v1 = NTLM only; Kerberos DEFERRED)
- **Windows**: pass through native OS SMB (UNC paths) — little/no new client code; browse via `LocalProvider`.
- **Linux/macOS**: **OS-mount fallback** (`mount.cifs` / `mount_smbfs` via CPE-1500) as the safe v1, rather than
  gambling on an immature in-app crate. Re-evaluate a pure-Rust in-app client (`smb2`/`smb-rs`) once it proves
  out — file that as a follow-up, don't block v1 on it.
- Auth: **NTLM/password + guest/anonymous** only for v1.
- **DEFERRED (do NOT scope until v1 ships + demand is real):** SMB **Kerberos** (needs a KDC/SPN/ticket
  delegation — a project in itself; `mount.cifs sec=krb5` is flaky per Samba bug reports). Note it here, no
  separate epic yet.

## Effort / deps / fit
Large (per-OS + the crate gamble). Mostly headless except the Windows native-UNC leg (needs GUI verify). Deps:
CPE-1500 (OS-mount) for the non-Windows path; F1–F3. Call the crate-immaturity risk out to the user at
activation.
