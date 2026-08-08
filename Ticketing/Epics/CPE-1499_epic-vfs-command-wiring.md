---
id: CPE-1499
title: "EPIC: Network F3 — wire vfs::open into the command layer (the crux refactor) + first protocol milestone"
type: Task
status: Proposed
priority: Medium
component: Backend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616). Foundation epic F3 — the enabling refactor CPE-616/CPE-685
> flagged as the main risk.** Filed 2026-08-08 (workshift PM, Network research). Dormant.

## Why (this is what turns SFTP + WebDAV ON end-to-end — the first user-visible protocol milestone)
`fs_route` today deliberately **rejects** remote URIs ("not connected"). Replace that with real dispatch so a
network location behaves like any folder — and because **SFTP + WebDAV providers already exist headless**, no
new client is needed for the first protocol milestone: just this wiring.

## Scope
- `connected_provider(uri)`: resolve a live provider via `cpe_vfs::open` (secret from CPE-1497, host-keys from
  `known_hosts`, TOFU: changed key refuses loudly). Per-`Connection` **provider pool / reconnect** (avoid
  reconnect-per-op).
- Route `list_dir` / `list_dir_stream` / preview / read / transfer through it, all under **`spawn_blocking`**
  (providers are sync; SFTP hides its own tokio). Remote listing goes through the existing streaming
  `list_dir_stream` ipc::Channel walker ([[prefer-streaming-liveness]]); skip-on-error preserved; the
  CPE-1461/1462 `guarded_join`/`safe_leaf_name` traversal guards inherited.
- **Fold in F6 (remote transfer UI):** wire the existing `download_tree`/`upload_tree` to the CPE-613 transfer
  queue (progress/cancel) so network↔local copies are normal queued transfers.
- **HARD constraint:** `fs_route::require_local` keeps local paths **byte-for-byte unchanged** (PURPOSE.md).

## Effort / deps / fit
L (the crux). Backend-heavy. Deps: CPE-1497 + CPE-1498. On completion: **SFTP + WebDAV are live in the UI** —
the payoff milestone. Every later protocol epic plugs into this seam.
