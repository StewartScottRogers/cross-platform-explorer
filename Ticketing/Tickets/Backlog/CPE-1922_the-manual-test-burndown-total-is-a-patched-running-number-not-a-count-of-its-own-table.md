---
id: CPE-1922
title: MANUAL-TEST-BURNDOWN.md's MVD total is a patched running number, not a count of its own table — it has drifted 2-4 rows in both directions
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` carries a running Manual Verification Debt total
in its header. That number is **maintained by patching it forward** (add the rows a shift added,
subtract the rows it automated) rather than by counting the table. It has drifted.

Independently recounted by PR #1042's UAT tester, bare grep over rows marked `⛰ manual`, excluding
the legend line:

| | doc's stated number | literal recount |
|---|---|---|
| pre-PR #1042 | 18 (10 supplementary as of 2026-08-20 + 2 untallied) | **14** (6 primary + 8 supplementary) |
| post-PR #1042 | 16 | **12** (6 + 6) |

The header's "16 total", carried since 2026-08-20, does not match a fresh count of what was in the
table at that point either — so the drift predates any single shift.

## What is NOT wrong

The substantive claims are sound and were verified: the two rows CPE-1822 flipped really are
automated now, and the CI job named as pinning them (`gui-smoke` on `ubuntu-latest`) really is
**blocking** — `continue-on-error` was explicitly removed from it per CPE-1594, while
`windows-latest` remains a `continue-on-error: true` canary. The two "untallied since 2026-08-20"
additions (CPE-1821, CPE-1833/1836) are real rows genuinely added on 2026-08-23 without the total
being updated. PR #1042's *local* arithmetic was done correctly; it reconciled forward from an
already-drifted baseline.

## Why it matters

This ledger is how the crew claims manual testing is disappearing. A number that is approximately
right and drifting is worse than no number, because it reads as measured. The user's standing goal
is to never test anything by hand — the burden of proof is on this file.

## Acceptance criteria

- [ ] **Recount from the table, once, from scratch.** Reset the header total to the literal count.
      Do not patch it forward again from the old value.
- [ ] Make the total **derived rather than asserted**: a script or test that counts the `⛰ manual`
      rows (and the `🔧 in progress` / `🟡 partial` rows separately) and fails CI when the header
      disagrees with its own table. That is the only fix that stops this recurring — this is the
      third bookkeeping correction in this file's history.
- [ ] Decide and document how `🔧 in progress` and `🟡 partial` rows count toward MVD. The ambiguity
      between "6 primary" (which includes them) and "4 primary manual" (which does not) is part of
      how the drift happened.
- [ ] While in there: record the Trash-view surfaces that CPE-1822 did **not** photograph, so they
      are visible debt rather than assumed-covered — row **selection** state ("N selected"), the
      Empty/Restore **ConfirmDialog** (the highest-stakes screen in that view), the
      **restoreErrors** banner, the **overflow menu**, narrow width, and long-filename truncation.
- [ ] Also note the one inconsistency found in the new specs: `trash-degraded-scrolled` is captured
      **dark-only**, despite its test title claiming "in both themes". Either fix the spec or record
      it honestly.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1042's independent UAT recount. This is QA-Architect
work — it is about the measuring instrument, not any one surface.
