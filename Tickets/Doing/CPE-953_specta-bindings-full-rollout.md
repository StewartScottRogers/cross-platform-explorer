---
id: CPE-953
title: tauri-specta typed bindings — full command/type rollout + runtime Builder + GUI verify
type: feature
component: Multiple
priority: medium
status: Open
tags: ready
created: 2026-07-23
epic: CPE-810
estimate: 4h+
---

## Summary
Follow-up to **CPE-812**, which landed the codegen **foundation**: the `specta-bindings` feature, the
`export_bindings` bin, the busy-cursor-routed `src/lib/bindings.gen.ts`, and a proving set of 6 commands.
This ticket completes the rollout so the whole app can call backend commands through the typed client.

## Work Log
- 2026-07-23 (dayshift, increment 2) — **Bulk rollout: 13 → 83 typed commands.** Derived `specta::Type`
  (feature-gated) on **every** `cpe-server` serde type (all 38 files compile under `--features specta` with
  zero problem types) and on the app-local serde types; annotated every non-sidecar `#[tauri::command]`;
  built `collect_commands!` from the full non-sidecar, non-stream handler list. Excluded (documented in
  `export_bindings`): the 7 `ipc::Channel` streamers (separate AC), the `sidecar-platform`-gated commands
  (bin doesn't enable that feature), and `rename_tag`/`apply_backup_plan` (params `new`/`delete` are JS
  reserved words tauri-specta emits verbatim — renaming would break callers). Verified: `bindings.gen.ts`
  now types **83 commands**, `npm run check` 0/0; default `cargo test` 67 pass + loads clean (specta off);
  cpe-server 311 pass; clippy clean default + `specta-bindings`. `generate_handler!` still owns dispatch.
  **Remaining:** Channel typing, the `sidecar-platform` superset codegen, the 2 reserved-param commands,
  the optional runtime `Builder` switch, call-site migration, GUI-verify.
- 2026-07-23 (dayshift, increment 1) — **Foundation increment landed** (the hard cross-crate part): added an optional
  `specta` dep + OFF-by-default `specta` feature to **`cpe-server`**, and `#[cfg_attr(feature = "specta",
  derive(specta::Type))]` on the core `model.rs` types (`DirEntry`/`OpResult`/`EntryInfo`/`Place`). Wired the
  app's `specta-bindings` feature to enable `cpe-server/specta`, so cpe-server types now flow into the typed
  client. Annotated + exported the first cpe-server-typed commands — `list_dir`, `entry_info`,
  `delete_to_trash`, `delete_permanent`, `copy_entries`, `move_entries`, `move_exact` (6→13 total). Verified:
  default `cargo test` loads + passes (specta gated off), cpe-server 311 pass (both feature modes compile),
  clippy clean default + `specta-bindings`, `npm run check` 0/0 (generated `bindings.gen.ts` now includes
  `DirEntry`/`OpResult`/`EntryInfo`). Remaining: the long tail of commands + their types across the other
  ~34 cpe-server files, Channel typing, runtime Builder, call-site migration, GUI-verify.

## Acceptance Criteria
- [~] `#[cfg_attr(feature = "specta-bindings", specta::specta)]` on **all** `#[tauri::command]` fns; `Type`
      derived (feature-gated) on **every** serde type they use — including the `cpe-server` types (add an
      optional `specta` dep + a `bindings`/`typescript` feature to `cpe-server`, keeping it OFF by default so
      the lean crate is unchanged in normal builds). *(cpe-server `specta` feature + core model types DONE;
      13 commands exported; remaining commands/types across ~34 files are the tail.)*
- [ ] The 3 `ipc::Channel` streamers are typed, not raw; streaming call sites keep using `rawInvoke` /
      `createChannel` (CPE-547/550), not the busy-cursor path.
- [ ] Codegen runs with **`sidecar-platform` ON** too, so the feature-gated commands/types get types (one
      superset `bindings.gen.ts`); document the regen command set.
- [ ] Optionally switch runtime dispatch from `generate_handler!` to the specta `Builder`'s
      `invoke_handler()` (only if it stays byte-for-byte; otherwise keep `generate_handler!` and just emit).
- [ ] Migrate a first batch of call sites to import from `bindings.gen.ts`; `npm run check` + vitest green.
- [ ] GUI-verified: app launches and a few commands (e.g. `list_dir`, `board_cards`) work through the
      generated typed client.

## Notes
Foundation reference (all in CPE-812): the loader failure is dodged by keeping tauri-specta in the plain
`export_bindings` bin (never a libtest binary) and gating everything behind the OFF-by-default
`specta-bindings` feature. Regenerate with `cargo run --bin export_bindings --features specta-bindings` and
commit `src/lib/bindings.gen.ts`. Watch the `u64`→`number` BigInt policy and the `@ts-nocheck` header the
generator adds. CPE-824's dispatch registry is the natural convergence point for the typed method surface.
