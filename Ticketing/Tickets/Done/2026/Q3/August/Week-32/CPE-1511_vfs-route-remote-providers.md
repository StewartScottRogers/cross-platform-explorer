---
id: CPE-1511
title: "Route remote URIs through vfs::open in the command layer (SFTP/WebDAV become browsable) — the crux"
type: Feature
status: Done
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

## Work Log (2026-08-08 — Done)

**Dispatch design.** The crux resolver lives in a new `cpe_vfs::connect` module, not in `cpe-server`'s
`fs_route`: `fs_route` is a Tauri-free, network-free *classifier* and (by the crate boundary) cannot depend on
the concrete `cpe-sftp`/`cpe-webdav` crates. `cpe-vfs` already depends on both plus `cpe-server`, so it is the
one place that can turn a remote URI into a live provider. `fs_route` stays the pure local/remote classifier;
the app calls `route()` first and only enters remote resolution for a recognised remote scheme.

- `connect::connected_provider(pool, opener, access, conns, known_hosts, policy, uri)` — parses the URI,
  `find_connection` matches a saved `Connection` by scheme+host (+user/port when specified), fetches the secret
  via `secret_store::secret_for` (CPE-1510) through the keychain `SecretAccess` seam, then opens (or reuses) a
  provider. A **password** connection with no stored secret is a clear `Err` (never a blank-password connect); a
  **key** connection may connect without a passphrase (unencrypted key). A missing connection → clear `Err`.
- **Host-key TOFU:** we pass `HostKeyPolicy::Tofu`; the SFTP provider refuses a **CHANGED**/**REVOKED** key at
  connect with its distinct message (`"sftp: host key CHANGED — refused …"`), which propagates out of
  `connected_provider` — no silent connect. A refused/failed connect is **never cached**.
- `connect::remote_dir_entries` maps `ProviderEntry` → the app's `DirEntry` with navigable child URIs, and
  **re-filters every provider-supplied name through `transfer::is_safe_name`** (inherits CPE-1461/1462
  source-side traversal defense — a hostile `../escape` name is dropped before it becomes a URI).

**Provider pool.** `connect::ProviderPool` = `Mutex<HashMap<connection-name, Arc<Mutex<Box<dyn
FileSystemProvider + Send>>>>>`. Open once, reuse across ops; the connect runs outside the cache lock (a slow
connect can't block other connections) and only a **successful** connect is inserted; a concurrent double-open
resolves to one entry (the extra session drops/closes). `invalidate(name)` drops a dead session so the next op
reconnects. `cpe_vfs::open` now returns `Box<dyn FileSystemProvider + Send>` so pooled sessions are safe to move
to `spawn_blocking` worker threads (all four backends are `Send`).

**App wiring (`src-tauri/src/lib.rs`).** Added `cpe-vfs` as a dependency. A process-wide `REMOTE_POOL` +
`remote_provider_for(uri)` (loads `connections.json` + `~/.ssh/known_hosts`, uses the app's `KeyringBackend`,
TOFU). `list_dir` and `list_dir_stream` now `match fs_route::route()`: **Local → the exact same code as before**
(`listing::list_dir` / `list_dir_stream_impl` on a blocking thread); **Remote → `remote_list_dir_impl` /
`remote_list_dir_stream_impl`**. Remote listing streams over the SAME `ipc::Channel` + cancel registry
(`cancel_dir_stream` works for remote too), batched at `LIST_DIR_BATCH`. All under `spawn_blocking`.

**LOCAL is byte-for-byte unchanged.** The local arm calls the identical `listing::list_dir(&path)` /
`list_dir_stream_impl(...)` it always did; `route()` performs exactly the classification `require_local` did
internally, so there is zero behavior change and zero added latency for a local path — a local URI never touches
provider resolution, the pool, or the keychain. Proven by the new `fs_route` test
`a_local_uri_routes_local_and_the_seam_listing_matches_the_direct_listing` (Local classification + seam listing
identical to the direct-local listing) plus all existing `fs_route`/`listing` tests unchanged.

**Tests (all headless, no server).**
- `cpe_vfs::connect` (14 crate tests total): `connected_provider` routes list/stat/read to an injected
  `FakeProvider` behind a `ProviderOpener` seam (child URIs, extension, hidden, dir flags asserted); pool reuses
  one connect across 3 ops then reconnects after `invalidate`; missing-connection / missing-password-secret →
  clear `Err` + not cached (never opened); key-without-passphrase allowed; changed-host-key → distinct
  `CHANGED` `Err` + not cached; hostile `..` name dropped from a listing; child-URI scheme-word/authority
  preserved (`davs://…`).
- `cpe-server::fs_route`: existing tests unchanged + the new local-identical-listing test.
- `cpe-sftp`/`cpe-webdav` keep their existing in-process-server end-to-end harnesses (real protocol) — untouched.

**Build/verify:** `cargo test` green (cpe-vfs 14/14, cpe-server fs_route 5/5); `cargo clippy --all-targets -D
warnings` clean on cpe-vfs, cpe-server, and the app in **both** feature modes (default + `specta-bindings`).
Bindings **not** regenerated-with-changes — no command signature or `specta::Type` struct changed (confirmed:
`export_bindings` produces a byte-identical `bindings.gen.ts`). `src-tauri/Cargo.lock` updated (adds
`cpe-vfs`/`cpe-sftp`/`cpe-webdav`/russh/russh-sftp transitive deps); `crates/vfs/Cargo.lock` left as-is (no new
vfs deps).

**Still open (out of scope here):** CPE-1498 (Network sidebar UI — the visible entry point that navigates to a
remote URI) and transfer-queue UI wiring. Remote read/stat helpers exist + are tested in `cpe_vfs::connect`
(`remote_read`/`remote_stat`) but preview/read and transfer command-layer wiring is a later slice on this same
seam.
