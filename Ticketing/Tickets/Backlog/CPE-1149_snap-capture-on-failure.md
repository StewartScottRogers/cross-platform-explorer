---
id: CPE-1149
title: "gui-smoke snap(): capture a screenshot on assertion FAILURE too (afterEach hook)"
type: chore
component: Testing
priority: low
status: Backlog
tags: ready
created: 2026-07-30
epic: CPE-579
---

## Summary
Follow-up surfaced by the independent review of CPE-1148 Part A (PR #464). The `snap(name)` calls are placed
**inline after each spec's assertions**, so a PNG is only written on a **passing** run — if an earlier `expect`
throws, `snap()` is never reached. But `gui-smoke/lib/snap.ts` + the specs + the README all claim "a failed
assertion still leaves a shot of whatever state it failed in," which is inaccurate for the current placement.

Two ways to reconcile — prefer the one that makes the docs true, because **capture-on-failure is the genuinely
more useful behaviour**: the failing frame is exactly what the Visual Critic (and a human) most wants to see.

## Acceptance Criteria
- [ ] Screenshots are captured on **both** pass and fail. Preferred: move/duplicate the capture into a
      WebdriverIO `afterEach`/`finally` hook (per spec, or a shared hook) that snaps the surface regardless of
      the test outcome, naming the PNG after the surface (keep the existing names; a failing shot may get a
      `-fail` suffix or overwrite — decide and document).
- [ ] The `snap.ts` header comment, the per-spec comments, and the `gui-smoke/README.md` claim are made
      **accurate** relative to the actual behaviour (no overstatement).
- [ ] Existing assertions and the non-blocking (`continue-on-error`) CI behaviour are unchanged; `snap` still
      swallows its own errors; `npm run check` + `gui-smoke` typecheck green; a real run still leaves the
      gallery of PNGs.

## Notes
- Small, isolated, test-infra-only change. Under epic CPE-579 (self-maintaining quality infra).
- Origin: CPE-1148 Part A reviewer's single non-blocking finding.
