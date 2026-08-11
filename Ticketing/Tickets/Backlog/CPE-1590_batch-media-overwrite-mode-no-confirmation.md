---
id: CPE-1590
title: "Batch media: unchecking \"non-destructive\" silently overwrites originals with no confirmation"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-10
---
## Why
Found while writing the deep Batch Media doc page (CPE-1587, epic CPE-1569) and verifying against the real
code (`src/lib/components/BatchMediaDialog.svelte`, `crates/server/src/batch_media.rs`).

`BatchMediaDialog.svelte` has a **"Write to new files (non-destructive)"** checkbox, checked by default. The
backend planner (`batch_media::plan`, `crates/server/src/batch_media.rs`) only enforces its
"output never equals input" / collision-safe-naming guarantee when `job.non_destructive` is `true`. Unchecking
the box sets `non_destructive: false`, which skips that guard entirely (see the `plan()` function and its test
`overwrite_mode_keeps_the_input_path`) — for an op combination with no dedicated filename suffix (a lone
**Compress**, **Strip metadata**, or **Watermark**), the planned output path becomes *identical* to the input,
so clicking **Apply** overwrites the original file's bytes in place.

There is **no confirmation dialog, warning banner, or extra friction** between unchecking the box and clicking
Apply — just the checkbox's own label text. This is inconsistent with how the rest of the app treats
irreversible/destructive actions: **Securely delete…** (`ShredConfirmDialog.svelte`) requires an explicit
danger-button confirm, and Organize-this-folder/checkpoint flows take a safety checkpoint first. Batch media's
writes are also **not** on the app's Undo stack (`safety-undo.md`), so there's no Ctrl+Z recovery either — the
only way back is a prior checkpoint or an external backup.

## Scope
Add a lightweight confirmation step when the user is about to run a batch with "Write to new files" unchecked
AND at least one op in the list has no dedicated output-renaming suffix (Compress / Strip metadata / Watermark
alone, or any combination that the planner would otherwise resolve to `output == input`) — e.g. a danger-styled
inline notice or a one-click confirm on the Apply button itself, consistent with [docs/design/MENUS.md](../../docs/design/MENUS.md)'s
theme-token conventions (no hard-coded red). Exact UX is an implementation choice; the acceptance bar is "the
user cannot overwrite originals by accident with zero warning."

## Acceptance criteria
- Running batch media with the box unchecked and an op combo that would overwrite in place surfaces a clear
  warning/confirmation before the write happens.
- Running with the box checked (the default, and any combo that already produces a distinct output name) is
  unaffected — no new friction for the common non-destructive path.
- A test in `BatchMediaDialog.test.ts` (or a new sibling) covers the new confirm step.
- `npm run check` clean; vitest green.

## Notes
Docs-audit find, not a crash/data-loss report of an actual incident — a UX safety gap relative to the app's own
established conventions. See the new `src/docs/explorer-batch-media.md` (CPE-1587) for the full, verified
behavior this ticket is fixing.
