// CPE-1660 notice-overflow verification harness — inner iframe document script. Reads ?notice=<short|
// long|none> from the URL (width comes from the OUTER page sizing the <iframe> itself), mounts the real
// StatusBar.svelte with a notice of the requested length, and publishes a diagnostic object on `window`
// that the outer page reads back via same-origin `iframe.contentWindow` access.
//
// CPE-1859 extends it with `?git=on|off&disk=on|off`, because the right-hand cluster's ANCHORING — not
// just its height — needs a real browser to see: this project's vitest config applies no component CSS,
// so `getComputedStyle`/`getBoundingClientRect` in jsdom cannot observe where in the row an element
// actually lands. The diagnostics below therefore report the measured horizontal POSITION of `.disk`
// (its distance from the bar's right padding edge, and from `.item-count`'s right edge), which is the
// single number that distinguishes "anchored right" from "left-adjacent to the item count".
//
// CPE-1836 extends it again with `?busy=1`, reproducing the ticket's own compound scenario (both
// advisory notes + a selection + "Hidden files shown" + a long git branch with ahead/behind/dirty, all
// at once) and adds a generic ALL-CHILDREN rect dump with pairwise overlap + parent-spill checks, per
// the ticket's own lesson: "This row has moved its failure between elements three times; measuring only
// the element you changed is how that happened." `?compact=1` additionally requests the case where
// `.git`'s pinned children alone (no branch) already threaten to overflow, isolating the exact defect.
//
// CPE-1883 extends it again with `?focus=filtered-hidden|unreadable` — programmatically focuses that
// pill after mount so its `:focus-visible` reveal rule is actually engaged before anything is measured
// or screenshotted (a `tabindex="0"` element matches `:focus-visible` in real Chrome on ANY focus, not
// only a keyboard-driven one, since it has no native "only show the ring after keyboard use" heuristic —
// verified via `.matches(':focus-visible')` in `diag.focusVisibleMatched` below, so a false pass can't
// slip through if that assumption ever stops holding). `?fh=<n>` / `?un=<n>` override
// `filteredHidden`/`unreadableCount` independently of `?busy=1`, so the "uncrowded" 900px case from the
// ticket (one pill focused, nothing else competing for row space) is reachable without the compound
// busy-row scenario. `?theme=dark` sets `document.documentElement.dataset.theme` the same way
// `src/lib/theme.ts` does in the real app (see `scripts/dev-harness/trash-titlebar/main.ts` for the
// same convention), for before/after screenshots in both themes.
import StatusBar from "../../../src/lib/components/StatusBar.svelte";

const params = new URLSearchParams(location.search);
const noticeParam = params.get("notice");
const kind = noticeParam === "long" ? "long" : noticeParam === "none" ? "none" : "short";
const showGit = params.get("git") !== "off";
const showDisk = params.get("disk") !== "off";
const busy = params.get("busy") === "1";
const theme = params.get("theme") === "dark" ? "dark" : "light";
document.documentElement.dataset.theme = theme;
const focusTarget = params.get("focus"); // "filtered-hidden" | "unreadable" | null
const fhParam = params.get("fh");
const unParam = params.get("un");
const filteredHiddenCount = fhParam !== null ? Number(fhParam) : busy ? 4 : 0;
const unreadableCount = unParam !== null ? Number(unParam) : busy ? 2 : 0;

// Mirrors the ticket's own evidence table: a long, real-world-shaped notice string (German-length
// class) vs. a short one that comfortably fits on one line at the tested widths.
const SHORT_NOTICE = "Saved.";
const LONG_NOTICE =
  "Es gibt nicht behobene Probleme in dieser Basislinie — bitte vor dem Fortfahren prüfen und bestätigen.";

// CPE-1836: the compound scenario from the ticket — a long branch (so `.git-branch` is already fully
// collapsed by the time the pinned children are measured), ahead+behind+dirty (so counts + dot + two
// buttons — Pull, Push, Sync… — are all on screen, `.git`'s worst realistic case).
const BUSY_BRANCH = "feature/very-long-branch-name-that-does-not-fit-at-all";

