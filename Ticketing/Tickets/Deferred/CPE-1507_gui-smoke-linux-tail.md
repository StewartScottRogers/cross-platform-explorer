---
id: CPE-1507
title: "gui-smoke Linux tail: 3 pre-existing failures revealed once the suite completes (populated-whitespace CDP-assumption + samples + saved-search)"
type: Bug
status: Deferred
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

## Work Log
- 2026-08-08: Pulled the FULL raw log for the failing ubuntu job (93164774027) via
  `gh api repos/.../actions/jobs/93164774027/logs` (the `gh run view --log`/`--log-failed` CLI paths both
  silently truncate ~11.8k lines short on this job — use the raw `/logs` blob endpoint instead if this needs
  re-checking) to get ground truth on exactly what each of the 3 specs failed on, rather than guessing from
  the spec source alone. Findings and actions below.

  **1. `populated-whitespace.smoke.ts` — FIXED (`gui-smoke/specs/populated-whitespace.smoke.ts`,
  `gui-smoke/lib/mouse.ts`).** The log showed THREE failing assertions in this one file, not just the
  ticket's headline one:
  - `CPE-1155: the CDP mouse-input channel is available` — `AssertionError: … expected false to equal true`
    (`cdpAvailable()` is false by design on WebKitWebDriver, exactly as diagnosed). Rewrote the `it` to assert
    `cdpAvailable() || actionsAvailable()` instead — added `actionsAvailable()` to `mouse.ts` (checks
    `browser.performActions` is attached; doesn't touch the CDP fast-path) so the spec proves "a faithful
    mouse-input channel exists" without hard-coding CDP.
  - `CPE-1157 (regression + diagnosis): right-click the BLANK area …` and `CPE-1155: real right-click in an
    EMPTY folder …` — BOTH threw `Error: Command failed: powershell … /bin/sh: 1: powershell: not found` from
    `osCursor()`, which shells out to Windows PowerShell unconditionally. This is a second, previously
    undocumented bug: it aborted these two its at the very first `osCursor()` call, BEFORE they ever reached
    the real app-menu-opens/no-native-menu assertions CPE-1157 exists to pin — i.e. the actual regression
    guard was never exercised on Linux at all, worse than the ticket's framing. Fixed by gating the whole
    cursor probe behind `CAN_CHECK_OS_CURSOR = process.platform === "win32"` (`maybeOsCursor()` returns `null`
    off-Windows) and skipping only the cursor-didn't-move `expect` off-Windows — the menu-opens/variant/
    quickrow assertions now run unconditionally on every platform.
  - `npm run typecheck` and `npm run test:unit` both pass clean in `gui-smoke/` after this change.

  **2. `samples.smoke.ts` (CPE-1358) — FIXED (`gui-smoke/specs/samples.smoke.ts`, `navigateTo()` only).**
  NOT a seeding gap and NOT meant to be non-blocking-and-ignored at the test level (the file's own header
  comment explains the `continue-on-error` is at the CI JOB level, CPE-1048, covering WebView2/WebKitGTK
  crash flakiness generally — it doesn't mean per-test failures here are expected/ignorable). Root-caused from
  the raw log, not guessed:
  - The FIRST sample (`archives/sample.rar`) failed `expected .pathedit to appear after Ctrl+L` after a clean
    10s of `findElements(".pathedit")` polls all returning `[]` — Ctrl+L's keypress never took effect at all
    that first time (WebKitWebDriver's Actions-based key delivery occasionally drops the first keypress).
  - Every SUBSEQUENT sample in the walk (~30 files) failed identically with
    `Can't call elementSendKeys on element … because element wasn't found`. Log detail: on the very next `it()`
    (`archives/sample.zip`), Ctrl+L worked instantly, `.pathedit` was found, `elementClear()` SUCCEEDED, but
    the immediately-following `elementSendKeys()` hit `WARN: stale element - terminating request` — the input
    had been unmounted between the two native WebDriver calls. Traced to `NavToolbar.svelte`'s
    `on:blur={() => (editingPath = false)}` (unconditional, no `relatedTarget` guard): WebKitWebDriver's native
    `elementClear` implementation appears to cause a transient blur on this input, which the app treats as
    "user clicked away" and closes the address bar — destroying the very node WebdriverIO's `setValue()` was
    mid-sequence with. Because every spec `it()` shares ONE app session, that first stale-element throw left
    the harness desynced for the rest of the walk, turning one root cause into ~30 near-identical failures.
  - Fix (gui-smoke-only, doesn't touch `NavToolbar.svelte` — the blur-closes behavior is plausibly correct for
    real users and not confidently a product bug worth changing under CI time pressure): rewrote `navigateTo()`
    to (a) retry the Ctrl+L keypress up to 3× with a short 4s poll each instead of one 10s wait, and (b) set
    the `.pathedit` value via `browser.execute` (direct `el.value = …` + dispatched `input` event) instead of
    WebdriverIO's native `elementClear`/`elementSendKeys` pair, which never triggers the blur race because it
    never calls those two native commands. `browser.execute` is already this harness's proven-reliable
    primitive against wry's webview (see `mouse.ts`'s `pointOf()` comment) — same class of fix CPE-1481 used
    for mouse input, applied here to keyboard/value-entry.
  - `npm run typecheck` and `npm run test:unit` both pass clean after this change.

  **3. `saved-search.smoke.ts` (CPE-1233) — NOT fixed; documented for a follow-up (no code change).** The log
  shows the palette flow worked correctly through the Save click (`Save search…` row found, name/extension
  fields filled, `elementClick` on `[data-testid="save-search-confirm"]` returned success, and the dialog
  confirmed CLOSED — `findElements("[aria-label=\"Select by criteria\"]")` → `[]`). Immediately after, exactly
  **3** `.fav-title` sidebar-section-header elements were found (stable, same 3 DOM node ids) and polled for
  the full 10s timeout — a 4th "Saved Searches" header never appeared, so `expect(sectionHeader).to.not.equal
  undefined` never got satisfied. A second, harder-to-explain wrinkle: `getElementText()` on ALL 3 of those
  pre-existing headers returned an EMPTY string on every single poll (confirmed from the raw response body,
  not just a log-formatting artifact) — yet `getText()` on other elements (breadcrumb `[aria-current="page"]`)
  works fine elsewhere in this same suite on Linux, so it isn't a universal WebKitWebDriver `getText()`
  breakage. `addSavedSearch`/`saveCurrentSearch` (`src/App.svelte:2603`, `src/lib/savedSearchStore.ts`) is a
  synchronous Svelte-store update with no async gap that could plausibly take 10 stuck seconds, which argues
  AGAINST "just needs a longer wait" (a longer timeout was not tried/confirmed to help, and the failure was
  perfectly stable/unchanging across all ~99 polls, not slowly resolving) — so this does NOT look like a
  simple render/seed race fixable with a robust wait, and I'm not confident enough to guess further (could be
  a genuine Linux-specific sidebar-reactivity gap in `Sidebar.svelte`'s `savedSearches` binding, or a
  WebKitWebDriver quirk specific to how `.fav-title` is laid out/scrolled within the sidebar's scroll
  container). Per the ticket's own instruction ("apply a fix ONLY if high-confidence … otherwise document"),
  left this spec UNCHANGED and recommend a follow-up ticket that captures a `.fav-title`-section HTML dump
  (`getHTML`, not `getText()`) plus a screenshot at the exact failure point on a live Linux run, to
  distinguish "the section never rendered" from "it rendered but text-retrieval is the problem."

  **Honest expectation for the next ubuntu run:** `populated-whitespace.smoke.ts` and `samples.smoke.ts`
  should go from FAILED to PASSED (both fixes are grounded in the actual CI log, not guesses, and both pass
  local typecheck/unit). `saved-search.smoke.ts` will almost certainly still FAIL — left untouched pending the
  Linux-side triage above. Net expected: ubuntu leg goes from 36 passing/3 failing to **38 passing/1 failing**,
  not fully green. Left this ticket in `Doing/` per its own acceptance criteria (needs CI verification) rather
  than closing early on an unconfirmed guess.

## 2026-08-08 (sprint) — populated-whitespace FIXED (PR #728); samples + saved-search remain (DEFERRED)
gui-smoke ubuntu leg improved 36→**37 passing** via PR #728. **populated-whitespace FIXED** — incl. a real
Linux-only bug (`osCursor()` shelled out to Windows `powershell`, throwing on Linux and aborting the CPE-1155/
1157 right-click guards before their real assertions ran; gated to win32) + the CDP assertion now accepts
`cdpAvailable() || actionsAvailable()`. The samples `navigateTo()` harness fix (retry Ctrl+L + set-value via
browser.execute) landed too but did NOT fully green **samples.smoke.ts** (CPE-1358) — still ≥1 failing case;
and **saved-search.smoke.ts** (CPE-1233, sidebar never grows the Saved Searches header) remains unfixed and
documented. Deferred with these 2 as the remaining tracked tail (needs another focused pass or per-spec tickets).
