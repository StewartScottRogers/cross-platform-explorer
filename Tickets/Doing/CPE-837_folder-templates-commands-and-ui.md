---
id: CPE-837
title: Folder templates — Tauri command layer + "New from template" UI
type: feature
component: Multiple
priority: medium
status: Doing
tags: ready
created: 2026-07-24
epic: CPE-740
estimate: 3-4h
---

## Summary
The folder-template **core + store** are done and tested (`cpe_server::folder_template` — capture /
substitute / stamp / save / list / load / delete / export / import, CPE-835/836). What was never built:
the **Tauri command layer** exposing them and a **UI** to capture/apply templates. This ticket adds the
thin command dispatchers (CLAUDE.md architecture) + the typed bindings + a Templates dialog.

Picked up 2026-07-24 (user said "do it" against the offer to build an epic's headless-testable logic and
leave the visual GUI verify to them).

## Acceptance Criteria
- [ ] Thin `#[tauri::command]` dispatchers into `cpe_server::folder_template`: capture / save / list /
      load / delete / stamp / export / import. `capture` + `stamp` are async + `spawn_blocking` (real fs);
      the config-store ops match the sync tag-store pattern. Registered in **both** `generate_handler!`
      **and** `collect_commands!` (per [[CPE-968]]), bindings regenerated.
- [ ] A Templates dialog (typed `commands.*`): list stored templates (name + dir/file counts), capture the
      current folder as a new template, stamp a chosen template into the current folder (with `{token}`
      variables), delete, and export/import JSON. Follows the dialog conventions (visible border, menu
      standard, reflowing pills).
- [ ] Launchable from the app (toolbar/section) with its docs page + `sectionDocs` entry ([[maintain-in-app-docs-library]]).
- [ ] `cargo check`/clippy/test (both feature modes) + `npm run check` + vitest all green. Dialog logic
      unit-tested headlessly (jsdom). **Visual GUI verification left for the user** (attended).

## Notes
- Substitution vocabulary (`{date}`, `{name}`, `{counter}`, …) is filled by the command/UI layer; the core
  stays pure. Keep stamping path-safe + never-clobber (already enforced in core).
- Split allowed: land the command layer + bindings + backend tests first (headless, unambiguous), then the
  dialog. Do NOT auto-merge — the UI needs the user's visual verification.

## Work Log
- 2026-07-24 — Built the whole slice in one PR (headless-verified):
  - **8 thin Tauri commands** into `cpe_server::folder_template` (`template_capture`/`_save`/`_list`/`_load`/
    `_delete`/`_stamp`/`_export`/`_import`); `capture`+`stamp` async+`spawn_blocking`, store ops sync.
    Registered in **both** `generate_handler!` and `collect_commands!`; bindings regenerated (+101 lines,
    specta mode compiles).
  - **`TemplatesDialog.svelte`** (typed `commands.*` via `unwrap`): list w/ dir/file pills, capture-current,
    stamp-into-current with `{date}`/`{name}` vars, delete, export-to-clipboard, import-paste. Follows the
    dialog conventions (visible border, backdrop, Esc, reflowing pills). Opened via a new `tool.templates`
    palette command (i18n key added to **all 12 complete locales**).
  - `buildVars` extracted to `src/lib/templateVars.ts` (pure, unit-tested).
  - Docs page `src/docs/15-templates.md` (auto-indexed).
  - **Green:** `cargo check` + clippy (both feature modes); `npm run check` 0/0; full frontend suite **935**
    (+5 TemplatesDialog); i18n 34; docs/sectionDocs 9.
  - **PR open, NOT merged** — the dialog's visual appearance/UX needs the user's attended GUI verification
    (capture a real folder, stamp it, confirm the layout). Left for the user.
