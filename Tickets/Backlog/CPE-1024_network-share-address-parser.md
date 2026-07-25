---
id: CPE-1024
title: Network-share address parser (pure model)
type: feature
component: Backend
priority: medium
tags: ready
epic: CPE-716
created: 2026-07-25
status: Backlog
---

## Summary
Headless building block for the Drive Bay epic's "map network drive / connect to server" dialog (CPE-716).
A **pure** parser+validator in a new `cpe_server::net_share` module that turns a user-typed share address
into a normalized `NetworkShare { protocol, host, share, path }`, so the connect dialog and the mount glue
(a later slice) share one tested understanding of an address. No network or mount I/O here.

Accept the common forms:
- `smb://host/share/sub/dir` → protocol=Smb, host, share=`share`, path=`/sub/dir`
- `nfs://host/export/path` → protocol=Nfs
- `ftp://host/path` / `sftp://host/path` → protocol=Ftp / Sftp
- Windows UNC `\\host\share\sub` → protocol=Smb (UNC is SMB)
Return a typed error for junk (empty, no host, unknown scheme).

## Acceptance Criteria
- [ ] `parse_share(input) -> Result<NetworkShare, String>` handles smb/nfs/ftp/sftp URLs **and** UNC paths,
      splitting host / share / trailing path correctly (forward or back slashes in UNC).
- [ ] Host is required; empty input, scheme-only, and unknown schemes return `Err`.
- [ ] `NetworkShare` derives `serde::Serialize` + the `specta` cfg derive like sibling model types; a
      `to_url()`/display round-trips a parsed SMB/NFS address back to a canonical `proto://host/share/path`.
- [ ] Pure — no std::net, no mounting; clippy clean both feature modes; ≥6 unit tests incl. UNC + error cases.

## Notes
New module `crates/server/src/net_share.rs`, declared `pub mod net_share;` in `crates/server/src/lib.rs`.
No new dependencies (hand-roll the small parse; do **not** pull a URL crate — lean-core). Follow the pure,
data-first pattern of `shell_menu`/`links`.

## Work Log
2026-07-25 — Implemented `crates/server/src/net_share.rs`: `parse_share(&str) -> Result<NetworkShare, String>`
hand-parses `smb://`/`nfs://`/`ftp://`/`sftp://` URLs and Windows UNC paths (`\\host\share\sub`, back- or
forward-slashed, normalized to `Smb`) into `NetworkShare { protocol, host, share, path }`; empty input,
scheme-only (missing host), and unknown schemes all return `Err`. Added `NetworkShare::to_url()` for the
canonical round-trip. Both types derive `Debug, Clone, PartialEq, Eq, serde::Serialize` +
`#[cfg_attr(feature = "specta", derive(specta::Type))]`, matching `shell_menu`'s pattern; `ShareProtocol`
uses `#[serde(rename_all = "snake_case")]`. No new dependencies. 14 unit tests added (UNC back/forward/mixed
slash, each scheme, host-only/no-share, to_url round-trip for smb/nfs/UNC, empty/scheme-only/unknown-scheme/
junk-input errors) — `cargo test net_share` → 14 passed, 0 failed. `cargo clippy --all-targets -- -D warnings`
and `cargo clippy --all-targets --all-features -- -D warnings` both clean. Branch `cpe-1024-net-share`.
