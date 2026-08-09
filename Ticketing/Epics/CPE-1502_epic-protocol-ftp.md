---
id: CPE-1502
title: "EPIC: Network protocol — FTP/FTPS provider (cpe-ftp) ⭐ EASIEST-FIRST new protocol"
type: Task
status: Proposed
priority: Medium
component: Backend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616). FIRST net-new protocol — the "do the easiest first" pick.**
> Filed 2026-08-08 (sprint PM, Network research — research-library `network-filesharing-program-2026-08-08`).
> Dormant.

## Why FTP is the easiest genuinely-new protocol (after the foundation)
It's structurally the closest cousin to the two providers CPE already shipped (`cpe-sftp`, `cpe-webdav`): a sync
client crate wrapping a connection-oriented remote protocol. Mature crate, simple auth, smallest crate-maturity
gamble in the whole catalogue. This is the flagship "first protocol milestone" the user asked for.

## Scope
- New `crates/ftp` (`cpe-ftp`) implementing `FileSystemProvider`, mirroring `cpe-webdav`'s crate shape.
- Crate: **`suppaftp`** (the maintained fork of the abandoned/vulnerable `ftp` crate), `rustls-ring` TLS feature
  to match the `ring` backend CPE already chose for `cpe-sftp`. Sync.
- Auth: user+pass, **Anonymous** (needs CPE-1501 auth-model growth), optional FTPS/TLS. Port 21.
- Register `ftp`/`ftps` scheme arms in `cpe_vfs::open` + `location.rs`. Inherits the transfer/traversal guards.

## Effort / deps / fit
Small–Medium. **Headless-buildable** (pure backend crate + `FakeProvider`-style tests; test against a local FTP
server fixture). Deps: F1–F3 (CPE-1497/1498/1499) live + F5 (CPE-1501) for Anonymous auth. Backend-only until the
UI wiring (which F2/F3 already provide). Source: suppaftp (crates.io, maintained ~2026).
