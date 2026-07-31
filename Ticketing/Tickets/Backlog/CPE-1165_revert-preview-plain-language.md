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
- [ ] The preview reads unambiguously as "what reverting will do", not a summary of the user's edits — a user
      who deleted+modified sees wording that matches (restore the deleted, undo the changed).
- [ ] Applied consistently in CheckpointDialog AND the AgentTimeline restore panel.
- [ ] No logic change (same RevertPreview data); `npm run check` green; existing checkpoint/restore-panel tests
      still pass (update any snapshot/label assertions to the new copy); i18n keys added to all locales if new
      strings are introduced.

## Notes
- Copy/clarity only; safe. Origin: the user's revert test — the plan was correct but the labels read inverse.
