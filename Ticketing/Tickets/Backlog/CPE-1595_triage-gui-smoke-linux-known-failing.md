---
id: CPE-1595
title: "Triage the gui-smoke Linux known-failing tail (network/archive-browse/archive-password/shred-dialog/transfer-panel)"
type: Task
status: Open
priority: Medium
component: CI/QA-infra
epic: CPE-810
tags: [ready]
created: 2026-08-10
closed:
---

## Why
CPE-1594 turned `gui-smoke`'s Linux leg into a real, blocking CI signal by ratcheting the 7 currently-failing
specs into `gui-smoke/known-failing.json` rather than fixing them (out of scope for that ticket). Five of those
seven are this ticket's responsibility (the other two, `samples.smoke.ts` and `saved-search.smoke.ts`, are
already owned by CPE-1507):

- `network.smoke.ts` — "expected the permanent Network section header to render". **Triage this ONE first** —
  it may be a live regression in CPE-1516's shipped Network sidebar, not a harness/environment quirk.
- `archive-browse.smoke.ts` — element click intercepted.
- `archive-password.smoke.ts` — element not interactable.
- `shred-dialog.smoke.ts` — `.ctx button.row` still not clickable after 10s.
- `transfer-panel.smoke.ts` — seeded CPE-1226 transfer row never appears.

## Scope
For each spec above: reproduce locally against a real `tauri build --no-bundle` under `xvfb-run` (Linux) or,
failing that, read the CI log carefully; determine whether the failure is (a) a real product regression, (b) a
WebKitGTK-under-Xvfb timing/interaction quirk (stale selector, needs a `mouse.ts` CDP-vs-W3C-Actions fallback
tweak, missing wait), or (c) a fixture/harness bug. Fix what's fixable; if a fix isn't feasible this pass,
narrow the reason string in `gui-smoke/known-failing.json` to something more specific than today's placeholder.

**One-way ratchet reminder:** deleting a spec's entry from `known-failing.json` while it still fails reds the
Linux leg (CPE-1594's `gui-smoke/lib/ratchet.ts`). Only remove an entry once the spec is verified passing —
locally with a real build, ideally confirmed on a CI run before merging the removal.

## Acceptance
- [ ] `network.smoke.ts` triaged first and a verdict recorded (regression vs. harness quirk) — file a follow-up
      product-bug ticket if it's real.
- [ ] Each of the 5 specs above either passes (its entry removed from `known-failing.json` in the same PR) or
      has an updated, more specific `reason` in the JSON if still failing after investigation.
- [ ] `gui-smoke`'s Linux leg stays green (ratchet passes) throughout — never leave a passing spec listed, never
      delete an entry for a spec still failing.

## Notes
Filed alongside CPE-1594 (which built the ratchet substrate this ticket uses). Needs a real `tauri build` +
`tauri-driver` run (Linux, xvfb) to reproduce — not verifiable in a pure-code sandbox.
