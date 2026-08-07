/**
 * CPE-1381 — test coverage for the CPE-1372 per-hovered-destination `sameVolume` cache in
 * FileList.svelte's `onDragOver` (PR #656 landed the behavior with zero coverage in the components
 * themselves; the pure `hoverEffect` fn in dnd.ts already had its own tests).
 *
 * Three behaviors under test, all read straight from FileList.svelte's onDragOver/onDragEnd:
 *   1. Dedup    — `commands.sameVolume` fires ONCE per distinct hovered destination, not once per
 *                 `dragover` tick (dest !== hoverVolumeDest guards the call).
 *   2. Reset    — `onDragEnd` (wired to the row's `dragend`) clears `hoverVolumeDest`/`hoverSameVolume`,
 *                 so a fresh drag re-queries the very same destination instead of silently reusing a
 *                 leftover cached value.
 *   3. Race     — a late-resolving promise for a destination that's no longer hovered must NOT overwrite
 *                 the current hover's cached value (`if (hoverVolumeDest === dest) hoverSameVolume = ...`
 *                 guard). Driven by controlling resolution ORDER independently of call order.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import FileList from "./FileList.svelte";
import { emptySelection } from "../selection";
import type { DirEntry } from "../types";

// The component tree imports Tauri APIs transitively; stub core so jsdom can render (mirrors FileList.test.ts).
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

const { sameVolumeMock, startFileDragMock, isTauriEnvMock, resolveDragIconMock } = vi.hoisted(() => ({
  sameVolumeMock: vi.fn(),
  startFileDragMock: vi.fn(
    async (_paths: string[], _opts: { icon?: string; mode?: "copy" | "move" }) => ({ status: "ok" as const }),
  ),
  isTauriEnvMock: vi.fn(() => true),
  resolveDragIconMock: vi.fn(async () => "/abs/resources/icons/icon.png"),
}));
vi.mock("../dragOut", () => ({
  startFileDrag: startFileDragMock,
  isTauriEnv: isTauriEnvMock,
  resolveDragIcon: resolveDragIconMock,
  DEFAULT_DRAG_ICON: "icons/icon.png",
}));
vi.mock("../bindings.gen", () => ({
  commands: { sameVolume: sameVolumeMock, extractArchiveEntryAny: vi.fn(), listDir: vi.fn() },
}));

const dir = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "dir",
  path: "/dir",
  is_dir: true,
  size: 0,
  modified: Date.now(),
  extension: "",
  hidden: false,
  is_symlink: false,
  ...over,
});

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
};

/** A minimal DataTransfer stub — jsdom doesn't provide one. `dropEffect` is a plain writable field so a
 *  test can read back what the handler set it to. */
function fakeDataTransfer() {
  return { setData: vi.fn(), getData: () => "", setDragImage: vi.fn(), effectAllowed: "", dropEffect: "" };
}

/** jsdom has no `DragEvent` — build a plain Event and assign the fields the handler reads, same technique
 *  as FileList.dragout.test.ts's `fireDragStart`. */
function fireDragOver(
  row: HTMLElement,
  dataTransfer: ReturnType<typeof fakeDataTransfer>,
  mods: { ctrlKey?: boolean; shiftKey?: boolean } = {},
) {
  const ev = new Event("dragover", { bubbles: true, cancelable: true });
  Object.assign(ev, { dataTransfer, ctrlKey: !!mods.ctrlKey, shiftKey: !!mods.shiftKey });
  return fireEvent(row, ev);
}

function fireDragEnd(row: HTMLElement) {
  const ev = new Event("dragend", { bubbles: true, cancelable: true });
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
  isTauriEnvMock.mockReturnValue(true);
  resolveDragIconMock.mockResolvedValue("/abs/resources/icons/icon.png");
});

describe("CPE-1381 FileList onDragOver same-volume hover cache (CPE-1372)", () => {
  it("dedup: queries sameVolume once per distinct hovered destination, not once per dragover tick", async () => {
    sameVolumeMock.mockResolvedValue(true);
    const entries = [dir({ path: "/vol1/dirA", name: "dirA" }), dir({ path: "/vol2/dirB", name: "dirB" })];
    const { container } = render(FileList, { ...base, entries, draggedPaths: ["/src/file.txt"] });
    await flush();

    const rows = container.querySelectorAll(".row");
    const rowA = rows[0] as HTMLElement;
    const rowB = rows[1] as HTMLElement;

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

  it("reset: dragend clears the cache so a fresh drag hovering the SAME destination re-queries", async () => {
    sameVolumeMock.mockResolvedValue(true);
    const entries = [dir({ path: "/vol1/dirA", name: "dirA" })];
    const { container, component } = render(FileList, { ...base, entries, draggedPaths: ["/src/file.txt"] });
    await flush();

    const rowA = container.querySelector(".row") as HTMLElement;
    await fireDragOver(rowA, fakeDataTransfer());
    expect(sameVolumeMock).toHaveBeenCalledTimes(1);

    // Drag ends on this row — onDragEnd resets hoverVolumeDest/hoverSameVolume (and draggedPaths) internally.
    await fireDragEnd(rowA);

    // A fresh drag starts (the parent hands draggedPaths back down, as App.svelte's binding would) and
    // re-hovers the exact same destination. If the cache were NOT reset, hoverVolumeDest would still equal
    // "/vol1/dirA" and this would be silently deduped away — asserting call count 2 proves the reset ran.
    component.$set({ draggedPaths: ["/src/file.txt"] });
    await flush();
    await fireDragOver(rowA, fakeDataTransfer());
    expect(sameVolumeMock).toHaveBeenCalledTimes(2);
  });

  it("race guard: a late-resolving promise for an abandoned hover does not clobber the current hover's dropEffect", async () => {
    const deferreds = deferredByDest();
    const entries = [dir({ path: "/vol1/dirA", name: "dirA" }), dir({ path: "/vol2/dirB", name: "dirB" })];
    const { container } = render(FileList, { ...base, entries, draggedPaths: ["/src/file.txt"] });
    await flush();

    const rows = container.querySelectorAll(".row");
    const rowA = rows[0] as HTMLElement;
    const rowB = rows[1] as HTMLElement;

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

    // A's stale promise resolves LATE with same-volume==true (which alone renders "move"). The guard
    // (`if (hoverVolumeDest === dest) hoverSameVolume = same`) must ignore it because the hover has since
    // moved to B — this is exactly what CPE-1372's inline comment calls out.
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
