---
id: CPE-968
title: forge_conflict_abort is registered but missing from collect_commands! (typed-bindings drift)
type: bug
component: Backend
priority: medium
status: Open
tags: ready
created: 2026-07-23
epic: CPE-810
estimate: 30m
---

## Summary
`forge_conflict_abort` is a real, registered Tauri command (`#[tauri::command]`, in the
`generate_handler!` list at `src-tauri/src/lib.rs:5690`) but it is **absent from the `collect_commands!`
list** used to generate the typed client (`src-tauri/src/lib.rs:6038` area — only `forge_conflict_continue`
is listed alongside `forge_conflict_state`/`forge_conflict_versions`/`forge_resolve_file`).

As a result `commands.forgeConflictAbort` does **not** exist in `src/lib/bindings.gen.ts`, so
`ConflictDialog.svelte`'s "abort merge" path can't be migrated to the typed client in CPE-964 and stays on
raw `invoke("forge_conflict_abort", …)`.

Discovered during CPE-964 increment 11 (2026-07-23) while migrating the dialogs cluster.

## Acceptance Criteria
- [ ] Add `forge_conflict_abort` to the `collect_commands!` macro list (next to `forge_conflict_continue`).
- [ ] Regenerate bindings: `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`;
      `commands.forgeConflictAbort(path): Promise<Result<string, string>>` now exists in `bindings.gen.ts`.
- [ ] Drift guard (`git diff --exit-code src/lib/bindings.gen.ts`) is clean after regen + commit.
- [ ] (Follow-on, may be same PR) migrate ConflictDialog's dynamic `cmd` ternary
      (`forge_conflict_continue` / `forge_conflict_abort`) to the typed `commands.*` client, closing the
      last ConflictDialog raw-invoke sites for CPE-964.

## Notes
- No behavioural risk: the command already runs correctly via raw invoke; this only exposes it to the typed
  client + drift guard so the generate_handler / collect_commands lists stop drifting.
- Worth a quick audit: are any OTHER `generate_handler!` commands missing from `collect_commands!`? A short
  diff of the two lists would catch the whole class. If several are missing, list them here and fix together.
