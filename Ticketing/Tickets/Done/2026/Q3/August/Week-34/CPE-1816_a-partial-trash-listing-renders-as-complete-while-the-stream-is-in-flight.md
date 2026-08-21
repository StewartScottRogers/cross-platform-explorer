---
id: CPE-1816
title: a partial trash listing renders as complete while the stream is still in flight
type: bug
priority: Low
status: Done
tags: ready
estimate: M
created: 2026-08-20
closed: 2026-08-20
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

**2026-08-20, round 3** — Reviewer APPROVED and most Visual Critic findings closed and verified
(rows-jump/flash/false-affordance gone, sticky-stack occlusion fixed, `role="status"` shape correct,
theme/contrast/focus PASS). The Critic found one of round 2's own fixes had broken the thing it was
protecting, plus a new a11y gap it introduced. Attempt 3 of 3, addressed both.

BLOCKING 1 — round 2's `.tv-title { min-width: 0; overflow: hidden; white-space: nowrap;
text-overflow: ellipsis; }` stopped the wrap/jump bug (finding 5) but, paired with `.tv-tools` never
shrinking (no `min-width: 0` of its own, so its full button-row content width was a de facto hard
floor), dumped 100% of every width deficit onto `.tv-title`. At the app's own permitted minimum window
(600px — `.min_inner_size(600.0, 400.0)`, `src-tauri/src/lib.rs:12438-12439`) the title collapsed to a
few px, clipping the icon, "Trash", and the caveat itself — CPE-1816's original bug, reintroduced by
its own fix, inside the app's supported width envelope (measured onset: intact >=880px, caveat clips
with a selection <=860px, caveat clips with nothing selected <=800px, ~86% gone by <=740px, title and
caveat entirely destroyed by <=684px). Also, `text-overflow: ellipsis` never worked: it truncates ONE
block-level box's own text, and `.tv-title` is a flex CONTAINER holding three separate flex items
(icon, an anonymous "Trash" text item, and the count span) — a flex container's ellipsis does not
summarize its children's combined overflow, so the round-2 comment claiming a truncated "…" was simply
wrong; the actual failure was a hard mid-word cut with no ellipsis ever rendered.

Fixed by giving `.tv-title` a floor instead of an open `min-width: 0` (`min-width: 34ch`, reasoned —
not pixel-measured, see the code comment for the full budget: icon+gaps ~4ch, "Trash" ~6ch, the
longest `trash.stillLoading` translation among the 12 shipped locales ~22ch (Italian "Caricamento in
corso…" / Russian "Продолжается загрузка…"), ~2ch safety margin for non-Latin glyph width uncertainty
= ~34ch; covers the base caveat alone, not the caveat plus a live "· N selected" suffix, since the
onset sweep showed that combination clipping starts at a materially wider ~860px, well outside the
600px target, so it's accepted as a secondary, non-blocking degradation) and giving `.tv-tools`
`min-width: 0` so it actually participates in the shrink instead of acting as an unyielding floor.
Removed the non-functional `text-overflow: ellipsis` rather than trying to make it work (the offered
alternative — moving truncation into a dedicated inner block-level span — was not taken, to avoid
adding markup/structure this harness cannot pixel-verify without a real browser); corrected the
comment that had claimed it worked. Exact pixel behaviour at 600px could not be verified here — jsdom
does not apply component CSS to `getComputedStyle` under this project's vitest config (confirmed
empirically: a `.tv-title` computed `min-width` reads back as `"auto"` regardless of the declared
rule), so this is a reasoned, documented estimate awaiting the Visual Critic's real-browser
confirmation, exactly as the Foreman's instructions anticipated for CSS-layout-only concerns.

BLOCKING 2 (a11y, newly introduced by round 2's own fix) — on the degraded exit, the title bar's live
region goes `"Still loading…"` -> `""` the instant the pass resolves degraded (nothing in its `{#if}`
chain matches `complete && degraded` with nothing selected), and neither degraded-notice placement
(`.tv-degraded-note`, used for both the empty-pane note and the entries-present rows banner) had a
`role` or `aria-live` of its own — so a screen-reader user was told the listing was still loading and
then told nothing: not a count, not "unreadable", nothing. Before this PR there was no live region at
all, so the app never made a claim it failed to withdraw; round 2's fix introduced the claim without
the withdrawal, and it also affects the round-2-introduced mid-stream-drain state (Restore/Empty
draining to zero while still in flight), not only the fully-resolved-degraded case. Fixed by adding
`role="status"` to BOTH `.tv-degraded-note` occurrences (the shared span used by both placements) —
each mounts fresh exactly when it has something new to say, which is the correct live-region shape for
genuinely new information (as opposed to the title-bar slot's persistent-node shape, chosen there
because THAT slot's content changes in place across states rather than mounting fresh).

NIT (fixed, not just noted) — the separator lost its leading space: the title-bar slot read "3
items· 1 selected" / "Still loading…· 1 selected" because Svelte trims the leading whitespace
immediately inside an `{#if}` block's static text. Pre-existing on the resolved-count branch (the
Critic checked specifically to avoid mis-blaming this PR for it), but this round's mid-stream branch
duplicated it into a second, now screen-reader-audible state. Fixed both occurrences with `&nbsp;·`
(a literal entity survives where collapsible ASCII whitespace doesn't) rather than moving the space
outside the block, to keep the two branches visually/structurally parallel.

Two doc nits (Reviewer) — `src/docs/38-trash.md`: "the same wording appears in the body too" implied
*in addition to* the title bar; the title slot is actually empty in that state, so the wording MOVES
rather than duplicates — reworded to "moves into the body instead." The CSS comment claiming ellipsis
truncation worked is corrected as part of the BLOCKING 1 fix above.

Left alone, per the Foreman's explicit instruction — `.tv-sticky-stack`'s structural test (pins class
nesting, not stickiness; deleting `position: sticky` leaves it green) is a disclosed, accepted
limitation of jsdom's lack of layout, tracked separately as CPE-1822 for real-browser coverage; adding
a fragile CSS-text assertion for it was explicitly declined. The caveat's 12px/italic/70%-opacity
styling is confirmed correct (contrast 6.35:1 light / 7.06:1 dark — legibility was never the question,
only emphasis, and calm reads right for a view with no motion) and left unchanged.

Red-proofs this round, each applied, observed red, then reverted:
- Removed `role="status"` from the empty-pane `.tv-degraded-note` → the new degraded-and-empty a11y
  test reds on `expect(note.getAttribute("role")).toBe("status")` (found `null`).
- Removed `role="status"` from the rows-banner `.tv-degraded-note` → the existing CPE-1805 notice test's
  new assertion reds the same way.
- Reverted the mid-stream branch's `&nbsp;·` back to a plain leading space (`{#if selected.size > 0}
  · {selectedCountLabel}{/if}`) → the new separator-spacing test reds with the EXACT reported shape:
  `"Still loading…· 1 selected"` (no gap) instead of the expected nbsp-separated string. Repeated for
  the resolved-count branch's `&nbsp;·` → same test reds the same way, confirming the fix (not just an
  incidental pass) on both occurrences.
- `.tv-title`'s `min-width: 34ch` / `.tv-tools`'s `min-width: 0` are NOT independently red-proofable in
  this harness: jsdom does not apply component-scoped CSS to `getComputedStyle` here (verified
  empirically — see above), so no vitest assertion on these two declarations can ever go red or green
  on real content; their correctness rests on the documented arithmetic and awaits the Critic's
  real-browser confirmation, stated here plainly rather than papered over with an assertion that
  cannot fail.

Gates: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 4190 passed (4190), 0 failed (was
4188 before this round; +2 net new tests: degraded-and-empty a11y test, separator-spacing test — the
rows-banner a11y check was added as an assertion inside the existing CPE-1805 notice test rather than
a new one). Still no `.rs` files touched — frontend-only, no cargo gates or bindings regen this round.

Files changed this round: `src/lib/components/TrashView.svelte`, `src/lib/components/TrashView.test.ts`,
`src/lib/bidiEscape.guard.test.ts` (REGISTRY line re-anchor only), `src/docs/38-trash.md`.

**2026-08-20, round 4 (converging fix, per the Foreman)** — Visual Critic re-measured round 3: the
title-collapse regression (round 3's own BLOCKING 1 fix) was completely gone — `title.w`, `tb.h`,
`tools.x` all identical across states at every measured width, no clipping, no jump, the 34ch budget's
own arithmetic confirmed sound (computed floor 245.4px vs. worst base caveat 204px = 41px margin) — but
the fix had pushed the damage onto the toolbar instead: `.tv-tools { min-width: 0 }` shrank the BOX
while its buttons kept their own default `min-width: auto`, so the box shrank but its ~608px of button
content didn't, spilling out under `overflow: visible` and getting silently clipped by `.tv-panel`'s
`overflow: hidden`. Refresh, Docs, and the Close button (no Escape-key fallback exists) became
unreachable in a ~700-880px band that was fully fine before this ticket and in round 1 — a worse defect
than the one being fixed, and a NEW regression (the pre-existing <=684px debt was not this PR's own).

Per the Foreman's explicit, already-measured instruction: deleted BOTH `min-width` declarations
(`.tv-title`'s `min-width: 34ch` floor and `.tv-tools`'s `min-width: 0`), keeping every other round-3
change (the `text-overflow: ellipsis` removal, the BLOCKING 2 `role="status"` additions, the `&nbsp;`
separator fix, all markup/logic changes). The Critic's own sweep on this exact diff confirmed: the
caveat is never clipped and never lost across every width 520-1200px, all three listing states, AND
all 12 shipped locales — no floor turns out to be necessary at all; instead of clipping, `.tv-title`
now wraps onto a second line when it runs out of room (this component's original, pre-CPE-1816
behaviour), and `.tv-tools` reverts to its own pre-this-ticket sizing, restoring the close button (and
Refresh/Docs) exactly. Cost: the titlebar wraps taller (`tb.h` 49->53, 69 with a selection) at
<=800px — this is the Critic's original finding 5 (ranked last of five, present before CPE-1816
touched this file at all, and made progressively worse by two rounds of CSS-only patches trying to
avoid it). Per the Foreman, this needs an actual toolbar-density decision (icon-only buttons under a
breakpoint, an overflow menu, or accepting the wrap) rather than another patch here, so it is
intentionally left as-is and tracked in a follow-up ticket the Foreman is filing separately — NOT fixed
in this PR. Rewrote the CSS comments on `.tv-title` and `.tv-tools` accordingly: no leftover prose
describing a 34ch budget rule that no longer exists.

Also corrected a false claim in the round-3 a11y comments (Foreman review, not a mechanism change):
both `role="status"` additions on `.tv-degraded-note` had claimed a freshly-mounted node "already
containing its text" was "the correct live-region shape for genuinely NEW information." That inverts
the accepted guidance — a node inserted WITH its content already present in the same DOM mutation is
frequently NOT announced at all (WebView2 + Windows AT is a particularly weak combination for it),
unlike the title-bar slot's persistent-node shape (content changes IN PLACE inside an already-mounted
node), which IS the reliable pattern — and it contradicted the reasoning already correctly applied to
the title-bar slot's own comment two paragraphs above it. Fixed both comments to state this plainly:
net effect is best-effort announcement rather than guaranteed silence (strictly better than before this
fix, never worse), with the actual reliability fix tracked as a follow-up rather than attempted here,
per the Foreman's explicit instruction to leave the mechanism (the `role="status"` attributes
themselves) untouched. No test changes needed — the existing tests assert the `role="status"` attribute
is present, which remains true and correct; they never asserted anything about live-region reliability.

Confirmed passing this round, untouched: `.tv-sticky-stack` occlusion fix (no overlap, checkbox
hit-testable in both degraded and streaming states); streaming-vs-complete geometry (no row jump);
theme contrast (6.35:1 light / 7.06:1 dark, no hard-coded colour); and the separator fix
(`"3 items · 1 selected"` / `"Still loading… · 1 selected"`, verified via `textContent` in round 3's
own tests, which still pass unchanged).

No new tests this round — the change is a pure CSS deletion (removing two declarations that were
actively causing the toolbar regression) plus comment corrections; the existing test suite already
covers the affected markup/behaviour and continues to pass unchanged. The CSS-value claims themselves
remain unpinnable in this harness for the same reason established in round 3 (jsdom applies no
component-scoped CSS to `getComputedStyle`), so their correctness rests on the Critic's real-browser
sweep, exactly as documented in the rewritten code comments.

Gates: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 4190 passed (4190), 0 failed
(unchanged from round 3 — no tests added or removed this round). Still no `.rs` files touched —
frontend-only, no cargo gates or bindings regen this round.

Files changed this round: `src/lib/components/TrashView.svelte` only (two CSS deletions + comment
rewrites on `.tv-title`/`.tv-tools`, plus the two a11y comment corrections on `.tv-degraded-note`).