const app = new StatusBar({
  target: document.getElementById("mount")!,
  props: {
    itemCount: 128,
    totalCount: busy ? 512 : 128, // busy: also exercises `.item-count`'s "X of Y" form
    selectedCount: busy ? 7 : 0,
    selectedSize: busy ? 987_654_321 : 0,
    hiddenShown: busy,
    filteredHidden: filteredHiddenCount,
    unreadableCount,
    notice: kind === "none" ? "" : kind === "long" ? LONG_NOTICE : SHORT_NOTICE,
    // CPE-1859: `{#if git && git.is_repo}` gates the whole chip, so `git: null` is the ordinary
    // NON-REPO folder — the state in which `.disk` has no preceding `margin-left: auto` sibling.
    git: showGit
      ? busy
        ? { is_repo: true, branch: BUSY_BRANCH, upstream: "origin/main", ahead: 3, behind: 12, dirty: true }
        : { is_repo: true, branch: "main", upstream: "origin/main" }
      : null,
    diskFree: showDisk ? 123_456_789_012 : null,
    diskTotal: showDisk ? 500_000_000_000 : null,
  },
});
(window as unknown as { __statusBar?: unknown }).__statusBar = app;

type Rect = { left: number; right: number; top: number; bottom: number; width: number; height: number } | null;

function rectOf(el: Element | null): Rect {
  if (!el) return null;
  const b = el.getBoundingClientRect();
  return { left: b.left, right: b.right, top: b.top, bottom: b.bottom, width: b.width, height: b.height };
}

/** CPE-1836: every DIRECT child of `.statusbar` — not a curated subset — plus `.git`'s own children,
 *  named by a stable label so the readout survives markup reordering. */
const STATUSBAR_CHILD_SELECTORS: Array<[label: string, sel: string]> = [
  ["item-count", ".item-count"],
  ["selected-count", ".selected-count"],
  ["hidden-shown", ".dim:not(.disk)"],
  ["filtered-hidden", ".filtered-hidden"],
  ["unreadable", ".unreadable"],
  ["notice", ".notice"],
  ["git", ".git"],
  ["disk", ".disk"],
  ["resize-grip", ".resize-grip"],
];
const GIT_CHILD_SELECTORS: Array<[label: string, sel: string]> = [
  ["git-branch", ".git .git-branch"],
  ["git-behind", ".git .git-ct[title*=behind]"],
  ["git-ahead", ".git .git-ct[title*=ahead]"],
  ["git-dirty", ".git .git-dirty"],
  ["git-btn-pull", ".git .git-btn:not(.resolve)"],
];

/** Rectangles overlap on both axes (not merely touching at an edge). */
function overlaps(a: NonNullable<Rect>, b: NonNullable<Rect>): boolean {
  return a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
}

/** CPE-1883: programmatically focuses the requested pill so its `:focus-visible` reveal rule is
 *  engaged before anything is measured. Idempotent (re-focusing an already-focused element is a no-op),
 *  which matters because `computeDiag` — this function's only caller — itself runs 3× (rAF/load/
 *  timeout backstops, see the bottom of this file). Returns the focused element (or null) so callers
 *  can confirm `:focus-visible` actually matched, rather than assuming `.focus()` alone proves it. */
function applyFocus(): HTMLElement | null {
  if (focusTarget !== "filtered-hidden" && focusTarget !== "unreadable") return null;
  const el = document.querySelector(`.${focusTarget}`) as HTMLElement | null;
  el?.focus({ preventScroll: true });
  return el;
}

