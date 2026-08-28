---
id: CPE-1965
title: `organize.smoke.ts` fails on gui-smoke shard 4 with `element not interactable` — a NEW case, and **not** CPE-1960's shape
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

    NEW GUI REGRESSION: "organize.smoke.ts :: opens via the command palette, picks a rule,
      and renders grouped proposal rows"
    ERROR webdriver: WebDriverError: element not interactable when running
      "element/node-9D570771-.../click" with method "POST"

Observed on **PR #1074** (job at 2026-08-28T01:38Z): shard 4 reported `14/14 spec file(s) reported,
29 case(s) — 25 passed, 4 failed, 0 skipped/pending`, `incomplete=false`, **3 known-failing listed**,
so this is the **fourth** failure and the only unlisted one.

**Measured against `main`:** run `33131388244` (01:31Z, `main`) reported shard 4 as
`26 passed, 3 failed, 3 known-failing` — i.e. clean, every failure accounted for. Seven minutes later
the same shard on #1074 carries one extra.

## Why this needs its own ticket rather than a re-run

**It is not CPE-1960.** That one is `element (".ctx .flyout .row") still not existing after 5000ms` —
an element that never appears, caused by webdriverio 9.31.4's `scrollIntoView` injecting a real mouse
wheel at viewport (0,0) and closing a flyout on `mouseleave`. This one is **`element not
interactable` on a `click`** against an element the driver *found*. Different failure, different
mechanism, and it must not be filed under CPE-1960's diagnosis by proximity.

**#1074's diff cannot plausibly cause it.** That PR touches `.github/workflows/ci.yml`,
`scripts/ci-verdict.mjs`, and two vitest files. Nothing it changes reaches a gui-smoke spec. So this is
either a genuine intermittent that happened to land on #1074, or something in the shared harness.

**The re-run reflex is exactly what CPE-1955/CPE-1960 cost a day to.** Runs that failed illegibly got
re-run; runs that failed legibly got re-run too; a real, named regression was discarded for a day
because nobody wrote it down. **Do not re-run and move on.** One clean `main` run and one red #1074 run
is two data points, not a diagnosis.

## Worth ruling in or out first

- **A second wdio 9.31.4 casualty.** CPE-1960's root cause was a lockfile-only bump (9.30.0 → 9.31.4)
  that turned `element.scrollIntoView()` from a no-op into a real wheel event. **PR #1072 replaced
  seven command call sites but `organize.smoke.ts` was not among them** — and the failing job's log
  shows `document.querySelector(".recent-row")?.scrollIntoView({block:"center"})`, i.e. a **DOM-API**
  call inside `browser.execute`, which is the *correct* form and should be inert. Establish whether any
  wheel is dispatched near the failure. If a scroll moved the target between find and click, `element
  not interactable` is exactly what you would see, and this is the same family with a different
  symptom.
- **A genuine app defect** in the command palette → rule-pick → proposal-rows path.
- **A spec that clicks before the element is ready** — a real defect, but a different one.

## Acceptance criteria

- [ ] **Establish a rate, not an observation.** Enumerate every shard-4 job in the window and
      fingerprint what `npm ci` installed (`added 479 packages` = wdio 9.30.0, `489` = 9.31.4) — that
      is what settled CPE-1960 after three wrong characterisations, and it is cheap. **The discriminator
      is what the runner installed, not what the branch merged.**
- [ ] Say plainly whether it is **the app**, **the spec**, or **the harness**, and give the evidence.
- [ ] **Do not add it to `known-failing.json` as the fix.** If it genuinely must be deferred, the entry
      needs a ticket and a reason, and the deferral must be argued.
- [ ] **Check `organize.smoke.ts`'s neighbours on shard 4** for the same click-before-ready pattern —
      `batch-media`, `context-menu`, `declutter`, `home-item-menu`, `macro-in-menu`, `metadata-studio`,
      `preview-pane`, `saved-search`, `snapshot-diff`, `terminal-panel`, `trash-titlebar`, `vault`.
      Enumerate rather than recall (CPE-1932).
- [ ] While there: **identify the 3 known-failing shard-4 cases** and confirm each still has a live
      owning ticket and reason. One of them surfaces in the same log as
      `Error: expected the permanent Network section header to render` (`network.smoke.ts`).
- [ ] Red-proof: show the failing condition and show it gone, at a rate comparable to the reproduction.

## Notes

Filed 2026-08-27 by the sprint Foreman, on finding it while verifying #1074's reds. **It blocks #1074's
merge** until it is understood — that PR is otherwise APPROVED with a clean review.

Related: **CPE-1960** (the *other* shard failure — different shape, do not conflate), **CPE-1955** (the
attribution fix that made shard failures legible at all), **CPE-1910** (shard 2's WebDriver socket
deaths), **CPE-1171** (the gui-smoke harness), **CPE-1728** (the slow-renderer family).

---

## Work Log — 2026-08-27

### The title of this ticket is wrong, and that matters

`organize.smoke.ts` did **not** fail with `element not interactable`. It failed with:

```
3) CPE-1143 — headless GUI smoke: auto-organize dialog renders a grouped preview
   opens via the command palette, picks a rule, and renders grouped proposal rows
   expected a PNG group for the seeded CPE-1143-photo.png
   Error: expected a PNG group for the seeded CPE-1143-photo.png
       at async Context.<anonymous> (gui-smoke/specs/organize.smoke.ts:120:5)
