/**
 * CPE-1381 — test coverage for the CPE-1372 per-hovered-destination `sameVolume` cache in
 * Sidebar.svelte's `onDragOver` (PR #656 landed the behavior with zero coverage in the components
 * themselves; the pure `hoverEffect` fn in dnd.ts already had its own tests).
 *
 * Mirrors FileList.hoverSameVolume.test.ts but drives Sidebar.svelte, whose reset mechanism differs:
 * Sidebar has no `dragend` listener of its own — it piggybacks on `draggedPaths` going empty via a
 * REACTIVE statement (`$: if (draggedPaths.length === 0) { hoverVolumeDest = ""; hoverSameVolume = null; }`),
 * since `draggedPaths` is bound down from the file list that owns the actual drag gesture.
 *
 * Three behaviors under test:
 *   1. Dedup — `commands.sameVolume` fires ONCE per distinct hovered nav-item destination, not once per
 *              `dragover` tick.
 *   2. Reset — `draggedPaths` going empty (simulating the bound prop reacting to the file list's own
 *              drag end) clears the cache, so a fresh drag re-hovering the SAME destination re-queries.
 *   3. Race  — a late-resolving promise for an abandoned hover must not clobber the current hover's
 *              dropEffect.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Sidebar from "./Sidebar.svelte";
import type { Place } from "../types";

// The component tree imports Tauri APIs transitively; stub core so jsdom can render (mirrors Sidebar.test.ts).
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const { sameVolumeMock } = vi.hoisted(() => ({ sameVolumeMock: vi.fn() }));
vi.mock("../bindings.gen", () => ({
  commands: { sameVolume: sameVolumeMock, listDir: vi.fn(async () => []) },
}));

const place = (over: Partial<Place> = {}): Place => ({ name: "Folder", path: "/vol1/dirA", kind: "documents", ...over });

/** A minimal DataTransfer stub — jsdom doesn't provide one. `dropEffect` is a plain writable field so a
 *  test can read back what the handler set it to. */
function fakeDataTransfer() {
  return { setData: vi.fn(), getData: () => "", setDragImage: vi.fn(), effectAllowed: "", dropEffect: "" };
}

/** jsdom has no `DragEvent` — build a plain Event and assign the fields the handler reads. */
function fireDragOver(
  row: HTMLElement,
  dataTransfer: ReturnType<typeof fakeDataTransfer>,
  mods: { ctrlKey?: boolean; shiftKey?: boolean } = {},
) {
  const ev = new Event("dragover", { bubbles: true, cancelable: true });
  Object.assign(ev, { dataTransfer, ctrlKey: !!mods.ctrlKey, shiftKey: !!mods.shiftKey });
  return fireEvent(row, ev);
}

const flush = () => new Promise((r) => setTimeout(r, 0));

/** Makes `sameVolume` return a caller-controlled promise keyed by the hovered destination, so a test can
 *  resolve promises out of call-order — the crux of the race-guard test. */
