/**
 * CPE-672 — native OS drag-OUT wiring + its coexistence with the existing HTML5 internal drag.
 *
 * The hard constraint (ticket + CLAUDE.md): drag-out must be **additive** — a plain drag still sets the
 * HTML5 DataTransfer payload (dnd.ts `setDragData`) that internal folder/sidebar drops rely on, and never
 * calls the native plugin. Drag-out is opt-in via a discriminator: **holding Alt** while starting the
 * drag, and only inside a Tauri webview. These tests lock all of that in headlessly (the actual drop into
 * Explorer/Finder is an attended step — see the ticket's Work Log).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import FileList from "./FileList.svelte";
import { emptySelection, selectOnly, selectIndices } from "../selection";
import type { DirEntry } from "../types";

// The component tree imports Tauri APIs transitively; stub core so jsdom can render (mirrors FileList.test.ts).
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

// Spy on the drag-out wrapper so we can assert exactly what a row hands the OS, and drive the env gate +
// icon resolution deterministically.
const { startFileDragMock, isTauriEnvMock, resolveDragIconMock } = vi.hoisted(() => ({
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

const entry = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "a.txt",
  path: "/x/a.txt",
  is_dir: false,
  size: 10,
  modified: Date.now(),
  extension: "txt",
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
  draggedPaths: [],
};

/** A minimal DataTransfer stub — jsdom doesn't provide one. `setData` is spied so a test can prove the
 *  HTML5 internal drag payload was (or wasn't) written. */
function fakeDataTransfer() {
  const store: Record<string, string> = {};
  return {
    setData: vi.fn((k: string, v: string) => {
      store[k] = v;
    }),
    getData: (k: string) => store[k] ?? "",
    setDragImage: vi.fn(),
    effectAllowed: "",
    _store: store,
  };
}

/** Flush pending microtasks so onMount's `resolveDragIcon().then(...)` has populated `dragOutIcon`. */
const flush = () => new Promise((r) => setTimeout(r, 0));

/** Dispatch a `dragstart` carrying modifier keys + a DataTransfer. jsdom has no `DragEvent`, and
 *  fireEvent's init doesn't propagate `altKey`/`dataTransfer` onto the fallback Event — so build one by
 *  hand and assign the fields the handler reads (`e.altKey`, `e.shiftKey`, `e.ctrlKey`, `e.dataTransfer`). */
function fireDragStart(
  row: HTMLElement,
  opts: { altKey?: boolean; shiftKey?: boolean; ctrlKey?: boolean; dataTransfer?: unknown } = {},
) {
  const ev = new Event("dragstart", { bubbles: true, cancelable: true });
  Object.assign(ev, {
    altKey: !!opts.altKey,
    shiftKey: !!opts.shiftKey,
    ctrlKey: !!opts.ctrlKey,
    dataTransfer: opts.dataTransfer ?? null,
  });
  return fireEvent(row, ev);
}

beforeEach(() => {
  vi.clearAllMocks();
  isTauriEnvMock.mockReturnValue(true);
  resolveDragIconMock.mockResolvedValue("/abs/resources/icons/icon.png");
});

