---
id: CPE-958
title: Migrate core call sites to the typed client (CPE-953 GUI-verify)
type: feature
component: Frontend
priority: medium
status: Doing
tags: ready
created: 2026-07-23
epic: CPE-810
---

## Summary
CPE-953/957 landed the full typed client (`bindings.gen.ts`, 125 commands) but only one call site was
migrated. This does the GUI-verify tail: migrate the two most-visible commands to the typed client so folder
navigation and the Agent Board exercise it end-to-end against the live backend, and fix a duplicate
`list_dir` entry in `collect_commands!`.

## Acceptance Criteria
- [x] Removed the duplicate `list_dir` in `collect_commands!` (deduped the whole block); regenerated
      `bindings.gen.ts` (listDir now appears once).
- [x] `navigateToTyped` uses `commands.listDir()`, unwrapping the `Result` (invalid path → `status:"error"`
      → "Can't find" notice preserved).
- [x] `BoardView.load()` uses `commands.boardCards()` (returns `Card[]` directly).
- [x] `npm run check` 0/0; vitest **929 pass** (BoardView test still green); default `cargo test` + clippy clean.
- [ ] GUI-verify in the installed 0.57.23: typing a valid/invalid path navigates / shows the notice; the
      Agent Board lists cards — both through the typed client. *(the attended step, done together)*

## Notes
`commands.listDir(path)` returns `Result<DirEntry[], string>` (unwrap `.status`/`.data`/`.error`);
`commands.boardCards(root)` returns `Card[]` directly (board_cards isn't a `Result` command). Regenerate:
`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`.
