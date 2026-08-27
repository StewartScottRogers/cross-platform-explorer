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
//   checks         — see engine.mjs's header for the four check kinds and which class of bug each one
//                    catches. A case can mix any number of any kind.
export const CASES = [
  {
    // CPE-1836 red-proof case: "the status bar's git block bleeds into the disk label at the 600px
    // floor" — scripts/dev-harness/statusbar-notice/'s own prototype, now driven by the generic engine
    // instead of only by a human loading it in a browser. `busy=1` reproduces the ticket's own compound
    // scenario (long branch + ahead/behind/dirty + a selection) — the worst-case row density.
    name: "statusbar-notice",
    path: "/scripts/dev-harness/statusbar-notice/inner.html?notice=short&git=on&disk=on&busy=1",
    height: 200,
    widths: [420, 480, 520, 600, 680, 760, 900, 1200],
    readySelector: ".statusbar",
    checks: [
      // No two of .statusbar's own direct children (item-count/git/disk/notice/resize-grip/...) may
      // occupy overlapping screen space.
      { kind: "siblingOverlap", root: ".statusbar" },
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