```

i.e. `pngGroup.waitForExist({ timeout: 10_000 })` timed out. The `ERROR webdriver: WebDriverError:
element not interactable ... node-9D570771 ... /click` line in the same log is at **01:39:01.382**;
`organize.smoke.ts` did not start until **01:39:28.210** (`phase=handleRunnableStart:newFile`). It
belongs to an earlier spec on the shard, it was absorbed by a `waitForClickable` poll, and it failed
nothing. Two unrelated lines in one 25,000-line log were correlated by proximity — the same reflex this
ticket was filed to prevent, one level up.

### 1. The rate — full enumeration, fingerprinted by install

Every `GUI smoke` run created **2026-08-27T12:24:22Z → 2026-08-28T01:46Z** (103 runs), its shard-4 job
resolved via `actions/runs/<id>/jobs`, every job that reached `completed` and was not `cancelled`
(**69 jobs**) downloaded via `actions/jobs/<id>/logs`, and each fingerprinted by what `npm ci`
installed in `gui-smoke/`.

**The window cuts on JOB-START time** — the shard-4 job's own `started_at`, not the run's `created_at`.
Cutting on **run-created** instead admits **72** jobs: three more (`98727388423`, `98726514061`,
`98731172209`) whose runs were created before 01:46Z but whose shard-4 jobs did not start until
01:54–02:16Z. All three are **489 successes**, so the job-start figure below is conservative by a hair
and nothing else moves; the run-created split is 479 → 51 jobs / 1 failure, 489 → 21 jobs / 2 failures,
**3 of 72 = 4.2%**. Both numbers are stated wherever they matter. (Round 2: this basis was implicit
before, which is how two careful people counted 69 and 72 from the same query.)

| install | wdio | shard-4 jobs | organize.smoke failures | rate |
|---|---|---|---|---|
| `added 479 packages` | 9.30.0 | 51 | **1** | 2.0% |
| `added 489 packages` | 9.31.4 | 18 | **2** | 11.1% |
| **total** | | **69** | **3** | **4.3%** |

The per-version split is **not** evidence of a version effect: 1/51 vs 2/18 is **p = 0.1645** by
Fisher's exact test (two-sided), and on the run-created window's 1/51 vs 2/21 it is **p = 0.2019**.
The mechanism below is version-independent. Quote **4.3%** (**4.2%** on the wider window).

The three: job `98561730405` (14:56Z, `cpe-1913-containment-gates`, **479**), `98713492478` (00:14Z,
**`main`**, 489), `98725158256` (01:39Z, `cpe-1956-ci-verdict` = PR #1074, 489). All three carry the
identical ratchet line — `NEW GUI REGRESSION: "organize.smoke.ts :: opens via the command palette,
picks a rule, and renders grouped proposal rows"` — and all three read `25 passed, 4 failed`.

