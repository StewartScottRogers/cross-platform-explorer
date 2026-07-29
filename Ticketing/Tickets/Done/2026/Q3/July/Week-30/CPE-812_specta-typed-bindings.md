---
id: CPE-812
title: tauri-specta typed bindings — generate a typed command client
type: feature
component: Multiple
priority: high
status: Done
tags: ready
created: 2026-07-20
closed: 2026-07-23
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810 — the **first shippable slice** (decision: local typed bindings first). Add
`specta`/`tauri-specta`, derive `specta::Type` on the ~41 serde types, annotate the 113
`#[tauri::command]` fns, and swap `generate_handler!` for the specta `Builder`/`collect_commands!` so
a build-time test emits a typed `commands.ts`. The 3 `ipc::Channel` streamers must be typed, not raw.

Two must-not-regress hooks: point the generated bindings at the **busy-cursor** wrapper
(`src/lib/invoke.ts`'s `withBusy`), keeping `rawInvoke` for streaming (CPE-547/550); and run codegen
with **`sidecar-platform` ON** so the 140 feature-gated items get types (one superset `commands.ts`).

## Acceptance Criteria
> Scope landed here = the **codegen foundation + blocker resolution + a representative first command set**
> (the ticket's own "first shippable slice"). The full ~113-command annotation rollout + optional runtime
> `Builder` switch + the GUI-verify are split to the follow-up **CPE-953**.
- [x] Codegen pipeline works: `#[specta::specta]` on a representative command set (6) + `specta::Type` on
      `CommandOutput`; `cargo run --bin export_bindings --features specta-bindings` emits `src/lib/bindings.gen.ts`.
- [x] Generated client **compiles** (`npm run check` 0/0) and **routes through the busy-cursor wrapper**
      (import repointed `@tauri-apps/api/core` → `./invoke`); a pure-fs test guards the committed output.
- [x] **Blocker resolved.** The `STATUS_ENTRYPOINT_NOT_FOUND` loader failure was dodged by keeping the
      tauri-specta call in a **plain bin** (loads like the app) instead of a libtest binary, and gating all
      of specta behind an OFF-by-default `specta-bindings` feature so a normal build/`cargo test` never
      compiles it (default `cargo test` 67 pass, loads clean; clippy clean default + sidecar-platform + feature).
- [~] Full rollout: `Type`/`#[specta::specta]` on **all** commands+types, Channel typing, runtime `Builder`,
      GUI-verify → **CPE-953**.

## Work Log
2026-07-21 — Ran a **feasibility spike** (branch `cpe-812-specta-bindings`, not merged). Findings:
- **Type derives work.** `specta = "=2.0.0-rc.22"` compiles and `#[derive(specta::Type)]` works on our
  cross-crate `cpe-server` types (e.g. `DirEntry`) with no trouble.
- **Version alignment is finicky but resolvable.** The tauri-specta docs pin `specta =2.0.0-rc.21`, but the
  current `specta-typescript = "0.0.9"` requires `specta =2.0.0-rc.22`, and `tauri-specta 2.0.0-rc.22`
  doesn't exist. The working set is **`tauri-specta =2.0.0-rc.21` + `specta =2.0.0-rc.22` +
  `specta-typescript 0.0.9`** (cargo resolves it).
- **BLOCKER (why deferred):** with `tauri-specta` added, the app's **test binary fails to LOAD on Windows**
  — `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)` — even for pre-existing unrelated tests, so the crate
  compiles but no test (incl. the codegen export test) can run. This is a DLL/link-level regression from
  the RC crates (likely a tauri/webview2 skew pulled in by tauri-specta), not our code. Debugging a
  Windows loader failure with RC crates is a live-debugging task, and this ticket **already** carries a
  GUI-verify AC (app launches + commands work through the generated client) — so it needs an attended
  session regardless. Reverted the spike; `main` untouched.

- 2026-07-23 (dayshift, attended) — **Un-deferred and shipped the foundation.** Revisit approach (a) was
  correct: the loader failure is specific to a **libtest** binary linking tauri-specta — a plain bin loads
  fine (like the app exe). Confirmed empirically: with the deps present but unreferenced, all 66 app tests
  still loaded; the `0xc0000139` only appeared once a `#[test]` instantiated `Builder::<Wry>` /
  `Builder::<MockRuntime>` (both link a native runtime whose WebView2 entrypoint is skewed in the test
  harness). So codegen lives in **`src/bin/export_bindings.rs`** (`cargo run --bin export_bindings
  --features specta-bindings`) calling `app_lib::export_bindings`, which builds a `tauri_specta::Builder`
  over the annotated commands, exports via `specta_typescript::Typescript` (with
  `BigIntExportBehavior::Number` so `u64` byte-counts map to `number`), and rewrites the invoke import to
  `./invoke` (the busy-cursor wrapper) + prepends `@ts-nocheck`.
  - **All specta is OFF by default** behind the `specta-bindings` feature (optional deps + `cfg_attr`
    annotations + `required-features` on the bin), so a normal build/`cargo test` never compiles it — zero
    default bloat, and the loader failure can't touch the default test path. Verified: default `cargo test`
    67 pass + loads clean; clippy clean in default, `sidecar-platform`, and `specta-bindings`; `npm run
    check` 0/0 with the committed `bindings.gen.ts`; regen under the feature is drift-free.
  - Wired 6 representative commands (`run_command`, `create_dir`, `create_file`, `write_file_text`,
    `rename_entry`, `can_restore_from_trash`) + `CommandOutput` as the proving set. `generate_handler!`
    still owns runtime dispatch (this only emits a client), so behaviour is unchanged and no GUI-verify is
    needed for *this* slice.

## Resolution (first slice — codegen foundation + blocker fixed)
The blocker that forced the deferral is **solved**: keep tauri-specta out of the libtest binary (plain
codegen bin) and gate it behind an OFF-by-default feature. The end-to-end pipeline is shipped —
`cargo run --bin export_bindings --features specta-bindings` regenerates `src/lib/bindings.gen.ts`, a typed
client routed through the busy-cursor `invoke`, guarded by a pure-fs test. Six commands prove it; the
mechanical rollout to **all** commands/types + Channel typing + optional runtime `Builder` switch + the
GUI-verify is tracked as **CPE-953**. `cpe-server` types are still primed for `Type` derives (behind the
same feature); CPE-824's dispatch registry remains the natural convergence point.
