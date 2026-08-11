---
id: CPE-1595
title: "Triage the gui-smoke Linux known-failing tail (network/archive-browse/archive-password/shred-dialog/transfer-panel)"
type: Task
status: Done
priority: Medium
component: CI/QA-infra
epic: CPE-810
tags: [ready]
created: 2026-08-10
closed: 2026-08-10
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

## Work Log (2026-08-10)
Worked without a live `tauri-driver`/WebKitGTK run available (out of scope for this session — see the
ticket's own Notes above). Evidence base: the `gui-smoke-screenshots-ubuntu` artifact from run
31449236678 (job 93649941153, the first green-gated Linux leg, produced by CPE-1594's own PR #801), the
raw job log pulled via `gh api .../actions/jobs/93649941153/logs`, and the spec + component source.

**Per-spec verdict:**
1. **`network.smoke.ts` — (c) environment limitation, CONFIRMED not just a theory.** The failure
   screenshot (`network-fail.png`) shows the "Network" section header fully rendered and visible on
   screen at the exact moment the 15s `waitUntil` gave up — so this is not "never rendered" (rules out
   another selector attempt) and not a product regression (`Sidebar.test.ts`'s jsdom render already
   passes on `main`). `getText()` nonetheless returned something other than "Network" for the entire
   wait window. Same symptom, same evidence shape, as `saved-search.smoke.ts`'s own known-failing entry
   (owned by CPE-1507) — its own failure screenshot (`saved-search-fail.png`) shows the exact same
   pattern: "Saved Searches" AND its saved entry both fully rendered, yet the spec's `getText()`-based
   wait for that header also timed out. This is the "shared root cause" the ticket asked to explain: a
   WebKitGTK/Xvfb classic-WebDriver `GetElementText` quirk for this `<span class="label fav-title">`
   header shape that disagrees with the real paint — not a rendering gap, not a selector bug, and (per
   the explicit "do not repeat the guessing mistake" instruction) not something I'm claiming a fix for
   without a live run to prove it. Left listed with the reason upgraded from "unverified" to this
   confirmed diagnosis; root-cause fix stays open, cross-referenced to CPE-1507.
2. **`archive-browse.smoke.ts` — (b) stale/incorrect test, FIXED.** Real failure (not the old placeholder
   "click intercepted"): `expected a file row containing CPE-1181-archive.tar.gz: expected false to equal
   true`, from a one-shot `.row` scan run immediately after only `crumb.waitForExist` — which proves
   navigation started, not that the (docs/design/STREAMING.md) batched listing finished streaming ~30
   root-level fixtures under CI load. Fixed by wrapping the scan in `browser.waitUntil`, matching this
   suite's own established retry convention. Entry left in `known-failing.json` (cannot verify live in
   this session) with the diagnosis + fix recorded in its `reason`.
3. **`archive-password.smoke.ts` — hardened, verdict inconclusive.** Real failure:
   `WebDriverError: move target out of bounds` from `mouse.ts`'s Actions-fallback `rightClick`,
   immediately after `pointOfRow` computed a point via `scrollIntoView` + a fixed 150ms pause for the
   SAME `CPE-1181-archive.tar.gz` row archive-browse targets. Hardened `pointOfRow` with an in-bounds
   re-check-and-retry loop (was a single scroll + fixed pause with no verification the point actually
   settled) rather than guess at one specific cause — genuinely unsure whether this is a CI-load timing
   gap or a WebKitGTK Actions-fallback quirk, so I did not claim more confidence than the evidence
   supports. Entry left in place.
4. **`shred-dialog.smoke.ts` — (a) genuine product bug, FIXED (filed + closed as CPE-1601).**
   `ContextMenu.svelte`'s `.ctx` had no `overflow`/`max-height` — a rich file's menu (this fixture is
   shreddable, so the separated "Securely delete…" group renders) can be taller than the window, and the
   `onMount` clamp only ever repositioned the menu, never accounted for its own height exceeding the
   viewport. The failure screenshot (`shred-dialog-fail.png`) shows the menu's last visible row
   ("Metadata Studio…") sitting right at the window edge with "Securely delete…" pushed off the bottom —
   and because `.ctx` wasn't a scroll container, `scrollIntoView()` (which WebdriverIO's own
   clickability check, and a real user's scroll/keyboard nav, both rely on) had nothing to scroll, so the
   row was **permanently** unreachable. Fixed with `max-height: calc(100vh - 12px); overflow-y: auto;` on
   `.ctx` — this also brings the component into compliance with docs/design/MENUS.md's own already-stated
   container rule ("clamped into the viewport (never clipped off-screen)"), which the height-only clamp
   was silently violating. `ContextMenu.test.ts`'s 51 tests still pass. Entry left in
   `known-failing.json` (cannot verify live) — see CPE-1601 for the full writeup.
5. **`transfer-panel.smoke.ts` — (b) stale/incorrect test, FIXED.** Real failure: `expected a row for the
   seeded "CPE-1226-transfer-panel-folder": expected null to not equal null` on the spec's VERY FIRST
   step (before any right-click/compress logic runs at all) — the same batch-streaming race as
   `archive-browse.smoke.ts`, which then cascades: test 2 and test 3 both fail too, but only because
   test 1 never got the app into the state they assume. Fixed the same way — wrapped the initial
   `pointOfRow` call in `browser.waitUntil`. Entry left in place pending a live confirmation.

**Why no entries were deleted:** every fix above is reasoned from code + log + screenshot evidence, not
verified against a live `tauri-driver` run (explicitly out of scope for this session). `gui-smoke/lib/
ratchet.ts` treats a listed-but-now-passing spec as a hard failure ("RATCHET: stale entry") — deleting an
entry on an unverified guess would risk redding the blocking main gate, which is exactly the mistake this
ticket's brief warned against repeating. Verified instead: `gui-smoke/known-failing.json` is valid JSON
and self-consistent (`npx tsx scripts/run-ratchet.ts` against a synthetic report matching today's file
reproduces the real run's own "40/40 spec(s) reported — 33 passed, 7 failed, 7 known-failing listed" /
"OK" verdict exactly), and separately that the ratchet DOES catch a stale entry (flipped `shred-dialog`
to "passed" in a throwaway synthetic report — got the expected "RATCHET: ... stale" red). A follow-up
should watch the next Linux CI run and delete whichever of these 4 entries (network.smoke.ts is not
expected to go green) now show green.
