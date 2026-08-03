/**
 * CPE-674 — extract-on-drag-out: Alt-dragging a row OUT of an open archive view (CPE-673's read-only,
 * synthetic-path rows) stages the entry to a real temp file first, then native-drags the temp path(s), so
 * the drop actually lands somewhere as a real file. Reuses the CPE-672 native-drag wrapper + the existing
 * `extractArchiveEntryAny` staging command — this only adds the archive-row wiring in `FileList.svelte`.
 *
 * Mirrors `FileList.dragout.test.ts`'s structure/helpers.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import FileList from "./FileList.svelte";
import { emptySelection, selectOnly, selectIndices } from "../selection";
import type { DirEntry } from "../types";

// The component tree imports Tauri APIs transitively; stub core so jsdom can render (mirrors FileList.test.ts).
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

const { startFileDragMock, isTauriEnvMock, resolveDragIconMock, extractArchiveEntryAnyMock } = vi.hoisted(() => ({
  startFileDragMock: vi.fn(
    async (_paths: string[], _opts: { icon?: string; mode?: "copy" | "move" }) => ({ status: "ok" as const }),
  ),
  isTauriEnvMock: vi.fn(() => true),
  resolveDragIconMock: vi.fn(async () => "/abs/resources/icons/icon.png"),
  extractArchiveEntryAnyMock: vi.fn(
    async (_zip: string, inner: string): Promise<{ status: "ok" | "error"; data?: string; error?: unknown }> => ({
      status: "ok",
      data: `/tmp/cpe-archive/${inner.split("/").pop()}`,
    }),
  ),
}));
vi.mock("../dragOut", () => ({
  startFileDrag: startFileDragMock,
  isTauriEnv: isTauriEnvMock,
  resolveDragIcon: resolveDragIconMock,
  DEFAULT_DRAG_ICON: "icons/icon.png",
}));
vi.mock("../bindings.gen", () => ({
  commands: { extractArchiveEntryAny: extractArchiveEntryAnyMock },
}));

const entry = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "a.txt",
  path: "a.txt",
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
  // Archive rows are read-only, exactly as ExplorerPane/App wire them (`canDrag={!archive}`).
  canDrag: false,
};

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

const flush = () => new Promise((r) => setTimeout(r, 0));

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
  extractArchiveEntryAnyMock.mockImplementation(async (_zip: string, inner: string) => ({
    status: "ok" as const,
    data: `/tmp/cpe-archive/${inner.split("/").pop()}`,
  }));
});

describe("CPE-674 archive extract-on-drag-out (Alt-drag discriminator)", () => {
  it("Alt-drag extracts the entry to a temp file, then native-drags the temp path", async () => {
    const entries = [entry({ path: "sub/a.txt", name: "a.txt" })];
    const { container } = render(FileList, {
      ...base,
      entries,
      selection: selectOnly(0),
      archivePath: "/archives/things.zip",
    });
    await flush();

    const row = container.querySelectorAll(".row")[0] as HTMLElement;
    const dt = fakeDataTransfer();
    await fireDragStart(row, { dataTransfer: dt, altKey: true });
    await flush();

    expect(extractArchiveEntryAnyMock).toHaveBeenCalledWith("/archives/things.zip", "sub/a.txt");
    expect(startFileDragMock).toHaveBeenCalledTimes(1);
    const [paths, opts] = startFileDragMock.mock.calls[0];
    expect(paths).toEqual(["/tmp/cpe-archive/a.txt"]);
    expect(opts.icon).toBe("/abs/resources/icons/icon.png");
    // The archive branch never touches the HTML5 internal drag payload.
    expect(dt.setData).not.toHaveBeenCalled();
  });

  it("multi-select Alt-drag extracts every selected file entry and drags them together, skipping directories", async () => {
    const entries = [
      entry({ path: "a.txt", name: "a.txt" }),
      entry({ path: "sub", name: "sub", is_dir: true }),
      entry({ path: "b.txt", name: "b.txt" }),
    ];
    const { container } = render(FileList, {
      ...base,
      entries,
      selection: selectIndices([0, 1, 2]),
      archivePath: "/archives/things.zip",
    });
    await flush();

    const row = container.querySelectorAll(".row")[0] as HTMLElement;
    await fireDragStart(row, { dataTransfer: fakeDataTransfer(), altKey: true });
    await flush();

    // Directory "sub" is never asked to extract.
    expect(extractArchiveEntryAnyMock).toHaveBeenCalledTimes(2);
    expect(extractArchiveEntryAnyMock).toHaveBeenCalledWith("/archives/things.zip", "a.txt");
    expect(extractArchiveEntryAnyMock).toHaveBeenCalledWith("/archives/things.zip", "b.txt");
    const [paths] = startFileDragMock.mock.calls[0];
    expect(paths).toEqual(["/tmp/cpe-archive/a.txt", "/tmp/cpe-archive/b.txt"]);
  });

  it("skips an entry that fails to extract but still drags the ones that succeeded", async () => {
    extractArchiveEntryAnyMock.mockImplementation(async (_zip: string, inner: string) => {
      if (inner === "bad.txt") return { status: "error" as const, error: "entry not found: bad.txt" };
      return { status: "ok" as const, data: `/tmp/cpe-archive/${inner}`, error: undefined };
    });
    const entries = [entry({ path: "bad.txt", name: "bad.txt" }), entry({ path: "good.txt", name: "good.txt" })];
    const { container } = render(FileList, {
      ...base,
      entries,
      selection: selectIndices([0, 1]),
      archivePath: "/archives/things.zip",
    });
    await flush();

    const row = container.querySelectorAll(".row")[0] as HTMLElement;
    await fireDragStart(row, { dataTransfer: fakeDataTransfer(), altKey: true });
    await flush();

    expect(startFileDragMock).toHaveBeenCalledTimes(1);
    const [paths] = startFileDragMock.mock.calls[0];
    expect(paths).toEqual(["/tmp/cpe-archive/good.txt"]);
  });

  it("never starts a native drag when every selected entry fails to extract", async () => {
    extractArchiveEntryAnyMock.mockResolvedValue({ status: "error" as const, error: "boom", data: undefined });
    const { container } = render(FileList, {
      ...base,
      entries: [entry({ path: "bad.txt", name: "bad.txt" })],
      selection: selectOnly(0),
      archivePath: "/archives/things.zip",
    });
    await flush();

    const row = container.querySelector(".row") as HTMLElement;
    await fireDragStart(row, { dataTransfer: fakeDataTransfer(), altKey: true });
    await flush();

    expect(startFileDragMock).not.toHaveBeenCalled();
  });
});

describe("CPE-674 plain drag inside an archive view stays a no-op (non-negotiable constraint)", () => {
  it("a PLAIN (no-Alt) drag on an archive row never extracts and never starts any drag, internal or native", async () => {
    const { container } = render(FileList, {
      ...base,
      entries: [entry({ path: "a.txt" })],
      selection: selectOnly(0),
      archivePath: "/archives/things.zip",
    });
    await flush();

    const row = container.querySelector(".row") as HTMLElement;
    const dt = fakeDataTransfer();
    await fireDragStart(row, { dataTransfer: dt, altKey: false });
    await flush();

    expect(extractArchiveEntryAnyMock).not.toHaveBeenCalled();
    expect(startFileDragMock).not.toHaveBeenCalled();
    expect(dt.setData).not.toHaveBeenCalled();
  });
});

describe("CPE-674 graceful gating (no Tauri bridge → no-op, no error)", () => {
  it("Alt-drag on an archive row OUTSIDE a Tauri webview does nothing (no extraction, no crash)", async () => {
    isTauriEnvMock.mockReturnValue(false);
    const { container } = render(FileList, {
      ...base,
      entries: [entry({ path: "a.txt" })],
      selection: selectOnly(0),
      archivePath: "/archives/things.zip",
    });
    await flush();

    const row = container.querySelector(".row") as HTMLElement;
    const dt = fakeDataTransfer();
    await fireDragStart(row, { dataTransfer: dt, altKey: true });
    await flush();

    expect(extractArchiveEntryAnyMock).not.toHaveBeenCalled();
    expect(startFileDragMock).not.toHaveBeenCalled();
    expect(dt.setData).not.toHaveBeenCalled();
  });
});
