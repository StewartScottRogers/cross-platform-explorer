---
id: CPE-1822
title: the Trash view has no gui-smoke coverage at all, so three visual tickets shipped unphotographed
type: task
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-20
closed: 2026-08-27
---

## Problem

`gui-smoke/specs/` holds 41 specs and **not one of them opens the Trash view**. Verified 2026-08-20:
nothing in that directory references `TrashView`, `.tv-`, or the Trash toolbar entry.

Three Trash tickets landed today — CPE-1803 (caught-panic degraded notice), CPE-1804/1805
(per-item skip count + the notice placement rule), CPE-1816 (partial listing renders as complete) —
and **every one of them changed what the Trash view looks like with no screenshot taken.** The
Visual Critic on CPE-1816 had to render the component itself, headlessly, from the extracted
`<style>` block, because the harness that exists for exactly this could not photograph the surface.

## Why it matters

The Visual Critic is the gauntlet leg that replaces the user's routine eyes-on. It can only do that
where `gui-smoke` produces a screenshot. On an unphotographed surface the crew silently falls back
to reading CSS, which is how CPE-1816's three measured defects (a status box rendering as a button,
a 55px row jump on the common path, and a sticky banner completely covering the sticky column
header including its select-all checkbox) reached review instead of being caught at build time.

An unphotographed surface is a surface where the visual gate is not running, while the pipeline
reads as if it were.

## Acceptance criteria

- [x] A `gui-smoke/specs/trash.smoke.ts` exists and is auto-discovered by `lib/specFiles.ts` into the
      shard partition (no workflow edit needed — confirm that is still true).
- [x] It opens the Trash view from the real toolbar/entry point, not by mounting the component.
- [x] It `snap()`s at minimum: empty Trash, populated Trash, the degraded notice with entries present
      (CPE-1805's ordinary shape), and the mid-stream state CPE-1816 added — in **both** light and dark.
- [x] It pins the sticky-header relationship the Visual Critic measured: with the list scrolled and a
      banner showing, the column header and its select-all checkbox must still be visible and hittable.
- [x] It is **not** added to `gui-smoke/known-failing.json` — it must run on the blocking
      `GUI smoke (ubuntu-latest)` shards and the `gui-smoke-linux-verdict` ratchet.
- [x] Seeding is honest: drive real trash state through whatever seam the existing specs use for this
      (see how `cost-ledger.smoke.ts` seeds through a real store seam rather than faking the render).
      Do not fabricate a render that the app cannot actually produce.
- [x] The corresponding rows in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` flip to automated,
      naming the pinning job, and MVD is decremented.

## Notes

Filed by the Foreman from the CPE-1816 Visual Critic's finding. Related: CPE-1819 extracts the
`gui-smoke` command-palette helper; if the Trash view is reachable only through the palette, reuse
that helper rather than duplicating the block a fourth time.

## Work Log

- 2026-08-27 — Built `gui-smoke/specs/trash.smoke.ts`, five `it()`s, all gated `IS_LINUX` (see the
  spec's own header comment "SCOPE" for the reasoning): `windows-latest` is `continue-on-error`/
  non-blocking in `gui-smoke.yml` (WebView2 crash, unrelated to this app), `ubuntu-latest` (sharded) is
  the actual blocking gate, and the Windows Recycle Bin cannot be hand-constructed the way the
  freedesktop.org Trash directory can — the same reasoning `src-tauri/src/lib.rs`'s own CPE-1791
  panic-boundary test already uses to be `#[cfg(target_os = "linux")]`-only.
- **Seeding technique**: every state is reached by writing real `<trashDir>/info/<name>.trashinfo` +
  `<trashDir>/files/<name>` pairs directly onto disk — the exact seam `trash::os_limited::list()` reads
  in production, and the same technique `src-tauri/src/lib.rs`'s CPE-1791 (malformed `.trashinfo` body
  line, panics `list()` internally) and CPE-1804 (`item_with_undecodable` — a raw non-UTF-8 byte as a
  filename) tests already use. Nothing goes through the app's own delete UI or a foreign OS trash tool.
  This is real state the app genuinely computes from, not a mocked render — matching the ticket's own
  "seeding is honest" AC and the `cost-ledger.smoke.ts` precedent it points at.
