---
id: CPE-1780
title: Three pre-existing listing gaps found while surfacing the filtered count
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-18
closed:
---

## Problem

Three separate pre-existing gaps surfaced by the PR #933 (CPE-1708) review while it was tracing where the
new hidden-entry count could go stale. None was introduced by that PR; all three were found by following
the same question — *where does a listing-scoped value stop being true?*

### 1. `revalidateDir` can fire while a non-listing view is showing

Because `loadPath` short-circuits HOME at `src/App.svelte:2162-2166` **before** `loadListing` runs,
`loadGen` is never bumped on that path. So a `revalidateDir` scheduled 300 ms earlier (from a cache hit on
the previous folder) can still fire while Home is on screen and **pass its `gen === loadGen` check**. It
would then re-assign `entries` for a view that is not showing a folder.

Invisible today only because Home renders `HomeView` rather than `FileList`. That is luck, not a guard.
This is the CPE-756 class: a generation token that does not cover every way you can leave a listing.

The right fix is probably to bump `loadGen` on every path that leaves a listing view — HOME,
`enterArchive`, smart-folder and structured-search entry — rather than to add another consumer-side gate.
CPE-1708 gated at the point of consumption for its own value, which was correct for one field but does not
generalise: the next listing-scoped value added will have the same problem.

### 2. Pane B's `filtered` count is never surfaced

The status bar is pane-A-scoped throughout — `itemCount` and `totalCount` both derive from pane A's
`visible`/`shown`. So a filtered folder opened in **pane B** reports nothing at all: no note, no count.

Consistent with the existing status-bar contract, so not a regression — but it means the guarantee
CPE-1708 established ("a listing is never quietly shorter than the folder really is") holds in one pane and
not the other, which is worse than a uniform rule either way. Decide: surface pane B's count too, or state
plainly in the docs that the note describes the active pane.

### 3. A local entry that cannot be read is dropped, and nothing counts it

`cpe_server::listing::stream_dir_entries` does `let Ok(entry) = entry else { continue }` and
`let Some(de) = dir_entry_from(&entry) else { continue }` — a `metadata()` failure silently drops the row
and **nothing counts it**. This is the documented `list_dir` skip-on-error guardrail in `CLAUDE.md`, and it
predates everything here.

Note the trap, because it is why this is its own ticket rather than a line in CPE-1708: folding these into
`filtered` would make the **message** a lie in the other direction. `filtered` means "the name could not be
shown safely"; an unreadable entry is a different fact and needs different words — something closer to
*"N entries could not be read"*. The local arm's `filtered: 0` is correct as scoped.

## What to do

- Fix (1) at the generation token, not at another consumer. Verify by scheduling a revalidate, navigating
  to Home before it fires, and asserting `entries` is untouched.
- Decide (2) and make it uniform. If pane B stays unreported, say so where a user would find it.
- For (3): count unreadable entries separately from name-refused ones, with its own wording, or record the
  decision not to. Do not merge the two counts.

## Acceptance criteria

- [x] A `revalidateDir` in flight when the user navigates to Home / into an archive / into a smart folder
      cannot mutate `entries`. Breaking the fix reds a distinct test.
- [x] Pane B's behaviour is either fixed or documented — not left implicit.
- [x] An unreadable local entry is either counted under its own name with its own wording, or the decision
      not to count it is recorded at the call site.
- [x] No count conflates "name could not be shown safely" with "could not be read". They are different
      facts and the user needs different words for them.

## Notes

Found by the Reviewer on **PR #933 / CPE-1708**, 2026-08-18, during the batched sprint; all three explicitly
scoped out of that PR. Related: CPE-1708, CPE-756 (the generation-token class), CPE-1704 (the S3 name
refusals being counted), and `CLAUDE.md`'s `list_dir` skip-on-error guardrail.

## Work Log

**2026-08-20** — All three gaps fixed:

1. **Generation-token gap.** `ExplorerPane.svelte` gained an exported `invalidateListing()` that bumps
   `loadGen` without starting a new load. `App.svelte` now calls it at every place that moves the pane's
   view away from a plain folder listing without routing through `loadListing`: `loadPath`'s HOME
   short-circuit, `navigateB`'s HOME short-circuit (pane B has the identical bug), `enterArchive`,
   `openSmartFolder`, and `openStructuredSearch`. A `revalidateDir`/stream scheduled before one of these
   fires can no longer pass its `gen === loadGen` check and reassign `entries` underneath the new view.
   Proven by `src/lib/components/ExplorerPane.invalidateListing.test.ts` (a positive case + a "sanity"
   case proving the race is real without the fix) — red-checked by disabling the `loadGen++` line.
2. **Pane B's filtered/unreadable count.** Decided, not fixed: pane B has no listing-metadata plumbing
   today (no archive/smart-folder/structured-search concepts either), so extending the single
   `<StatusBar>` to cover it is out of scope here. Documented at the `<StatusBar>` call site in
   `App.svelte` and in `docs/03-explorer.md`'s Dual-pane section: the status bar always describes the
   left pane, even when the right pane is active.
