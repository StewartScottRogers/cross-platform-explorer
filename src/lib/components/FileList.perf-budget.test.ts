/**
 * Multi-size perf-budget benchmark harness (CPE-1304, completing CPE-691, epic CPE-688).
 *
 * `FileList.virtualize-guard.test.ts` proves windowing engages for ONE size (5,000 entries).
 * This suite is the other half CPE-691 claimed but never shipped: it drives FileList across
 * 100 / 1,000 / 10,000 / 50,000 synthetic entries and asserts the rendered-row BUDGET holds at
 * every size — i.e. the mounted `.row` count must stay bounded and NOT scale with N. A folder
 * 500× larger must cost the same DOM. It also records mount/settle timing per size via
 * `performance.now()`, logged (not asserted on — CI timing is too noisy for a hard ms budget)
 * so a human/CI-log skim can spot a real regression.
 *
 * Reuses the `getBoundingClientRect` stub trick from `FileList.virtualize-guard.test.ts`: jsdom
 * does no real layout, so FileList's `measureGeometry()` would read all-zero rects and fall back
 * to full-list rendering (exactly the bug this suite exists to catch) unless `.filelist-pane` and
 * `.row` report realistic pixel geometry.
 *
 * **Falsifiability, proven manually (see CPE-1304 Work Log):** the windowing guard
 * (`$: virtualize = entries.length >= VIRTUALIZE_THRESHOLD` in FileList.svelte) was temporarily
 * forced to `false` — simulating a revert to `{#each entries as entry}` full-list rendering —
 * and every size above the threshold failed with the rendered count equal to N instead of the
 * budget. The change was reverted immediately after (confirmed via `git diff` returning empty)
 * before this suite was finalized; this test file's own diff never touched FileList.svelte.
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { render } from "@testing-library/svelte";
import { tick } from "svelte";
import FileList from "./FileList.svelte";
import { emptySelection } from "../selection";
import type { DirEntry } from "../types";

// The component tree imports Tauri APIs transitively; stub them so jsdom can render without a Tauri
// runtime (matches FileList.test.ts / FileList.virtualize-guard.test.ts).
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const ROW_HEIGHT = 30; // matches FileList.svelte's fallback `rowH` default
const VIEWPORT_HEIGHT = 600; // ~20 rows visible
// The rendered window is `viewportH / rowH` rows + `OVERSCAN_ROWS * 2` (FileList.svelte const = 6) on
// either side, rounded generously — the exact value doesn't matter here, only that it stays FLAT
// across every N. 100 is comfortably above the real ~32-row window and matches the sibling guard test's
// budget (`FileList.virtualize-guard.test.ts` asserts < 100 for N=5000).
const RENDERED_ROW_BUDGET = 100;

function makeEntries(n: number): DirEntry[] {
  return Array.from({ length: n }, (_, i) => ({
    name: `file-${i}.txt`,
    path: `/x/file-${i}.txt`,
    is_dir: false,
    size: 100,
    modified: Date.now(),
    extension: "txt",
    hidden: false,
    is_symlink: false,
  }));
}

const base = {
  selection: emptySelection(),
  sortKey: "name" as const,
  sortDir: "asc" as const,
  view: "details" as const,
  error: "",
  loading: false,
  searching: false,
  cutPaths: [],
  renamingPath: "",
  renameValue: "",
  rowEls: [],
  draggedPaths: [],
};

let rectSpy: ReturnType<typeof vi.spyOn> | undefined;
// Sweeping four mounts up to N=50,000 in one file: an un-disposed FileList (ResizeObserver, scroll
// listener, Svelte internals) from a PRIOR mount piles up alongside the next, larger one and exhausts
// the worker's heap. Track + `unmount()` the live instance before/after every mount so only one
// component's worth of state is ever alive at a time.
let liveUnmount: (() => void) | undefined;

afterEach(() => {
  liveUnmount?.();
  liveUnmount = undefined;
  rectSpy?.mockRestore();
  document.body.innerHTML = "";
});

/** Same stub as `FileList.virtualize-guard.test.ts`: hand `.filelist-pane`/`.row` realistic pixel
 *  geometry so FileList's real `measureGeometry()` computes non-zero `viewportH`/`rowH` and the
 *  genuine windowed path runs, instead of jsdom's all-zero rects tripping the full-list fallback. */
function stubGeometry() {
  rectSpy = vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (
    this: Element,
  ) {
    const zero = { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, x: 0, y: 0, toJSON() {} };
    if (this.classList.contains("filelist-pane")) {
      return { ...zero, bottom: VIEWPORT_HEIGHT, height: VIEWPORT_HEIGHT, width: 800 } as DOMRect;
    }
    if (this.classList.contains("row")) {
      return { ...zero, bottom: ROW_HEIGHT, height: ROW_HEIGHT, width: 800 } as DOMRect;
    }
    return zero as DOMRect;
  });
}

