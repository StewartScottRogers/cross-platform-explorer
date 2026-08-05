---
id: CPE-1319
title: File-Health panel visual fixes (Visual Critic) — mismatch badge overflow + orphan row badge
type: bug
component: Frontend
priority: medium
tags: ready
created: 2026-08-05
epic: CPE-1002
estimate: 1-2h
---

## Summary
The Visual Critic, judging real gui-smoke screenshots of the shipped File-Health panel, found 2
screenshot-visible layout defects (jsdom/markup review missed them):

1. **Type-mismatch tab — badge overflow.** The right-side badge renders the FULL sentence "claims jpg → looks
   like Windows executable/library", which squeezes the filename column to "f…" (unreadable). Every other
   tab's badge is a short 1-3 word label.
2. **Orphan-sidecars tab — no status badge.** Orphan rows show icon+name+path but NO badge/pill, breaking the
   icon+name+path+badge pattern the dangling + mismatch tabs establish.

## Acceptance Criteria
- [ ] Mismatch rows: the long "claims {ext} → looks like {label}" reason no longer competes with the filename
      for width. Cleanest fix (choose the better): move the reason to a **secondary line/subtitle UNDER the
      filename** (full text visible, name gets full width), OR keep a short right-side badge (e.g. just the
      detected label) + put the full "claims → looks like" as a `title`/tooltip + `max-width`+ellipsis pill.
      Filename must stay legible. (Prefer the subtitle approach — the reason is genuinely a sentence.)
- [ ] Orphan rows: add a short, consistent status badge (e.g. "Orphaned" / "No primary") so the row matches the
      other tabs' pattern — OR, if a badge is genuinely redundant, make the row's layout intentionally
      consistent (don't leave it looking like a missed badge). Use a short label, theme-var coloured.
- [ ] Badge/pill rows still reflow (flex-wrap container + nowrap pills); any long-text pill has max-width+ellipsis.
- [ ] Update/extend the FileHealthDialog jsdom tests for the new structure (mismatch reason as subtitle / orphan
      badge present). `npm run check` clean + full `npm run test:unit` green. i18n any new key × 12 locales.

## Notes
Real re-verification: the Foreman re-runs the gui-smoke file-health screenshots + Visual Critic after this lands.
Do NOT run gui-smoke/tauri build in the worker.

## Work Log
2026-08-05 (workshift run 2) — Filed by the Foreman from the Visual Critic's screenshot-grounded findings.
