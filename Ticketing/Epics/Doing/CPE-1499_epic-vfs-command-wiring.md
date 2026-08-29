---
id: CPE-1499
title: "EPIC: Network F3 — wire vfs::open into the command layer (the crux refactor) + first protocol milestone"
type: Task
status: In Progress
priority: Medium
component: Backend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616). Foundation epic F3 — the enabling refactor CPE-616/CPE-685
> flagged as the main risk.** Filed 2026-08-08 (sprint PM, Network research). Dormant.

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

## ACTIVATED 2026-08-08 (Sprint, user-directed Network SFTP/WebDAV; built BEFORE CPE-1498 sidebar)
F1 keychain (CPE-1510) is merged. Building this backend crux next (headless-testable via FakeProvider + the
crates/sftp & crates/webdav harnesses) before the GUI sidebar (CPE-1498), since it's the functional core and
fully verifiable without eyes-on. Uses CPE-1510's `secret_store::secret_for(name)` for the secret + `known_hosts`
for host-keys. HARD constraint: `fs_route::require_local` keeps local paths byte-for-byte unchanged.

## Closeout audit 2026-08-29 - KEEP OPEN

1 child (CPE-1511) Done. **A user can browse a remote folder but cannot open, preview, or copy any file on it.**

**Shipped:** `crates/vfs/src/connect.rs` with `connected_provider`, a `ProviderPool` to avoid reconnects, and TOFU host-key refusal. Both `list_dir` and `list_dir_stream` branch on `Route::Remote` under `spawn_blocking`, with remote results streaming over the same channel and cancel registry as local ones. The hard constraint holds: the `Route::Local` arm is the identical pre-existing path, untouched.

**Missing - two halves of the same slice:**
1. `remote_read` and `remote_stat` exist and are unit-tested but are **called from nowhere** in `src-tauri` (zero hits outside `crates/vfs`). No preview, read or stat command routes remote.
2. The F6 fold-in never happened: `transfer::{download_tree, upload_tree}` are referenced by **zero** Tauri commands and zero frontend code, and the generated bindings have no remote transfer surface. Nothing reaches the CPE-613 transfer queue.

CPE-1511's own Work Log says this plainly - *"preview/read and transfer command-layer wiring is a later slice on this same epic"* - **and no such ticket was ever filed.**

Cost: roughly one L slice. The same shape as the `list_dir` remote arm, repeated for the read and transfer commands, plus queue wiring for progress and cancel.

**Confirmed by a second sweep:** `remote_read` / `remote_stat` appear **only** inside
`crates/vfs/src/connect.rs` - their definitions and their own unit tests - with no caller anywhere in
`src-tauri` or `crates`. And the code says so itself: the comment at `connect.rs:785` names
*"CPE-1499's still-to-come remote-open wiring"*. **The gap is self-documenting at the site**, so
whoever picks this up will find the seam rather than have to rediscover it.
