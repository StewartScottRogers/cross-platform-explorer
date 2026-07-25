---
id: CPE-1033
title: Action-macro persistence store (headless)
type: feature
component: Backend
priority: medium
tags: ready
status: Backlog
created: 2026-07-25
epic: CPE-739
estimate: 1-2h
---

## Summary
Epic CPE-739 (scriptable actions / user macros). The pure headless macro model landed in CPE-938
(`crates/server/src/action_macro.rs`: `MacroStep`/`ActionMacro`/`PlannedOp` + `validate` + `plan`). The
next slice is **persisting named macros** so a user's action library survives restarts. Add a headless
store `crates/server/src/macro_store.rs` that saves/lists/loads/deletes/imports named `ActionMacro`s,
following the EXISTING store pattern in `crates/server/src/folder_template.rs` (CPE-836): a JSON catalog
in the app config dir reached through `ServerCtx`, tested via `HeadlessCtx`.

Study BOTH `folder_template.rs` (its `templates.json` store: `read_catalog_from`/`save`/`list`/`load`/
`delete`/`import` + `HeadlessCtx` tests) and `action_macro.rs` (the `ActionMacro` model you're persisting —
it is already serde-serializable). Mirror the template store's shape exactly.

## Design
- A `macros.json` catalog = a map `macro_name -> ActionMacro` in `ctx.app_config_dir()`.
- `pub fn save(ctx, macro: ActionMacro) -> Result<Catalog, String>` (keyed by the macro's name; creates
  the dir if needed; returns the updated catalog). Reuse the macro's existing name field as the key — if
  it lacks one, key by a `name: &str` parameter (check the `ActionMacro` shape first and follow it).
- `pub fn list(ctx) -> Result<Vec<MacroSummary>, String>` (name + a light summary like step count).
- `pub fn load(ctx, name: &str) -> Result<Option<ActionMacro>, String>`.
- `pub fn delete(ctx, name: &str) -> Result<Catalog, String>`.
- `pub fn import(ctx, json: &str) -> Result<Catalog, String>` (merge/insert imported macro(s)).
- An absent or corrupt `macros.json` yields an empty catalog (never panics/errors on read), mirroring
  `read_catalog_from`. Register `pub mod macro_store;` in `lib.rs`.

## Acceptance Criteria
- [ ] `save` then `load` round-trips an `ActionMacro`; `list` reflects saved macros; `delete` removes one;
      `import` inserts from JSON; unknown-name `load` ⇒ `None`.
- [ ] A corrupt/absent `macros.json` yields an empty catalog (never panics, never errors the read).
- [ ] ≥5 `HeadlessCtx`-based unit tests (save+load round-trip; list; delete; import; corrupt-file
      tolerated). No new dependency.
- [ ] `cargo clippy --all-targets -- -D warnings` and `--all-features` both clean.

## Notes
New module only (`macro_store.rs`) + one `mod` line in `lib.rs` + this Work Log. Do NOT modify
`action_macro.rs` (consume its `ActionMacro` as-is; if a needed field is missing, adapt the store's key
handling rather than editing the model — note the decision in the Work Log). A sibling worker (CPE-1032)
adds a different new store module + its own `lib.rs` mod line — keep additions self-contained so the
`lib.rs` merge is trivial. Follow the `folder_template.rs` store conventions precisely.