function deferredByDest() {
  const map = new Map<string, { resolve: (v: boolean) => void }>();
  sameVolumeMock.mockImplementation((_from: string, dest: string) => {
    let resolve!: (v: boolean) => void;
    const promise = new Promise<boolean>((res) => {
      resolve = res;
    });
    map.set(dest, { resolve });
    return promise;
  });
  return map;
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("CPE-1381 Sidebar onDragOver same-volume hover cache (CPE-1372)", () => {
  it("dedup: queries sameVolume once per distinct hovered destination, not once per dragover tick", async () => {
    sameVolumeMock.mockResolvedValue(true);
    const places = [place({ path: "/vol1/dirA", name: "dirA" }), place({ path: "/vol2/dirB", name: "dirB" })];
    const { container } = render(Sidebar, {
      places,
      drives: [],
      favorites: [],
      draggedPaths: ["/src/file.txt"],
    });
    await flush();

    const rows = container.querySelectorAll(".nav-item[data-drop-path]");
    const rowA = Array.from(rows).find((r) => (r as HTMLElement).dataset.dropPath === "/vol1/dirA") as HTMLElement;
    const rowB = Array.from(rows).find((r) => (r as HTMLElement).dataset.dropPath === "/vol2/dirB") as HTMLElement;
    expect(rowA).toBeTruthy();
    expect(rowB).toBeTruthy();

    // Three dragover ticks hovering the SAME destination — must query only once.
    await fireDragOver(rowA, fakeDataTransfer());
    await fireDragOver(rowA, fakeDataTransfer());
    await fireDragOver(rowA, fakeDataTransfer());
    expect(sameVolumeMock).toHaveBeenCalledTimes(1);
    expect(sameVolumeMock).toHaveBeenCalledWith("/src/file.txt", "/vol1/dirA");

    // Hovering a DIFFERENT destination queries again, exactly once for it.
    await fireDragOver(rowB, fakeDataTransfer());
    await fireDragOver(rowB, fakeDataTransfer());
    expect(sameVolumeMock).toHaveBeenCalledTimes(2);
    expect(sameVolumeMock).toHaveBeenCalledWith("/src/file.txt", "/vol2/dirB");
    await flush();
  });

  it("reset: draggedPaths going empty clears the cache so a fresh drag over the SAME destination re-queries", async () => {
    sameVolumeMock.mockResolvedValue(true);
    const places = [place({ path: "/vol1/dirA", name: "dirA" })];
    const { container, component } = render(Sidebar, {
      places,
      drives: [],
      favorites: [],
      draggedPaths: ["/src/file.txt"],
    });
    await flush();

    const rowA = container.querySelector('.nav-item[data-drop-path="/vol1/dirA"]') as HTMLElement;
    await fireDragOver(rowA, fakeDataTransfer());
    expect(sameVolumeMock).toHaveBeenCalledTimes(1);

    // The bound draggedPaths prop goes empty (the file list's own drag ended) — Sidebar's reactive
    // statement (`$: if (draggedPaths.length === 0) ...`) must clear hoverVolumeDest/hoverSameVolume.
    component.$set({ draggedPaths: [] });
    await flush();

    // A fresh drag starts and re-hovers the exact same destination. If the cache were NOT reset,
    // hoverVolumeDest would still equal "/vol1/dirA" and this would be silently deduped away — asserting
    // call count 2 proves the reactive reset ran.
    component.$set({ draggedPaths: ["/src/file.txt"] });
    await flush();
    await fireDragOver(rowA, fakeDataTransfer());
    expect(sameVolumeMock).toHaveBeenCalledTimes(2);
  });

  it("race guard: a late-resolving promise for an abandoned hover does not clobber the current hover's dropEffect", async () => {
    const deferreds = deferredByDest();
    const places = [place({ path: "/vol1/dirA", name: "dirA" }), place({ path: "/vol2/dirB", name: "dirB" })];
    const { container } = render(Sidebar, {
      places,
      drives: [],
      favorites: [],
      draggedPaths: ["/src/file.txt"],
    });
    await flush();

    const rowA = container.querySelector('.nav-item[data-drop-path="/vol1/dirA"]') as HTMLElement;
    const rowB = container.querySelector('.nav-item[data-drop-path="/vol2/dirB"]') as HTMLElement;

    // Hover A — query fires, left pending.
    await fireDragOver(rowA, fakeDataTransfer());
    // Move to B before A resolves — a second, independent query fires for B.
    await fireDragOver(rowB, fakeDataTransfer());
    expect(sameVolumeMock).toHaveBeenCalledTimes(2);

    // B resolves FIRST: cross-volume (false) ⇒ cursor should read "copy".
    deferreds.get("/vol2/dirB")!.resolve(false);
    await flush();
    const dt1 = fakeDataTransfer();
    await fireDragOver(rowB, dt1);
    expect(dt1.dropEffect).toBe("copy");

    // A's stale promise resolves LATE with same-volume==true (which alone renders "move"). The guard must
    // ignore it because the hover has since moved to B.
    deferreds.get("/vol1/dirA")!.resolve(true);
    await flush();
    const dt2 = fakeDataTransfer();
    await fireDragOver(rowB, dt2);
    // Still deduped (no 3rd query for B) and the cursor is unaffected by A's late resolution — if the
    // guard were removed, this dropEffect would flip to "move".
    expect(sameVolumeMock).toHaveBeenCalledTimes(2);
    expect(dt2.dropEffect).toBe("copy");
  });
});