- **States covered** (5 `it()`s, all snapped light+dark except the sticky-header hit-test which is a
  single-theme layout check): genuinely empty Trash; a populated Trash (3 real rows); CPE-1803's
  degraded-with-no-entries note (own distinct wording, not `trash.empty` — via the same malformed
  `.trashinfo` construction CPE-1791's own rust test uses); CPE-1805's degraded-WITH-entries banner (via
  one undecodable-name entry + 30 decodable siblings, CPE-1804's per-item-skip route) plus a real-layout
  sticky-header + Select-all-checkbox hit-test after scrolling; and CPE-1816's mid-stream "Still
  loading…" state on a real, large (2,500-item) streaming pass. The degraded-with-no-entries state is
  *not* in this ticket's own AC (only "degraded with entries present" is) but *is* named by the
  `MANUAL-TEST-BURNDOWN.md` row this ticket retires, and the fixture was cheap once the sibling test
  existed, so it was added too.
- **Why 2,500 items for mid-stream**: `list_trash_stream`'s whole body runs synchronously inside one
  `spawn_blocking` closure with no `.await` between channel batches, so the only thing that can make
  "first batch rendered, summary not yet resolved" observable from outside the process is real
  wall-clock cost — per-item OS `metadata()` lookups past the first 256-item batch, plus (the spec's
  real lever) unvirtualized DOM insertion of every `.tv-row` (TrashView.svelte's own doc comment: "No
  virtualized DOM windowing here"). Reasoned, not empirically timed against real Linux CI — see
  "Verification" below for what could and couldn't be checked from this environment.
- Kept the mid-stream detection keyed on the RENDERED TEXT (`.tv-count` containing "Still loading"),
  not the `.tv-count-loading` class alone — a red-proof probe (see below) found the class can be renamed
  without redding `TrashView.test.ts`'s own suite, so the class alone isn't the whole load-bearing
  contract; the visible string is, and it's what the Visual Critic actually judges in the screenshot.
- **CPE-1819** (separate, open ticket — the copy-pasted `gui-smoke` command-palette-open block):
  doesn't apply here. Trash is reached via the Sidebar's own "Open Trash" row (same entry point
  `trash-titlebar.smoke.ts` already uses), never the command palette, so this spec is not a candidate
  for that extraction and doesn't add a fourth copy of the block.
- `wdio.conf.ts` was **not** touched — every fixture is seeded inline, per-`it()`, directly against the
  real OS trash directory (with cleanup in a `finally`, plus an `after()` safety net), rather than in
  `onPrepare`, because the four/five states need to be reached in sequence against one already-running
  app process, not as one static pre-launch snapshot the way every other spec's fixture is.
- **Verification.** This environment (Windows sandbox, no Linux runner, no time budget for a full
  `tauri build` release binary + `tauri-driver`/`msedgedriver` local run) could not execute
  `gui-smoke/specs/trash.smoke.ts` itself. What WAS verified directly:
  - `gui-smoke`'s own `npm run typecheck` (`tsc --noEmit`) — clean.
  - `gui-smoke`'s `lib/specFiles.ts#listSpecFiles` run directly against the real `specs/` directory
    confirms auto-discovery with zero workflow changes: 41→43 specs, `trash.smoke.ts` present, sorted
    after `trash-titlebar.smoke.ts` as designed (`-` < `.`).
  - `gui-smoke`'s own `lib/*.test.ts` unit suite (130 tests, incl. `shard.test.ts`/`specFiles`-adjacent
    coverage) — all passing, unaffected.
  - Root `npm run check` (svelte-check) — 0 errors, 0 warnings.
  - **Red-proof, as a proxy**: since the real E2E harness couldn't run locally, the markup contracts my
    new spec depends on were red-proofed against the EXISTING real-browser-adjacent jsdom suite,
    `src/lib/components/TrashView.test.ts` (28 tests) — committed the real work first, then, one at a
    time: renamed `.tv-degraded-banner` → RED (1 test), restored → green; renamed `.tv-sticky-stack` →
    RED (1 test), restored → green; changed the mid-stream render condition
    (`!complete && entries.length > 0` → `false && …`) → RED (4 tests), restored → green; changed the
    degraded-empty branch condition (`(degraded || !complete) && entries.length === 0` → `false && …`)
    → RED (6 tests), restored → green. Each restore confirmed byte-identical to `HEAD` via `md5sum`
    before moving on. This proves the classes/conditions the gui-smoke spec keys off are real,
    actively-guarded contracts — it is NOT the same as running `trash.smoke.ts` itself against a real
    build, which is the genuine gap: CI's `ubuntu-latest` shard run is the first live confirmation this
    spec actually passes, and in particular that the mid-stream race is actually observable in that
    environment. If it isn't, that's a follow-up (`known-failing.json` `"intermittent": true` entry
    citing runs, per that file's own convention), not something to quietly loosen here.
  - `gui-smoke` deps were not pre-installed in this worktree (`npm ci` run to typecheck) — left installed
    in `gui-smoke/node_modules/` (gitignored), not a machine-global change.
- Updated `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`: flipped the **CPE-1560** row and the
  **CPE-1803 / CPE-1804 / CPE-1805** row to ✅ automated, named the pinning job (`GUI smoke
  (ubuntu-latest)` shards, ratcheted by `gui-smoke-linux-verdict`), added inline retirement notes for
  both, and appended a dated reconciliation section: the running MVD total hadn't been tallied since
  2026-08-20 (two later additions, CPE-1821 and CPE-1833/1836, were logged as new rows but never
  folded into a `supplementary N→N+1` delta line) — stated that gap plainly, computed the corrected
  pre-flip total (supplementary 12, total 18), then applied this shift's decrement (supplementary
  12→10, total 18→16). Primary ledger unchanged at 6.

## Round 2 (independent Reviewer: CHANGES REQUESTED; UAT: PASS)

Two blockers, two silent-pass holes, one failure-cascade fix, several factually-wrong comments, and five
should-fixes. All addressed in `gui-smoke/specs/trash.smoke.ts` and
`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`; no change to `TrashView.svelte` this round.

**BLOCKER 1 (`this.timeout()` inside an `it()` body is not honoured)** — the mid-stream test's
`this.timeout(180_000)` call was the exact violation `wdio.conf.ts`'s CPE-1702 comment forbids (cited CI
evidence: CPE-1679's stress harness died at ~90s on all 3 real runs despite calling
`this.timeout(2_060_000)`). Fixed both parts: split the single mid-stream `it()` into one `it()` per
theme (light, dark) inside a nested `describe`, per `wdio.conf.ts:1367`'s own instruction for exactly
this case; moved the shared, expensive fixture setup (2,500 real files) into that describe's `before()`
hook and teardown into `after()`, where `this.timeout()` reliably widens THAT hook's own budget (unlike
inside an `it()` body — matches `preview-pane.smoke.ts:179`'s own documented distinction). Also replaced
the hot poll's `getText()` + `$$(".tv-row").length` (which serializes up to 2,500 WebDriver element
handles per tick under classic WebDriver's `findElements`) with one `browser.execute()` call computing
`document.querySelectorAll(...).length` entirely in-page — one round trip instead of thousands of
handles. Also dropped the "wait for the pass to fully resolve" step (60s alone) — unneeded, since
`TrashView.svelte`'s own `loadGen` supersession already drops a still-in-flight stream's later batches
once the next test's `openTrash()` starts a fresh load.

**BLOCKER 2 (burndown table broken)** — the CPE-1803/1804/1805 retirement note had been inserted
*between* two table rows, turning the following `| CPE-1708 / CPE-1775 ... |` row into a lazy Markdown
paragraph continuation (literal pipe text, not a table row) — silently deleting the still-open StatusBar
row from the rendered ledger. Moved the note below the whole table (matching the existing `→ ✅ CPE-1586
RETIRED` precedent). **Round 2 self-check was wrong and reached the wrong way**: it checked that the
paragraph text no longer sat *between* the two rows in the source, rather than actually RENDERING the
file — and left a blank line at the new boundary, which GFM treats as ending the table body exactly the
same way the paragraph text had, so the row was still silently lost. Round 3's independent Reviewer
caught it by rendering through `marked --gfm` and counting `<tr>`s. Re-verified for round 3 the same way
this time (see "Verification, round 3" below) — checking that a paragraph is textually gone is not the
same claim as checking the table still renders, and only the second one is the actual acceptance
criterion for a ledger row that must not silently stop rendering.

**Silent-pass holes, both fixed and red-proofed** (see "Verification, round 2" below):
- The sticky-header test's scroll step (`if (body) body.scrollTop = body.scrollHeight`) was a silent
  no-op if `.tv-body` were renamed or the list wasn't actually taller than its container — the sticky
  assertion would then pass on an unscrolled list. Added `expect(scrollTop).to.be.greaterThan(0)`
  immediately after the scroll.
- The degraded-empty test's `noteText.to.not.include("Trash is empty")` also silently passes on the
  transient mid-stream render of the SAME element (`.tv-degraded-note` shows `trash.stillLoading` before
  `degraded` resolves — `noticeMessage` in TrashView.svelte). Changed to positively wait for and assert
  the actual CPE-1803 wording ("...couldn't be fully read...") before reading the text at all.

**Failure cascade** — `closeTrash()` was inside each `try`, never a `finally` or `afterEach`. A mid-test
failure left `.tv-overlay` (z-index 60) over the Sidebar, so the NEXT `it()`'s `openTrash()` click would
be intercepted — the exact `WebDriverError: element not interactable` signature from a live PR #1038
shard-4 run (a different spec, same failure shape). Moved `closeTrash()` (already defensive) into a
shared `afterEach`, which now also resets theme to light (`preview-pane.smoke.ts`'s own `afterEach` is
named in `resetAppState.ts` as "the model to copy, not a coincidence" for exactly this — this harness's
state reset runs once per spec FILE, not per test).

**Comment corrections (code kept, prose fixed)**:
- The undecodable-entry fixture's header/comments claimed it matches `item_with_undecodable(Some("name"))`.
  It doesn't: the `trash` crate's freedesktop backend derives `TrashItem.name` from the decoded `Path=`
  value's basename, not from the `.trashinfo` filename — the skip actually fires on `id`. More
  importantly, `item_with_undecodable` fabricates a struct in memory and never writes a non-UTF-8
  `.trashinfo` file or runs `list()` over one; this spec is the first thing in this repo that does, over
  the real dependency — reworded to say that instead of claiming equivalence with an existing test.
- A comment claimed sort order keeps `trash-titlebar.smoke.ts` and `trash.smoke.ts` from ever colliding
  "regardless of shard packing" — wrong; they land on different SHARDS (different runners, different
  Trash directories), which is why they can't collide, not sort order. Reworded; the cleanup discipline
  itself doesn't depend on this either way.
- Burndown: "the header line ... is itself stale relative to even the 2026-08-20 entry" was wrong — it
  matched 2026-08-20 exactly and only drifted on 2026-08-23. Fixed the claim.
- Burndown: "re-balanced automatically as the ratchet measures this spec's real cost" was the opposite of
  what `lib/shard.ts` says — the cost table (`specWeightMs`) is hand-maintained and the file's own header
  says "NOTHING CATCHES" a stale entry or an uncosted new spec. Fixed the claim; noted `trash.smoke.ts`
  is uncosted today and a follow-up should add its measured runtime once a live CI run reports it.
- "3 classes of breakage" → "four" (degraded-banner, sticky-stack, mid-stream condition, degraded-empty
  condition), matching the Work Log above.
- Independently confirmed correct and left alone: the burndown arithmetic itself (16 + 2 = 18 pre-flip,
  16 post-flip; 6 primary + 10 supplementary).

**Should-fixes**:
- `wipeTrashDir`'s only call sites now also gate on `process.env.CI` (not just `IS_LINUX`) — wiping a
  Linux contributor's own real Trash during a local run would be destructive; the rust equivalent
  (`lock_real_trash`) redirects `XDG_DATA_HOME` for the same reason, which this spec can't do to an
  already-launched app process.
- `fs.rmSync` in `wipeTrashDir` now passes `recursive: true` — without it, a trashed DIRECTORY throws
  `ERR_FS_EISDIR`, silently swallowed by the `catch`, leaving the entry behind and failing test 1.
- `before()`'s `IS_LINUX` guard now runs BEFORE reading/parsing `STATE_FILE`.
- `trash-degraded-scrolled` (dark-only capture, despite the old it() title implying both themes) renamed
  to `trash-degraded-scrolled-dark` and the it() title now says so explicitly, rather than adding a
  second full scroll+hit-test pass in light for one extra screenshot.

**Verification, round 2.** Same constraint as round 1 — this environment could not run the real
`gui-smoke`/WebDriver harness. What was checked:
- `gui-smoke && npm run typecheck` — clean.
- `gui-smoke && npx tsx --test lib/*.test.ts` — 130/130, unaffected.
- Root `npm run check` — 0 errors, 0 warnings.
- **Red-proofed the two silent-pass fixes specifically**, as instructed, via a throwaway vitest probe
  (`src/lib/components/CPE1822RedProof.probe.test.ts`, written, run, then deleted — not part of the
  permanent suite) reusing TrashView's real render + the SAME mid-stream-drain-to-zero setup an existing
  permanent test (`TrashView.test.ts`, "does not claim 'Trash is empty' when Restore drains...") already
  exercises:
  - Confirmed the real rendered text in that exact state is `"Still loading…"` — proving the OLD
    assertion shape (`.to.not.include("Trash is empty")`) would have silently PASSED there (a true
    silent-pass hole), while the NEW assertion (`.to.include("couldn't be fully read")`) correctly
    REJECTS it, and correctly ACCEPTS the real resolved degraded wording once `finishStream(0, true, 0)`
    lands.
  - Confirmed jsdom's real `.tv-body` (rendered from 30 real entries) reports `scrollHeight: 0` and
    `scrollTop` stays `0` after the exact assignment the gui-smoke spec runs — demonstrating the fixed
    `expect(scrollTop).to.be.greaterThan(0)` is NOT vacuous even here (jsdom cannot lay out, which is
    exactly why this check has to live in gui-smoke rather than the jsdom suite) — and confirmed a
    renamed selector genuinely returns `null`.
  All three probe assertions passed as predicted; full console output captured before the probe file was
  deleted.

## Round 3 (Reviewer: one blocker, relocated defect; attempt 3 of 3)

**BLOCKER — same defect, relocated.** Round 2 moved the retirement paragraph below the table but left a
BLANK LINE between the two table rows (`.../MANUAL-TEST-BURNDOWN.md:477`). A blank line ends a GFM table
body exactly like a paragraph does — so `| CPE-1708 / CPE-1775 ... |` was still rendering as a literal
`<p>| CPE-1708...` orphan, not a `<tr>`, and the still-open StatusBar row was still silently missing from
the rendered ledger. **Fix: deleted the blank line at :477. That was the whole change.**

**Verification, round 3 — by rendering, not by reading.** Installed nothing new to the project; ran the
real `marked` package transiently via `npx --yes marked --gfm` (no `package.json`/lockfile change) piped
the whole burndown file through it, and grepped the resulting HTML:
- The CPE-1803/1804/1805 `<table>...</table>` block (was: table cut short after 1 body row) now contains
  BOTH `<td>CPE-1803 / CPE-1804 / CPE-1805</td>` and `<td>CPE-1708 / CPE-1775 (+ CPE-1660, CPE-1798)</td>`
  inside the same `<table>`, with the `→ ✅ ... RETIRED` note rendering as a `<p>` immediately AFTER
  `</tbody></table>`, not inside it.
- Whole-file orphan scan (`grep -c '<p>|'`): **4**, matching the reviewer's cited `origin/main` baseline
  — not 5. The other 4 orphans are pre-existing, unrelated rows elsewhere in the document (tray/CPE-1090/
  CPE-1114/CPE-1586), confirmed by line number, not something this round touched.
- Total `<tr>` count: **26** (was 25 with the bug), matching the reviewer's own count exactly.

Corrected the round-2 Work Log entry above in place: it had claimed the fix was "confirmed" by checking
the paragraph text no longer sat between the two rows in the SOURCE — which is not the same claim as
checking the table still RENDERS, and missed the blank-line variant of the identical defect. That is the
more useful lesson than the one-line fix itself, per the Reviewer's own framing, so it's recorded here
rather than silently corrected.

**Real new risk, fixed (non-blocking per the Reviewer, done anyway).** The shared `afterEach`'s
`closeTrash()` (added round 2) could run while the 2,500-item mid-stream test's stream was still
processing several remaining batches plus unvirtualized DOM insertion for up to 2,500 rows — its old 5s
reverse-wait could time out there even though the view would close given a moment longer, and the
`afterEach`'s own `try { } catch { /* best-effort */ }` would then silently swallow that, leaving
`.tv-overlay` up for the next test's `openTrash()` click — the exact cascade round 2's `afterEach` change
existed to prevent, reintroduced at its busiest boundary. Fixed two ways: raised `closeTrash()`'s
internal reverse-wait from 5s to 15s (see its own updated doc comment), and changed the `afterEach`'s
catch from a silent swallow to a `console.error` naming the failure — never a throw (an `afterEach`
throw would skip every remaining test in the file, strictly worse than one more test seeing a stuck
overlay), but no longer invisible in the CI log either.

**Four nits, all one-liners, all applied:**
- `await setTheme("light")` in `afterEach` is now wrapped in its own `try`/`catch`, matching the other
  two calls in that hook.
- `wipeTrashDir`'s doc comment said "the ONLY call site" — there are two (`before()` and `after()`),
  both correctly CI-gated; corrected the sentence.
- The shard-separation comment said the partition "can change run to run" — corrected: `lib/shard.ts`
  makes determinism its own headline property; the assignment only changes when the spec set or the
  hand-maintained cost table changes, never between otherwise-identical runs.
- Added a comment to test 1 ("a genuinely empty Trash...") stating plainly that outside CI on Linux
  (where the wipe is deliberately skipped — round 2's should-fix), this test will fail loudly for a
  developer whose own real Trash isn't already empty, and that this is the intended trade-off (fail
  loudly rather than wipe a contributor's real Trash, or pass an assertion the fixture never earned),
  not a bug in the test.

**What the Reviewer confirmed already fixed in round 2 and did not ask re-checked:** fixture I/O
genuinely in `before()`/`after()` with `function` (not arrow) callbacks for correct `this`; the
2,500-handle serialization off the hot path; the 60s resolve-wait removed from the `it()`; measured
per-theme cost ≈1.3–7s typical against a ≈43s worst case on the 90s budget (≈2× headroom, no longer near
the cliff); removing `this.timeout(60_000)` from the CPE-1805 test was a strict improvement (it had been
LOWERING that test's budget, not raising it); both silent-pass fixes verified closed (the degraded-empty
needle matches only the real CPE-1803 wording and correctly rejects `stillLoading`/`empty`/`skippedOne`;
the scroll check returns `-1` on a missing `.tv-body` and the real clamped value otherwise); `afterEach`
runs on failure and is failure-tolerant; all four round-2 comment corrections are accurate.

**Verification, round 3 (full).** `gui-smoke && npm run typecheck` — clean. Markdown rendering per above.
No change to `TrashView.svelte`, `wdio.conf.ts`, or any dependency this round — 2 files touched
(`gui-smoke/specs/trash.smoke.ts`, `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`), plus this ticket.
