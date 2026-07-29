---
id: CPE-836
title: Folder-template store — ServerCtx-based persistence (save/list/load/delete + import/export)
type: feature
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-740
estimate: 1-2h
---

## Summary
Second child of CPE-740. The persistence layer for folder templates, following the existing
`tags`/`settings` store pattern: a `templates.json` document (name → `Template`) in the app config dir,
with `ServerCtx`-based entry points so the eventual Tauri commands (CPE-837) are one-line dispatchers.

- **`save(ctx, template)`** — insert/replace by `template.name`, persist; returns the updated catalog.
- **`list(ctx)`** — the template names + a small summary (node/dir/file counts) for the gallery.
- **`load(ctx, name)`** — one template by name (`None` if absent).
- **`delete(ctx, name)`** — remove by name; returns the updated catalog.
- **`export(template)` / `import(ctx, json)`** — a single template's JSON out, and a template (or a whole
  catalog) merged in — reusing the `folder_template` model.

Headless and cargo-tested via `HeadlessCtx` (no Tauri), exactly like `tags`/`settings`.

## Acceptance Criteria
- [x] `templates.json` in the config dir holds a name→`Template` catalog; absent/corrupt file → empty.
- [x] `save`/`load`/`delete` round-trip through `HeadlessCtx`; `save` replaces a same-named template.
- [x] `list` returns names with a summary (dir/file counts) in a stable (name-sorted) order.
- [x] `import` merges a template **or** a whole catalog by name (replace-by-name, documented); `export`
      yields a single template's JSON that `import` accepts; garbage JSON errors.
- [x] `cargo test` green in `crates/server` (147 passed); `cargo clippy --all-targets -D warnings` clean.
      App untouched.

## Resolution
Extended `cpe-server::folder_template` with the persistence layer, mirroring the `tags`/`settings` store
pattern: a `templates.json` `Catalog` (name → `Template`) in the config dir, reached through `ServerCtx`.

- `read_catalog_from` / `write_catalog_to` — absent/corrupt → empty; pretty-printed, stable BTreeMap order.
- `save(ctx, template)` (insert/replace by name) · `list(ctx)` → `TemplateSummary{name,dirs,files}`
  name-sorted · `load(ctx, name)` → `Option<Template>` · `delete(ctx, name)`.
- `export(template)` → a single template's JSON; `import(ctx, json)` accepts a single `Template` **or** a
  whole `Catalog`, merged by name (Template tried first — a catalog's values aren't bare templates, so the
  two shapes disambiguate cleanly), garbage → error.

Files: extended `crates/server/src/folder_template.rs` (store fns + 3 tests). No new dependency.

Verification (local, Windows): `save/list/load/delete` and `export/import` round-trip through `HeadlessCtx`;
`cargo test` in `crates/server` → **147 passed** (was 144, +3 store tests); `cargo clippy --all-targets -D
warnings` clean. App untouched — CPE-837 wires the thin Tauri commands (`#[tauri::command]` one-liners over
these) + the gallery/"New from template…" UI.

## Work Log
- 2026-07-21 — Picked up (dayshift, autonomous). Estimate 1-2h. Added the ServerCtx-based template store to
  `folder_template`, following the tags/settings pattern; save/list/load/delete + export/import, all
  round-tripped via HeadlessCtx. 3 new tests; full suite 147 green; clippy clean. Closing.
