---
id: CPE-1960
title: "`macro-param-prompt.smoke.ts`: webdriverio 9.31.4's `scrollIntoView` wheels at (0,0) and closes the flyout the spec is about to click"
type: bug
priority: High
status: Done
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

*(Superseded — the hypothesis below was **measured false** in round 2: 13 complete shard-2 runs on
webdriverio 9.30.0 reported no such failure. See "The discriminator is LOCKFILE CONTENT" in the Work
Log. Kept as the reasoning that prompted the enumeration.)*

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

### The discriminator is LOCKFILE CONTENT, not merge time — and the failure is racy, ~90%

*(Corrected in round 2. Round 1 sampled five shard-2 jobs, saw them sort cleanly around the CPE-1945
merge, and wrote "100 % deterministic on either side of `48aa8697`". Enumerating **all 32** shard-2 jobs
in the window falsifies that. Both the earlier "intermittent" framing and the *Raised to High* section's
"every completed run shows it" are also wrong. The measured version is below.)*

Fingerprint each shard-2 job by what `npm ci` actually installed: **`added 479 packages` = webdriverio
9.30.0, `added 489 packages` = 9.31.4**, both confirmed against
`git show <sha>:gui-smoke/package-lock.json`. Over the **32** shard-2 jobs from 2026-08-27 19:12Z to
2026-08-28 00:42Z (every shard-2 job in that span; 5 were cancelled before `npm ci` and 3 died
incomplete at spec #2 on CPE-1955's transport death):

| webdriverio | complete (14/14) runs | failed `macro-param-prompt` |
|---|---|---|
| **9.30.0** (479 pkgs) | 13 | **0** |
| **9.31.4** (489 pkgs) | 11 | **10** |

**Onset: 2026-08-27 20:33Z, job `98661503323`, on the `cpe-1945-gui-smoke-npm-audit` branch**
(`c33a9609`) — `14/14 … 23 passed, 1 failed`, `NEW GUI REGRESSION: macro-param-prompt…`. That is about
**two hours before** `48aa8697` merged to main at 22:27:49Z. Job `98669198175` (21:00Z, `92ddf70e`)
failed identically, also pre-merge. The branch had carried the bump since `c33a9609` at 20:12Z.

**Job `98681871872` — cited in round 1 as a clean *pre-bump* run — is nothing of the kind.** It checked
out `f656f36` (PR #1065's own merge commit), installed **489** packages, and reported
`14/14 … 24 passed, 0 failed, 2 skipped`. It is a clean, complete run **ON 9.31.4**. It only looks
pre-boundary if you sort by merge time.

**Why this is not pedantry.** At ~90% rather than 100%, **one green CI run does not verify this fix** —
a green run already happened on the broken version. Saying "100 % deterministic" inside a permanent
guard file, where a passing test reads as vouching for it, is exactly the CPE-1933 failure mode.

`48aa8697` is CPE-1945's audit pass; the only change in it that reaches gui-smoke is
`gui-smoke/package-lock.json`, and the relevant entry is **webdriverio 9.30.0 → 9.31.4**. That same
lockfile diff also carried **`expect-webdriverio` 5.7.0 → 6.0.9**, a semver-major. Considered and
**excluded**: the wheel trace below accounts for the failure end to end, and `expect-webdriverio` is not
on the `scrollIntoView` path at all.

### The defect is in THE SPEC (the harness helper), not the app

`macro-param-prompt.smoke.ts`'s `pointByText()` called WebdriverIO's `element.scrollIntoView()`
**command** on `.ctx .flyout .row` — a popup-menu row. That command does not call the DOM API; it
computes a delta and injects a real mouse wheel through the driver. The two versions differ exactly
here. **Only the 9.30.0 payload is in the CI logs verbatim** — Node's inspector elides 9.31.4's as
`actions: [Array]` — so the 9.31.4 side is read out of its installed source and the log's own rect
probe, which is a stronger claim anyway:

* **9.30.0** (job `98686079109`, a passing run, logged verbatim):
  `{"type":"scroll","x":-249,"y":-334,"deltaX":0,"deltaY":0,"origin":{"element-…":"node-C660…"}}`
  — the deltas were assigned to the origin **offset** fields and `deltaX`/`deltaY` were omitted,
  defaulting to 0. A no-op anchored on the element.
* **9.31.4** (`node_modules/webdriverio/build/index.js`, `scrollIntoView`):
  `await browser.action('wheel').scroll({ duration: 0, x: 0, y: 0, deltaX, deltaY }).perform()`
  — **no `origin`**, so the wheel lands at viewport **(0, 0)**, nowhere near the element, carrying a
  real non-zero delta computed from the element's rect.

So **9.31.4 did not break `scrollIntoView`**: it fixed a wrong-field bug and thereby made the command
actually scroll for the first time. The suite had been calling a no-op and relying on it.

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

### The wheel, derived from CI's own log (round 2 — this replaces the local-Chrome repro)

Round 1 reproduced the *trigger* locally, by driving the same webdriverio 9.31.4 against real Chrome 151
over a static page mirroring the menus. That worked, but it carried a caveat — a hand-built page, a
different browser engine — and it is unnecessary, because the whole derivation is already in the failing
CI run's log. From job `98705756557` (the real WebKitGTK shard, times as logged):

```
23:37:02.858  findElements(".ctx .flyout .row")  -> 1 element  (node-C4C10110)
23:37:02.903  getHTML on it                      -> "… CPE-1190 Ask Macro"   (present, populated)
23:37:02.908  rect probe -> elemRect {x:553.296875, y:589, width:178, height:32}
                            viewport {1000x700}, scroll {0,0}
23:37:02.908  performActions  wheel3             <-- the stray wheel
23:37:03.116  findElements(".ctx .flyout .row")  -> []   (and on every retry for 5s)
```

Through 9.31.4's installed `scrollIntoView`, for the call `scrollIntoView({ block: "center" })`:

* `deltaY = targetByOption.center.y = 589 − (700 − 32) / 2 = **255**`
* `inline` is **undefined**, so `deltaX` keeps its initial value `targetByOption.start.x` = 553.296875
* `Math.round` → **`(553, 255)`**, non-zero, so `if (deltaX === 0 && deltaY === 0) return` does **not**
  fire and the wheel is dispatched at viewport **(0, 0)**
* `isVisibleY` / `isVisibleX` *are* computed, but are consulted **only** inside the `block === "nearest"`
  / `inline === "nearest"` branches — so `block: "center"` bypasses the already-visible check entirely.

The row existed **250 ms before** the wheel and was gone **208 ms after**, with nothing in between. That
is in-CI, on the real driver, and it removes the WebKitGTK-repro caveat almost entirely. (The end-to-end
*hover relocation* is still WebKitGTK-specific — Chrome does not move hover for a driver wheel that
scrolls nothing, which is why the Windows gui-smoke leg never saw this. Said plainly rather than papered
over.)

**The intermittency is explained rather than explained away.** The *passing* 9.31.4 run
(`98681871872`) has a **byte-identical** rect probe — `elemRect {x:553.296875, y:589, width:178,
height:32}`, viewport 1000x700, scroll 0,0 — and dispatches the same `wheel3`. Same wheel, racy outcome:
the flyout simply survived that once. The mechanism predicts ~90%, and ~90% is what the 32-job
enumeration measures. "100 % deterministic" would have contradicted it.

### Red-proof

`git stash push -- specs/` (restoring the pre-fix specs) makes `lib/scrollIntoViewUsage.test.ts` name
all **seven** offending call sites and fail; with the fix in place `npm run test:unit` is 133/133.

The guard's regex was also probed by hand rather than assumed. It catches
`await el.scrollIntoView(…)` and `await Promise.all([row.scrollIntoView(…)])`, but **misses**
`const p = el.scrollIntoView(…); await p;`, `return el.scrollIntoView();`, and
`await (await $(".x")).scrollIntoView();` — it requires a literal `await` immediately before an unbroken
element expression. It also scans `specs/` and `lib/` **non-recursively** (both are flat today). None of
these can produce a false red; they can only let a bad call through. All of it is now stated in the
guard's own header rather than left implied by a comment claiming more than the regex does.

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
* root: `npm run check` — 0 errors / 0 warnings; `npm test` — **345 files / 4932 tests passed**,
  2 skipped.

## Work Log — round 2 (2026-08-28) — prose correction only, no code change

An independent Reviewer verified the code, helper, guard and call-site enumeration as correct and
shippable, then falsified round 1's **diagnosis narrative**. Round 1 sampled **5** shard-2 jobs; the
Reviewer enumerated **32**. I re-derived all of it from the GitHub Actions logs before writing any of it
down (the numbers below are mine, measured, not transcribed):

* **Fingerprint verified both ways.** `added 479 packages` ↔ `webdriverio 9.30.0` (checked at
  `aa6a0378:gui-smoke/package-lock.json`); `added 489 packages` ↔ `9.31.4` (checked at
  `9965f366:gui-smoke/package-lock.json`).
* **Onset re-measured.** Job `98661503323` started 20:30:36Z, checked out `eb3bf53` (merge of `c33a9609`
  into main), installed **489** packages, and at **20:33:30Z** printed `14/14 spec file(s) reported,
  26 case(s) — 23 passed, 1 failed` with `NEW GUI REGRESSION: "macro-param-prompt…"`. That is **two
  hours before** `48aa8697` merged at 22:27:49Z. Round 1's boundary was an artefact of sorting by merge
  time instead of by lockfile.
* **The counterexample confirmed.** Job `98681871872`: `HEAD is now at f656f36 Merge 9965f366… into
  7e03957…`, `added 489 packages`, `14/14 … 24 passed, 0 failed, 2 skipped`. Clean, complete, **on
  9.31.4**.
* **Rate re-measured over the full window** — the 32 shard-2 jobs from 2026-08-27 19:12Z to 2026-08-28
  00:42Z: **9.30.0 → 13 complete runs, 0 failures; 9.31.4 → 11 complete runs, 10 failures.** (5 jobs
  cancelled before `npm ci`; 3 died incomplete at spec #2.) The Reviewer's independently derived 9/10
  vs 0/14 differs only in where the window's edges fall; the conclusion is identical.
* **Wheel arithmetic verified against the log and the installed source.** The rect probe in job
  `98705756557` at 23:37:02.908 reads `elemRect { x: 553.296875, height: 32, width: 178, y: 589 }`,
  `viewport { height: 700, width: 1000 }`, `scroll { x: 0, y: 0 }` — confirmed line by line. Through
  9.31.4's `scrollIntoView`: `deltaY = 589 − (700 − 32)/2 = 255`, `deltaX` stays at
  `targetByOption.start.x = 553.296875`, rounded to `(553, 255)`; the
  `if (deltaX === 0 && deltaY === 0) return` early-out therefore does not fire. Confirmed in source that
  `isVisibleY`/`isVisibleX` are consulted **only** in the `nearest` branches.
* **"Both payloads are in the CI logs verbatim" was false and is gone.** `grep -c "type: 'wheel'"` on
  job `98705756557` is **3** but `grep -c deltaX` is **0** — Node's inspector elides them as
  `actions: [Array]`. The 9.30.0 payload *is* verbatim (job `98686079109`).
* **Byte-identical geometry in the passing run.** Job `98681871872`'s third rect probe is
  `elemRect { x: 553.296875, height: 32, width: 178, y: 589 }` — identical to the failing run's — and it
  dispatched the same `wheel3`. Same wheel, racy outcome. That is what makes ~90% the honest number.
* **`expect-webdriverio` 5.7.0 → 6.0.9** confirmed present in `48aa8697`'s lockfile diff; named and
  excluded rather than left unaddressed.
* **Guard regex scope** probed by hand (`return el.scrollIntoView();` and
  `await (await $(".x")).scrollIntoView();` also miss, on top of the detached-`await` form the Reviewer
  found) and now documented in the guard's own header. Widening it would be a code change; this round
  was prose-only, so it is written down rather than silently overclaimed.

**Operational consequence, stated in all five places: one green CI run does not verify this fix.**

**Expect a new Visual Critic baseline for `transfer-panel`.** Because 9.30.0's command was a no-op, the
five previously-latent call sites have never actually scrolled anything and now will —
`scrollIntoViewCentered` really does centre the row. That is the direction those comments always
intended, but `transfer-panel`'s is a screenshot case, so its framing genuinely changes (the broken-link
row now centred). That is a **new baseline, not a regression**.

Files corrected: the PR body, this ticket, `gui-smoke/README.md`,
`gui-smoke/lib/scrollIntoViewUsage.test.ts` (header only), `gui-smoke/specs/macro-param-prompt.smoke.ts`
(comment only). No executable code changed in this round.

## Closed 2026-08-27 — and BOTH earlier characterisations were wrong, including the Foreman's

Merged as PR #1072.

**Correcting this ticket's own "Raised to High" note, which I wrote.** It argued the word "intermittent"
should come out because *"every shard-2 run that actually completed has reported this failure."* **That
was false.** Fourteen completed runs are clean. The tell was in the evidence I used to argue it:
*"three unrelated branches"* is not the signature of a race — it is the signature of three branches
rebased past the same commit — and I drew the opposite conclusion from a fact I already had.

**The worker's correction was better and also wrong.** It root-caused the mechanism correctly and
called the boundary *"100% deterministic on either side of `48aa8697`"*, from **5 sampled jobs**.

**The measured answer** came from its Reviewer enumerating **all 32** shard-2 jobs in the window and
fingerprinting what `npm ci` actually installed — `added 479 packages` = webdriverio 9.30.0, `added 489
packages` = 9.31.4, a clean split:

| webdriverio | complete (14/14) runs | failed `macro-param-prompt` |
|---|---|---|
| **9.30.0** | 13 | **0** |
| **9.31.4** | 11 | **10** |

**Onset 20:33Z on the CPE-1945 branch** (job `98661503323`) — about **two hours before** `48aa8697`
merged. **The discriminator is lockfile content, not merge time**, which is why the falsifier hid in
plain sight: job `98681871872`, cited in three separate files as the clean *pre-bump* control, checked
out PR #1065's own **merge commit**, installed **489** packages, and passed. A clean, complete run **on
the broken version**.

**The consequence is operational, and it is why the prose blocked the merge: at ~90%, one green CI run
does not verify this fix.** That claim was about to land inside a permanent guard file where a passing
test would appear to vouch for it.

**The cause.** `48aa8697` is CPE-1945's `npm audit` sweep, and its only functional change is
`gui-smoke/package-lock.json`: **webdriverio 9.30.0 → 9.31.4**. `element.scrollIntoView()` is not the
DOM API — it injects a real mouse wheel. 9.30.0 assigned the computed deltas to the origin **offset**
fields and left `deltaX`/`deltaY` at 0 — a no-op anchored on the element. 9.31.4 fixed that
wrong-field bug and thereby made the command **scroll for the first time**, with **no `origin`**, so
the wheel lands at viewport **(0,0)**.

**Causation was established in CI, not by proxy.** From failing run `98705756557`'s own rect probe:
`block:"center"` → `deltaY = 589 − (700−32)/2 = 255`; `inline` undefined → `deltaX` keeps
`553.296875`; rounded to **(553, 255)**, non-zero, so the `deltaX===0 && deltaY===0` early return does
not fire. `isVisibleY`/`isVisibleX` are consulted **only** in the `nearest` branches, so `block:"center"`
bypasses the already-visible check entirely. The row existed 250 ms before the wheel and was gone
208 ms after, with nothing in between. The *passing* 9.31.4 run has byte-identical geometry and
dispatches the same wheel — the flyout simply survived that once, which is what ~90% means.

**Five of the seven call sites were latent.** Because 9.30.0's command was a no-op, they had never
scrolled anything; they now will.

**The transferable lesson: a lockfile-only dependency bump is a functional change.** This one silently
converted a no-op into an input event, inside the harness that guards the entire GUI verification leg,
in a diff nobody reviews.
