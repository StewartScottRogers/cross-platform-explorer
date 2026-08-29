---
id: CPE-1501
title: "EPIC: Network F5 — provider capability descriptor + auth-model extension (unblocks S3/cloud/FTP)"
type: Task
status: Done
priority: Low
component: Backend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed: 2026-08-29
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

## ACTIVATED 2026-08-09 (Sprint) — unblocks S3/cloud protocols
Network foundation + SFTP/WebDAV/FTP are live. Building this enabling extension (headless, FakeProvider-tested)
so S3 (CPE-1503) + cloud (CPE-1506) can express their non-POSIX shape. Slice: CPE-1515.

## Closed 2026-08-29

Closed 2026-08-29 (closeout audit) WITH ONE RESIDUAL. 1 child (CPE-1515) Done. An enabling epic; its user-visible payoff is S3 being savable in the Add-a-connection form.

Verified: `ProviderCapabilities { supports_write, supports_rename, random_read, supports_watch, has_real_dirs }` has a full-POSIX `Default` and a defaulted trait method, **so no existing provider changed**. The override is genuinely exercised - the S3 provider asserts `has_real_dirs == false` and `supports_rename == false`, which is the impedance mismatch the epic named. `AuthMethod` grew `Anonymous`, `Token` and `AccessKey`, and both new refs are **non-secret labels** with the real value in the keychain. FTP maps `Anonymous` first-class while still honouring the pre-CPE-1515 blank-username heuristic for old profiles.

RESIDUAL - `ProviderCapabilities` **never crosses the IPC boundary**: zero occurrences in `src/`, including the generated bindings. The epic's "so the UI reflects read-only shares" half is unbuilt. Moot until CPE-1499's remote write/transfer path exists, so it is recorded here rather than holding the epic open.
