# Decoupled server architecture — GUI ⟂ transport ⟂ Server

_The shape of the app after epic CPE-810: a Tauri-free **Server** reached through a versioned,
transport-neutral **contract**, guarded by a pluggable **security** layer._

## What it is

The explorer is factored into three separable concerns:

```
Remote:  GUI ──(network)──► Client(Rust) ──(RPC)──► Server(Rust)   [other machine / OS]   ← planned
Local:   GUI ──(in-process IPC)────────────────────► Server(Rust)   [same box — today]     ← shipped
```

The **Server is the same component** in both topologies; only the transport under it changes. Today it
runs in-process behind Tauri; the seams for a remote Client/Server exist so the identical GUI can later
drive a remote server unchanged.

The hard tiebreaker still wins locally: **local mode keeps the Server in-process with zero network and a
null/passthrough security stack**, so the plain explorer stays fast/small/predictable. In fact the split
made it *lighter* — 17 heavy format/preview crates moved out of the app binary and behind the Server (see
"The payoff").

## The crates

Three standalone, **Tauri-free** crates under `crates/`, deliberately **not** in the app's cargo
workspace (the same one-way-boundary discipline as `sidecar/`). `src-tauri` `path`-depends on them; they
never depend on `src-tauri`. CI's `Server crates` job builds + lints + tests each on all 3 shipped OSes.

| Crate | Owns | Deps |
|-------|------|------|
| **`cpe-contract`** (CPE-811) | the wire envelope: `ContractVersion` + `negotiate()`, `Hello`/`Welcome` handshake, `Envelope { schema_version, id, session, message }`, `Request`/`Response`, streaming frames, error taxonomy | serde only |
| **`cpe-security`** (CPE-816–818) | the pluggable security core (below) | `cpe-contract`, serde |
| **`cpe-server`** (CPE-815/821/822) | the entire filesystem + preview **domain logic** | `cpe-contract`, serde + pure-Rust format crates; **no Tauri** |

`cpe-server` is the big one — **27 domain modules**: `listing` (core `list_dir`), `name_search` + `content_search`,
`model` (DirEntry/EntryInfo/Place/OpResult + properties), `checksum`, `duplicates`, `text_stats`,
`folder_stats`, `disk_usage`, `compare`, `archive` (list + create/extract), `binary_preview`, `doc_text`,
`data_preview`, `image_preview`, `thumbnail`, `backup`, `tags`, `settings`, `geometry`, `audit_journal`,
`ticket_board`, `links`, `location`, `provider`, plus the `ctx` seam and `fsutil` shared helpers.

## The `ServerCtx` seam

Domain logic never touches Tauri. It names *exactly* what it needs from the runtime behind one
object-safe trait, `cpe_server::ctx::ServerCtx`:

```rust
pub trait ServerCtx: Send + Sync {
    fn app_data_dir(&self) -> Result<PathBuf, String>;
    fn app_config_dir(&self) -> Result<PathBuf, String>;
    fn app_cache_dir(&self) -> Result<PathBuf, String>;
    fn emit_json(&self, event: &str, payload: serde_json::Value) -> Result<(), String>;
    fn is_cancelled(&self) -> bool { false }
}
```

- The **app** supplies the real impl, `TauriCtx` (in `src-tauri/src/server_ctx.rs`) — a thin wrapper over
  a cloned `AppHandle`.
- `HeadlessCtx` (in the crate) is a Tauri-free impl over a base dir that captures emitted events, so every
  command that needs the runtime is unit-testable **headless** — the crate has 100+ tests, no Tauri.

## The security planes

Security is **not** baked into the transport or the commands. It sits at the contract boundary as a
distinct layer of composable providers behind three plane traits (`cpe-security`):

| Plane | Trait | Concern |
|-------|-------|---------|
| Transport | `TransportSecurity` | is the channel authenticated/encrypted? (TLS/mTLS · *local: passthrough*) |
| Authentication | `Authenticator` | *who* is this client? (API token · mTLS identity · OAuth/OIDC · *local: implicit*) |
| Authorization | `Authorizer` | may *this principal* do *this op on this path*? (path-scope · capability) |

Each plane is an ordered interceptor chain with a **combine policy** (`all-must-pass` / `any-passes` /
`first-match`), assembled from a config-driven `ProviderRegistry`. Two invariants, both tested:

- **Default-deny is structural** — an empty or all-abstain plane denies under *every* policy. You can't
  leave the boundary open by forgetting to configure it.
- **Local = null/passthrough** — `SecurityChain::local()` wires the built-in passthrough provider in every
  plane with no audit, so local mode pays nothing. Security bills only in remote mode.

## The one rule for adding domain logic

**A `#[tauri::command]` fn is a thin adapter, not a home for logic.** Put the logic in `cpe-server`; make
the command a one-line `spawn_blocking` dispatcher into it.

```rust
// src-tauri/src/lib.rs — the adapter
#[tauri::command]
async fn list_dir(path: String) -> Result<Vec<cpe_server::model::DirEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::listing::list_dir(&path))
        .await.map_err(|e| e.to_string())?
}
```

Anything that needs the runtime takes `&dyn ServerCtx` and the command builds a `TauriCtx` at the single
dispatch point (`cpe_server::tags::load(&TauriCtx::new(&app))`).

### Streaming survives the split

A streaming producer (see [STREAMING.md](STREAMING.md)) keeps its **`ipc::Channel` in the app adapter**;
the pure walker lives in the crate and takes a `flush(batch) -> ControlFlow` callback. The collect command
and the streaming command drive the *same* walker — one in the crate, the transport in the app. `list_dir`,
`name_search`, `content_search`, and the backup engine all follow this.

## The recipe for extracting a domain (and the payoff)

The `cpe-server` crate was built by repeating one behaviour-preserving slice ~20 times:

1. Move the domain (types + pure fns) into a new `cpe_server::<domain>` module; reuse `model`/`fsutil`.
2. Make the `#[tauri::command]` fns one-line dispatchers; keep any `ipc::Channel` in the app.
3. Move the tests with the code (they're headless).
4. **If a format crate becomes app-unused, drop it from `src-tauri/Cargo.toml`.**

Step 4 is the payoff: **17 dependencies** — `zip · tar · flate2 · sevenz-rust · iso9660 · goblin · midly ·
wasmprinter · serde_bencode · rusqlite · calamine · parquet · rust_xlsxwriter · image · psd · kamadak-exif
· sha2` — now live behind the Server, so the plain explorer is *smaller*, not larger.

## What's shipped vs planned

- **Shipped:** the contract crate, the security planes, the `ServerCtx` seam, and the full `cpe-server`
  domain extraction driving the **local** in-process topology.
- **Planned:** the frontend pluggable transport seam (local IPC vs remote RPC, **CPE-819**) and the
  Client(Rust) proxy + reference headless Server that closes the remote loop (**CPE-820**), plus the
  `tauri-specta` typed bindings that make the contract the single source of truth for the frontend
  (**CPE-812/813**). The Server is already headless-runnable; those pillars add the network transport
  around it.

## What stays in the Tauri adapter (by design)

Not everything belongs in a portable Server. **OS-integration** commands stay in `src-tauri`: recycle-bin
delete (`trash`), `special_folders` (OS known-folders / OneDrive registry), `drive_type`
(`GetDriveTypeW`), elevation (`run_as_admin`), and the window/menu/updater plumbing. These are adapter
concerns, not transport-agnostic domain logic.
