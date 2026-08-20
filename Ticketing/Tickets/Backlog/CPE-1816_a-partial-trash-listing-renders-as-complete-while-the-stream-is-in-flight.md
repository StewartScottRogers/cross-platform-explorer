---
id: CPE-1816
title: a partial trash listing renders as complete while the stream is still in flight
type: bug
priority: Low
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

While a trash listing is **still streaming**, rows paint as batches arrive and the title bar asserts
"Trash 1 item", "Trash 2 items" — **with no incompleteness notice**. The notice and the count-suppression
appear only when the stream's summary resolves.

So for the duration of the stream, a partial list renders exactly as a complete one would.

## Why it is not simply a bug in CPE-1803/CPE-1804

It is **inherent to the streaming design**: the `degraded` / `skipped` flags ride on the *summary*, which by
construction arrives last. Entries stream over the channel; the verdict does not exist until the walk ends.
Unchanged since CPE-1560 and untouched by CPE-1803 and CPE-1804 — both of which fixed the *completed* case
correctly.

The window closes on completion, so this is materially milder than the bugs those tickets fixed: the user
is briefly under-informed rather than durably misinformed. On a small trash it is imperceptible.

## Why it is still worth fixing

The harm scales with the thing that makes streaming worth having. A large or slow trash — a network mount,
a spinning disk, thousands of entries — is exactly when the window is widest **and** when the user is most
likely to read a partial list and act on it. "Your file isn't in the trash" is a conclusion someone can
reach in two seconds from a list that is still filling.

## What to do

- The honest shape is probably to **let the stream carry incompleteness as it happens** rather than only in
  the summary — the walker knows it skipped an entry at the moment it skips it. Weigh sending it with the
  batch against the simplicity the current design buys, and **say why** either way.
- Failing that, consider suppressing the *count* until the summary resolves, so the app at least never
  asserts a total it cannot yet back. That is strictly weaker but much cheaper.
- Whatever the shape, keep the property CPE-1804 established: the notice must be driven by the flag, never
  inferred from `entries.length`.
- Follow the streaming standard in [docs/design/STREAMING.md](../../../docs/design/STREAMING.md), including
  supersession by generation token — a fix here must not resurrect a stale banner over a fresh listing.

## Evidence

Per the Evidence Rules in `Ticketing/wiki.md`. The test must observe the **mid-stream** state, not just the
resolved one — a test that only asserts the final render passes today and would pass with this unfixed.

## Notes

Filed by the Foreman from PR #962's UAT, 2026-08-20. The UAT observed the timeline directly rather than
inferring it, and correctly classified it as pre-existing rather than a regression in the PR it was testing.

Related: **CPE-1803**, **CPE-1804**, **CPE-1805**, **CPE-1560**.

## Work Log

**2026-08-20** — Implemented the "failing that" fallback the ticket sanctioned: suppress the count
rather than carry `skipped` on every batch. Reasoning for taking the cheaper route: `degraded`/`skipped`
are folded from a caught-panic route (which by definition has no per-item information to send early)
and a per-item skip route — a shared per-batch signal would need to model "maybe-degraded-so-far" as a
third, meaningfully different state from `degraded`, adding a new wire shape (batch envelope change,
`specta::Type` touch, bindings regen, all 5 command call sites in `stream_trash_entries`) to close a
window that is inherently transient and self-corrects the moment the summary lands — CPE-1804/CPE-1805
already made the *resolved* state fully honest, which is the state that persists.

Reused the exact mechanism CPE-1805 built (the banner above the rows) rather than inventing a second
one: added one frontend-only `complete` boolean (true once the stream's summary resolves or the invoke
throws), and drive both the title-bar item-count suppression and the banner from `degraded || !complete`
— `degraded` wins once it's known (it's never true before `complete` is), otherwise a new
`trash.stillLoading` string ("Still loading…") reuses the same box and position CPE-1805 introduced.
No backend change; no `specta::Type` touched; no bindings regen needed.

