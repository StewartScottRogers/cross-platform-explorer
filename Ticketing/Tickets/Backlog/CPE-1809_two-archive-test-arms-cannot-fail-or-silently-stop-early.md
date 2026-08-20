---
id: CPE-1809
title: an archive test assertion cannot fail, and a staging failure returns where it should continue
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

Two independent defects in `crates/server/src/archive.rs`, both found while reviewing PR #958:

1. **`err.contains("hard")` at `7237` cannot fail.** The scratch directory in that test is named
   `..._hardlink`, so the substring is present in the error text no matter what the error actually says.
   The assertion certifies nothing.
2. **A staging failure `return`s instead of `continue`s at `5605`.** A fixture that fails to stage one
   entry abandons the rest of the setup, so the test then runs against a **partially built archive** —
   and passes, having exercised far less than it claims.

## Why it matters

Both belong to the same family: a test that reports success while proving less than its name says. This
crew found **nine candidate cannot-fail tests in one sprint and eight were real** — the pattern is not
theoretical here, it is the dominant defect class in this file.

The second is the more insidious of the two, because the test still *does* something; it just silently
does less, and no output distinguishes the truncated run from the full one.

## What to do

- Fix `7237` to assert against something the fixture's own naming cannot supply. Then **red-proof it**:
  make the error say something else entirely and confirm the assertion now fails — the test as written
  would not have.
- Fix `5605` to `continue`, or fail the test outright on a staging failure. **Failing loudly is probably
  right**: a fixture that cannot be staged is a broken test, not a smaller test.
- Sweep the file for both shapes — a `contains` assertion whose needle appears in the fixture's own path,
  and an early `return` inside setup. Report what the sweep found even if it found nothing.

## Notes

Filed by the Foreman from the independent review of PR #958, 2026-08-20. Both pre-existing and explicitly
left out of that PR rather than widening it.

Related: **CPE-1759**, and the Evidence Rules in `Ticketing/wiki.md`.