/** Mounts FileList with `n` synthetic entries inside a real `.filelist-pane` ancestor (FileList locates
 *  its scroller via `rowsEl.closest(".filelist-pane")`) and returns the settled rendered-row count plus
 *  the "grow to N" mount-to-settled timing in ms.
 *
 *  Mounts with a 1-entry SEED first, lets `onMount`'s `measureGeometry()` read real geometry off the
 *  stub, THEN grows to `n` — deliberately not a single `render()` with `n` entries already set. `win`
 *  (FileList.svelte) only windows once `rowH > 0 && viewportH > 0`; both start at 0/unset on a brand-new
 *  instance, so a component instantiated ALREADY holding N=50,000 entries hits the (intentional, see
 *  measureGeometry's doc comment) full-list fallback on its very first synchronous render — mounting
 *  50,000 real rows (Icon/ThumbnailImage children and all) before shrinking to the window one tick
 *  later. That doesn't reflect real usage: `ExplorerPane.loadListing` streams entries onto a pane that
 *  already exists and was already measured (STREAMING.md), and reliably exhausted the test worker's
 *  heap for N=50,000 (see CPE-1304 Work Log). The 1-entry seed mirrors that real "already-measured pane,
 *  growing" shape, so only the genuinely-windowed growth is what gets timed. */
async function mountAndMeasure(n: number): Promise<{ rows: number; ms: number }> {
  stubGeometry();
  const scrollPane = document.createElement("div");
  scrollPane.className = "filelist-pane";
  document.body.appendChild(scrollPane);

  const { rerender, unmount } = render(FileList, {
    target: scrollPane,
    props: { ...base, entries: makeEntries(1) },
  });
  liveUnmount = unmount;
  await tick();
  await tick();
  await tick(); // seed settled — viewportH/rowH are now real, off the geometry stub

  const t0 = performance.now();
  await rerender({ entries: makeEntries(n) });
  // Let the reactive `windowed`/re-measure pipeline flush through Svelte's update cycle, mirroring the
  // sibling guard test.
  await tick();
  await tick();
  await tick();
  const ms = performance.now() - t0;

  return { rows: scrollPane.querySelectorAll(".row").length, ms };
}

describe("FileList multi-size perf budget (CPE-1304, completes CPE-691, epic CPE-688)", () => {
  const sizes = [100, 1_000, 10_000, 50_000];
  const results: { n: number; rows: number; ms: number }[] = [];

  for (const n of sizes) {
    it(`keeps rendered DOM row count within budget for N=${n}`, async () => {
      const { rows, ms } = await mountAndMeasure(n);
      results.push({ n, rows, ms });

      // THE BUDGET: regardless of how many entries the folder has, the mounted `.row` count must stay
      // within a small, FLAT window — never scale with N. A regression to `{#each entries as entry}`
      // full-list rendering would make `rows === n`.
      expect(rows).toBeGreaterThan(0); // sanity: something rendered
      expect(rows).toBeLessThan(RENDERED_ROW_BUDGET);
      expect(rows).toBeLessThan(n === 100 ? n + 1 : n); // falsifiable regression check at every size
    });
  }

  it("proves the budget is FLAT across sizes — rendered rows do not scale with N", () => {
    // This assertion only makes sense once every size above has run and recorded a result.
    expect(results.length).toBe(sizes.length);
    const rowCounts = results.map((r) => r.rows);
    const min = Math.min(...rowCounts);
    const max = Math.max(...rowCounts);
    // A correctly windowed renderer mounts essentially the same row count (viewport height / row
    // height + fixed overscan) no matter the folder size — the spread across a 500× size range
    // (100 → 50,000) should be a handful of rows, not thousands. This is the assertion a full-list
    // regression would blow through (max would track N, i.e. ~50,000 vs ~30).
    expect(max - min).toBeLessThan(RENDERED_ROW_BUDGET);

    // Log mount/settle timing per size (CPE-1304 AC: "recording mount/settle timing") — not asserted
    // on as a hard ms budget (CI timing is too noisy/shared-runner-dependent for that), but visible in
    // CI/local output for a human to skim for a real regression.
    for (const r of results) {
      console.log(`[perf-budget] N=${r.n} rows=${r.rows} mount+settle=${r.ms.toFixed(1)}ms`);
    }
  });

  it("still renders every row for a small folder (below the virtualization threshold)", async () => {
    const N = 12; // well under FileList.svelte's VIRTUALIZE_THRESHOLD (100)
    const { rows } = await mountAndMeasure(N);
    // Below the threshold, FileList intentionally renders the full (small) list — mirrors
    // FileList.virtualize-guard.test.ts's "small folder pays nothing" companion assertion, kept here
    // too so this file stands alone as a complete multi-size sweep (threshold through 50k).
    expect(rows).toBe(N);
  });
});
