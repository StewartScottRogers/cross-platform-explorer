---
id: CPE-1502
title: "EPIC: Network protocol — FTP/FTPS provider (cpe-ftp) ⭐ EASIEST-FIRST new protocol"
type: Task
status: Done
priority: Medium
component: Backend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed: 2026-08-29
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

## ACTIVATED 2026-08-09 (Sprint, continuing Network per user direction) — first net-new protocol
Foundation merged: CPE-1510 keychain + CPE-1511 vfs-route + CPE-1513 sidebar UI. FTP mirrors cpe-sftp/cpe-webdav
exactly (a sync provider crate wrapping a connection-oriented protocol). Slice: CPE-1514 (cpe-ftp provider crate
+ ftp/ftps scheme in vfs::open + location.rs). Headless-buildable.

## Closed 2026-08-29

Closed 2026-08-29 (closeout audit) WITH TWO RESIDUALS. 1 child (CPE-1514) Done. Reachable: Network -> Add a connection -> scheme `ftp`.

Verified: `crates/ftp` is a real provider (~2,000 lines, list/stat/read/write/mkdir/delete/rename, 24 unit tests, **no `todo!` or `unimplemented!`**), on suppaftp v10 with `rustls-ring` matching `cpe-sftp`'s backend as the epic specified. Scheme routing, the vfs `open` arm and the port-21 default all ship.

RESIDUAL 1 - **FTPS is unreachable from the UI.** The backend selects TLS purely from the scheme word `ftps`, but `SUPPORTED_SCHEMES` lists only `"ftp"`, so `isSavableScheme("ftps")` is false and the dropdown never offers it - **an FTPS-only server cannot be saved.** The fix is one array entry plus a field hint. This is the one item in this batch a user would hit directly.
RESIDUAL 2 - `FormAuthKind` offers only password/key/access_key, so `AuthMethod::Anonymous` cannot be picked explicitly; public FTP still works via the blank-username heuristic.
Not yet E2E-tested against the QNAP NAS (CPE-1518, open and unparented).