The 479-package one is decisive: **it predates the wdio 9.30.0 → 9.31.4 lockfile bump entirely** (that
landed in 48aa8697 / CPE-1945 / #1065 at 22:27Z; the first shard-4 job carrying 489 on `main` is
`98700261735` at 23:04Z, the last carrying 479 is `98695228264` at 22:39Z). So this is **not** a second
CPE-1960 casualty, not a 9.31.4 regression, and #1072's fix would not have touched it. It is a
pre-existing intermittent that happened to land on #1074. The 479/489 mapping was re-derived locally:
`npm ci` in `gui-smoke/` on today's lockfile prints `added 489 packages`.

`main` itself carried the failure at 00:14Z, an hour before the 33131388244 run the ticket cites as
clean — so "#1074 has one extra" was a sampling artefact, not a signal.

### 2. Verdict: **the spec**, exercising a real app papercut (now CPE-1968)

Proof, from run 33131342785's own failure screenshot `organize-dialog-fail.png` (artifact
`gui-smoke-screenshots-ubuntu-shard-4`): **"By kind" is still the highlighted pill**, the by_kind plan
is rendered (17 files into 6 groups — Archives, Audio, Code, Documents, …), and `CPE-1143-photo.png` is
plainly visible in the file list behind the dialog. So the folder was right, `organize_plan` worked,
and the fixture was present — the rule simply never changed. WebDriver reported the click as a
**success** (`elementClick(node-55E22567…)` → `RESULT null`); nothing was intercepted.

The mechanism is a **reflow between the driver computing the click point and dispatching it**:

- `.backdrop { display: grid; place-items: center }` — the dialog is vertically centred.
- `.preview { min-height: 120px; max-height: 45vh }` — 120 px while the first `organize_plan` is in
  flight, up to 315 px at the harness's 1000x700 window once it lands.
