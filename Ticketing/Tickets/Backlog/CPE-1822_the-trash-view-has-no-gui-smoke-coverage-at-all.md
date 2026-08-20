---
id: CPE-1822
title: the Trash view has no gui-smoke coverage at all, so three visual tickets shipped unphotographed
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
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

- [ ] A `gui-smoke/specs/trash.smoke.ts` exists and is auto-discovered by `lib/specFiles.ts` into the
      shard partition (no workflow edit needed — confirm that is still true).
- [ ] It opens the Trash view from the real toolbar/entry point, not by mounting the component.
- [ ] It `snap()`s at minimum: empty Trash, populated Trash, the degraded notice with entries present
      (CPE-1805's ordinary shape), and the mid-stream state CPE-1816 added — in **both** light and dark.
- [ ] It pins the sticky-header relationship the Visual Critic measured: with the list scrolled and a
      banner showing, the column header and its select-all checkbox must still be visible and hittable.
- [ ] It is **not** added to `gui-smoke/known-failing.json` — it must run on the blocking
      `GUI smoke (ubuntu-latest)` shards and the `gui-smoke-linux-verdict` ratchet.
- [ ] Seeding is honest: drive real trash state through whatever seam the existing specs use for this
      (see how `cost-ledger.smoke.ts` seeds through a real store seam rather than faking the render).
      Do not fabricate a render that the app cannot actually produce.
- [ ] The corresponding rows in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` flip to automated,
      naming the pinning job, and MVD is decremented.

## Notes

Filed by the Foreman from the CPE-1816 Visual Critic's finding. Related: CPE-1819 extracts the
`gui-smoke` command-palette helper; if the Trash view is reachable only through the palette, reuse
that helper rather than duplicating the block a fourth time.