function computeDiag() {
  const focusedEl = applyFocus();
  const statusbar = document.querySelector(".statusbar") as HTMLElement | null;
  const diag: Record<string, unknown> = {
    innerWidth: window.innerWidth,
    noticeLineCount: 0,
    focusTarget: focusTarget ?? null,
    // CPE-1883: `:focus-visible` matching real Chrome behaviour, not merely "the element has focus" —
    // a tabindex-only element has no native click-suppresses-the-ring heuristic, so any `.focus()`
    // should match it, but asserting that here (rather than assuming it) is what makes this harness
    // trustworthy for the actual regression rather than a weaker "was .focus() called" proxy.
    focusVisibleMatched: focusedEl ? focusedEl.matches(":focus-visible") : null,
  };
  if (statusbar) {
    const r = statusbar.getBoundingClientRect();
    diag.statusbarRect = { height: r.height, width: r.width, left: r.left, right: r.right };
    // CPE-1859: the right padding edge of the bar's content box — what a right-anchored child's own
    // right edge should sit flush against (modulo `.disk`'s own zero right margin). Read from the
    // live computed style rather than hard-coding app.css's 14px, so a padding change can't quietly
    // turn a real regression into a passing number.
    const padLeft = parseFloat(getComputedStyle(statusbar).paddingLeft) || 0;
    const padRight = parseFloat(getComputedStyle(statusbar).paddingRight) || 0;
    diag.contentLeft = r.left + padLeft;
    diag.contentRight = r.right - padRight;
    const rect = (sel: string) => rectOf(statusbar.querySelector(sel));
    diag.itemCount = rect(".item-count");
    diag.git = rect(".git");
    diag.disk = rect(".disk");

    // CPE-1836: the full sweep. Every rect named, plus pairwise-overlap and parent-spill checks
    // computed HERE (in the real browser) rather than left for the test file to reconstruct from
    // rough numbers, so `--dump-dom` output is itself the complete, self-checking record.
    const all: Record<string, Rect> = {};
    for (const [label, sel] of STATUSBAR_CHILD_SELECTORS) all[label] = rect(sel);
    for (const [label, sel] of GIT_CHILD_SELECTORS) all[label] = rectOf(document.querySelector(sel));
    diag.allRects = all;

    const present = Object.entries(all).filter(
      (e): e is [string, NonNullable<Rect>] => e[1] !== null,
    );
    const overlapPairs: string[] = [];
    for (let i = 0; i < present.length; i++) {
      for (let j = i + 1; j < present.length; j++) {
        const [labelA, rectA] = present[i];
        const [labelB, rectB] = present[j];
        // A parent/child pair (e.g. `.git` and `.git-branch`) is EXPECTED to overlap — only flag
        // sibling-vs-sibling overlaps, which is what "bleeding into the next box" actually means.
        const isGitChild = (l: string) => l.startsWith("git-");
        if (labelA === "git" && isGitChild(labelB)) continue;
        if (labelB === "git" && isGitChild(labelA)) continue;
        if (isGitChild(labelA) && isGitChild(labelB)) continue; // git's own children vs each other: fine
        if (overlaps(rectA, rectB)) overlapPairs.push(`${labelA}×${labelB}`);
      }
    }
    diag.overlapPairs = overlapPairs;

    const contentLeft = diag.contentLeft as number;
    const contentRight = diag.contentRight as number;
    const spillPairs: string[] = [];
    for (const [label, rr] of present) {
      if (rr.left < contentLeft - 0.5 || rr.right > contentRight + 0.5) {
        spillPairs.push(`${label} (left=${rr.left.toFixed(1)} right=${rr.right.toFixed(1)})`);
      }
    }
    diag.spillOutside = spillPairs;

    // CPE-1836: the geometry-only checks above CANNOT see the fix's actual effect. `overflow: hidden`
    // on `.git` clips PAINTING, not layout — a pinned child's own `getBoundingClientRect()` is identical
    // whether `.git` clips it or not, so `git-btn-pull`'s rect legitimately extends past `.git`'s own
    // right edge in BOTH the broken and fixed builds (measured — see the ticket's work log for the
    // side-by-side numbers). What actually differs is whether that excess is PAINTED. `elementFromPoint`
    // follows the browser's real clip/paint region (unlike `getBoundingClientRect`), so probing a point
    // just past `.git`'s own right edge is the one measurement that tells broken from fixed: it hits a
    // `.git` descendant when the overflow paints through (the bug), and falls through to whatever is
    // actually visible there (`.disk`, or the bar's own background) once `.git` clips it (the fix).
    const gitRect = all.git;
    // Geometric fact (true regardless of the fix): do `.git`'s own pinned children, by LAYOUT ALONE,
    // extend past `.git`'s own box? This is the raw "~16-33px" the ticket measured, independent of
    // whether it is currently being painted.
    let worstOverhangRect: NonNullable<Rect> | null = null;
    if (gitRect) {
      const gitChildOverhang: Record<string, number> = {};
      for (const [label, sel] of GIT_CHILD_SELECTORS) {
        const rr = rectOf(document.querySelector(sel));
        if (rr && rr.right > gitRect.right + 0.5) {
          gitChildOverhang[label] = Number((rr.right - gitRect.right).toFixed(1));
          if (!worstOverhangRect || rr.right > worstOverhangRect.right) worstOverhangRect = rr;
        }
      }
      diag.gitChildOverhangPx = gitChildOverhang;
    }
    // Probed at the MIDPOINT of the overhanging region itself (between `.git`'s own right edge and the
    // overhanging child's right edge) — NOT a fixed few-px offset from `.git`'s edge, because a pinned
    // child can itself start painting a few px past its parent's edge (flex children are positioned by
    // the container's content box, which can already be narrower than a non-shrinking child needs), so a
    // probe too close to `.git`'s edge can land in a genuine gap before that child's own paint begins.
    if (gitRect && worstOverhangRect) {
      const w = worstOverhangRect; // narrowed for TS
      const probeX = (gitRect.right + w.right) / 2;
      const probeY = (gitRect.top + gitRect.bottom) / 2;
      const hit = document.elementFromPoint(probeX, probeY) as HTMLElement | null;
      const hitIsGitDescendant = !!(hit && hit.closest(".git"));
      diag.gitOverflowPaintProbe = {
        x: probeX,
        y: probeY,
        hitClass: hit ? hit.className : null,
        hitIsGitDescendant,
      };
    } else {
      diag.gitOverflowPaintProbe = null;
    }
  }
  (window as unknown as { __sbDiag?: unknown }).__sbDiag = diag;
  (window as unknown as { __sbDiagReady?: boolean }).__sbDiagReady = true;
  renderInnerReadout(diag);
}

