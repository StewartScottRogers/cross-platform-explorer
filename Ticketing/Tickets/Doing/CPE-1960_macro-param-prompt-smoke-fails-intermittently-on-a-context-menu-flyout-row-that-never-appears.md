---
id: CPE-1960
title: "`macro-param-prompt.smoke.ts`: webdriverio 9.31.4's `scrollIntoView` wheels at (0,0) and closes the flyout the spec is about to click"
type: bug
priority: High
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

    NEW GUI REGRESSION: "macro-param-prompt.smoke.ts :: running a bound {ask:suffix} macro
      opens MacroParamPrompt before any dry-run confirm"
    element (".ctx .flyout .row") still not existing after 5000ms

Observed on **two independent branches** on 2026-08-27 — job `98697809924` (sha `373ee259`) and job
`98705756557` (PR #1068) — with byte-identical output: `14/14 spec file(s) reported, 26 case(s) —
23 passed, 1 failed, 2 skipped/pending`.

It is **not** listed in `gui-smoke/known-failing.json`, and it is **intermittent** — other shard-2
runs the same day reported 14/14 with no failure.

## Why nobody saw it until now

Shard 2 had **two independent failure modes on 2026-08-27**, and they masked each other:

1. **A transport death (CPE-1955)** that killed the shard at spec #2 and reported *"0 new failing
   cases"*. `macro-param-prompt` is spec #6, so on those runs the app was already gone and the spec
   never really ran — it appears in those logs only as cascade noise
   (`newFile → resetFailedRestartingSession` in ~3 ms with a `WebDriverRequestError`).
2. **This genuine failure**, on the runs that *survived*.

Runs that died reported nothing actionable and were re-run. Runs that survived reported **this**, and
were re-run too. So the re-run reflex the CPE-1955 ticket worried about was discarding a **legible,
named regression the ratchet had correctly reported** — not only evidence that had never been written.

**Correction to an earlier claim:** the Foreman first supposed this failure had been hidden *inside*
CPE-1955's swallowed thirteen. It had not — `grep -c 'ctx .flyout .row'` on job `98646323315` is
**0**. PR #1068's worker established that rather than accepting the hypothesis. The two defects are
adjacent, not nested.

## Acceptance criteria

- [x] **Reproduce it before fixing.** It is intermittent, so run the spec repeatedly and report a rate,
      not a single observation. If it will not reproduce locally, say so and work from CI logs — but do
      not fix on a guess.
- [x] Establish what `.ctx .flyout .row` is waiting for and why it sometimes does not arrive within
      5 s. Candidates worth ruling in or out: the flyout is opened but empty; the context menu opens on
      a different element; a render the spec does not wait for; or the **CPE-1728 slow-renderer**
      family, which is the same shape that triggers CPE-1955's reset failure two specs earlier.
- [x] Decide whether the defect is in **the app** or **the spec**, and say which. A spec that waits for
      the wrong thing is a real defect too, but a different one — and this repo has a standing rule
      that a fixture that happens to reproduce is the same defect class as the bug.
- [x] **Do not add it to `known-failing.json` as the fix.** It is a real intermittent failure in a
      surface users touch. If it genuinely must be deferred, the entry needs a ticket and a reason, and
      the deferral should be argued rather than assumed.
- [x] Red-proof: whatever the cause, show the failing condition and show it gone, at a rate comparable
      to the reproduction.
- [x] While there: check whether `macro-param-prompt`'s neighbours in shard 2 share the wait pattern —
      the two `skipped/pending` cases in the same run are pre-existing and unexplained.

## Notes

Filed 2026-08-27 by the sprint Foreman. Surfaced by **CPE-1955** / PR #1068: the attribution fix
turned an illegible `SUITE DID NOT COMPLETE` into `14/14 reported` with a named failing case on its
first CI run. Deliberately **not** exempted to let that PR go green — never exempt the thing your tool
just found in order to land the tool.

Related: **CPE-1955** (the transport death and the attribution fix, PR #1068), **CPE-1728** (the
slow-renderer family), **CPE-1753** (the `incomplete=true ⇒ RED` verdict job), **CPE-1171** (the
gui-smoke harness).

## Raised to High 2026-08-27 — and it may not be intermittent at all

A third occurrence, on **PR #1066** (job at 23:47Z), byte-identical again: `14/14 spec file(s)
reported, 26 case(s) — 23 passed, 1 failed, 2 skipped/pending`, same case, `incomplete=false`.

So it has now been seen on **three unrelated branches** — `373ee259`, #1068, #1066.

**Reconsider the "intermittent" framing.** Every shard-2 run that *actually completed* has reported
this failure. The runs that appeared clean are the ones that died at spec #2 and never reached spec
#6 (CPE-1955's transport death), which reported `0 new failing cases` and were re-run. If that holds,
the spec is failing **consistently** and was simply never visible — which makes the reproduction step
easier, not harder, and changes the diagnosis.

**Check that first**: find any shard-2 run with `14/14 reported` and **no** `macro-param-prompt`
failure. If none exists, the word "intermittent" should come out of this ticket.

**It now blocks the merge queue.** With CPE-1955's attribution fix surfacing it on every complete run,
this is a permanent red on the `gui-smoke-linux-verdict` job for every PR — so it is no longer a
background defect, it is the thing standing between the queue and green.

## Work Log — 2026-08-27/28

### It is neither intermittent nor "always failing on completed runs" — it has a commit boundary

The ticket's own first check (*"find any shard-2 run with `14/14 reported` and no `macro-param-prompt`
failure"*) **found two**, so the "every completed run shows it" theory in the *Raised to High* section
is wrong — but so is "intermittent". Every shard-2 run that completed sits on one side or the other of a
single merge:

| shard-2 job | time (Z) | branch | ratchet line | this case |
|---|---|---|---|---|
| `98681871872` | 21:44 | `cpe-1945-gui-smoke-npm-audit` | `14/14 … 24 passed, 0 failed, 2 skipped` | passed |
| `98686079109` | 22:02 | `worktree-agent-add93db74672448c3` | `14/14 … 24 passed, 0 failed, 2 skipped` | passed |
| — | **22:27:49** | **`48aa8697` — CPE-1945 (PR #1065) merges to main** | | |
| `98697809924` | 22:45 | `worktree-agent-add93db74672448c3` (`373ee259`) | `14/14 … 23 passed, 1 failed, 2 skipped` | FAILED |
| `98705756557` | 23:35 | `cpe-1955-gui-smoke-shard-death` (#1068) | `14/14 … 23 passed, 1 failed, 2 skipped` | FAILED |
| (#1066) | 23:47 | `worktree-agent-add93db74672448c3` | `14/14 … 23 passed, 1 failed, 2 skipped` | FAILED |

It is **100 % deterministic on either side of `48aa8697`**. "Three unrelated branches" was the tell that
this was not a race: they were unrelated branches that had all rebased past the same commit.

`48aa8697` is CPE-1945's audit pass, and the only functional change in it is
`gui-smoke/package-lock.json`: **webdriverio 9.30.0 → 9.31.4**.

### The defect is in THE SPEC (the harness helper), not the app

`macro-param-prompt.smoke.ts`'s `pointByText()` called WebdriverIO's `element.scrollIntoView()`
**command** on `.ctx .flyout .row` — a popup-menu row. That command does not call the DOM API; it
computes a delta and injects a real mouse wheel through the driver. The two versions differ exactly
here, and both payloads are in the CI logs verbatim:

* **9.30.0** (job `98686079109`, the passing run):
  `{"type":"scroll","x":-249,"y":-334,"deltaX":0,"deltaY":0,"origin":{"element-…":"node-C660…"}}`
  — origin *on the element*, delta **zero**. A harmless no-op.
* **9.31.4** (`node_modules/webdriverio/build/index.js`, `scrollIntoView`):
  `await browser.action('wheel').scroll({ duration: 0, x: 0, y: 0, deltaX, deltaY }).perform()`
  — **no `origin`**, so the wheel lands at viewport **(0, 0)**, nowhere near the element, carrying a
  real non-zero delta computed from the element's rect.

On WebKitGTK that stray wheel relocates the webview's hover target; `Submenu.svelte`'s
`on:mouseleave={() => closeMenu(false)}` fires, the flyout unmounts, and the next command on the row
re-resolves `.ctx .flyout .row`, finds nothing, and burns the 5 s implicit wait.

**The failure screenshot settles it** (`gui-smoke-screenshots-ubuntu-shard-2` →
`macro-param-prompt-fail.png`, run `33126192786`): the context menu is **still open and still
unscrolled** — every row from the icon strip down to "Properties", with "Run macro ▸" exactly where it
was — and only the hover-opened flyout is gone. That rules out the other candidates the ticket listed:
the flyout was **not** empty (the first `findElements` returned the row and its `getHTML` read
`… CPE-1190 Ask Macro`), the menu did **not** open on a different element, and `.ctx` never scrolled, so
`Submenu`'s CPE-1601 anchor-scrolled-out path is not involved. It is not the CPE-1728 slow-renderer
family either — nothing was slow; something was actively closed.

The app is behaving correctly: a pointer leaving a hover-opened submenu should close it.

### Reproduction + red-proof

Reproducing the WebKit *hover relocation* needs WebKitGTK, which is Linux-only — so the end-to-end
failure does not reproduce on this Windows box, and it does not reproduce under Chromium at all (Chrome
does not move hover for a driver wheel that scrolls nothing, which is also why the Windows gui-smoke leg
never saw this). Said plainly rather than papered over.

What **does** reproduce locally, deterministically, is the trigger — the unrequested wheel. Driving the
**same webdriverio 9.31.4** out of `gui-smoke/node_modules` against real Chrome 151 (headless,
off-screen, non-focused) over a static page mirroring `ContextMenu.svelte` + `Submenu.svelte`, with
`performActions` instrumented to record what the helper emits:

```
[old] flyout open; row rect {"top":556,"height":32,"viewportH":700}
[old] stray wheel action sequences dispatched by the helper: 1
[old]   [{"type":"scroll","x":0,"y":0,"deltaX":560,"deltaY":222,"duration":0}]
RESULT[old]: FAILED — resolved the row, but dispatched 1 unrequested wheel scroll(s) into the open menu

[new] stray wheel action sequences dispatched by the helper: 0
RESULT[new]: PASSED — resolved the macro row at {"x":620,"y":572}, no stray wheel
```

A wheel of **(560, 222)** fired at the viewport origin, into an open menu, for an element that was
already fully on screen — and zero after the fix.

The guard test was red-proofed the same way: `git stash push -- specs/` (restoring the pre-fix specs)
makes `lib/scrollIntoViewUsage.test.ts` name all **seven** offending call sites and fail; with the fix
in place `npm run test:unit` is 133/133.

### The fix

* **`gui-smoke/lib/scrollIntoView.ts`** (new) — `scrollIntoViewCentered(el)`: the page's own
  `Element.scrollIntoView({ block: "center", inline: "nearest" })` run inside `element.execute`.
  A correct no-op for the `position: fixed` menu rows that can never be scrolled, and — for rows that
  really are below the fold — it scrolls their **real** scrollable ancestor (`.filelist-pane`), which
  the wheel-at-(0,0) never did, because this app's document does not scroll. It is also already the
  majority convention here (`archive-browse`, `archive-password`, `drive-menu`, `home-item-menu` and
  `vault` all call the DOM API from inside `browser.execute`).
* All **seven** uses of the WebdriverIO command replaced: `macro-param-prompt` x2, `macro-in-menu` x2,
  `link-badge` x2, `transfer-panel` x1. The other five were latent instances of the same defect.
* **`gui-smoke/lib/scrollIntoViewUsage.test.ts`** (new) — fails the build if the command returns.
  Scans `specs/*.ts` + `lib/*.ts`, asserts it scanned a non-trivial number of files (a guard that
  silently scanned nothing would pass forever), and pins both directions of the regex so it cannot rot
  into one that matches nothing.
* `gui-smoke/README.md` — the standard, with the measured payloads.

**Not** added to `known-failing.json`: it is a real red in a surface users touch, and the cause turned
out to be a one-line dependency-behaviour change, not something to defer.

### The two `skipped/pending` cases — pre-existing and already explained

`drive-menu.smoke.ts`'s *"Home shows at least one drive tile to right-click"* and *"right-clicking a
Home DRIVE TILE opens the drive menu and it stays open"*, gated by `SKIP_HOME_DRIVE_TILE =
process.platform === "linux"`. Not unexplained: **CPE-1483** re-gated them deliberately after a real
ubuntu CI run (PR #747, job `31354691439`) dumped `{"homeExists":false,"qaGridExists":false,…}` — the
whole Home landing fails to stay mounted under WebKitGTK/Xvfb, not just one tile. The sibling
*sidebar* drive-ROW test exercises the same drive-context-menu behaviour and is **not** gated, so no
menu coverage is lost. Nothing to do here; they are correctly counted as skipped, not as passes.

### Side benefit

`lib/logSignature.ts` classifies `Failed to execute "scrollIntoView" using WebDriver Actions API: move
target out of bounds` as an environment marker (its fixture is CPE-1728's own evidence excerpt). That
warning came from this same command; removing the command removes that noise class from the logs too.

### Checks

* `gui-smoke`: `npm run typecheck` clean, `npm run test:unit` **133/133**.
* root: `npm run check` — 0 errors / 0 warnings; `npm test` — **345 files / 4926 tests passed**,
  2 skipped.
