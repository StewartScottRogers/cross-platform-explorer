// CPE-1882 — the ONE file a ticket author touches to add a component + a width list to the layout
// guard. Nothing in engine.mjs, run.mjs, vite.harness.layout-guard.config.ts, or the CI job
// (.github/workflows/gui-smoke.yml) needs editing to add a case here — that is this ticket's own
// acceptance criterion ("a ticket author can add a component and a width list without touching harness
// internals"). What DOES still need writing, same as it does for every real-browser harness in this
// repo (statusbar-notice, checkpoint-narrow, revert-heldback-copy, sidebar-drop-stack-overlap): a small
// `scripts/dev-harness/<your-case>/index.html` + `main.ts` that mounts the REAL component with fixture
// props/data (see scripts/dev-harness/trash-titlebar/ for a component with backend-talking imports, or
// scripts/dev-harness/statusbar-notice/ for one without) — that page is domain-specific to the
// component under test and can't be generalised away; the harness MACHINERY around it now is.
//
// Each case:
//   name          — used in CI/console output.
//   path           — URL path (+ query string) on the layout-guard dev server that loads this case's
//                    harness page.
//   height         — fixed viewport height for this case (only `widths` is swept).
//   widths         — the list of CSS viewport widths (px) to sweep. Include the app's own real floor
//                    (`src-tauri/src/lib.rs`'s `.min_inner_size`, 600px) for anything that renders inside
//                    the main window.
//   readySelector  — CSS selector the engine polls for before measuring (proves the component actually
//                    mounted and rendered, not just that the page loaded).
//   checks         — see engine.mjs's header for the full reasoning behind each check kind. Which one
//                    to reach for (found by an independent UAT pass building a case from scratch — its
//                    first instinct, `siblingOverlap`, did NOT catch a missing `flex-wrap`, because
//                    removing wrap doesn't make chips overlap, it pushes the row past the viewport or
//                    shoves the next block down — a case built on that instinct alone would look
//                    protective and not be):
//
//      Row of pills/chips that should wrap?                        -> siblingOverlap on the row (two
//                                                                      chips must never share a pixel).
//      A container has (or should have) overflow: hidden and a
//      pinned child might poke through?                            -> clipProbe.
//      Text might not fit its own box and could paint outside it?   -> textOverflow.
//      A button/control might get squeezed or covered and become
//      unclickable?                                                 -> selfPaint.
//      A button/control LOOKS reachable via elementFromPoint but a
//      REAL click might still land on something else invisibly
//      overlapping it?                                              -> clickReaches (needs an actual
//                                                                      CDP-dispatched click; hit-test
//                                                                      APIs alone are not trustworthy
//                                                                      for this shape — see engine.mjs).
//      A revealed/expanded box (focus, hover, ...) should grow WIDE
//      up to its own max-width, not stack into a tall column?       -> rectBounds (maxHeight) on the
//                                                                      element, from a harness page
//                                                                      that already put it in that
//                                                                      state (see statusbar-notice's
//                                                                      `?focus=` param).
//
//    Note: removing flex-wrap alone does NOT reliably trigger siblingOverlap — a wrap regression
//    usually needs a clipProbe (if the row's container clips) or a manual visual check. A case can mix
//    any number of any kind.
export const CASES = [
  {
    // CPE-1836 red-proof case: "the status bar's git block bleeds into the disk label at the 600px
    // floor" — scripts/dev-harness/statusbar-notice/'s own prototype, now driven by the generic engine
    // instead of only by a human loading it in a browser. `busy=1` reproduces the ticket's own compound
    // scenario (long branch + ahead/behind/dirty + a selection) — the worst-case row density.
    name: "statusbar-notice",
    path: "/scripts/dev-harness/statusbar-notice/inner.html?notice=short&git=on&disk=on&busy=1",
    height: 200,
    widths: [600, 680, 760, 900, 1200],
    readySelector: ".statusbar",
    checks: [
      // No two of .statusbar's own direct children (item-count/git/disk/notice/...) may occupy
      // overlapping screen space. `.resize-grip` is excluded deliberately, not overlooked: it's
      // `position: absolute; right: 0; bottom: 0` — a corner-pinned resize handle that by design can
      // sit over the tail of trailing flow content (the original CPE-1836 prototype's own
      // `inner-main.ts` made the same call — see its `spillOutside` comment).
      { kind: "siblingOverlap", root: ".statusbar", exclude: [".resize-grip"] },
      // CPE-1836 itself: does .git's `overflow: hidden` actually clip an overhanging pinned child
      // (branch/ahead/behind/dirty/pull-button), or does it paint through onto .disk's territory?
      {
        kind: "clipProbe",
        container: ".git",
        candidates: [
          ".git .git-branch",
          ".git .git-ct[title*=ahead]",
          ".git .git-ct[title*=behind]",
          ".git .git-dirty",
          ".git .git-btn:not(.resolve)",
        ],
      },
      // The repo's pill/chip rule, second half: text never overflows its own background.
      { kind: "textOverflow", selectors: [".notice", ".item-count", ".disk", ".git-branch"] },
    ],
  },
  {
    // CPE-1883 red-proof case: "the status bar's focus-reveal box ignores its own max-width and stacks
    // one word per line". `?focus=filtered-hidden` (see inner-main.ts's header) programmatically
    // focuses `.filtered-hidden` before this engine ever measures it, engaging its `:focus-visible`
    // reveal rule (`max-width: min(90vw, 420px)`) — the compound busy row (`busy=1`) is the ticket's
    // own worst-measured case (148px tall at 600px pre-fix). `height: 300` gives the pre-fix column
    // room to render its full (broken) height rather than being clipped by a too-small viewport, which
    // would have hidden the very defect this case exists to catch.
    name: "statusbar-focus-reveal",
    path: "/scripts/dev-harness/statusbar-notice/inner.html?notice=short&git=on&disk=on&busy=1&focus=filtered-hidden",
    height: 300,
    widths: [600, 900],
    readySelector: ".filtered-hidden:focus-visible",
    checks: [
      // The AC itself, measured on the `::after` pseudo-element that actually renders the reveal (the
      // fix moved it there — see StatusBar.svelte's own CPE-1883 comment for why resizing the real span
      // directly, tried first, either reproduced the stacking bug or broke `.git`/`.disk`). `pseudo:
      // "::after"` — see rectBounds' own doc above and engine.mjs's implementation note: pseudo-element
      // geometry has no getBoundingClientRect, so this reads getComputedStyle(el, "::after") instead,
      // width/height only (no position). The fixed box measures 16px tall (one line — this notice's
      // fixed sentence fits under 420px at its own natural width) at both 600px and 900px, never the
      // 148px a one-word-per-line column produces. `maxHeight: 90` is comfortably above single- or
      // double-line prose (allowing a future longer sentence to wrap once) and comfortably below the
      // broken-state 148px. `minWidth: 100` closes the OTHER failure shape maxHeight alone would miss:
      // if a future edit removes the `::after` rule entirely (or its `content` goes empty), the pseudo
      // renders at 0×0 — a maxHeight-only check would read that as a trivial pass, so minWidth catches
      // "the reveal silently stopped rendering at all" as its own distinct regression. See the ticket's
      // work log for the exact before/after numbers.
      { kind: "rectBounds", selector: ".filtered-hidden", pseudo: "::after", maxHeight: 90, minWidth: 100 },
      // CPE-1883 round 2 (Visual Critic UAT): `rectBounds` above proves the SHAPE is right but nothing
      // proves WHERE it lands — the first shipped CSS anchored via `left: 0` (grow rightward), which ran
      // this exact box ~100px past a 600px viewport with the compound-busy row, silently clipped by
      // `body { overflow: hidden }` with no ellipsis/scroll/cue — WORSE than the original bug (fewer
      // words visible than the one-word-per-line column showed). Fixed by anchoring `right: 0` instead
      // (grows leftward from the pill, which is always on-screen) — this check proves that holds at
      // both tested widths, not just at the one width it happened to fit.
      { kind: "pseudoOnScreen", anchorSelector: ".filtered-hidden", pseudo: "::after", edge: "right" },
      // CPE-1883 round 3 (Reviewer finding, via a REAL dispatched CDP click — see engine.mjs's own
      // `clickReaches`/`runClickReachesChecks` doc for why `selfPaint`'s `elementFromPoint` approach is
      // NOT trustworthy for this exact shape and had to be replaced): round 2's `color: transparent`
      // fix on `.filtered-hidden`'s base `:focus-visible` rule left that span's own raw text still
      // painting -- invisibly, at zero alpha -- across its full unclipped ~367px natural width, with
      // default `pointer-events: auto`. That invisible text physically overlaps `.git`'s Pull/Push/Sync
      // buttons, and with no `pointer-events` override a real click there lands on the SPAN, not the
      // BUTTON -- confirmed reproducible, and NOT caught by `document.elementFromPoint` /
      // `document.elementsFromPoint`, which both reported the buttons reachable anyway. Fixed with
      // `pointer-events: none` added to the base rule (see StatusBar.svelte's own round-3 comment); this
      // check exists so the next person who touches that rule and drops the override again gets a red
      // build instead of a silently-swallowed click.
      // CPE-1930 (Reviewer, round 3's own follow-up finding): `.git .git-btn` matches THREE buttons
      // (Pull/Push/Sync) but this check's first version resolved the selector via `querySelector`,
      // which only ever tests the first one -- a regression isolated to Push or Sync alone would have
      // slipped past as a false-negative PASS. `runClickReachesChecks` now iterates every match via
      // `querySelectorAll` and clicks each one that is actually on-screen; at 600px busy, Push/Sync sit
      // off-viewport for a pre-existing, unrelated row-overflow reason (not this ticket's bug) and are
      // reported honestly as skipped rather than counted as a miss.
      { kind: "clickReaches", selectors: [".git .git-btn"] },
    ],
  },
  {
    // CPE-1827 red-proof case: "the Trash titlebar cannot fit seven buttons and a title on one line at
    // supported widths" — the app's own real floor (600px) up through comfortably-wide. New case: no
    // harness for TrashView existed before this ticket (see scripts/dev-harness/trash-titlebar/).
    name: "trash-titlebar",
    path: "/scripts/dev-harness/trash-titlebar/index.html",
    height: 400,
    widths: [600, 640, 700, 760, 880, 1000, 1200],
    readySelector: ".tv-titlebar",
    checks: [
      // .tv-title vs .tv-tools must never collide, at any width.
      { kind: "siblingOverlap", root: ".tv-titlebar" },
      // The CPE-1827 invariant itself: the overflow-menu trigger and, above all, Close (×) must stay
      // actually hit-testable — not merely present in the DOM — at every tested width. This is the
      // "control becomes unreachable" failure shape (distinct from CPE-1836's "content bleeds onto a
      // neighbour"): `.tv-panel` is `overflow: hidden`, so a `.tv-tools` pushed wide enough by an
      // unwrapped `.tv-title` is silently clipped rather than visibly broken.
      { kind: "selfPaint", selectors: [".tv-overflow-btn", ".tv-x"] },
      { kind: "textOverflow", selectors: [".tv-title"] },
    ],
  },
];
