---
id: CPE-1165
title: "Revert preview counts read backwards — reframe as \"what reverting will do\" (plain language)"
type: chore
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-07-31
---

## Summary
User-found (2026-07-31, during the revert test). The revert preview shows bare counts
`creates N · overwrites N · deletes N · drift N`. These describe **what the revert will do to restore the
checkpoint** — but they read like a summary of the USER's changes, which is the inverse and confusing: the
user DELETED a file and MODIFIED a file, yet the preview said "creates 1, overwrites 1, deletes 0" (revert
will re-create the deleted one + overwrite the modified one). The numbers are CORRECT; the framing misleads.

## Fix (wording only — no logic change)
Reframe the counts as an explicit "if you revert" plan, in plain language. In BOTH places that render this
(they share the pattern):
- `src/lib/components/CheckpointDialog.svelte` (`.preview-counts`, ~line 189).
- `src/lib/components/AgentTimeline.svelte` restore panel (CPE-1126) — same counts.
Options (pick the clearest, keep it compact + MENUS/theme-consistent):
- A lead-in label: **"Reverting will:"** then `re-create N · overwrite N · delete N` (with the deletes clearly
  meaning "remove N files added since the checkpoint"), or
- Relabel to user-outcome terms: **"restore N deleted · undo N changed · remove N new"**, plus keep
  `N bytes to write` and the `drift N` (which flags "changed since the checkpoint").
- Tooltips on each count spelling out the meaning; and make "drift" read as "N changed since this checkpoint".
Keep it accurate to the RevertPreview fields (creates/overwrites/deletes/bytes_written/drift_count) — this is a
labeling/copy change only.

## Acceptance Criteria
- [x] The preview reads unambiguously as "what reverting will do", not a summary of the user's edits — a user
      who deleted+modified sees wording that matches (restore the deleted, undo the changed).
- [x] Applied consistently in CheckpointDialog AND the AgentTimeline restore panel.
- [x] No logic change (same RevertPreview data); `npm run check` green; existing checkpoint/restore-panel tests
      still pass (update any snapshot/label assertions to the new copy); i18n keys added to all locales if new
      strings are introduced.

## Notes
- Copy/clarity only; safe. Origin: the user's revert test — the plan was correct but the labels read inverse.

## Work Log
- 2026-07-31 — Reframed the revert-preview counts as an explicit plan in BOTH renderers.
  - **Lead-in added:** `Reverting will:` (CheckpointDialog `.preview-lead`; AgentTimeline `.cp-counts-lead`).
  - **Before → after** (bare counts that read like a summary of the user's edits → plain user-outcome plan):
    - `creates N`  → `restore N`   (tooltip: "Files you deleted come back")
    - `overwrites N` → `overwrite N` (tooltip: "Changed files are reset to the checkpoint")
    - `deletes N`  → `delete N`    (tooltip: "Files added since the checkpoint are removed")
    - `{bytes} to write` → unchanged (tooltip added: "Total bytes written back to disk")
    - `drift N`    → `N changed since this checkpoint` (drift emphasis class kept; tooltip:
      "Changed since this checkpoint — reverting overwrites that newer work")
  - **Field mapping (exact):** creates→restore, overwrites→overwrite, deletes→delete, bytes_written→bytes to
    write, drift_count→changed since this checkpoint. LOGIC UNCHANGED — same RevertPreview data, same
    actions, same two-step confirm, drift echo (CPE-1151) and drift-warning list untouched.
  - **i18n:** neither component is i18n-keyed (all copy is hardcoded English); kept the new copy hardcoded to
    match — no new locale keys introduced, so the CPE-481 gate is not triggered.
  - **Tick-tack:** counts stay a reflowing pill row (flex-wrap container, nowrap non-shrinking pills).
  - **Tests:** updated CheckpointDialog.test.ts (asserts "Reverting will:", "restore 1", "overwrite 2",
    "1 changed since this checkpoint"; asserts old "drift 1" gone). Added an AgentTimeline.test.ts case
    asserting the plain-language framing in the restore panel counts. `npm run check` → 0 errors / 0 warnings;
    `npx vitest run` → 129 files, 1470 tests passed.
