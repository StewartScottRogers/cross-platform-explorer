---
id: CPE-1031
title: Regenerate stale typed bindings (bindings.gen.ts drift — main is red)
type: bug
component: Backend
priority: high
tags: ready
status: Backlog
created: 2026-07-25
epic: CPE-810
---

## Summary
The CI "Typed-bindings drift guard" (CPE-813, epic CPE-810) is **failing on `main`** — `src/lib/bindings.gen.ts`
is stale. Earlier merged commands added/changed `#[tauri::command]`s with specta bindings (shell integration
`install/uninstall/shellIntegrationInstalled` — CPE-1020/1023; `drive_type` — CPE-805) but the committed
`bindings.gen.ts` was never regenerated, so the guard's `git diff --exit-code` fails. This turns the whole
CI red on every PR and violates the keep-the-green-pipeline guardrail.

## Fix
Regenerate and commit the typed client:
```
cd src-tauri
cargo run --bin export_bindings --features "specta-bindings sidecar-platform"
```
then commit the updated `src/lib/bindings.gen.ts`. (Build the frontend first if the Tauri build script
requires the dist dir.)

## Acceptance Criteria
- [ ] `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` leaves
      `src/lib/bindings.gen.ts` with **no** `git diff` (guard passes locally).
- [ ] `npm run check` still passes with the regenerated bindings.
- [ ] CI "Typed-bindings drift guard" is green on the PR.

## Notes
Generated-file-only change (plus this ticket). No hand-editing of `bindings.gen.ts`. Unblocks CI for the
in-flight metadata-column PRs (#345/#347) and everything after.
