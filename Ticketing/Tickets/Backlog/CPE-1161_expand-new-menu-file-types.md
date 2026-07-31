---
id: CPE-1161
title: "Expand the New ▸ menu with a full set of file types (Markdown/JSON/YAML/HTML/CSS/JS/Python/XML/CSV + Zip)"
type: feature
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-07-31
---

## Summary
User-requested (2026-07-31), after web research into what a "New" menu should offer. The `New ▸` submenu
(empty-area, on-item, and drive menus — CPE-1153/1156/1158) currently offers only **Folder** + **Text file**.
Expand it to a comprehensive, cross-platform-sensible set.

## Research (what Windows/others offer, and what fits a cross-platform dev explorer)
- **Windows 11 "New" defaults:** Folder, Shortcut, Bitmap image, Text Document, Compressed (zipped) Folder,
  Library — plus app-registered types (Word/Excel/PowerPoint) when those apps are installed.
  (Sources: elevenforum "Add or Remove Default Items on New Context Menu in Windows 11"; Microsoft Q&A
  "Edit filetypes in File Explorer Context Menu"; winbuzzer "Customize the New Context Menu".)
- **Cross-platform dev formats** (fileinfo/filestack developer-file lists): txt, md, json, yaml, xml, html,
  css, js, py, csv.
- **Decision:** offer the text-based set (empty files — no backend needed) + Compressed (zipped) Folder
  (valid empty zip). **Exclude** Bitmap (binary template), Shortcut/.lnk (Windows COM/shell, platform-specific),
  Library (Windows-only concept), and Office .docx/.xlsx/.pptx (need OOXML templates + only useful with those
  apps) — note them as out of scope with the reason.

## The set to add (each: label, extension, icon, creation)
Existing: **Folder**, **Text file** (.txt). Add:
- **Markdown** (.md) · **Rich Text** (.rtf, minimal `{\rtf1\ansi }` stub) · **JSON** (.json) · **YAML** (.yaml)
  · **XML** (.xml) · **HTML** (.html) · **CSS** (.css) · **JavaScript** (.js) · **Python** (.py) · **CSV** (.csv)
- **Compressed (zipped) Folder** (.zip — a *valid empty* archive)

Grouping (keep the submenu readable, MENUS.md): Folder — sep — Text file / Markdown / Rich Text — sep —
JSON / YAML / XML / HTML / CSS / JavaScript / Python / CSV — sep — Compressed (zipped) Folder. Leading icons
per item (reuse `Icon` glyphs: `document` for text, `code` for js/py, `archive` for zip, etc.).

## Implementation notes
- **Text types (the bulk) — NO backend change.** Reuse the existing `create_file(path, name)` (creates an empty
  file). The new items just call the existing new-file path with the right extension via `uniqueNameWithExt`.
  Wire them into ALL three New ▸ menus (empty-area, on-item/`-in`, drive) so they create in the correct target
  folder (CPE-1156 rules) / drive root (CPE-1158).
- **A few need a tiny stub to be valid:** Rich Text (`{\rtf1\ansi }`). Others (json/yaml/xml/html/css/js/py/csv)
  are fine EMPTY (users fill them) — do not over-stub. If you add a small HTML/JSON stub, keep it minimal; the
  simplest correct choice is empty for all except RTF. For any stub you need file *content* at create time,
  add a minimal backend `create_file_with_content(path, name, content)` (or extend `create_file` with optional
  content) and regenerate specta bindings — OR write the stub from the frontend via an existing write command
  if one exists. Prefer the smallest change; document it.
- **Compressed (zipped) Folder — needs a valid empty zip.** An empty file is NOT a valid `.zip`. Create a valid
  empty archive (22-byte End-Of-Central-Directory record, or via the `zip` crate already used by the app's
  compress/extract feature). This needs a small backend command (e.g. `create_empty_zip(path, name)` or reuse
  the compress path) + bindings regen. If clean, include it; if it balloons, land the text types first and
  split the zip into a follow-up (note it).

## Acceptance Criteria
- [x] The New ▸ submenu (empty-area, on-item folder `-in`, and drive) offers the text-based types above; each
      creates a correctly-named file in the right target folder and enters inline-rename (like the existing
      Text file).
- [x] Compressed (zipped) Folder creates a **valid empty .zip** (opens/extracts without error) — or is split
      to a documented follow-up if the backend work is non-trivial. **Landed** (valid empty EOCD archive).
