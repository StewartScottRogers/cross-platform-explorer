---
id: CPE-1559
title: "Trash: register commands in generate_handler! + regen specta bindings + Cargo.locks"
type: Task
status: Backlog
priority: Medium
component: Backend
epic: CPE-1486
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1486 slice 2. Wire the slice-1 commands (`list_trash`/`restore_trash_items`/`empty_trash`, CPE-1558) into
the IPC boundary so the frontend can call them.

## Scope
- Register the 3 commands in `generate_handler!` at **both** call sites in `src-tauri/src/lib.rs` (the
  `specta-bindings`-feature export list AND the runtime list — same two places `restore_from_trash`/`can_restore_from_trash` appear).
- Verify `src-tauri/capabilities/default.json` — likely no new grant (plain `invoke`, like existing trash commands); confirm.
- Regenerate `src/lib/bindings.gen.ts` (the specta export) and commit it.
- Commit **both** `src-tauri/Cargo.lock` and `crates/server`'s lock if the model-struct edit touched either.

## Acceptance criteria
- CI "Typed-bindings drift guard" (the `bindings.gen.ts` read-back test in `lib.rs`) passes.
- `npm run check` clean; `cargo build`/`clippy` green.
- The 3 commands are invokable from TS via the generated bindings (`TrashEntry` type present).

## Notes
**Must serialize after CPE-1558** (needs the real signatures + DTO). Low conflict surface (handler tail + generated
file). Per [[regen-specta-bindings-on-struct-change]] + [[multiple-independent-cargo-locks]] — do not skip either lock.
Blocked-by: CPE-1558. Model: sonnet (mechanical).
