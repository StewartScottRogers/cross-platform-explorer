---
id: CPE-1264
title: "Drag-out plumbing: tauri-plugin-drag + capability + dragOut.ts wrapper (headless)"
type: feature
component: frontend
priority: medium
status: Doing
tags: ready
created: 2026-08-02
epic: CPE-661
---

## Summary
Slice A (of 3) for drag-OUT to the OS (CPE-672/674). Pure, headless plumbing — NOT wired into any row yet, so it
carries zero interactive-verification risk. See research-library `drag-out-to-os-tauri-plugin-drag-2026-08-02.md`.

## Build
- Rust: add `tauri-plugin-drag = "2"` to `src-tauri/Cargo.toml`; register `.plugin(tauri_plugin_drag::init())` in `run()` (`src-tauri/src/lib.rs`).
- JS: add `@crabnebula/tauri-plugin-drag` to `package.json`.
- Capability: add `"drag:default"` to the `permissions` array in `src-tauri/capabilities/default.json` (else `plugin:drag|start_drag` is denied at runtime).
- New `src/lib/dragOut.ts`: a thin wrapper over the plugin's `startDrag({ item: string[], icon, mode }, onEvent)` —
  maps a selection (array of absolute paths) → startDrag opts, resolves a required `icon` (bundle/app icon path or a
  reasonable default — the plugin REQUIRES a non-empty icon), and a graceful no-op / feature-gate when the plugin is
  unavailable (e.g. non-Tauri/test env). Do NOT wire it into FileList/Sidebar yet (that's Slice B, attended).
- Unit tests (jsdom, plugin mocked) for `dragOut.ts`: param mapping (paths→item), icon resolution, mode passthrough,
  graceful no-op when the plugin/`startDrag` is unavailable. Mirror how `src/lib/dnd.ts` is unit-tested.

## Acceptance criteria
- Plugin + capability + npm dep wired; `cargo check` (src-tauri) compiles with the plugin registered; `cargo clippy --all-targets -- -D warnings` clean.
- `dragOut.ts` wrapper + unit tests; `npm run check` clean; new vitest tests green; full suite no regressions.
- Nothing wired into a draggable row yet (no behavior change to existing internal drag) — this is plumbing only.
- Dependency justified: one small, single-purpose, MIT/Apache dual-licensed plugin delivering a capability Tauri v2 lacks natively (the Dependency Steward's call — it's the de-facto standard, spike-vetted).

## Notes
Slice B (wire drag-out into rows + native/HTML5 coexistence) and Slice C (archive extract-on-drag, reusing the existing
`extract_archive_entry_any` command) both need ATTENDED drag-drop verification and are separate tickets. This slice is
the headless foundation. Do NOT move this ticket file — the Foreman owns ticket lifecycle in the main tree.
