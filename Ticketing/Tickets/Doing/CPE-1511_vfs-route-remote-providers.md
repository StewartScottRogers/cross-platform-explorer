---
id: CPE-1511
title: "Route remote URIs through vfs::open in the command layer (SFTP/WebDAV become browsable) — the crux"
type: Feature
status: Doing
priority: High
component: Backend
tags: [ready]
epic: CPE-1499
created: 2026-08-08
---
## What (CPE-1499 F3 — makes the already-built SFTP + WebDAV providers actually work through the app)
`crates/server/src/fs_route.rs` today REJECTS remote URIs ("not connected"). Replace that with real dispatch so
a remote location (sftp://.., webdav://..) is browsed like any folder. The clients EXIST (`crates/sftp`,
`crates/webdav`) and the router `cpe_vfs::open(conn, secret, known_hosts, policy)` EXISTS — this ticket is the
wiring.

## How
- `connected_provider(uri) -> Box<dyn FileSystemProvider>`: parse the URI (`location.rs`), load the `Connection`
  (`connections.rs`), fetch the secret via CPE-1510 `secret_store::secret_for(conn.name)`, host-keys via
  `known_hosts`, then `cpe_vfs::open(...)`. TOFU: a CHANGED host key refuses loudly (surface a distinct error).
- A per-`Connection` **provider pool / cache** so we don't reconnect per op (open once, reuse; reconnect on drop/
  error). Keep it simple + correct.
- Route `list_dir` / `list_dir_stream` / preview/read / stat / transfer through the resolved provider, all under
  `spawn_blocking` (providers are sync; SFTP hides its own tokio). Remote listing goes through the existing
  streaming `list_dir_stream` ipc::Channel walker (STREAMING.md); skip-on-error preserved; inherit the
  CPE-1461/1462 `guarded_join`/`safe_leaf_name` traversal guards.
- **HARD CONSTRAINT (do not violate):** `fs_route::require_local` / the local path MUST stay byte-for-byte
  unchanged — a `local` URI takes the exact same code path as today, zero behavior change, zero added latency.
  Add a fast-path check so a local path never touches the remote resolution.

## Verify (HEADLESS)
- **Local unchanged:** existing fs_route/list_dir tests still pass identically; add a test asserting a local URI
  is dispatched to `LocalProvider` (not the remote path) and its output is byte-identical to today.
- **Remote routing via FakeProvider:** register/inject a `FakeProvider` (crates/server/src/provider.rs) behind a
  test scheme (or reuse its test seam), create a `Connection`, and assert `list_dir`/`stat`/`read` route to it
  through `connected_provider` — the crux logic without a live server.
- **Real-ish protocol** where the existing crate harnesses allow (crates/sftp / crates/webdav already test their
  providers — reuse their fixtures/in-process servers if present) to prove end-to-end at least at the crate level.
- Missing secret → clear Err; changed host-key → distinct "host key changed" Err (no silent connect); missing
  connection → Err. No panic.
- `cargo test` (crates/server) green; `cargo clippy --all-targets -D warnings` both feature modes; regenerate
  `bindings.gen.ts` if a command/struct surface changed; commit any Cargo.lock delta incl src-tauri/Cargo.lock.

## Notes
High blast radius (core dispatch) — opus worker, adversarial review. This turns SFTP+WebDAV LIVE at the command
layer; CPE-1498 (Network sidebar UI) is the visible entry point on top of it. Transfer-queue UI wiring is a
later slice. Effort L.
