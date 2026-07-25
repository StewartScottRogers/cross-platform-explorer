---
id: CPE-1033
title: Action-macro persistence store (headless)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
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
- [x] `save` then `load` round-trips an `ActionMacro`; `list` reflects saved macros; `delete` removes one;
      `import` inserts from JSON; unknown-name `load` ⇒ `None`.
- [x] A corrupt/absent `macros.json` yields an empty catalog (never panics, never errors the read).
- [x] ≥5 `HeadlessCtx`-based unit tests (save+load round-trip; list; delete; import; corrupt-file
      tolerated). No new dependency.
- [x] `cargo clippy --all-targets -- -D warnings` and `--all-features` both clean.

## Notes
New module only (`macro_store.rs`) + one `mod` line in `lib.rs` + this Work Log. Do NOT modify
`action_macro.rs` (consume its `ActionMacro` as-is; if a needed field is missing, adapt the store's key
handling rather than editing the model — note the decision in the Work Log). A sibling worker (CPE-1032)
adds a different new store module + its own `lib.rs` mod line — keep additions self-contained so the
`lib.rs` merge is trivial. Follow the `folder_template.rs` store conventions precisely.

## Work Log

**2026-07-25** — Implemented `crates/server/src/macro_store.rs`, mirroring
`folder_template.rs`'s `templates.json` store shape exactly: `Catalog = BTreeMap<String,
ActionMacro>`, `read_catalog_from`/`write_catalog_to`/`save`/`list`/`load`/`delete`/`export`/
`import`, all reached through `&dyn ServerCtx`. Registered `pub mod macro_store;` in `lib.rs`
(added as a standalone one-line addition right after the existing `pub mod macro_library;` block,
so it merges trivially alongside the sibling CPE-1032 `mod` line).

**Name-key decision**: `action_macro.rs`'s `ActionMacro` already has a `pub name: String` field
(confirmed by reading the struct and its tests), so `save` is keyed directly by
`macro_.name.clone()` — no extra `name: &str` parameter was needed, matching how
`folder_template::save` keys by `template.name`.

**Relationship to `macro_library.rs`**: noted in the module doc comment that this is intentionally
distinct from the existing `crate::macro_library` module (CPE-951) — that module is a pure,
in-memory, order-preserving, case-insensitive-dedupe `Vec<ActionMacro>` model with no disk I/O.
`macro_store` is the actual on-disk persistence layer (a simple name-keyed catalog file), which is
what this ticket calls for. No overlap/duplication of responsibility; `action_macro.rs` was not
touched.

Added 7 `HeadlessCtx`-based unit tests (exceeds the ≥5 minimum): round-trip save/load, save-replaces
+ list reflects it, delete, import (single macro + whole catalog + garbage rejection), unknown-name
load, missing-catalog-is-empty, and corrupt-file-is-tolerated (writes invalid JSON to `macros.json`
directly and asserts `list`/`load` degrade to empty rather than erroring, then a subsequent `save`
overwrites the corruption cleanly).

**Verification** (run from `crates/server`, cargo at `%USERPROFILE%\.cargo\bin\cargo.exe`):
- `cargo test -q macro_store` → 7 passed, 0 failed.
- `cargo test -q` (full `cpe-server` suite) → 651 passed, 0 failed (no regressions).
- `cargo clippy --all-targets -- -D warnings` → clean.
- `cargo clippy --all-targets --all-features -- -D warnings` → clean.

No new dependency added. Scope held to `macro_store.rs` (new), the one `mod` line in `lib.rs`, and
this Work Log.
