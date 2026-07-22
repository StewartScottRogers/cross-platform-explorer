---
id: CPE-573
title: "Home — remember section open/closed + active tab across sessions"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Home always opens with both sections expanded and the **Recent** pill selected, even if you last used
Favorites. Persist the Home layout — the Quick-access + lower-section open state and the active pill tab —
so Home reopens the way you left it.

## Acceptance Criteria
- [x] The active lower-section tab (Recent / Favorites / Recent folders) persists across opens.
- [x] Each section's open/closed state persists.
- [x] A malformed/absent value degrades to the defaults (open, Recent).
- [x] `npm run check` clean; a component test covers restore-on-open.

## Resolution
`HomeView` initialises `quickOpen`/`recentOpen`/`tab` from localStorage (`cpe.homeQuickOpen` /
`cpe.homeRecentOpen` / `cpe.homeTab`, defensively read; tab validated to the 3 allowed values, default
`recent`) and reactive `$:` statements persist changes. Behaviour-only (no new user-facing text, so no
i18n). Added a `HomeView.test.ts` case (saved `favorites` tab → its items show on open) and a `beforeEach`
localStorage clear so the suite's HomeView tests stay isolated. Full suite **613 pass / 63 files**;
`npm run check` 0/0.
