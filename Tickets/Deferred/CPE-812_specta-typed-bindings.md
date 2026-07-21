---
id: CPE-812
title: tauri-specta typed bindings — generate a typed command client
type: feature
component: Multiple
priority: high
status: Deferred
tags: needs-prereq
created: 2026-07-20
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
- [ ] `specta::Type` on all serde types; `#[specta::specta]` on all 113 commands; Channel types typed.
- [ ] Generated `commands.ts` compiles; a build-time test regenerates it.
- [ ] Generated client routes through the busy-cursor wrapper; streaming uses `rawInvoke`.
- [ ] Codegen runs with `sidecar-platform` enabled (superset contract); clippy clean both modes.
- [ ] GUI-verified: app still launches and a few commands (list_dir, board_cards) work through the client.

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

## Deferred
deferred-on: RC-crate Windows integration (the `tauri-specta` test-binary `STATUS_ENTRYPOINT_NOT_FOUND`
loader failure) **plus** the ticket's own GUI-verify AC — both need an **attended** session with live
debugging + a running app. Not externally gated (it's a solvable integration issue), so it stays pickable.
revisit-when: an attended session; try (a) a separate codegen binary / `build.rs` that avoids the test
harness, or run the export via `cargo run` rather than `cargo test`; (b) confirm the tauri/webview2
versions tauri-specta rc.21 pulls vs. the app's; (c) once codegen emits `commands.ts`, do the 117-command
rollout + wire the generated client through `src/lib/invoke.ts`'s busy-cursor and GUI-verify a few calls.
The `cpe-server` types are already primed for the `Type` derives; **CPE-824's dispatch registry is the
natural place the typed method surface converges.**