describe("CPE-672 native drag-out (Alt-drag discriminator)", () => {
  it("Alt-drag starts a native OS drag with the selection's absolute paths + resolved absolute icon", async () => {
    const entries = [entry({ path: "/x/a.txt", name: "a.txt" }), entry({ path: "/x/b.txt", name: "b.txt" })];
    const { container } = render(FileList, { ...base, entries, selection: selectOnly(0) });
    await flush(); // let the mount-time icon pre-warm land

    const row = container.querySelectorAll(".row")[0] as HTMLElement;
    const dt = fakeDataTransfer();
    await fireDragStart(row, { dataTransfer: dt, altKey: true });

    expect(startFileDragMock).toHaveBeenCalledTimes(1);
    const [paths, opts] = startFileDragMock.mock.calls[0];
    expect(paths).toEqual(["/x/a.txt"]);
    expect(opts.icon).toBe("/abs/resources/icons/icon.png");
    expect(opts.icon!.startsWith("/")).toBe(true); // absolute, not a bare "icons/icon.png"
    // Discriminator suppresses the HTML5 internal drag so the two never run at once.
    expect(dt.setData).not.toHaveBeenCalled();
  });

  it("Alt-drag carries the whole multi-selection when the grabbed row is part of it", async () => {
    const entries = [
      entry({ path: "/x/a.txt", name: "a.txt" }),
      entry({ path: "/x/b.txt", name: "b.txt" }),
      entry({ path: "/x/c.txt", name: "c.txt" }),
    ];
    const { container } = render(FileList, { ...base, entries, selection: selectIndices([0, 2]) });
    await flush();

    const row = container.querySelectorAll(".row")[0] as HTMLElement;
    await fireDragStart(row, { dataTransfer: fakeDataTransfer(), altKey: true });

    const [paths] = startFileDragMock.mock.calls[0];
    expect(paths).toEqual(["/x/a.txt", "/x/c.txt"]);
  });

  it("Alt-drag with no Ctrl/Shift resolves mode to copy (safe default — never removes the source)", async () => {
    const { container } = render(FileList, { ...base, entries: [entry()], selection: selectOnly(0) });
    await flush();
    const row = container.querySelector(".row") as HTMLElement;
    await fireDragStart(row, { dataTransfer: fakeDataTransfer(), altKey: true });
    expect(startFileDragMock.mock.calls[0][1].mode).toBe("copy");
  });

  it("Alt+Shift-drag resolves mode to move (OS Ctrl=copy/Shift=move convention, shared with internal drops)", async () => {
    const { container } = render(FileList, { ...base, entries: [entry()], selection: selectOnly(0) });
    await flush();
    const row = container.querySelector(".row") as HTMLElement;
    await fireDragStart(row, { dataTransfer: fakeDataTransfer(), altKey: true, shiftKey: true });
    expect(startFileDragMock.mock.calls[0][1].mode).toBe("move");
  });
});

describe("CPE-672 internal drag preserved (regression guard — the non-negotiable constraint)", () => {
  it("a PLAIN drag still writes the HTML5 DataTransfer payload and never touches the native plugin", async () => {
    // Even inside a Tauri webview, a drag WITHOUT Alt must stay 100% internal.
    isTauriEnvMock.mockReturnValue(true);
    const { container } = render(FileList, { ...base, entries: [entry()], selection: selectOnly(0) });
    await flush();

    const row = container.querySelector(".row") as HTMLElement;
    const dt = fakeDataTransfer();
    await fireDragStart(row, { dataTransfer: dt, altKey: false });

    expect(dt.setData).toHaveBeenCalledWith("text/plain", "/x/a.txt");
    expect(startFileDragMock).not.toHaveBeenCalled();
  });
});

describe("CPE-672 graceful gating (no Tauri / read-only rows → internal only, no error)", () => {
  it("Alt-drag OUTSIDE a Tauri webview falls through to the HTML5 internal drag (no native call)", async () => {
    isTauriEnvMock.mockReturnValue(false);
    const { container } = render(FileList, { ...base, entries: [entry()], selection: selectOnly(0) });
    await flush();

    const row = container.querySelector(".row") as HTMLElement;
    const dt = fakeDataTransfer();
    await fireDragStart(row, { dataTransfer: dt, altKey: true });

    expect(startFileDragMock).not.toHaveBeenCalled();
    expect(dt.setData).toHaveBeenCalledWith("text/plain", "/x/a.txt");
  });

  it("Alt-drag on a read-only row (canDrag=false, e.g. an open archive) never starts a native drag", async () => {
    const { container } = render(FileList, {
      ...base,
      entries: [entry()],
      selection: selectOnly(0),
      canDrag: false,
    });
    await flush();

    const row = container.querySelector(".row") as HTMLElement;
    await fireDragStart(row, { dataTransfer: fakeDataTransfer(), altKey: true });

    expect(startFileDragMock).not.toHaveBeenCalled();
  });
});