- [x] Items grouped + icon'd per MENUS.md; the submenu stays readable (not an unwieldy flat wall).
- [x] `npm run check` green; tests cover the new-type dispatch (right action/extension per item); if a backend
      command was added, `cargo clippy --all-targets --features sidecar-platform -- -D warnings` clean +
      bindings regenerated (CI drift guard).
- [x] Excluded types (Bitmap/Shortcut/Library/Office) noted in the Work Log with the reason.

## Notes
- Builds on CPE-1153 (submenu), CPE-1156 (create-in-target), CPE-1158 (drive menu). Cross-platform-agnostic.

## Work Log
- 2026-07-31 — Implemented on branch `cpe-1161-new-menu-file-types`. **Both P1 (text types) and P2
  (RTF + Zip) landed** in one change.
- **DRY approach.** A single source of truth, `src/lib/newFileTypes.ts`, holds the extra types as a
  grouped list (`NEW_FILE_TYPE_GROUPS`) + an ext→spec lookup (`NEW_FILE_TYPE_BY_EXT`). Each spec carries
  `{ ext, labelKey, icon, base, content?, zip? }`. `ContextMenu.svelte` renders that list with `{#each}`
  in all THREE New ▸ submenus (empty-area, on-item-folder, drive), dispatching a generalized action
  `new-file:<ext>` / `new-file-in:<ext>` / `drive-new-file:<ext>` — no 9-types × 3-menus hardcoding.
  The two built-ins (Folder, Text file) keep their existing plain actions (`new-folder`/`new-file`/…),
  so the CPE-1153/1156/1158 tests stay green. `App.svelte`'s `runAction` parses the `:<ext>` suffix,
  resolves the spec, and routes to `newFile(target, spec)` with the correct target (current folder /
  clicked folder / drive root — reusing the CPE-1156/1158 wiring). `createNewItem` picks the extension
  via `uniqueNameWithExt` and the create strategy from the spec.
- **Types added (11):** Markdown `.md`, Rich Text `.rtf`, JSON `.json`, YAML `.yaml`, XML `.xml`,
  HTML `.html`, CSS `.css`, JavaScript `.js`, Python `.py`, CSV `.csv`, Compressed (zipped) Folder `.zip`.
  Creation: **empty file** (md/json/yaml/xml/html/css/js/py/csv, via existing `create_file`);
  **stub content** (rtf → `{\rtf1\ansi }`, via new `create_file_with_content`); **valid empty archive**
  (zip, via new `create_empty_zip`). Grouping per MENUS.md: Folder — sep — Text file / Markdown / Rich
  Text — sep — JSON…CSV — sep — Compressed (zipped) Folder. Icons reuse `document` (text/md/rtf),
  `code` (json/yaml/xml/html/css/js/py), `filter` (csv), `archive` (zip).
- **P2 outcome — LANDED (not split).** Added two thin Tauri commands in `src-tauri/src/lib.rs`:
  `create_file_with_content(path,name,content)` and `create_empty_zip(path,name)`, both
  `#[cfg_attr(feature="specta-bindings", specta::specta)]`, registered in BOTH the `generate_handler!`
  and `collect_commands!` blocks. The empty-zip archive writing is delegated to
  `cpe_server::archive::create_empty_zip` (the `zip` crate lives in `cpe-server`, keeping src-tauri
  lean); it writes a real EOCD-only archive via `ZipWriter::finish()`. Bindings regenerated with
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` — `createFileWithContent`
  + `createEmptyZip` now in `bindings.gen.ts`.
- **i18n.** Added 11 `ctx.new*` label keys to ALL 12 COMPLETE_LOCALES (format names kept as-is since
  JSON/YAML/etc. are proper nouns; Rich Text + Compressed (zipped) Folder translated). CPE-481 coverage
  gate green.
- **Excluded types (with reason):**
  - **Bitmap (.bmp)** — needs a binary image template, not a plain/empty file; out of scope for a text-first New menu.
  - **Shortcut (.lnk)** — Windows COM/shell-specific (IShellLink); not cross-platform.
  - **Library** — a Windows-only Explorer concept with no cross-platform meaning.
  - **Office .docx/.xlsx/.pptx** — require full OOXML zip templates and are only useful with those apps installed.
- **Verification.** `npm run check` → 0 errors / 0 warnings. `npx vitest run` on the touched suites
  (ContextMenu + DriveContextMenu + i18n) → 59/59 pass; full suite + `cargo clippy`/`cargo test`
  reported on the PR. Docs: added a **New ▸** bullet to `src/docs/03-explorer.md`.
