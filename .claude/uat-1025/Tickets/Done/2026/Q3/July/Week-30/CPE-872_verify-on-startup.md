---
id: CPE-872
title: Opt-in auto-verify of baselined folders on startup (integrity monitor)
type: feature
component: Frontend
priority: low
status: Done
tags: ready
epic: CPE-737
created: 2026-07-21
closed: 2026-07-21
---

## Summary
Turns the integrity guard from purely on-demand into light continuous monitoring: an **opt-in** "verify all
baselined folders on startup" toggle. When on, the app checks every baselined folder once, a beat after
launch, and shows the same summary notice (silent corruption / missing files) — honouring the epic's
fast-when-off rule (nothing runs unless you enable it).

## Acceptance Criteria
- [x] `settings.load/saveVerifyOnStartup` (bool, off by default).
- [x] On `onMount`, if enabled AND baselines exist, run `verifyAllBaselines` (deferred 1.5s so it never
      blocks first paint) — reuses the tested verify + notice (CPE-871).
- [x] IntegrityDialog has a "Verify all baselined folders on startup" checkbox, two-way to the persisted
      setting via App.
- [x] `npm run check` + suite (902) green.

## Work Log
- 2026-07-21 (autonomous) — Setting + startup hook + dialog toggle. Advances epic CPE-737 (opt-in periodic
  monitoring; a richer scheduler remains a later child).
