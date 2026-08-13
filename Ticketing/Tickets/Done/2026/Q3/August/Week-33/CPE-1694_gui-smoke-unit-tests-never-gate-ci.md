---
id: CPE-1694
title: The gui-smoke suite's 59 unit tests have never gated CI — only typecheck and the real wdio run do
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-12
closed: 2026-08-13
---

## Problem

Found by the PR #873 reviewer while verifying CPE-1680. `.github/workflows/gui-smoke.yml` runs
`npm run typecheck` and `npm run ratchet` (the real wdio run) — it **never runs `npm run test:unit`**.

So the ratchet's own unit tests — 48 pre-existing plus the 11 CPE-1680 added — run only when a
developer chooses to run them locally. Nothing in CI executes them. A change that breaks the ratchet's
logic reaches `main` unless the real GUI run happens to exercise that exact scenario.

This is the **same shape as CPE-1690** (`cpe-mdns`'s 17 tests had never run in CI) and the same shape as
the bug CPE-1680 itself fixed: *a check that cannot fail is not evidence.* The ratchet is now the thing
that decides whether a GUI regression ships, which makes its own untested-in-CI status the more
uncomfortable of the two.

**Precisely what is and is not true**, because the distinction matters:

- The ratchet **library is type-checked** in CI (`npm run typecheck`), so a type error is caught.
- The ratchet **is executed** in CI by `npm run ratchet` against a real wdio run, so its happy path runs.
- Its **unit tests never run anywhere in CI.** The clause-by-clause behaviour they pin — including every
  guard CPE-1680 just added — is unprotected.

## Scope

`.github/workflows/gui-smoke.yml`, and `gui-smoke/package.json` if the test glob needs widening.

Note the related gap the same reviewer found: `test:unit`'s glob is `gui-smoke/lib/*.test.ts`, so even
when it does run it excludes `gui-smoke/scripts/**` — where `run-ratchet.ts` lives. CPE-1680's follow-up
work may already move that code into `lib/`; check before widening the glob, and do not do both.

## Acceptance criteria

- [ ] `npm run test:unit` runs in CI on every PR that can affect `gui-smoke/`, and its failure blocks
      the merge.
- [ ] Prove it gates: break one ratchet unit test on its own, push or run the workflow, and paste the
      **real CI failure output** — not a local run. Per the Evidence Rules in `Ticketing/wiki.md`,
      verify through the channel that will actually carry the message; a local green says nothing about
      a workflow step.
- [ ] Restore, and confirm the job goes green again.
- [ ] State the exact scope of the check: which workflow, which OS legs, which triggers. If it runs on
      Linux only, say so rather than implying every leg covers it.
- [ ] Decide and record whether the job should also run on the Windows leg (the GUI-smoke Windows leg
      is currently `skipping` on PRs — say whether that is intended).

## Notes

Filed by the Foreman from the PR #873 review, 2026-08-12. The reviewer explicitly declined to block
CPE-1680 on this, correctly: the gap is pre-existing and outside that ticket's stated scope.

Related: **CPE-1680** (the guards that are currently untested in CI), **CPE-1690** (the identical
never-run-in-CI hole in `cpe-mdns`), **CPE-1677** (the gate itself).
