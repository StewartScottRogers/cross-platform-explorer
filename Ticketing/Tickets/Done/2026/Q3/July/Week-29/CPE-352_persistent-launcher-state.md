---
id: CPE-352
title: "AI Console: persist launcher selections across usages (per-agent), via host storage"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Backend
created: 2026-07-13
---

## Summary

Part A of the user's request: the launcher's upper-panel selections (agent, provider, model,
small model) should come back the way they were left. Today's `last_used` is an in-memory
`HashMap` keyed by cwd that resets every time the sidecar spawns — so nothing persists across
restarts. Persist it durably via the **host Storage capability** (`storage.dir`, CPE-268),
reached through the broker client (CPE-344) and the servicing loop (CPE-349), which now also
registers the StorageProvider.

Model is designed to also hold **named presets per agent** so CPE-353 (the dropdown UI) drops
straight on top.

## Scope
- `src-tauri`: register `StorageProvider` (base = app data dir) in `serve_ai_console_requests`.
- `ai-console`: `storage_dir()` broker call; `presets.rs` = `Preset { name, provider, model,
  small_model, key_ref? }` + `PresetStore { agents: { <id>: { presets: [], last_used } } }`
  with a `PresetsBackend` (broker-backed reads/writes `presets.json` under the storage dir;
  `MemPresets` fallback for dev/tests). `ConsoleState` holds it.
- Persist per-agent last-used on launch; the catalog returns it so the launcher restores the
  agent's last selection on open.

## Acceptance
- Launch, close, reopen → the launcher restores the last agent + its provider/model/small
  model. Survives sidecar/app restart. Per agent (switching agent shows that agent's last set).
- Unit tests for the preset model + Mem backend + last-used update. `cargo test`/`clippy` clean.

## Work Log
2026-07-13 — Agreed design with the user (per-agent, last-used auto-apply, no cwd pinning).
Building the persistence foundation; named-set dropdown UI follows as CPE-353.

2026-07-13 — Implemented on branch `CPE-352-persistent-launcher-state`.
- `presets.rs`: `Preset`/`AgentPresets`/`PresetStore` (camelCase JSON) with `remember` /
  `save_preset` / `delete_preset`, a `PresetsBackend` trait, and `MemPresets`. 7 unit tests
  incl. "JSON never contains a key value".
- `broker_client.rs`: `BrokerClient::storage_dir()` (calls `storage.dir`) + `BrokerPresets`
  (reads/writes `presets.json` under the host storage dir).
- `console.rs`: `ConsoleState` swaps the in-memory per-cwd `last_used` HashMap for the
  persistent presets backend; `handle_launch` remembers the selection (provider/model/small
  model — never a key value); the catalog serves the whole store as `presets`.
- `main.rs`: wires `BrokerPresets` alongside `BrokerSecrets`.
- `src-tauri`: the CPE-349 servicing loop now also registers `StorageProvider` (base = app
  data dir/`sidecar-storage`), so `storage.dir` is answered.
- `launcher.html`: restores the last agent + its remembered provider/model/small model on
  open and on agent switch.

"Keeping API keys": the remembered/selected provider means its keychain key (CPE-287) is
auto-injected at launch — so keys are effectively kept by reference, never stored in a preset.

Verified: ai-console `cargo test` 105 lib + 7 integration, `clippy` clean; src-tauri
`cargo check`/`clippy --features sidecar-platform` clean, 61 tests. The persistence round-trip
(launch → reopen → restored) needs a GUI eyeball — the model/plumbing are unit-tested.

Named-set dropdown UI is CPE-353; launcher folder-picker is CPE-354.
