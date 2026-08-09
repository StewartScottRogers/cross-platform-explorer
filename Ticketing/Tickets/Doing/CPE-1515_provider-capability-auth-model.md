---
id: CPE-1515
title: "FileSystemProvider capability descriptor + auth-model growth (Anonymous/Token/AccessKey) — unblocks S3/cloud"
type: Feature
status: Doing
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1501
created: 2026-08-09
---
## What (CPE-1501 F5 — enabling; headless)
Three additive extensions to the provider layer so protocols beyond SFTP/WebDAV/FTP fit:
1. **Capability descriptor** on `FileSystemProvider` (or a companion): `supports_write/rename/random_read/watch`,
   `has_real_dirs` — so the UI + router can adapt to read-only shares, S3 (no real directories), FTP (weak
   rename). Existing providers return sensible defaults (Local/SFTP/WebDAV/FTP: has_real_dirs=true, writable=true).
2. **Auth-model growth**: today `connections.rs` `AuthMethod = Password | Key`; add `Anonymous`,
   `Token{token}` (OAuth/bearer, for later cloud), `AccessKey{id, secret_ref}` (S3 SigV4). Keep it
   backward-compatible (serde — existing connections.json still parses). Note CPE-1514 currently does anonymous
   as a cpe-vfs heuristic; this is where Anonymous becomes first-class (migrate the heuristic to the enum if clean).
3. **Streaming read** already exists per-provider (sftp/webdav/ftp stream); ensure the trait/capability documents
   it (a `random_read` capability flag) — no behavior change required here.

## How
- Add the capability struct + a `capabilities(&self) -> ProviderCapabilities` trait method (default impl =
  full-POSIX so existing providers need no change; override where they differ). Well-tested via `FakeProvider`.
- Extend `AuthMethod` in `crates/server/src/connections.rs` with the new variants; update the `vfs::open` auth
  mapping to handle them (AccessKey → pass id+secret to a future S3 provider; Token → future cloud; Anonymous →
  first-class). Regenerate `bindings.gen.ts` if `AuthMethod`/`Connection` (specta) change (they will — additive).
- NO new Cargo dep. Keep the diff additive + backward-compatible (a v1 connections.json with Password/Key still loads).

## Verify (HEADLESS)
`cargo test` (crates/server + crates/vfs): capability defaults for Local/Fake; a provider reporting
has_real_dirs=false / read-only behaves; AuthMethod round-trips through serde incl. the new variants AND an old
Password/Key connections.json still deserializes (back-compat); vfs::open maps each auth variant correctly (via
FakeProvider). `cargo clippy --all-targets -D warnings` both feature modes. Regenerate + commit bindings +
`src-tauri/Cargo.lock` if touched.

## Ship
Move CPE-1515 Doing→Done, Work Log (the capability struct, the AuthMethod variants + back-compat approach, the
vfs mapping). Note S3 (CPE-1503) now buildable on this. Effort M, mostly pure/backend.
