---
slug: network-filesharing-program-2026-08-08
title: Network / "mount anything" filesharing program — architecture + protocol ladder + epic tree
tags: [product, epics, network, filesharing, smb, sftp, webdav, ftp, s3, nfs, vfs, pm-reference]
status: current
created: 2026-08-08
---
## User directive
A **Network section in the left pane** that can **mount every network filesharing protocol, cross-platform**
("mount anything and everything"). Big — broken into many epics, **easiest protocol first**. Two-researcher
deep pass (architecture/audit + protocol landscape).

## Headline: ~60% already built — EXTEND, don't restructure
Two existing dormant epics already own this: **CPE-616** (in-app VFS arm) + **CPE-716** (OS-mount drive-bay arm).
Reconcile them under ONE **hybrid** program. Built + CI-green already:
- Provider seam: `FileSystemProvider` trait (`crates/server/src/provider.rs`), `cpe_vfs::open` scheme router
  (`crates/vfs`), `fs_route` command guard (remote deliberately walled off), `location.rs` URI parser,
  `connections.rs` (secret-free profiles), `known_hosts.rs` (TOFU), `transfer.rs` (`download_tree`/`upload_tree`,
  path-traversal-hardened CPE-1461/1462), `net_share.rs` (OS-share enumeration).
- **SFTP DONE** (`crates/sftp`, russh+russh-sftp, ring). **WebDAV DONE** (`crates/webdav`, ureq+roxmltree).
- NOTE: `cpe-net`/`remoteTransport.ts` is the CPE-810/819/820 *client-server transport*, NOT this feature — don't conflate.
- Keychain: proven `keyring` v3 pattern EXISTS in `sidecar/host` (AI keys) — lift into the app; the main app has
  no connection-secret storage yet (the real gap).

## Architecture = HYBRID, in-app-VFS-first
- **In-app VFS provider = default ("Connect")** — portable, no elevation, cross-platform-identical; already built
  for SFTP+WebDAV.
- **OS-level mount = optional ("Mount as drive")** — `net use` / `mount_smbfs` / `mount.cifs`|`gio mount`; then
  browse as a normal local path; also the safe fallback for immature-client protocols.
- New protocol implements `connect + list/stat/read/write/mkdir/delete/rename` + a scheme arm in `vfs::open`;
  inherits cancellable walk + traversal guards + skip-on-error. Needs 3 trait extensions for "anything":
  capabilities descriptor, richer auth (Anonymous/Token/AccessKey), streaming read.
- Reuse: async+spawn_blocking, STREAMING.md channel walker, transfer queue (CPE-613), `require_local` keeps local
  byte-for-byte (PURPOSE.md).

## Protocol difficulty ladder (easiest → hardest; drives epic order)
DONE: WebDAV, SFTP. Next-new easiest→hardest: **FTP/FTPS** (`suppaftp`, mature — DO FIRST) → **S3** (`rust-s3`/
`aws-sdk-s3`/`opendal`; key-auth, unlocks B2/GCS free) → **SMB** (NTLM v1 + OS-mount on non-Win; NO mature
cross-platform pure-Rust crate — `pavao` C-binding/no-Windows, `smb2`/`smb` very new; Kerberos deferred) →
**NFSv3** (`nfs3_client`; Linux/mac only, Win deferred; NFSv4 deferred). **Cloud OAuth** (Drive/OneDrive/Dropbox)
= SEPARATE track after S3 (browser consent ≠ protocol client). **AFP: DO NOT BUILD** (Apple removes it macOS 27).
Out: iSCSI (block), Git-over-SSH (VCS), MEGA (proprietary).

## Epic tree filed 2026-08-08 (all Proposed/queued; parent CPE-616, OS-mount extends CPE-716)
**Foundation (order F1→F2→F3, then F4/F5 parallel):** CPE-1497 keychain (F1) · CPE-1498 Network sidebar+UI (F2)
· CPE-1499 vfs::open command wiring — the crux, turns SFTP+WebDAV LIVE, folds transfer-queue UI (F3) · CPE-1500
OS-mount bridge (F4) · CPE-1501 capability+auth extension (F5).
**Protocols (after F1–F3):** CPE-1502 FTP ⭐first · CPE-1503 S3 · CPE-1504 SMB · CPE-1505 NFSv3 · CPE-1506 cloud-OAuth (last).
**Suggested order:** F0-reconcile(docs) → F1 → F2 → F3 → *(SFTP+WebDAV milestone)* → F4(SMB via OS-mount) → F5 →
FTP → S3 → SMB-inapp → NFS → cloud. Most backend halves are headless-buildable (good workshift batches).
