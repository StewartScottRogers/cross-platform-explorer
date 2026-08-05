---
id: CPE-1329
title: "Declutter: junk-review dialog surfacing organize_clutter findings (safe move-to-bin)"
type: feature
component: frontend
priority: high
status: Done
tags: ready
created: 2026-08-05
epic: CPE-979
---

## Summary
The `organize_clutter` backend command + `find_clutter` engine are fully built, cargo-tested, and wired to a
binding — but have **zero frontend callers**. The `ClutterReason` model ({ZeroByte, Installer, TempOrPartial,
Backup}, each with a human `label()` the doc-comment calls "for the declutter UI") was clearly built for a UI
that was never shipped. Epic CPE-979's DoD requires junk suggestions be *surfaced, never auto-actioned*. This
ticket builds that surface: a Declutter dialog that lists clutter findings for a folder and lets the user
safely send selected junk to the Recycle Bin. (The AI classifier is a SEPARATE, gated concern — this is the
rules-based, model-free surface only.)

## Build
- New `src/lib/components/DeclutterDialog.svelte`, modelled on `NearDuplicatesDialog.svelte` (CPE-1324) but
  SIMPLER — a flat junk list, no per-group keeper guard (there are no groups; each finding is independent junk).
- Call `commands.organizeClutter(dir)` → render the `ClutterFinding[]` grouped/labelled by `ClutterReason`
  (Zero-byte, Installer, Temp/partial, Backup) using each reason's human label. Read-only until the user acts.
- Per-item selection (checkboxes) + a **"Move selected to Bin"** action:
  - Nothing pre-selected (safety); the action is disabled with zero selection.
  - **Best-effort checkpoint first**, then `commands.deleteToTrash(paths)` — reuse the exact safe pattern from
    `NearDuplicatesDialog.svelte` (recoverable trash, never a hard delete).
  - **Apply the CPE-1328 lesson:** wrap the checkpoint call in `unwrap(...)` (from `src/lib/invoke.ts`) so a
    `{status:"error"}` failure is truthfully logged/handled and never reported as a success — but STILL
    non-blocking (a failed checkpoint must not block the trash move).
  - Prune moved items from the list on success.
- Wire it in: a command-palette entry + a **Tools menu** item (`src/lib/components/MenuBar.svelte`, near the
  find-similar entries) + dialog mount in `src/App.svelte` (near the existing dialog block). Match how the
  File-Health / near-dup dialogs are opened.
- i18n: all new strings in ALL 12 COMPLETE_LOCALES.

## Acceptance criteria
- Opening Declutter on a folder lists clutter findings grouped by reason with human labels; nothing is
  auto-actioned (read-only until the user selects + confirms).
- The user can select junk items and Move-to-Bin them; disabled at zero selection; a checkpoint is attempted
  (truthfully, via `unwrap`) before the trash move and a checkpoint failure does not block it; moved items are
  pruned from the list.
- `npm run check` clean; a `DeclutterDialog.test.ts` jsdom suite covers: findings render grouped by reason,
  selection gating, checkpoint-before-trash order + non-blocking on failure, prune-on-success. i18n 12 locales.
  No new deps.
- A `gui-smoke/specs/declutter.smoke.ts` + a `wdio.conf.ts` seeder (seed a folder with a zero-byte file,
  `setup.exe`, `movie.part`, `notes.bak`) drives the real `tauri build` binary, opens Tools → Declutter, and
  asserts the finding rows render — modelled on `gui-smoke/specs/file-health.smoke.ts` + `seedFileHealthFixture`.

## Notes
- FRONTEND-ONLY — merge on the Frontend CI job. Backend (`organize_clutter`/`find_clutter`) already exists +
  is cargo-tested; do NOT touch it.
- Conflict surface: new dialog + test + gui-smoke spec (isolated); append-only edits to `App.svelte`,
  `MenuBar.svelte`, `i18n.ts`, `wdio.conf.ts`.
- Reference: `src/lib/components/NearDuplicatesDialog.svelte` (safe move-to-bin), `organize.rs`/`organize_apply.rs`
  (the `ClutterFinding`/`ClutterReason` shape), `gui-smoke/specs/file-health.smoke.ts` (spec+seeder pattern).
