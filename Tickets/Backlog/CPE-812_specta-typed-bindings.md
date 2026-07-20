---
id: CPE-812
title: tauri-specta typed bindings — generate a typed command client
type: feature
component: Multiple
priority: high
status: Open
tags: ready
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
