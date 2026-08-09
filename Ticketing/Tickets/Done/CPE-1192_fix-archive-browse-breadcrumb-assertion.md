---
id: CPE-1192
title: "Fix archive-browse gui-smoke spec: breadcrumb assertion misses the in-archive crumb"
type: bug
component: Testing
priority: low
status: Done
tags: ready
created: 2026-07-31
closed: 2026-08-01
epic: CPE-705
estimate: 30m
---

## Summary
`gui-smoke/specs/archive-browse.smoke.ts` (CPE-1181) fails on the live build even though the FEATURE works:
its first live run captured `archive-browse-targz-fail.png` which clearly shows the `.tar.gz` **was** entered
(inner file `CPE-1181-note.txt` listed, breadcrumb visually ending on the archive). The assertion queried
`button.crumb` and got only the 8 filesystem crumbs — the **in-archive breadcrumb segment isn't a
`button.crumb`** (the `archiveCrumbs` render uses a different element/class), so the assertion wrongly fails.
gui-smoke is non-blocking, so this doesn't red main, but the pin is broken and doesn't actually guard the
feature.

## Build
- Inspect how the in-archive breadcrumb (`archiveCrumbs`, `src/App.svelte`) renders vs the filesystem
  breadcrumb, and update the spec's assertion to match the actual archive-crumb element (or assert on the
  entered-state a different robust way — e.g. the inner-entry row rendering, which already works). Keep it
  asserting real "we entered the archive" state.

## Acceptance Criteria
- [x] The archive-browse spec passes on a live gui-smoke run (or is re-scoped to a robust assertion of the
      entered state); `cd gui-smoke && npm run typecheck` green.

## Work Log
- 2026-07-31 — Filed by Foreman (sprint) after the epic-705 Visual-Critic capture: the browse feature works
  (screenshot-confirmed) but the spec's breadcrumb selector is wrong.
- 2026-08-01 — Root-caused: `NavToolbar.svelte` renders every crumb except the last as
  `<button class="crumb">`, but the LAST (active) crumb — the archive name once inside it — as a
  non-button `<span class="crumb current" aria-current="page">`. `archive-browse.smoke.ts`'s assertion
  #2 queried only `button.crumb`, which structurally excludes the trailing crumb no matter what, so it
  could never pass even though the feature works. Fixed by querying `.crumb` (covers both the button and
  span variants; `.crumb-sep` is a distinct class so it isn't swept in), keeping both checks — the
  archive name appears in the trail AND is the last entry — genuinely verifying the in-archive state.
  `gui-smoke && npm run typecheck` clean; `npm run test:unit` 21/21 pass. Full live wdio run
  (`npm test`) needs a release build + msedgedriver + display, not available in this headless
  worktree — verified by reading the real `NavToolbar.svelte` markup instead. PR opened.

## Resolution
Changed `gui-smoke/specs/archive-browse.smoke.ts`'s assertion #2 from `$$("button.crumb")` to
`$$(".crumb")`. `NavToolbar.svelte` (`{#each crumbs as crumb, i}`) renders every crumb but the last as
`<button class="crumb">`, and the last (active, `aria-current="page"`) crumb — which is exactly the
in-archive crumb this test needs, since `archiveCrumbs()` in `App.svelte` always appends the archive
name as the final crumb — as `<span class="crumb current" aria-current="page">`, not a button. The old
selector structurally could never see that span, so it always failed regardless of whether the feature
worked (confirmed by the ticket's own `archive-browse-targz-fail.png` showing the archive genuinely
entered). `.crumb` matches both element kinds (`.crumb-sep` is a distinct class, not swept in), so it
now sees the real full crumb trail; the assertion still requires the archive name to be present AND to
be the trailing entry, so it's a like-for-like fix, not a weakened no-op. Only file touched:
`gui-smoke/specs/archive-browse.smoke.ts`. Verified with `gui-smoke && npm run typecheck` (clean) and
`npm run test:unit` (21/21 pass); the full live `npm test` wdio run needs a release build +
msedgedriver + display not available here, so it was validated by cross-checking against the real
`NavToolbar.svelte` markup instead of executed.