- The plan lands ~120 ms after mount (`scheduleLoad`'s own debounce). The dialog grows ~195 px and,
  being centred, the 28 px rule pills **slide up ~98 px**.
- The click lands ~98 px low, inside `.preview`, whose ancestor `.dialog` has
  `on:click|stopPropagation` — **swallowed in silence**. No rule change, no error, dialog stays open.

**Directly observed, not merely inferred.** In the failing log the driver issues `elementClick` at
**28.956** and logs `RESULT null` at **28.985**; the dialog's debounce fires at mount+120 ms ≈
**28.960**. The reflow lands *inside the click command's own 29 ms window*. That is the mechanism seen
happening, not reconstructed from CSS.

**The clustering, stated in its strongest TRUE form** (round-2 correction — the round-1 phrasing implied
115/116/117 ms was an otherwise-empty band, and it is not):

> Across the 72-job run-created sweep, `rule-picker` found → `elementClick` on `rule-by_extension`
> ranges 27–159 ms. **Zero failures occur outside a 113–119 ms band.** Of the **13** in-band jobs,
> **3 failed**; of the **59** out-of-band jobs, **0** failed. Fisher's exact, two-sided:
> **p = 0.0048**. (On the 69-job job-start sweep: 3 of 13 in-band vs 0 of 56 out, **p = 0.0055**.)

In-band is a **coin flip, not a death sentence** — 3 in-band jobs at 115/116/117 ms *passed*. That is
exactly what a race predicts, and it is what the spec comment already said ("clicking inside that window
is a coin flip"); only the summary prose implied exclusivity. Exclusivity is not needed and was never
true: what carries the argument is that **nothing outside the band ever fails**.

The app *logic* is not at fault: `OrganizeDialog.test.ts` already proves a rule click switches the rule
and that `loadGen` handles out-of-order plan resolution, and a new case re-proves it. The defect is
**positional**.

That the app lets a click be swallowed at all is a genuine user-facing papercut, filed as **CPE-1968**
with the three candidate fixes. Deliberately not fixed here: every one of them is a visual-design
decision (28 components share the exact centred-backdrop rule), which does not belong in a PR whose job
is to unblock #1074.

### 3. The fix

`gui-smoke/specs/organize.smoke.ts`:

1. Wait for the **default rule's preview to land** before clicking a pill — on
   **`[data-testid="summary"]` alone** (round-2 correction; see Work Log round 2 for why the round-1
   three-testid version was satisfied at t=0 and gated nothing).
2. After the click, **assert the pill actually became `.active`**, with a message naming the swallowed
   click. This is the legibility half: the failure used to surface 10 s later as "expected a PNG group
   for the seeded CPE-1143-photo.png", which reads like a broken `organize_plan` or a missing fixture —
   and did, for a day.

**Not** added to `known-failing.json`. That would have been wrong twice over: it is a real defect, and
it is one this harness can simply stop tripping.

### 4. Red-proof

- **Derivation** (CPE-1933): the spec's comment asserts three facts about `OrganizeDialog.svelte`
  (centred backdrop, preview grows, dialog swallows stray clicks). All three are now re-read out of the
  component at run time by a new block in `src/lib/components/OrganizeDialog.test.ts`, so the story
  cannot go stale silently. **Red-proved by hand**: setting `.preview`'s `min-height` to `45vh` (equal
  to its `max-height`) reds it — `expected '45vh' to not deeply equal '45vh'` — and reverting restores
  10/10 green.
- **Honest limit, stated at the site and here**: this does not reproduce the swallowed click. jsdom has
  no layout, so the ~98 px shift is not measurable locally, and at a 4.3% base rate **no single CI run
  settles it either way** — `ln 0.05 / ln(1 − 3/69)` = **67.4**, i.e. **68** consecutive green shard-4
  jobs for 95% confidence the rate has dropped (**71** on the run-created window's 3/72; round-2
  correction — "~65" was a guess, not a computation). The empirical half of the red-proof is the enumeration
  above plus the failure screenshot. The new in-spec assertion guarantees that if it *does* recur it
  fails at the click, naming the cause, instead of 10 s later pointing at the wrong thing.

### 5. Neighbour sweep — enumerated, not recalled (CPE-1932)

`git ls-files 'gui-smoke/specs/*.smoke.ts'` → **43 spec files, 138 `.click()` sites**; the 14 on shard 4
account for 51 of them. (Round-2 correction: the count was stated as 139. Re-derived —
`git ls-files "gui-smoke/specs/*.smoke.ts" | xargs grep -oh "\.click()" | wc -l` → **138**. A one-off
miscount in a CPE-1932 enumeration is exactly the thing that must not be waved through.) `context-menu.smoke.ts` and `home-item-menu.smoke.ts` have **zero** `.click()`
sites (they drive `lib/mouse.ts` / `browser.execute` instead) — absent from the table because the
enumeration found none, not because they were skipped.

`waitForClickable` is **not** protection against this class: it passed in every failing run. The real
predicate is *"clicks a control while an async body in the same container is still in its placeholder
and about to resize"*. Checked all 51:

| spec | verdict |
|---|---|
| `organize.smoke.ts:95` | **the bug** — fixed here |
| `macro-in-menu.smoke.ts:95` | **latent, same shape** — `new-macro-btn` sits in the Macros dialog *header* and is clicked as soon as the dialog exists, while `MacrosDialog.svelte`'s `onMount(refresh)` → `macroList()` is still in flight. Harmless today only because the smoke run's list resolves to `[]`, so nothing changes height. Recorded in CPE-1968; not changed here, because there is no "load landed" signal to wait on (`macro-list` renders in both states) and inventing one is an app change. |
| `snapshot-diff.smoke.ts` ×3 | safe — each click is gated on the async body first (`checkpoint-list` before `create-btn`, `drift-list` before `diff-btn`) |
| `batch-media.smoke.ts:158` | safe — asserts the rendered dialog header ("2 files") before clicking; the body is synchronous |
| `declutter.smoke.ts:144` | safe — clicked in the pre-scan intro state, nothing in flight, and a `snap()` round-trip sits in front of it |
| `metadata-studio`, `network`, `preview-pane`, `saved-search`, `terminal-panel`, `trash-titlebar`, `vault` | safe — no click into a still-loading container |

### 6. The 3 known-failing shard-4 cases

| spec :: case | ticket | status |
|---|---|---|
| `network.smoke.ts` :: the permanent Network section renders … | CPE-1595 → **CPE-1507** | **repointed** |
| `network.smoke.ts` :: clicking the entry point opens the add-connection popover … | CPE-1595 → **CPE-1507** | **repointed** |
| `saved-search.smoke.ts` :: saves a search from the palette … | CPE-1507 | live (Deferred) |

All three carry real, evidenced reasons (the WebKitGTK/Xvfb `getElementText()` quirk on `.fav-title`
headers, with `network-fail.png` showing the header painted while `getText()` returned something else).
**One audit finding**: both `network.smoke.ts` entries named **CPE-1595**, which is **Done** — it was
the *triage* ticket, and its own Work Log says "root-cause fix stays open, cross-referenced to
CPE-1507". Repointed both to **CPE-1507** (Deferred, pickable); reasons preserved and extended. Entry
count unchanged at 25, so no ratchet movement. `ratchet.ts` only shape-checks `ticket`, and only for
`intermittent: true` entries — which is how this drifted unnoticed.

### 7. Checks

- `gui-smoke`: `npm run typecheck` clean; `npm run test:unit` 149/149 pass.
- root: `npm run check` **0 errors, 0 warnings**; `npm test` **348 files, 4989 passed, 2 skipped, 0 failed**.

---

## Work Log — 2026-08-27 (round 2, after review of PR #1079)

### The blocker: the settle-wait was satisfied at t=0. It waited for nothing.

Round 1 waited on `summary` **or** `empty-state` **or** `error`, justified by this sentence in the spec
comment:

> the loading placeholder carries no testid, so "one of these three exists" IS "the dialog has stopped
> resizing"

**That sentence is false**, and it was load-bearing. `OrganizeDialog.svelte` initialises
`loading = false` and `plan = []`, and `$: rule, path, scheduleLoad()` only **arms**
`setTimeout(loadPlan, 120)` — `loading` does not become `true` until t=120 ms. So for the entire
pre-load window the markup takes the `{:else if plan.length === 0}` branch and renders
`data-testid="empty-state"` **synchronously at mount**.

Reproduced here with a jsdom probe over a real render (`round1` = the round-1 three-testid selector,
`round2` = the shipped one):

```
t=  0  round1=1  round2=0  | ids: help-btn,rule-picker,rule-by_kind,…,preview,empty-state,cancel-btn,apply-btn
t=119  round1=1  round2=0  | ids: help-btn,rule-picker,rule-by_kind,…,preview,empty-state,cancel-btn,apply-btn
t=179  round1=1  round2=1  | ids: help-btn,rule-picker,rule-by_kind,…,preview,summary,group-Documents,group-Images,…
```

**Consequence.** `browser.waitUntil` returned on its first poll, so the "fix" bought one `findElements`
round-trip — ~10–15 ms in these logs — and nothing else. It shifted the find→click gap distribution by
~10 ms, which moves runs that used to land at 100–107 ms **into** the 113–119 ms hazard band. The ~4%
flake was **re-labelled, not removed**, and the PR body's "it is fixed here" / "removes the CI red"
were not supported. Both corrected.

The second half of round 1 — asserting the pill actually became `.active` — is genuinely load-bearing
and is unchanged. It converts a 10-s-later "expected a PNG group" into a failure named at the click.
But re-labelling is all it does.

### The fix: wait on `summary` ALONE, and argue it

`[data-testid="summary"]` renders only in the `plan.length > 0` branch — i.e. only once the plan has
come back and `.preview` has grown to its final height. It therefore genuinely gates the reflow, which
is the whole point of the wait.

The round-1 worry that motivated the three-way tolerance — *"this must not become a silent second
assertion that a plan was produced"* — was misplaced twice over:

- **It is not silent.** The `timeoutMsg` names it: *"expected the Organize dialog's default (by_kind)
  preview to settle … the plan never rendered, so the dialog never reached its final height."*
- **It is not second.** The very next assertion (`[data-testid^="group-"]`, then the named PNG/ZIP
  groups) already asserts loudly that a plan was produced. An empty plan fails there regardless; this
  wait only decides *where* it is named.

Tolerating `error` was the same mistake in miniature: the error branch renders inside a `.preview` that
has **not** grown, so it is a "settled" state that does not settle the layout.

The rejected alternative — polling `$('[data-testid="preview"]').getSize().height` until stable across
two samples ≥100 ms apart — is fixture-agnostic, but it is a *heuristic* (two equal samples can
straddle a change), it adds ≥100 ms to every run, and it buys nothing here: this spec seeds its own
mixed-kind fixture, so a non-empty by_kind plan is guaranteed by construction and `summary` is a
**deterministic** state gate rather than a stability guess.

### The fact the wait depends on is now PINNED (CPE-1933)

`src/lib/components/OrganizeDialog.test.ts` gained a `CPE-1965 — organize.smoke's settle-wait does not
match until the plan renders` block that:

1. reads the selector literal **back out of `gui-smoke/specs/organize.smoke.ts`**, anchored at column 0
   on a real `const CPE1965_SETTLED_PREVIEW = "…";` declaration asserted to occur exactly once (same
   extractor shape as `guiSmokeFixtureLiterals.test.ts`, so a commented-out or quoted copy cannot
   match — CPE-1933 rule 2); and
2. drives a **real render** of the component through the debounce, asserting the selector matches
   **0** at t=0, **0** at t=119 ms, and **>0** after t=180 ms; and
3. pins the t=0 state directly — `empty-state` present, `summary` absent — and asserts the round-1
   three-testid selector **does** match at t=0, so the defect itself stays reproducible rather than
   becoming folklore.

**Red-proofed** (CPE-1933 rule 3): widening the spec's `CPE1965_SETTLED_PREVIEW` back to the round-1
three-testid selector reds **2 of 12** — *"organize.smoke.ts's settle-wait selector … already matches
at MOUNT, so `browser.waitUntil` on it returns on its first poll and gates nothing"* and *"expected
'[data-testid=\'summary\'], …' not to match /empty-state/"*. Reverted; 12/12 green.

This is the check that would have caught the round-1 defect on the day it was written.

### Number corrections (all four)

| claim | round 1 | round 2 | how |
|---|---|---|---|
| `.click()` sites across the suite | 139 | **138** | re-derived from `git ls-files` piped through `grep -oh "\.click()" \| wc -l` |
| Fisher p, per-version split | ≈ 0.15 | **0.1645** (1/51 vs 2/18); **0.2019** on 1/51 vs 2/21 | computed, two-sided |
| consecutive greens for 95% confidence | ~65 | **68** (`ln 0.05 / ln(1 − 3/69)` = 67.4); **71** at 3/72 | computed, not guessed |
| the enumeration window | "created 12:24Z–01:46Z" | same, **cut on job-start**; run-created admits 3 more (**72**) | stated explicitly in §1 |

The three run-created extras (`98727388423`, `98726514061`, `98731172209`) are all **489 successes**,
so the headline 4.3% is conservative by a hair and no conclusion moves. Both bases are now labelled
wherever a number appears.

### The clustering, restated as its stronger true form

Round 1 implied 115/116/117 ms was an otherwise-empty band. It is not — **3 in-band jobs passed**.
Exclusivity was never claimed by the spec comment (which says "a coin flip") but was implied by the PR
body. The honest and much stronger claim, now used everywhere: **zero failures outside a 113–119 ms
band; 3 of 13 in-band failed, 0 of 59 out-of-band; Fisher p = 0.0048.** A race predicts exactly this —
in-band is a coin flip, out-of-band is safe.

Added, from the failing log and previously unquoted: `elementClick` issued at **28.956** → `RESULT
null` at **28.985**, with the debounce firing at mount+120 ms ≈ **28.960**. **The reflow lands inside
the click command's own 29 ms window** — the mechanism directly observed rather than reconstructed.

### What round 2 does NOT claim

The wait now genuinely blocks until the reflow has happened, so the mechanism is closed at the point
the spec controls. But the rate is 4.3%: **no CI run settles it**, and 68 consecutive greens is the
bar. The `.active` assertion remains the safety net — if it recurs, it fails at the click with the
cause in the message instead of 10 s later pointing at the fixture.

### Round-2 checks

- root `npm run check`: **0 errors, 0 warnings**
- root `npm test`: **348 files, 4991 passed, 2 skipped, 0 failed** (4989 + the 2 new cases)
- `gui-smoke`: `npm run typecheck` clean; `npm run test:unit` **149/149**
- rebased on `origin/main` after #1072 and #1074 merged

## Closed 2026-08-27 — what the gauntlet actually proved

Merged as PR #1079, **fully green (25/25)**, after two rounds. **Both of the Foreman's premises for this
ticket were wrong**, and disproving them was most of the value.

**Wrong stack trace.** The ticket quoted `WebDriverError: element not interactable` as the symptom. That
line is at **01:39:01.382**; `organize.smoke.ts` does not start until **01:39:28.210**. It belongs to an
**earlier spec** (`batch-media`, which passed), where a `waitForClickable` poll absorbed it and it failed
nothing. The real failure was `expected a PNG group for the seeded CPE-1143-photo.png` — a `waitForExist`
timeout. Grepping a 14-spec interleaved shard log for error-shaped lines near a spec's name finds
"nearby", not "related".

**No denominator.** The ticket reasoned from one clean `main` run and one red PR run. The worker
enumerated **all 103** GUI-smoke runs in the window and downloaded **69** completed shard-4 logs,
fingerprinting each by what `npm ci` installed:

| install | wdio | jobs | failures | rate |
|---|---|---|---|---|
| 479 pkgs | 9.30.0 | 51 | 1 | 2.0% |
| 489 pkgs | 9.31.4 | 18 | 2 | 11.1% |
| **total** | | **69** | **3** | **4.3%** |

Fisher **p ≈ 0.16** — **not a version effect**; one failure predates the lockfile bump entirely, so it is
not a CPE-1960 casualty. And **`main` itself carried the failure at 00:14Z**, forty minutes before the run
the ticket called clean. *"This branch has something main doesn't"* is a **rate** claim, and a rate needs a
denominator.

**The diagnosis is better than either premise: a reflow between the driver computing a click point and
dispatching it.** `.preview` grows `120px → 45vh` when the first plan lands ~120 ms after mount, growing
the centred dialog ~195 px and sliding the rule pills **up ~98 px**, so the click lands inside `.preview`
— whose ancestor has `on:click|stopPropagation` and **swallows it silently**. Its Reviewer confirmed the
arithmetic **to the pixel** against the failure screenshot (dialog y≈113→585 against a predicted 473 px
tall at top 113.5) and found the smoking gun the PR had not quoted: `elementClick` issued at **28.956** →
`RESULT null` at **28.985**, with the debounce firing at ≈**28.960**. **The reflow lands inside the click
command's own 29 ms window.**

**Round 1's fix was inert, and its Reviewer red-proved that.** The settle-wait polled for
`summary | empty-state | error`, but `OrganizeDialog` initialises `loading = false` / `plan = []`, so
`empty-state` renders **synchronously at mount** — the wait returned on its first poll and gated nothing.
It shifted the gap distribution ~10 ms, moving previously-safe runs *into* the hazard band. Round 2 gates
on **`summary` alone**, which renders only in the `plan.length > 0` branch — a **deterministic state gate**,
not a stability heuristic — and rejected height-polling with reasons (a heuristic; two equal samples can
straddle a change; costs ≥100 ms every run).

**The new pin keeps the defect reproducible rather than folklore:** it reads the selector literal back out
of the spec, drives a real render through the debounce (0 matches at t=0, 0 at t=119 ms, >0 after
t=180 ms), and asserts the **round-1 selector does match at t=0**. Widening the const back reds 2 of 12 with
*"already matches at MOUNT … returns on its first poll and gates nothing."*

**Statistics stated at their strongest true form**, not their most dramatic: **zero failures outside a
113–119 ms band across 59 jobs; 3 of 13 in-band; Fisher p = 0.0048** — with the three in-band **passes**
explicit, so the argument rests on nothing failing outside the band rather than on exclusivity. Round 2
also corrected the Reviewer's own arithmetic (consecutive greens 68/71, not 67/70 — it had truncated one
and ceiling'd a rounded rate).

**An honest limitation, honestly placed:** jsdom has no layout, so the derivation test does **not**
reproduce the swallowed click, and at 4.3% no single CI run settles it (**68** consecutive greens for 95%
confidence). Said at the site, not only in the PR body.

**A known-failing audit fell out of it:** both `network.smoke.ts` entries named **CPE-1595**, which is
**Done** — its own Work Log says the root-cause fix stays open under **CPE-1507**. Repointed, reasons
preserved, 25 entries before and after.

**The app half is CPE-1968** — a rule-pill click within ~150 ms of the dialog opening is silently
swallowed — deferred deliberately, because every candidate fix is a visual-design decision across the
**28** components sharing the centred-backdrop rule.
