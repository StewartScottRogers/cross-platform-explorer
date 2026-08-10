/**
 * Regression guard (CPE-691, epic CPE-688): the file list must NEVER go back to rendering one DOM
 * row per entry. CPE-690/CPE-766 gave FileList a windowed renderer (`../virtualize`'s `windowRange`)
 * that, above `VIRTUALIZE_THRESHOLD` entries, mounts only the visible slice + overscan — a folder
 * with 50,000 files costs the same DOM as one with 50. `virtualize.test.ts` already covers the pure
 * windowing math; this test proves the *component* actually uses it, by mounting FileList with a
 * large synthetic listing and asserting the number of rendered `.row` elements stays bounded and
 * independent of the entry count. If someone reverts to `{#each entries as entry}` with no windowing,
 * this test fails (rendered rows would equal N).
 *
 * jsdom performs no real layout, so FileList's on-mount geometry measurement (`measureGeometry` in
 * FileList.svelte, via `getBoundingClientRect` on the `.filelist-pane` scroll ancestor and a rendered
 * row) would otherwise read all-zero rects and viewportH would stay 0 — which is exactly the guard
 * FileList uses to fall back to full-list rendering. `Element.prototype.getBoundingClientRect` is
 * stubbed below to hand back realistic pixel geometry, exercising the real windowed path rather than
 * that fallback (CPE-690's Work Log flags this exact gap as unverified headlessly).
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { render } from "@testing-library/svelte";
import { tick } from "svelte";
import FileList from "./FileList.svelte";
import { emptySelection } from "../selection";
import { windowRange } from "../virtualize";
import type { DirEntry } from "../types";

// The component tree imports Tauri APIs transitively; stub them so jsdom can render without a Tauri
// runtime (matches FileList.test.ts).
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const ROW_HEIGHT = 30; // matches FileList.svelte's fallback `rowH` default
const VIEWPORT_HEIGHT = 600; // ~20 rows visible

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

afterEach(() => {
  rectSpy?.mockRestore();
  document.body.innerHTML = "";
});

/**
 * Stub every element's `getBoundingClientRect`: the `.filelist-pane` scroll ancestor reports a real
 * viewport height, a `.row` reports a real row height, everything else keeps jsdom's zeroed rect. This
 * is what lets FileList's real `measureGeometry()` compute non-zero `viewportH`/`rowH` and switch on
 * windowing, instead of silently falling back to rendering every entry.
 */
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

describe("FileList windowed rendering — regression guard (CPE-691, epic CPE-688)", () => {
  it("keeps rendered DOM row count bounded for 5,000 entries, independent of N", async () => {
    stubGeometry();

    // Mount FileList inside a real `.filelist-pane` ancestor — FileList locates its scroll container
    // via `rowsEl.closest(".filelist-pane")` (see FileList.svelte's wireScroller()), matching how
    // ExplorerPane wraps it at runtime.
    const scrollPane = document.createElement("div");
    scrollPane.className = "filelist-pane";
    document.body.appendChild(scrollPane);

    const N = 5000;
    render(FileList, { target: scrollPane, props: { ...base, entries: makeEntries(N) } });

    // Let onMount's measureGeometry (and the follow-up `tick().then(measureGeometry)` re-measure once
    // rowsEl exists) flush through Svelte's update cycle.
    await tick();
    await tick();
    await tick();

    const renderedRows = scrollPane.querySelectorAll(".row").length;

    // THE INVARIANT: FileList's VIRTUALIZE_THRESHOLD is 100, well below N=5000, so a correctly
    // windowed render mounts only the visible rows + overscan (a small double-digit count for a
    // 600px viewport at 30px rows) — never one row per entry. A regression to
    // `{#each entries as entry}` full-list rendering would make renderedRows === N (5000).
    expect(renderedRows).toBeGreaterThan(0); // sanity: something rendered
    expect(renderedRows).toBeLessThan(100); // bounded window, not the full list
    expect(renderedRows).toBeLessThan(N); // the falsifiable regression check
  });

  it("still renders every row for a small folder (below the virtualization threshold)", async () => {
    stubGeometry();

    const scrollPane = document.createElement("div");
    scrollPane.className = "filelist-pane";
    document.body.appendChild(scrollPane);

    const N = 12; // well under VIRTUALIZE_THRESHOLD (100)
    render(FileList, { target: scrollPane, props: { ...base, entries: makeEntries(N) } });
    await tick();
    await tick();

    // Below the threshold, FileList intentionally renders the full (small) list — this test only
    // guards against windowing kicking in where it shouldn't (a small-folder regression), mirroring
    // CPE-690's "common case pays nothing" acceptance criterion.
    expect(scrollPane.querySelectorAll(".row").length).toBe(N);
  });
});

