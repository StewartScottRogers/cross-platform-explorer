---
id: CPE-871
title: Verify all baselined folders at once (integrity monitor)
type: feature
component: Multiple
priority: low
status: Done
tags: ready
epic: CPE-737
created: 2026-07-21
closed: 2026-07-21
---

## Summary
The integrity guard's "verify + alert" goal, one-shot: a **Verify all baselined folders** action that
re-scans every folder you've baselined and surfaces a single summary — how many have silent corruption or
missing files — so you can spot bitrot across all monitored folders without opening each one.

## Acceptance Criteria
- [x] `verify_all_baselines(baselines)` backend command re-scans + classifies each folder (reusing
      `verify_manifest`, CPE-870), skipping unscannable folders; returns a report per folder.
- [x] Palette command **Verify all baselined folders…** (enabled only when baselines exist) runs it and
      shows a clear notice: clean sweep vs. "N of M folders have issues — K files corrupted or missing".
- [x] `npm run check` + suite (902) green; clippy `-D warnings` green.

## Work Log
- 2026-07-21 (autonomous) — Backend command + palette action + `palette.verifyAll` across all 12 locales.
  Advances epic CPE-737 (on-demand monitor-all; a background scheduler remains a later child).