Added `trash.stillLoading` to all 12 complete locales in `src/lib/i18n.ts` (en/es/de/fr/it/pt/nl/pl/
ru/zh/ja/ko), following the `trash.skippedOne`/`skippedMany` precedent from CPE-1804. Updated
`src/docs/38-trash.md` to describe the mid-stream state. Updated two stale-line guard tests whose
recorded line numbers shifted (`bidiEscape.guard.test.ts`'s `TrashView.svelte` REGISTRY entry,
`mojibakeGuard.test.ts`'s i18n.ts `ALLOWLIST` entry for the Portuguese "NÃO" line) — both re-anchored
on content, not arithmetic, same as prior tickets' notes on those two guards.

New test `does not assert a finished item count while the stream is still in flight, and says so`
observes the mid-stream state directly (delivers a batch, does NOT resolve the summary) per the
ticket's Evidence Rules — a suite asserting only the resolved state would pass today and would still
pass with the bug intact.

Red-proofs, each applied, observed red, then reverted:
- Removed `&& complete` from the title-bar count's `{#if}` (`TrashView.svelte`, was
  `!loading && !error && !degraded && complete`) → the new test reds on
  `expect(screen.queryByText("1 item")).toBeNull()` (found a live `<span>1 item</span>` instead).
- Changed the rows-banner `{#if degraded || !complete}` to `{#if degraded}` → the same test reds on
  `expect(screen.getByText("Still loading…")).toBeTruthy()` (element not found).

Gates: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 4183 passed (4183), 0 failed (was
4182 on main; +1 from the new CPE-1816 test). No `.rs` files touched (frontend-only fix), so no cargo
gates or bindings regen apply this round.

Files changed: `src/lib/components/TrashView.svelte`, `src/lib/components/TrashView.test.ts`,
`src/lib/i18n.ts`, `src/lib/bidiEscape.guard.test.ts`, `src/lib/mojibakeGuard.test.ts`,
`src/docs/38-trash.md`.

**2026-08-20, round 2** — Independent Reviewer returned CHANGES REQUESTED (two mutation-proven
blockers) and a Visual Critic review (rendered + measured the states, four objective defects) landed
on the same PR. Addressed both in the same worktree/branch (attempt 2 of 3).

REVIEWER BLOCKER 1 — `complete` reset on Refresh was correct but untested, and the mutant (deleting
the `complete = false;` reset line in `load()`) left all 21 existing tests green: a Refresh's new,
still-in-flight stream inherited the PREVIOUS pass's resolved `true`, reproducing CPE-1816 verbatim on
the second load. Added the twin of the CPE-1804 "clears a previous pass's skipped count on refresh"
test, using the Reviewer's own probe shape (open → deliver → finish → Refresh → deliver again on the
new stream without finishing it → assert no stale count and the caveat present). Also corrected the
catch-branch `complete = true` comment, which had implied it was a covered/observable path — reworded
to say plainly it's defensive-only and not currently test-observable (`error` already gates every
branch that reads `complete`).

REVIEWER BLOCKER 2 — "Trash is empty" was reachable mid-stream: the empty-pane branch was
`{:else if degraded && entries.length === 0}`, so draining `entries` to zero via Restore or Empty
while the stream was still in flight (`!complete`, more rows could still arrive) fell through to the
plain "Trash is empty" branch — exactly the claim CPE-1803 exists to forbid, demonstrated live by the
Reviewer via Restore. Fixed by widening the condition to `(degraded || !complete) && entries.length
=== 0`, reusing the same message. Safe for a genuinely healthy empty Trash: `complete` and `loading`
resolve in the same `finally` tick, so that case never observes `!complete` here — `loading` is still
true and the branch above (the loading placeholder) renders first. Also surfaced and fixed a latent
defect this exposed: the Restore/Empty test suites' shared `renderWithTwoEntries` helpers never called
`finishStream`, so EVERY existing Restore/Empty test was already running against an unresolved stream
by accident — after this fix, the ordinary "purge everything" test started asserting "Trash is empty"
against a pass that was still (accidentally) incomplete and got "Still loading…" instead. Fixed the
helpers to resolve a clean pass before returning (Restore/Empty are ordinarily clicked against a
finished listing), then added two NEW, deliberate tests — one via Restore, one via Empty Trash — that
render, deliver a batch, and explicitly do NOT call `finishStream` before draining the list, to prove
the fix without relying on that accident.

VISUAL CRITIC — rendered and measured (getBoundingClientRect/getComputedStyle), not eyeballed. Design
decision (Foreman): move the mid-stream caveat OUT of a rows-above-the-list banner and INTO the title
bar's item-count slot (dim italic text, in place of the count) — that slot is already reserved and
empty in exactly this state, so swapping its text costs no layout. This one move closed three
measured defects at once:
- **Finding 1** (false affordance): the two-word "Still loading…" box collapsed to near-identical
  dimensions/colours as the adjacent `.tv-btn` "Select all" button (101×34 vs 73×28, matching border/
  radius/text colour) — read as clickable. Gone once the caveat isn't a bordered box at all.
