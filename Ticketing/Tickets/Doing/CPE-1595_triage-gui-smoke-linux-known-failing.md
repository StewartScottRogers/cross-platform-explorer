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

- `network.smoke.ts` — its test **selector** was fixed in the CPE-1594 PR (see "Partially resolved" below), but
  it **still fails on the live Linux CI stack**. Triage the remaining cause next.
- `archive-browse.smoke.ts` — element click intercepted.
- `archive-password.smoke.ts` — element not interactable.
- `shred-dialog.smoke.ts` — `.ctx button.row` still not clickable after 10s.
- `transfer-panel.smoke.ts` — seeded CPE-1226 transfer row never appears.

### Partially resolved in the CPE-1594 PR: `network.smoke.ts`'s selector was a real bug, but wasn't the whole story
`gui-smoke/specs/network.smoke.ts` (pre-fix) located the header with `$("=Network")`. In WebdriverIO the bare
`=text` form maps to the W3C WebDriver **"link text"** locator strategy, which by spec matches **only `<a>`
anchor elements** — but the real markup is `src/lib/components/Sidebar.svelte:862`,
`<span class="label fav-title">Network</span>`, a plain span. That selector could never match, on any OS, in any
app state — structurally unmatchable, not flaky, and not a CPE-1516 product regression (`Sidebar.test.ts`
"always renders the Network header, even with zero connections and zero shares" passes on `main`). Fixed in the
CPE-1594 PR to use the same `.fav-title` + `getText()` filter convention `saved-search.smoke.ts` already uses
for its own non-anchor header.

**But a live PR #801 CI run (run 31446269217, job 93641134303) proved the corrected selector STILL fails**,
timing out on the exact same `waitUntil` that replaced the broken one:
`expected the permanent Network section header to render`. This matches `saved-search.smoke.ts`'s own
known-failing reason almost exactly (`.fav-title getText() returns empty` on this WebKitGTK/Xvfb stack, per
CPE-1507) — strongly suggesting a **shared root cause**: something about how WebKitGTK-under-Xvfb resolves
`.fav-title` element text (a rendering/paint-timing quirk, a stale-element-reference after a re-render, or
similar) affects the Sidebar's section headers in general on this CI stack, not something specific to either
spec. **`network.smoke.ts` therefore stays in `known-failing.json` (7 entries, not 6)** — this ticket's job is
to find and fix that shared cause (which would very plausibly fix `saved-search.smoke.ts` too), not to
re-investigate the selector, which is already correct.

## Scope
For each of the 5 specs above: reproduce locally against a real `tauri build --no-bundle` under `xvfb-run`
(Linux) or, failing that, read the CI log carefully; determine whether the failure is (a) a real product
regression, (b) a WebKitGTK-under-Xvfb timing/interaction quirk (missing wait, stale element reference, a
`mouse.ts` CDP-vs-W3C-Actions fallback tweak), or (c) a fixture/harness bug. **Start with `network.smoke.ts` +
`saved-search.smoke.ts` together** — the working theory above is that they share one root cause in how
`.fav-title` text resolves on this stack; a fix there may retire both known-failing entries in one PR. Fix
what's fixable; if a fix isn't feasible this pass, narrow the reason string in `gui-smoke/known-failing.json` to
something more specific than today's placeholder.

**One-way ratchet reminder:** deleting a spec's entry from `known-failing.json` while it still fails reds the
Linux leg (CPE-1594's `gui-smoke/lib/ratchet.ts`). Only remove an entry once the spec is verified passing —
locally with a real build, ideally confirmed on a CI run before merging the removal.

## Acceptance
- [ ] `network.smoke.ts`'s remaining live-CI failure triaged (the selector itself is already fixed — see above);
      verdict recorded, and either fixed (entry removed) or given an updated, more specific reason.
- [ ] Each of the other 4 specs above either passes (its entry removed from `known-failing.json` in the same PR)
      or has an updated, more specific `reason` in the JSON if still failing after investigation.
- [ ] `gui-smoke`'s Linux leg stays green (ratchet passes) throughout — never leave a passing spec listed, never
      delete an entry for a spec still failing.

## Notes
Filed alongside CPE-1594 (which built the ratchet substrate this ticket uses). Needs a real `tauri build` +
`tauri-driver` run (Linux, xvfb) to reproduce — not verifiable in a pure-code sandbox. `network.smoke.ts`'s
selector history is worth reading before diving in — see the PR #801 discussion and this ticket's "Partially
resolved" section above, so the fix isn't re-attempted the same way twice.
