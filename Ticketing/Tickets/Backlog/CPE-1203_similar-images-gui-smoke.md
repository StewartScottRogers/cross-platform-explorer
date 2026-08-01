---
id: CPE-1203
title: "gui-smoke spec pinning the similar-images dialog"
type: chore
component: Testing
priority: low
status: Backlog
tags: ready
created: 2026-08-01
epic: CPE-997
---

## Summary
Part of CPE-997 (after CPE-1202). Pin the similar-images review dialog in gui-smoke.

## Build
- New `gui-smoke/specs/similar-images.smoke.ts` + a `wdio.conf.ts` fixture seeder writing two near-duplicate
  PNGs (one re-encoded/resized copy of the other), modelled on `specs/batch-media.smoke.ts`. Drive the real
  built app: open the dialog via its real opener, scan, assert one group with BOTH images renders, `snap("similar-images-dialog")` + `snapFailure` (CPE-1149).

## Acceptance Criteria
- [ ] Spec passes headless against the built app; assertion tied to the seeded fixture (both filenames in the
      one group). `cd gui-smoke && npm run typecheck` clean.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-997). Depends on CPE-1202; can fold into 1202's DoD.