- **Finding 2** (55px jump on the common path): the banner's appear-then-vanish moved every row up
  ~1.6 row-heights (34px rows) the instant a CLEAN stream resolved — rows are click targets with
  checkboxes, so the row under the pointer changed mid-reach. Title-bar text swap moves nothing below
  it.
- **Finding 4** (one-frame flash): a typical trash flushes in one batch (`TRASH_LIST_BATCH = 256`,
  one synchronous pass) so the banner's entire lifetime could be a single frame. Same fix, moot.
The degraded-with-rows banner (CPE-1805's original mechanism) is now deliberately NOT also keyed on
`!complete` — it's reserved for the resolved, known-degraded case only, and gets to keep its bordered
box since that state isn't subject to the above three problems (it's already resolved by the time it
shows, and rarer).

- **Finding 3** (sticky-header occlusion, separate bug, fixed regardless of the above move): both
  `.tv-degraded-banner` and `.tv-head-row` were independently `position: sticky; top: 0`, so once the
  degraded case scrolled they competed for the same slot — the banner (taller) fully covered the
  header INCLUDING its Select-all checkbox, unclickable. Fixed by wrapping both in one
  `.tv-sticky-stack` container (the only sticky element); the pair now lays out in normal flow inside
  it and stick as one unit, with no hard-coded pixel offset to break on text reflow. Added a
  structural test pinning that both elements share the same `.tv-sticky-stack` ancestor.
- **Finding 5** (narrow-window title reflow): at 584px the title wrapped to two lines and the toolbar
  jumped 36px right when the count/caveat text appeared, because `.tv-title` had no `min-width: 0`.
  Added `min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;` per the
  Critic's own diagnosis.
- **A11Y (blocking)** — the whole fix was visual-only: `.tv-degraded-banner` had no `role`/`aria-live`,
  so CPE-1816's bug (a partial list reading as complete) persisted for a screen-reader user even after
  the visual fix. Put `role="status"` on the title-bar count/caveat slot — the one node that carries
  BOTH the mid-stream caveat and the eventual final count, kept persistent in the DOM (not toggled)
  so content CHANGES announce reliably. Added a test asserting the region exists by role and that its
  announced text actually changes across the mid-stream → resolved transition.
- **Nit taken**: the selection count (`· N selected`) now rides alongside `trash.stillLoading` too,
  same as it already does alongside the resolved item count — a fact the app genuinely knows
  regardless of whether the pass itself has finished.
- **Nit deferred**: the rows-still-shift-slightly-on-open cosmetic concern (banner box height vs. no
  box) was left to the Visual Critic's own separate pass, per the Foreman's explicit instruction not
  to pre-empt it.
- **Finding 7** (this round's own comment-arithmetic error, corrected): `mojibakeGuard.test.ts`'s
  ALLOWLIST comment had said "9 locale blocks" when only 5 blocks (en, es, de, fr, it) sit above the
  anchored line — 9 was the LINE count (en's 5 lines including its comment, plus 1 apiece for the
  other 4), not a block count. Reworded to state both numbers explicitly and not conflate them.

Red-proofs this round, each applied, observed red, then reverted:
- Commented out `complete = false;` in `load()`'s reset block → the new refresh-reset test reds on
  `expect(screen.queryByText("1 item")).toBeNull()` (found a live `<span>1 item</span>` from the
  stale prior pass).
- Reverted the empty-pane condition to `{:else if degraded && entries.length === 0}` → BOTH new
  Restore/Empty mid-stream-drain tests red on `expect(screen.queryByText("Trash is empty")).toBeNull()`
  (found "Trash is empty").
- Renamed the `.tv-sticky-stack` wrapper's class to break the selector (one-line class-name change) →
  the new structural test reds on `expect(stack).toBeTruthy()` (banner and head row no longer share a
  `.tv-sticky-stack` ancestor).
- Removed `role="status"` from the title-bar count span → the new a11y test reds on
  `screen.getByRole("status")` (element not found).

Gates: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 4188 passed (4188), 0 failed (was
4183 before this round; +5 net new tests: reset-on-refresh, two mid-stream-drain tests, sticky-stack
structural test, role=status a11y test). Still no `.rs` files touched — frontend-only, no cargo gates
or bindings regen this round either.

Files changed this round: `src/lib/components/TrashView.svelte`, `src/lib/components/TrashView.test.ts`,
`src/lib/bidiEscape.guard.test.ts` (REGISTRY line re-anchor only), `src/lib/mojibakeGuard.test.ts`
(comment correction only), `src/docs/38-trash.md`.
