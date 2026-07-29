---
id: CPE-1032
title: Per-folder metadata-column config store (headless)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-25
epic: CPE-707
estimate: 1-2h
---

## Summary
Epic CPE-707 (custom metadata columns). The per-family extractors (audio/image/video/doc) and the
`column_extract` dispatcher are done; the details view now needs to **remember which columns a user chose
for a folder**. Add a headless persistence store `crates/server/src/column_config.rs` that saves an
ordered list of column identifiers per folder path, following the EXISTING store pattern in
`crates/server/src/folder_template.rs` (CPE-836): a JSON catalog in the app config dir reached through
`ServerCtx`, tested via `HeadlessCtx`.

Study `folder_template.rs` (its `templates.json` store: `read_catalog_from`/`save`/`list`/`load`/`delete`
+ `HeadlessCtx` tests) and mirror its shape exactly. Keep it decoupled: the store persists **string
column-ids** (`Vec<String>`), NOT the `MetaColumn` enum — the string↔`MetaColumn` mapping stays in the
command/UI layer, so this module needs no dependency on `column_extract`.

## Design
- `ColumnConfig { columns: Vec<String> }` (serde) — the ordered visible-column ids for one folder.
- A `column_config.json` catalog = a map `folder_path -> ColumnConfig` in `ctx.app_config_dir()`.
- `pub fn get(ctx: &dyn ServerCtx, folder: &str) -> ColumnConfig` (absent/corrupt file ⇒ default empty).
- `pub fn set(ctx: &dyn ServerCtx, folder: &str, config: ColumnConfig) -> Result<(), String>`
  (creates the dir if needed; overwrites that folder's entry, preserving others).
- `pub fn clear(ctx: &dyn ServerCtx, folder: &str) -> Result<(), String>` (removes that folder's entry).
- An absent or corrupt catalog file yields an empty catalog rather than erroring (mirrors
  `read_catalog_from`). Register `pub mod column_config;` in `lib.rs`.

## Acceptance Criteria
- [ ] `set` then `get` round-trips a folder's ordered column list; `get` on an unknown folder ⇒ default
      empty; `clear` removes only that folder's entry, leaving others intact.
- [ ] A corrupt/absent `column_config.json` yields an empty catalog (never panics, never errors the read).
- [ ] ≥4 `HeadlessCtx`-based unit tests (round-trip; unknown-folder default; clear-one-preserves-others;
      corrupt-file-tolerated). No new dependency (serde_json already used by the template store).
- [ ] `cargo clippy --all-targets -- -D warnings` and `--all-features` both clean.

## Notes
New module only (`column_config.rs`) + one `mod` line in `lib.rs` + this Work Log. Do NOT touch
`column_extract.rs`/`metadata_column.rs`. A sibling worker (CPE-1033) adds a different new store module +
its own `lib.rs` mod line — keep additions self-contained so the `lib.rs` merge is trivial. Follow the
`folder_template.rs` store conventions precisely (same JSON-in-config-dir, tolerant-read shape).

## Work Log
- 2026-07-25: Built `crates/server/src/column_config.rs` mirroring `folder_template.rs`'s store shape:
  `ColumnConfig { columns: Vec<String> }` (serde, `Default`/`Clone`/`Debug`/`PartialEq`/`Eq`), a
  `Catalog = BTreeMap<String, ColumnConfig>` keyed by folder path persisted as `column_config.json` in
  `ctx.app_config_dir()`, and `get`/`set`/`clear` following the `read_catalog_from`/`write_catalog_to`
  tolerant-read pattern (absent/corrupt file ⇒ empty catalog, never panics). No dependency on
  `column_extract`/`metadata_column` — string column-ids only. Declared `pub mod column_config;` in
  `lib.rs` right after `pub mod inspect;`.
- Tests: 6 `HeadlessCtx`-based unit tests in `column_config::tests` (set→get round-trip, unknown-folder
  default-empty, clear-removes-only-that-folder, corrupt-file-tolerated, missing-file-tolerated,
  set-overwrites-same-folder-entry — exceeds the ≥4 acceptance bar). `cargo test -q column_config` (run
  from `crates/server`) → **6 passed; 0 failed**.
- `cargo clippy --all-targets -- -D warnings` → clean. `cargo clippy --all-targets --all-features -- -D
  warnings` → clean.
- Touched only `crates/server/src/column_config.rs` (new), the one `mod` line in `lib.rs`, and this
  ticket file, per the ticket's scope note (sibling CPE-1033 adds its own `lib.rs` mod line elsewhere).
