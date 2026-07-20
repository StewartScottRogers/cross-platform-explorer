---
id: CPE-814
title: ServerCtx trait — abstract AppHandle/Window/State off the commands
type: refactor
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-20
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810. The heaviest coupling: **45** commands take `AppHandle`, **29** take
`Window`/`State`/`Manager`, and 6 emit events. Introduce a `ServerCtx` trait covering exactly what
those commands need from the runtime (resolve paths/cache dir, emit events, cancellation), and make the
commands depend on `ServerCtx` instead of concrete Tauri types. The Tauri layer provides a
`TauriCtx` implementation; a `TestCtx`/`HeadlessCtx` enables headless use. Prereq for extracting the
pure Server crate (CPE-815). **Coordinate with CPE-676** (ExplorerPane extraction) — both touch
`lib.rs`/`App.svelte`; sequence to avoid conflicting reshapes.

## Acceptance Criteria
- [ ] `ServerCtx` trait covers path/cache resolution, event emit, and cancellation used by the 45+29 commands.
- [ ] Commands take `&impl ServerCtx` (or `&dyn`) rather than `AppHandle`/`Window`/`State`.
- [ ] `TauriCtx` (real) + `HeadlessCtx` (tests) implementations; existing behaviour unchanged.
- [ ] clippy clean both modes; GUI-verified the app is functionally identical.

## Work Log