3. **Unreadable local entries now counted, separately from `filtered`.** `cpe_server::listing` gained
   `DirWalkStats { total, unreadable }` (with `fold_walk_entry` pulled out so the counting rule is
   deterministically unit-testable without racing a real OS metadata failure) and
   `list_dir_with_unreadable`. `ListDirResult`/`StreamDirResult` both gained an `unreadable: usize` field,
   always `0` for a remote listing. The frontend threads it through as `unreadableCount` (mirroring
   `filteredHidden`'s lifecycle exactly) and `StatusBar.svelte` renders a distinctly-worded note ("N
   entries could not be read", `--warn` toned) that can appear alongside the `filteredHidden` note without
   either conflating the other's count.

Gates: `npm run check` (0 errors), `npx vitest run` (320 files / 4224 tests, all green), `cargo clippy
--all-targets -- -D warnings` + `cargo test` in both `src-tauri` feature modes (default: 200 tests;
`--features sidecar-platform`: 255 tests), plus `cpe-server`/`cpe-net` clippy+test (2254 / 37 tests).
`bindings.gen.ts` regenerated (`ListDirResult`/`StreamDirResult` gained `unreadable`). PR: see the branch
`cpe-1780-listing-gaps`.

**2026-08-20 (follow-up)** — Foreman relayed a MERGE BLOCKER from the independent Visual Critic
(Playwright over the real `StatusBar.svelte` + `src/app.css`, 1200/880/800/684/600px, light/dark, five
prop scenarios): with `filteredHidden > 0` AND `unreadableCount > 0` at the same time, at 684px and
600px, the pre-existing `.disk` free-space label had no overflow strategy at all (unlike
`.filtered-hidden`/`.unreadable`/`.notice`, which this ticket's own new note correctly mirrored) — its
text wrapped onto a second line and spilled outside the status bar's fixed 26px box. Latent bug in old
code; this ticket's own acceptance scenario (both notes non-zero at once) was the first thing able to
trigger it, since `filteredHidden` and `unreadableCount` could never both be non-zero before this ticket.

Fixed inline, one CSS rule (`src/lib/components/StatusBar.svelte`'s `.disk`), matching `.notice`'s exact
treatment: `flex: 0 1 auto; min-width: 0; white-space: nowrap;` plus `overflow: hidden; text-overflow:
ellipsis;`. The app's window floor is 600×400 (`.min_inner_size`, `src-tauri/src/lib.rs`), so 600px is a
size the app explicitly permits. `.disk` is the last flex item before the (position:absolute) resize
grip, and every other sibling already carries the same nowrap/ellipsis/min-width:0 treatment, so shrinking
`.disk` further doesn't push overflow onto any neighbour.

Two a11y findings from the same Critic pass are explicitly follow-up, NOT fixed here (per the Foreman's
instruction) — filed for a later ticket: (1) neither the status bar nor either note carries
`role="status"`/`aria-live`, and the correct fix is a persistent always-mounted container whose text
changes, not a naive attribute add to the conditionally-mounted span; (2) at ≤684px both notes truncate
to an ellipsis with the full sentence reachable only via `title` (mouse-hover-only), pre-existing for
`.filtered-hidden` and now doubled by `.unreadable`.

Re-ran gates after the CSS fix: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 320 files /
4224 tests, all green (same counts as before; this is a CSS-only fix and jsdom does not apply component
CSS to `getComputedStyle` under this project's vitest config, so no test could pin the pixel-level bug or
its fix — visual correctness here rests on the Critic's real-browser measurement, not the harness).
Pushed to the same branch `cpe-1780-listing-gaps` / PR #974.

**2026-08-20 (follow-up, round 2)** — The Foreman relayed a second Visual Critic finding: the `.disk`
spill was genuinely fixed, but the deficit MOVED rather than disappeared — with both notes on at
600/684px, the LEADING unclassed item-count span ("42 items") now wrapped and spilled instead, because it
was the next unprotected child once `.disk` could shrink safely. Fixing one element at a time was moving
the same bug, not removing it.

Audited every direct child of `.statusbar` (the resize grip excluded — `position: absolute`, out of flex
flow) and assigned each a deliberate role, documented in a new ordering comment in
`src/lib/components/StatusBar.svelte`:
- **Stays whole, never truncates:** the (now classed) `.item-count`/`.selected-count` spans and `.dim`
  ("Hidden files shown") — `min-width: 0; white-space: nowrap;` only (no ellipsis), since these are short
  and load-bearing.
- **Allowed to truncate, in this order:** `.filtered-hidden`/`.unreadable`/`.notice` (unchanged from
  before), then `.git-branch` (a repo branch name can be long — `.git`'s counts/dirty-dot/buttons stay
  `flex: 0 0 auto`, fixed-size, since shrinking a clickable button is worse than truncating a name), then
  `.disk` last.

Re-ran gates: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 320 files / 4224 tests, all
green (same counts as both prior rounds — CSS-only, invisible to jsdom, exactly as expected). Pushed to
the same branch `cpe-1780-listing-gaps` / PR #974.
