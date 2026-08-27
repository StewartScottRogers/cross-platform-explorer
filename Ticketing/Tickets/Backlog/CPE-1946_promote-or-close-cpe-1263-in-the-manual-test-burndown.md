---
id: CPE-1946
title: promote-or-close CPE-1263 in the manual-test burndown — by the file's own rule it is countable debt, so the total is 13 and arguably 14
type: task
priority: Low
status: Open
tags: ready
estimate: XS
created: 2026-08-27
---

## Summary

`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` carries one deliberately-recorded discrepancy, put
there by CPE-1922 so that a decision would be **filed and assignable** rather than left as a prose
promise about "the next pass".

The ledger's `excluded` annotation excludes the 2026-07 historical tables because their residual is
**feel/taste** — *"work that has no retiring ticket."* One row in that table does not meet that
criterion:

**CPE-1263** (the file-content search dialog, `ContentIndexSearchDialog.svelte`). Its residual is
**render/gui-smoke**, not feel/taste, and it **does** have a named retiring ticket: **CPE-1819**
— *"The gui-smoke palette-open block is copy-pasted in three specs, and the one palette-only search
dialog has never rendered in CI"* — live in `Ticketing/Tickets/Backlog/`, `tags: ready`. Ledger row
#12 names CPE-1819 as the shared blocker and says solving it retires the CPE-1263 residual too.

By the rule the file states, that is countable debt.

## Why it was not simply counted

Counting it during CPE-1922 would have moved the recount off **12** — the value PR #1042's UAT
tester reached **independently**, by a different method (bare grep vs a GFM parser). That
cross-check is the only external verification the number has, and trading it to fold in one row was
not worth it. The decision was deferred; the record of it was not.

## The arithmetic, if promoted

    primary        6  ->  6   (unchanged)
    supplementary  7  ->  8
    total         13  -> 14

## Acceptance criteria

- [ ] **Decide: promote or close.** Promote if the render/gui-smoke residual is real manual debt that
      CPE-1819 will retire. Close if it turns out CPE-1819 already covers it in a way that means it
      was never separately countable, or if the residual has since been automated.
- [ ] If promoted: move the row out of the `excluded` table into the supplementary ledger, update the
      header total to **14**, and delete the discrepancy note and its short form in the `excluded`
      annotation. The derived-count test from CPE-1922 will fail until the header matches, which is
      the point.
- [ ] If closed: say why in the ledger, against the criterion — not "decided not to count it".
- [ ] Either way, **remove both halves of the note** so the ledger stops carrying a known
      discrepancy. A recorded discrepancy is a good interim state and a bad permanent one.
- [ ] Check whether any **other** row in the excluded tables fails the same criterion. CPE-1263 was
      found by reading the annotation against its own rows; nobody has checked the rest. Enumerate
      (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman, off wording PR #1055's Reviewer verified claim-by-claim
against CPE-1819's frontmatter and Ledger row #12's cells.

Related: **CPE-1922** (the recount that surfaced it, PR #1055), **CPE-1819** (the retiring ticket),
**CPE-1263** (the residual itself), **CPE-1042** (the independent UAT recount to 12 that the
deferral protects).
