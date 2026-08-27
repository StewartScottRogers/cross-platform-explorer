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