/** CPE-1859: mirror the diagnostics into this document's own DOM (see inner.html for why). Same numbers
 *  the outer shell prints; deliberately duplicated as TEXT rather than shared, so `--dump-dom` on THIS
 *  url captures a complete measurement with no cross-frame access and no polling. */
function renderInnerReadout(diag: Record<string, unknown>) {
  const el = document.getElementById("inner-readout");
  if (!el) return;
  const fmt = (r: Rect) =>
    r ? `left=${r.left.toFixed(1)} right=${r.right.toFixed(1)} w=${r.width.toFixed(1)} h=${r.height.toFixed(1)}` : "ABSENT";
  const contentRight = diag.contentRight as number | undefined;
  const disk = diag.disk as Rect;
  const itemCount = diag.itemCount as Rect;
  const lines = [
    `innerWidth=${diag.innerWidth} notice=${kind} git=${showGit ? "on" : "off"} disk=${showDisk ? "on" : "off"} busy=${busy ? "1" : "0"} theme=${theme}`,
    `focusTarget=${diag.focusTarget ?? "none"} focusVisibleMatched=${diag.focusVisibleMatched ?? "n/a"}`,
    `.item-count ${fmt(itemCount)}`,
    `.git        ${fmt(diag.git as Rect)}`,
    `.disk       ${fmt(disk)}`,
  ];
  if (contentRight !== undefined && disk) {
    lines.push(`diskRightGap=${(contentRight - disk.right).toFixed(1)}px  (0 ⇒ anchored right)`);
    if (itemCount) {
      lines.push(`diskFromItemCount=${(disk.left - itemCount.right).toFixed(1)}px  (≈14 ⇒ glued to the count)`);
    }
  }
  // CPE-1836: the full per-child sweep plus the two derived checks that matter — overlapPairs (a
  // sibling painting over a sibling) and spillOutside (a child escaping `.statusbar`'s own padding
  // box). Both MUST be empty arrays for a clean render.
  const all = (diag.allRects ?? {}) as Record<string, Rect>;
  lines.push("--- all children ---");
  for (const [label, rr] of Object.entries(all)) {
    lines.push(`${label.padEnd(14)} ${fmt(rr)}`);
  }
  lines.push(`overlapPairs=${JSON.stringify(diag.overlapPairs ?? [])}`);
  lines.push(`spillOutside=${JSON.stringify(diag.spillOutside ?? [])}`);
  lines.push(`gitChildOverhangPx=${JSON.stringify(diag.gitChildOverhangPx ?? {})}`);
  lines.push(`gitOverflowPaintProbe=${JSON.stringify(diag.gitOverflowPaintProbe ?? null)}`);
  el.textContent = lines.join("\n");
}

// The double-rAF is the interactive path: it publishes after the browser has laid the component out.
// CPE-1859 adds two idempotent backstops, because under `--headless=new --virtual-time-budget=N` the
// rAF callbacks are not guaranteed to run before the DOM is dumped — without them roughly one driven
// run in three snapshotted the readout still saying "booting…". `computeDiag` only reads and reassigns,
// so running it three times costs a measurement, not a state change.
requestAnimationFrame(() => requestAnimationFrame(computeDiag));
window.addEventListener("load", computeDiag);
setTimeout(computeDiag, 0);
