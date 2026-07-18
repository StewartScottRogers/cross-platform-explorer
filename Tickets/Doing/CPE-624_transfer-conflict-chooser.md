---
id: CPE-624
title: Transfer conflict chooser (replace/skip/keep-both on paste)
type: feature
component: Frontend
priority: medium
status: In Progress
tags: ready
estimate: 1-2h
created: 2026-07-18
epic: CPE-613
---

## Summary
Child of CPE-613. When a copy-paste would overwrite existing names, ask once how to resolve the whole
batch (Replace / Keep both / Skip) instead of silently auto-renaming. Detected client-side against the
current folder's entries; the chosen policy is passed to `start_transfer`.

## Acceptance Criteria
- [x] Pure `collidingNames(sources, existing)` returns the base names that already exist; unit-tested.
- [x] `TransferConflictDialog.svelte` — modal offering Replace / Keep both / Skip / Cancel (i18n).
- [ ] `xfer.*` i18n keys added to all 12 locales.
- [ ] `doPaste` (copy) shows the dialog when there are collisions and starts with the chosen policy; no
      collisions → keep-both directly.
- [ ] `npm run check` clean; suite green; GUI-verified.

## Work Log
2026-07-18 (dayshift) — Built the pure helper + dialog component (part 1). i18n keys + App wiring next.
