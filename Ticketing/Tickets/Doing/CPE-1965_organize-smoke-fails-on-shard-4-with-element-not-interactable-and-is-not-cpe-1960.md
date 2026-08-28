---
id: CPE-1965
title: `organize.smoke.ts` fails on gui-smoke shard 4 with `element not interactable` — a NEW case, and **not** CPE-1960's shape
type: bug
priority: High
status: In Progress
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
installed in `gui-smoke/`:

| install | wdio | shard-4 jobs | organize.smoke failures | rate |
|---|---|---|---|---|
| `added 479 packages` | 9.30.0 | 51 | **1** | 2.0% |
| `added 489 packages` | 9.31.4 | 18 | **2** | 11.1% |
| **total** | | **69** | **3** | **4.3%** |

The per-version split is **not** evidence of a version effect: 3 events over 51 vs 18 trials is
p ≈ 0.15 by Fisher's exact test, and the mechanism below is version-independent. Quote **4.3%**.

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

Measured across the 69 logs, `rule-picker` found → `elementClick` on `rule-by_extension` is 27–159 ms,
straddling the 120 ms debounce; the three failures sit at 115, 116 and 117 ms. The app *logic* is not
at fault: `OrganizeDialog.test.ts` already proves a rule click switches the rule and that `loadGen`
handles out-of-order plan resolution, and a new case re-proves it. The defect is **positional**.

That the app lets a click be swallowed at all is a genuine user-facing papercut, filed as **CPE-1968**
with the three candidate fixes. Deliberately not fixed here: every one of them is a visual-design
decision (28 components share the exact centred-backdrop rule), which does not belong in a PR whose job
is to unblock #1074.

### 3. The fix

`gui-smoke/specs/organize.smoke.ts`:

1. Wait for the **default rule's preview to settle** before clicking a pill — `summary` /
   `empty-state` / `error`, which are exactly the three settled states (the loading placeholder carries
   no testid), i.e. "the dialog has stopped resizing". Deliberately tolerant of all three so the wait
   cannot become a silent second assertion that a plan was produced.
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
  settles it either way** — roughly 65 consecutive green shard-4 jobs would be needed for 95%
  confidence the rate has dropped below 4.3%. The empirical half of the red-proof is the enumeration
  above plus the failure screenshot. The new in-spec assertion guarantees that if it *does* recur it
  fails at the click, naming the cause, instead of 10 s later pointing at the wrong thing.

### 5. Neighbour sweep — enumerated, not recalled (CPE-1932)

`git ls-files 'gui-smoke/specs/*.smoke.ts'` → **43 spec files, 139 `.click()` sites**; the 14 on shard 4
account for 51 of them. `context-menu.smoke.ts` and `home-item-menu.smoke.ts` have **zero** `.click()`
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