/**
 * Compact-density windowing invariant (CPE-1527, epic CPE-1488): FileList.svelte's details/list `rowH`
 * is fixed per density (`ROW_H_COMFORTABLE = 30` / `ROW_H_COMPACT = 22`), not DOM-measured — see the
 * comment on that declaration. That means the `.filelist-pane`/`.row` geometry stub above (which always
 * reports a flat 30px `.row` height, real layout or not) has NO bearing on the details-view window these
 * tests exercise; what's under test is that the component's *own* internal `rowH` genuinely switches
 * with the `density` prop and that the CPE-690/766 windowing math (`windowRange`, imported directly
 * below — the exact function FileList.svelte's `win` reactive statement calls) is recomputed from it, not
 * silently left at the comfortable pitch.
 */
describe("FileList compact density — virtualization invariant (CPE-1527, epic CPE-1488)", () => {
  const ROW_H_COMFORTABLE = 30; // must track FileList.svelte's ROW_H_COMFORTABLE / app.css's --row-h
  const ROW_H_COMPACT = 22; // must track FileList.svelte's ROW_H_COMPACT
  const OVERSCAN_ROWS = 6; // must track FileList.svelte's OVERSCAN_ROWS
  const N = 5000;

  it("compact density renders exactly the windowRange(rowH=22) slice — more rows than comfortable for the identical viewport", async () => {
    stubGeometry();
    const scrollPane = document.createElement("div");
    scrollPane.className = "filelist-pane";
    document.body.appendChild(scrollPane);

    render(FileList, { target: scrollPane, props: { ...base, entries: makeEntries(N), density: "compact" } });
    await tick();
    await tick();
    await tick();

    const renderedRows = scrollPane.querySelectorAll(".row").length;
    const compactExpected = windowRange(0, VIEWPORT_HEIGHT, ROW_H_COMPACT, N, 1, OVERSCAN_ROWS);
    expect(renderedRows).toBe(compactExpected.end - compactExpected.start);

    // The whole point of a tighter pitch: strictly more rows fit the same 600px viewport than comfortable.
    const comfortableExpected = windowRange(0, VIEWPORT_HEIGHT, ROW_H_COMFORTABLE, N, 1, OVERSCAN_ROWS);
    expect(renderedRows).toBeGreaterThan(comfortableExpected.end - comfortableExpected.start);
  });

  it("comfortable density renders exactly the windowRange(rowH=30) slice — no regression from CPE-1527", async () => {
    stubGeometry();
    const scrollPane = document.createElement("div");
    scrollPane.className = "filelist-pane";
    document.body.appendChild(scrollPane);

    render(FileList, { target: scrollPane, props: { ...base, entries: makeEntries(N), density: "comfortable" } });
    await tick();
    await tick();
    await tick();

    const renderedRows = scrollPane.querySelectorAll(".row").length;
    const expected = windowRange(0, VIEWPORT_HEIGHT, ROW_H_COMFORTABLE, N, 1, OVERSCAN_ROWS);
    expect(renderedRows).toBe(expected.end - expected.start);
  });

  it("bottom spacer height (padBottom) is computed from the density's own rowH — proves the pitch drives the windowing math itself, not just the visible row count", async () => {
    stubGeometry();
    const scrollPane = document.createElement("div");
    scrollPane.className = "filelist-pane";
    document.body.appendChild(scrollPane);

    render(FileList, { target: scrollPane, props: { ...base, entries: makeEntries(N), density: "compact" } });
    await tick();
    await tick();
    await tick();

    const spacers = scrollPane.querySelectorAll<HTMLElement>(".vspacer");
    const bottomSpacer = spacers[spacers.length - 1];
    const expected = windowRange(0, VIEWPORT_HEIGHT, ROW_H_COMPACT, N, 1, OVERSCAN_ROWS);
    expect(bottomSpacer.style.height).toBe(`${expected.padBottom}px`);
    // Sanity: that padBottom is NOT what the comfortable pitch would have produced (would silently pass
    // a broken "always comfortable" implementation otherwise).
    const comfortableExpected = windowRange(0, VIEWPORT_HEIGHT, ROW_H_COMFORTABLE, N, 1, OVERSCAN_ROWS);
    expect(expected.padBottom).not.toBe(comfortableExpected.padBottom);
  });
});
