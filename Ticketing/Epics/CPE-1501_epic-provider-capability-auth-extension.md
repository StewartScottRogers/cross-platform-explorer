---
id: CPE-1501
title: "EPIC: Network F5 — provider capability descriptor + auth-model extension (unblocks S3/cloud/FTP)"
type: Task
status: Proposed
priority: Low
component: Backend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616). Foundation epic F5 — enabling; needed only for protocols
> beyond SFTP/WebDAV/SMB-browse.** Filed 2026-08-08 (sprint PM, Network research). Dormant.

## Why
The `FileSystemProvider` trait fits SFTP/WebDAV/SMB-browse, but "anything and everything" needs three
extensions so the UI + router adapt to non-POSIX shares:
- **Capabilities descriptor** — `supports_write/rename/random_read/watch`, `has_real_dirs` — so the UI reflects
  read-only shares, S3 (no real directories), FTP (weak rename).
- **Auth-model growth** — today `AuthMethod = Password | Key`; add `Anonymous` (public FTP/WebDAV), `Token`/OAuth
  (cloud), `AccessKey{id,secret}` (S3 SigV4). This is CPE-616's own noted S3 impedance mismatch.
- **Streaming read** for large remote files (honor STREAMING.md rather than buffering whole files).

## Effort / deps / fit
M — mostly pure/backend, well-tested via the existing `FakeProvider`. Deps: CPE-1499 (so capabilities can drive
real UI). Purpose-fit: enabling. Sequence before S3/FTP-anonymous/cloud need it.
