---
id: CPE-1507
title: "gui-smoke Linux tail: 3 pre-existing failures revealed once the suite completes (populated-whitespace CDP-assumption + samples + saved-search)"
type: Bug
status: Doing
priority: Medium
component: CI/QA-infra
tags: [ready]
epic: CPE-810
parent: CPE-1481
created: 2026-08-08
---
## Context
CPE-1481 (merged f010276f, PR #724) took the gui-smoke Linux leg from **totally broken** (0 specs, hard 20-min
timeout) to a **completing suite: 36 passing, 3 failing** — the mouse harness (CDP→W3C Actions fallback),
timeout (20→45min), drive-menu (double-fire + drive-tile poll → gated) and home-item-menu (MRU seed race) are
all fixed. But finishing the suite for the first time **revealed 3 pre-existing failures** the timeout had
always hidden (the specs never ran to completion before). They are NOT regressions from CPE-1481 (round 5 only
touched `drive-menu.smoke.ts` + the workflow timeout). Each is its own distinct issue — filed here rather than
grinding CPE-1481 into more rounds (circuit-breaker discipline).

## The 3 failing specs (from ubuntu job 93164774027)
1. **`populated-whitespace.smoke.ts` (CPE-1155/1157)** — asserts *"the CDP mouse-input channel is available in
   this driver"*, which is **false on Linux WebKitWebDriver by design** (that's the whole reason CPE-1479 added
   the W3C-Actions fallback). This spec tests the OLD CDP assumption. **Fix:** rewrite its "CDP available"
   assertion to "mouse input works (via CDP *or* Actions)", or gate that specific assertion on Linux like
   drive-menu's tile tests. The right-click behavior it checks (app menu vs native, non-grabbing) should be
   validated through the fallback, not by asserting CDP presence.
2. **`samples.smoke.ts` (CPE-1358)** — "every samples/ file opens without crashing"; a batch of sample files
   (rar/zip/flac/mp3/ogg/ics/vcf/jwt/…) failed. Determine whether this is (a) samples not seeded on the Linux
   runner, (b) previews genuinely degrading/crashing on Linux (a real bug worth its own ticket), or (c) it's
   meant to be `continue-on-error`/non-blocking per its own design (the burndown noted it as non-blocking) and
   the leg shouldn't count it. Confirm and fix accordingly.
3. **`saved-search.smoke.ts` (CPE-1233)** — "save a search from the palette, show it in the sidebar, open the
   filtered view" fails one case. Triage: seed/timing race vs real bug.

## Acceptance
gui-smoke ubuntu leg is **green** (all pass) OR only-cleanly-gated env/CDP-assumption cases remain, each with a
filed reason. Then flip the QA burndown "gui-smoke GUI-driving" row fully green + name the pinning job. Note:
the leg's per-test 90s timeout + 45min job cap are already in place from CPE-1481.

## Notes
Sibling of CPE-1483 (Linux Home-landing drive-tile). Both are the honest tail of the gui-smoke restoration.
Epic CPE-810. QA-Architect owned.
