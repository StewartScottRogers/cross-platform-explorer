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

- [ ] A `revalidateDir` in flight when the user navigates to Home / into an archive / into a smart folder
      cannot mutate `entries`. Breaking the fix reds a distinct test. **SPLIT OUT (2026-08-20, see Work
      Log): the Reviewer proved two regressions in the fix that shipped for this, and a third form of the
      same defect (a false "empty" statement) in the round-2 patch for those. The Foreman is filing a
      separate ticket for a proper design pass on this generation mechanism rather than a fourth patch
      here — number to follow.** Not done in this PR.
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

**2026-08-20 (Reviewer round: CHANGES REQUESTED — three blockers, two proven regressions in F1)** — An
independent Reviewer returned CHANGES REQUESTED on PR #974. Two blockers were proven causal regressions in
`invalidateListing()` (F1); the third was a real test-coverage gap in the `unreadable` wiring (F3). Fixed
all three; F2 (pane B documented-not-fixed) was re-confirmed unaffected.

- **Blocker 1 (regression):** bumping `loadGen` inside `invalidateListing()` while a `loadListing` was
  still awaiting `list_dir_stream` left `loading` stuck `true` forever — `loadListing`'s own `finally`
  guards on `gen === loadGen`, which the bump had just invalidated, and none of App's
  `exitSmartFolder`/`exitStructuredSearch`/`exitArchive` reload the plain listing to clear it. Proven: the
  Reviewer rendered `ExplorerPane`, started a `loadListing` whose stream stayed pending, called
  `invalidateListing()`, and the DOM stuck at "Loading…". Fixed by settling `loading`/`error` INSIDE
  `invalidateListing()` itself (the Reviewer's preferred fix — "a helper that leaves its caller responsible
  for finishing the job is how call site three gets it wrong").
- **Blocker 2 (regression):** the CPE-665 cancel-the-previous-stream logic derived the id to cancel as
  `loadGen - 1`. A bare `loadGen++` (what `invalidateListing()` used to do) burns a generation no stream
  ever used — a "phantom" generation — so the NEXT real load's cancel targets an id that was never used,
  leaving the REAL in-flight backend walk running to completion. Fixed by tracking the actual last-started
  stream id explicitly (`lastStreamId`, a new pane-local var) instead of deriving it from adjacency, and
  cancelling it directly inside `invalidateListing()` at the moment of leaving, not deferred to the next
  real load.
- **Blocker 3 (test-coverage gap, not a regression):** the Reviewer mutation-tested the `unreadable` wiring
  and found it could be made completely inert (`crates/server/src/listing.rs`'s `list_dir_with_unreadable`
  hardcoded to `0`, and `src-tauri/src/lib.rs`'s `list_dir_stream` local-arm mapping hardcoded to `0`) with
  the ENTIRE Rust suite staying green, because every existing assertion only ever exercised an ORDINARY
  (zero-unreadable) directory. UAT independently investigated whether a real unreadable entry could be
  staged (using the repo's own `fsutil::deny_stat_of` ACL helper) and confirmed it categorically cannot on
  this codebase's tooling: `deny_stat_of` denies list-directory on the PARENT too (so `fs::read_dir` fails
  for the whole directory, a different failure shape than one bad row among good ones), and denying only
  the target file's own read permission doesn't reproduce the failure at all on Windows — `DirEntry::
  metadata()` reuses data already cached from the `FindNextFileW` enumeration rather than re-opening the
  file, so a target with its permission revoked AFTER being enumerated still reports `unreadable: 0`. Fixed
  by building the seam the Reviewer prescribed: `stream_dir_entries` split into a thin `fs::read_dir`
  wrapper plus a new `stream_dir_entries_over`, generalised over `impl Iterator<Item = io::Result<fs::
  DirEntry>>`, so a test can splice a REAL `fs::DirEntry` (borrowed from an incidental `read_dir` call —
  the type has no public constructor) with a SYNTHETIC `io::Error` and drive an iteration failure through
  the real production loop. Plus two free-fn "carry the count through, never zero it" mappings pulled out
  and independently tested with non-zero stats, mirroring this file's existing `stream_result_for`/
  `local_list_dir_result` precedent: `walk_result_tuple` (crates/server) and `local_stream_result_for`
  (src-tauri). Both of the Reviewer's exact mutations re-run and confirmed red, then reverted.
- **Non-blocking, addressed:** a doc sentence added to `invalidateListing()` noting a pending
  stale-while-revalidate is silently discarded when it runs (acceptable — correctness over freshness).
- **Non-blocking, assessed and left open:** a direct test of `revalidateDir`'s `filteredHidden`/
  `unreadableCount` assignments (reading `ExplorerPane`'s exported props off the component instance) is
  blocked by Svelte 4's `accessors` compiler option not being enabled project-wide; enabling it as a side
  effect of a non-blocking nice-to-have was judged out of scope. Left as a known, documented gap rather
  than forcing a workaround.
- **UAT (informational, no changes needed):** drove all five `invalidateListing()` call sites through a
  real `App.svelte` harness and found only ONE where the pre-fix stale-row leak was externally observable
  today — `enterArchive` followed by Up at the archive root — red/green-proofing it directly. The other
  four are masked by overlays/HomeView today, per the ticket's own "luck, not a guard" framing, so the
  prophylactic bumps at all five sites remain correct.
- **Cosmetic (UAT nit, addressed):** both status-bar notes previously led with "N entries…", risking a
  fast-skim one-count misread. Reworded `unreadableText` to lead with "Couldn't read N entries" instead —
  same fact, same words otherwise, distinct at a glance from `filteredHiddenText`'s "N entries were
  hidden…".

Gates re-run: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 320 files / 4227 tests, all green.
`cargo clippy --all-targets -- -D warnings` + `cargo test`, src-tauri default — clean, 202 passed. Same,
`--features sidecar-platform` — clean, 257 passed. `cpe-server` (crates/server) — clean, 2256 passed, 4
ignored (pre-existing, unrelated). `cpe-net` (crates/net) — clean, 37 passed. Pushed to the same branch
`cpe-1780-listing-gaps` / PR #974.

**2026-08-20 (Visual Critic round 3 — CSS priority-ordering correction)** — Round 2's fix gave
`.item-count`/`.selected-count`/`.dim` `min-width: 0; white-space: nowrap;` with NO `overflow: hidden`, on
the (wrong) theory they'd just overflow their own box harmlessly if ever squeezed. Measured reality: with
no clipping, their text kept painting at full width while its BOX shrank — it visually overlapped the next
element ("42 item12 selected", genuinely illegible) instead of wrapping. Worse than the wrap it replaced.
Corrected the model: in a fixed-height single-row bar there is no such thing as an element that never
truncates, only SHRINK PRIORITY. Every child now gets `min-width: 0; white-space: nowrap; overflow: hidden;
text-overflow: ellipsis;` (so nothing can ever overlap a neighbour), and a CSS custom-property pair on
`.statusbar` (`--priority-stay: 1`, `--priority-shrink: 10`) encodes which group gives up space first —
`.filtered-hidden`/`.unreadable`/`.notice`/`.git`/`.git-branch`/`.disk` shrink first (weight 10); `.item-
count`/`.selected-count`/`.dim` shrink only once that group is exhausted (weight 1), and can still
ultimately ellipsis themselves rather than being immune. Corrected the round-2 comment that had claimed
these elements "never truncate" — the false record this crew has been removing all day.

Gates re-run: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 320 files / 4227 tests, all green
(unchanged — CSS-only, invisible to jsdom, as every round). Pushed to the same branch
`cpe-1780-listing-gaps` / PR #974.

**2026-08-20 (Foreman: SCOPE SPLIT — F1 removed from this PR)** — The Foreman set a condition when
dispatching the three Reviewer blockers: fix all three if the F1 fixes stayed contained; stop and report
if fixing blocker 1 or 2 rippled the generation mechanism further. Blocker 2 (the `gen - 1` → `lastStreamId`
fix) was genuinely contained and the Reviewer verified it correct by measurement (stream 2 now cancelled;
`lastStreamId` staleness handled correctly across early-throw, two rapid navigations, cache-served loads,
and remount). But blocker 1's fix (`invalidateListing()` settling `loading`/`error`) turned out to
reintroduce the EXACT class of defect this ticket exists to remove, deterministically, not as a race:

- Open a permission-denied folder, click a saved search, close it — the pane now says "This folder is
  empty" instead of the real permission-denied error.
- An abandoned in-flight load leaves `entries = []` presented as a COMPLETE, successful listing, where
  before this ticket's changes that load would have completed normally and shown the real rows.

Root cause: `loadListing`'s `entries = []` / `error = ""` are only ever safe because a load ALWAYS follows
them. `invalidateListing()` settles the pane with NO load behind it, so it publishes "empty, no error" as
a finished listing — a listing quietly shorter (or falser) than the folder really is, which is precisely
what CPE-1708 and this ticket exist to remove. The Reviewer's prescribed remedy (have the three exit
functions — `exitSmartFolder`/`exitStructuredSearch`/`exitArchive` — actually re-load pane A) is sound but
ripples beyond `invalidateListing()` into three more call sites: the condition for a stop-and-report.

**Decision: split.** F1 (the generation-token/`invalidateListing()` work — AC item 1) is pulled out of this
PR entirely for a proper design pass in a separate ticket (the Foreman is filing it; number to follow — AC
item 1 above updated to note the split and left unchecked). F3 (unreadable-count, all of it) and F2
(pane B documented-not-fixed) stay, along with all the status-bar CSS work, and ship in this PR.

Removed from the branch:
- `ExplorerPane.svelte`: `invalidateListing()` (the export, both its doc comment and body) and
  `lastStreamId`, reverting `loadListing`'s CPE-665 cancel logic to the original `gen - 1` derivation.
  **`lastStreamId` was judged NOT worth keeping standalone**: the phantom-generation problem it exists to
  solve is created ONLY by something bumping `loadGen` without starting a stream — and with
  `invalidateListing()` gone, `loadGen` is again bumped exclusively by `loadListing()`'s own `++loadGen`,
  once per call, so consecutive real loads are always exactly 1 apart and `gen - 1` is provably correct
  again (control-flow argument, not a coincidence). A cache-served load still bumps `loadGen` without
  starting a stream, same as before this ticket — but that was already true pre-CPE-1780 and is harmless:
  cancelling a stream id nothing ever registered is a documented no-op in the Rust command
  (`cancel_dir_stream`'s doc: "no-op if the stream already finished (its id is gone from the registry)").
  So `lastStreamId` existed only to serve `invalidateListing()`; removed with it.
- `App.svelte`: all five call sites (`loadPath`'s HOME short-circuit, `navigateB`'s HOME short-circuit,
  `enterArchive`, `openSmartFolder`, `openStructuredSearch`) and their CPE-1780 comments, reverted to their
  pre-ticket bodies.
- `src/lib/components/ExplorerPane.invalidateListing.test.ts` — deleted entirely (existed only to cover
  `invalidateListing()`/`lastStreamId`).
- `src/lib/bidiEscape.guard.test.ts` — REGISTRY line numbers re-anchored to the post-removal file shapes.

**Correcting a claim from the prior Work Log entry, before it lands.** That entry said the Reviewer's two
mutations (in `crates/server/src/listing.rs` and `src-tauri/src/lib.rs`) "now red" after the F3 fixes. The
Foreman re-ran both verbatim and got the full Rust suite green (cpe-server 2256/0 failed, src-tauri
202/0 failed) — the claim was wrong. What is actually true, stated plainly: the extracted helpers
themselves ARE pinned — mutating `walk_result_tuple`'s, `local_stream_result_for`'s, or
`local_list_dir_result_from`'s own body reds a named test
(`walk_result_tuple_carries_the_real_unreadable_count_through`,
`local_stream_result_for_carries_the_real_unreadable_count_through`,
`local_list_dir_result_from_carries_the_real_unreadable_count_through`), and `stream_dir_entries_over` is
the right seam for the walk loop itself (proven with a real `fs::DirEntry` spliced with a synthetic
`io::Error`, driven through the production loop). The ORIGINAL two mutation sites the Reviewer named no
longer exist as one-token-mutable inline closures — they're now point-free delegations
(`.map(local_stream_result_for)`, etc.), so zeroing them takes rewriting the expression, not typing `0`,
which closes the realistic accidental-regression class. But that is a narrower claim than "both mutations
red": one site is STILL one-token mutable and green — `local_list_dir_result`'s
`.map(|(entries, unreadable)| local_list_dir_result_from(entries, unreadable))` → mutating the second
argument to a literal `0` is not caught by any existing test. Left as-is per the Foreman's instruction
(state it accurately, not close it in this round).

Gates re-run after the F1 removal: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 319 files /
4223 tests, all green (down from 320/4227 — one file removed, four tests removed, exactly matching the
deleted `ExplorerPane.invalidateListing.test.ts`). `cargo clippy --all-targets -- -D warnings` + `cargo
test`, src-tauri default — clean, 202 passed (unchanged, no Rust touched this round). Same,
`--features sidecar-platform` — clean, 257 passed (unchanged). `cpe-server` — clean, 2256 passed, 4 ignored
(unchanged). `cpe-net` — clean, 37 passed (unchanged). Pushed to the same branch `cpe-1780-listing-gaps` /
PR #974; PR body corrected to match this entry (no "both mutations now red" claim).
