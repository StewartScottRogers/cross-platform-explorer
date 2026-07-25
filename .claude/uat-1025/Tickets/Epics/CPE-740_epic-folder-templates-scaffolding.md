---
id: CPE-740
title: "EPIC: Folder templates & scaffolding"
type: Task
status: In Progress
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

> **Activated 2026-07-21** (dayshift, autonomous — best-guess decisions logged, user delegated PM). Chosen
> as a **small, self-contained, high-everyday-value** epic whose core (capture + token substitution +
> stamp) is fully headless and **verifiable via a real filesystem round-trip on the running OS**. Started
> with that backend core (CPE-835); the gallery + "New from template…" flow are the attended UI children.

## Decisions (activated 2026-07-21 — autonomous best-guess, PM delegated)
- **Template format/storage:** a serde-JSON `Template { name, nodes }` tree (`Dir`/`File`), persisted under
  the app config dir; import/export is just that JSON. No new dependency.
- **Token vocabulary + scope:** `{key}` substitution driven by a caller-supplied variable map, applied to
  folder names, file names, **and** file contents. The command layer fills the vocabulary (`{date}`,
  `{name}`, `{counter}`, …) so the core stays pure/deterministic; unknown `{tokens}` pass through verbatim
  (a template author may want literal braces for downstream tools).
- **Placeholder contents:** captured for small UTF-8 files (≤64 KB) so boilerplate survives; larger or
  non-text files are captured as empty placeholders, keeping templates small and text-only.
- **Safety:** stamping sanitizes each substituted name to a single path component (no separators, no `..`)
  so a token value can never escape the destination, and it refuses to overwrite an existing file.

## Goal
Save any folder's structure (subfolders, placeholder files, naming conventions) as a reusable template and
stamp it out on demand — new-project, new-client, new-shoot layouts in one click, with token substitution
(date, name, counter).

## Why
Kills the repetitive "create the same six subfolders again" chore for anyone with a recurring project
structure — developers, photographers, accountants. Small, self-contained, high everyday value.

## Rough scope (areas, not child tickets)
- Template capture from an existing folder (structure + placeholder files).
- A token/substitution engine (`{date}`, `{name}`, `{counter}`, ...).
- A template gallery (manage / import / export).
- A "New from template..." flow in the file view / context menu.

## Open questions (resolve at activation)
- Template storage/format and sharing (import/export).
- Token vocabulary and where substitution applies (folder names, file names, file contents?).
- Placeholder file contents (empty vs. templated boilerplate).

## Definition of Done
- Users can capture a folder as a template and stamp it out with token substitution.
- A template gallery manages templates with import/export.
- "New from template..." creates the structure in one action; no cost when unused.

## Child tickets
1. **CPE-835** — Folder-template core (`cpe-server::folder_template`): the `Template`/`Node` model, real-fs
   `capture(folder)`, `{key}` `substitute`, and `stamp(template, dest, vars)` with path-safety + no-clobber.
   Pure model + real-fs, cargo-tested via a capture→stamp round-trip. *Headless — buildable now.*
2. **CPE-836** — Template **store** (persistence): `ServerCtx`-based save/list/load/delete + import/export
   of named templates in the config dir (`templates.json`), following the `tags`/`settings` store pattern.
   Headless, cargo-tested via `HeadlessCtx`. *(prereq: 835)*
3. **CPE-837** — Tauri commands + template gallery + "New from template…" flow (frontend): capture-from-
   folder, manage, stamp with a token-fill dialog; the thin `#[tauri::command]` dispatchers over the store
   and core. **GUI-verified — attended.** *(prereq: 835, 836)*
