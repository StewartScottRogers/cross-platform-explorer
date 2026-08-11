---
id: CPE-1595
title: "Triage the gui-smoke Linux known-failing tail (archive-browse/archive-password/shred-dialog/transfer-panel)"
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
CPE-1594 turned `gui-smoke`'s Linux leg into a real, blocking CI signal by ratcheting the then-7 currently-failing
specs into `gui-smoke/known-failing.json` rather than fixing them (out of scope for that ticket). One of those
seven — `network.smoke.ts` — was triaged and fixed in the same PR (see "Already resolved" below), leaving four
in this ticket's scope (the other two, `samples.smoke.ts` and `saved-search.smoke.ts`, are already owned by
CPE-1507):

- `archive-browse.smoke.ts` — element click intercepted.
- `archive-password.smoke.ts` — element not interactable.
- `shred-dialog.smoke.ts` — `.ctx button.row` still not clickable after 10s.
- `transfer-panel.smoke.ts` — seeded CPE-1226 transfer row never appears.

### Already resolved in the CPE-1594 PR: `network.smoke.ts` — stale test selector, NOT a product regression
Triaged and fixed before this ticket was even picked up, so it's removed from `known-failing.json` (6 entries,
not 7). Verdict, for the record:

- `gui-smoke/specs/network.smoke.ts` (pre-fix, line 43) located the header with `$("=Network")`. In WebdriverIO
  the bare `=text` form maps to the W3C WebDriver **"link text"** locator strategy, which by spec matches
  **only `<a>` anchor elements**.
- The real markup is `src/lib/components/Sidebar.svelte:862` — `<span class="label fav-title">Network</span>`, a
  plain span. That selector could never match, on any OS, in any app state — structurally unmatchable, not
  flaky.
- The product is fine: `Sidebar.svelte:838-871` renders the Network section header unconditionally (CPE-1516's
  "always render" behaviour is intact), and `Sidebar.test.ts` ("always renders the Network header, even with
  zero connections and zero shares") passes on `main`.
- It was the only spec in the whole suite using the bare `=text` strategy — the established convention
  elsewhere is a CSS/DOM query, e.g. `saved-search.smoke.ts` uses `$$(".fav-title")` filtered by `getText()`.
  `network.smoke.ts` now uses the same convention.

No further action needed on `network.smoke.ts` unless a *different* failure shows up after this fix lands in CI
(unverified live — this worktree has no `tauri-driver` — watch the first few `main` runs).

## Scope
For each of the 4 specs above: reproduce locally against a real `tauri build --no-bundle` under `xvfb-run`
(Linux) or, failing that, read the CI log carefully; determine whether the failure is (a) a real product
regression, (b) a WebKitGTK-under-Xvfb timing/interaction quirk (stale selector — like `network.smoke.ts` just
was — needs a `mouse.ts` CDP-vs-W3C-Actions fallback tweak, missing wait), or (c) a fixture/harness bug. Fix
what's fixable; if a fix isn't feasible this pass, narrow the reason string in `gui-smoke/known-failing.json` to
something more specific than today's placeholder.

**One-way ratchet reminder:** deleting a spec's entry from `known-failing.json` while it still fails reds the
Linux leg (CPE-1594's `gui-smoke/lib/ratchet.ts`). Only remove an entry once the spec is verified passing —
locally with a real build, ideally confirmed on a CI run before merging the removal.

## Acceptance
- [x] `network.smoke.ts` triaged and fixed in the CPE-1594 PR (stale `=text` link-text selector against a
      `<span>`, not a regression) — entry already removed from `known-failing.json`.
- [ ] Each of the remaining 4 specs above either passes (its entry removed from `known-failing.json` in the same
      PR) or has an updated, more specific `reason` in the JSON if still failing after investigation.
- [ ] `gui-smoke`'s Linux leg stays green (ratchet passes) throughout — never leave a passing spec listed, never
      delete an entry for a spec still failing.

## Notes
Filed alongside CPE-1594 (which built the ratchet substrate this ticket uses). Needs a real `tauri build` +
`tauri-driver` run (Linux, xvfb) to reproduce — not verifiable in a pure-code sandbox.
