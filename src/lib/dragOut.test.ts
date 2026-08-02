// CPE-1264: dragOut.ts is pure plumbing (nothing calls it yet — see the module header). These tests cover
// the wrapper's own contract: paths -> item mapping, icon resolution (default vs override), mode
// passthrough, the onEvent bridge, and — the whole point of wrapping the plugin — that it degrades to a
// clear "unavailable" result instead of throwing when there's no Tauri IPC bridge (mirrors how
// invoke.test.ts mocks `@tauri-apps/api/core`).
import { describe, it, expect, beforeEach, vi } from "vitest";

const { startDragMock } = vi.hoisted(() => ({ startDragMock: vi.fn() }));
vi.mock("@crabnebula/tauri-plugin-drag", () => ({
  startDrag: startDragMock,
}));

import { startFileDrag, DEFAULT_DRAG_ICON, isTauriEnv } from "./dragOut";

function setTauriBridge(present: boolean) {
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  if (present) {
    w.__TAURI_INTERNALS__ = {};
  } else {
    delete w.__TAURI_INTERNALS__;
  }
}

beforeEach(() => {
  vi.clearAllMocks();
  setTauriBridge(false);
});

describe("isTauriEnv (CPE-1264)", () => {
  it("is false in a plain jsdom test env (no __TAURI_INTERNALS__)", () => {
    expect(isTauriEnv()).toBe(false);
  });

  it("is true once the Tauri bridge global is present", () => {
    setTauriBridge(true);
    expect(isTauriEnv()).toBe(true);
  });
});

describe("startFileDrag param mapping (CPE-1264)", () => {
  it("maps paths to the plugin's `item` array, preserving order", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt", "/a/two.txt"]);

    expect(startDragMock).toHaveBeenCalledTimes(1);
    const [options] = startDragMock.mock.calls[0];
    expect(options.item).toEqual(["/a/one.txt", "/a/two.txt"]);
  });

  it("does not mutate the caller's paths array", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);
    const paths = ["/a/one.txt"];

    await startFileDrag(paths);
    const [options] = startDragMock.mock.calls[0];
    options.item.push("/mutated");

    expect(paths).toEqual(["/a/one.txt"]);
  });
});

describe("startFileDrag icon resolution (CPE-1264)", () => {
  it("falls back to DEFAULT_DRAG_ICON when no icon is given", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"]);

    const [options] = startDragMock.mock.calls[0];
    expect(options.icon).toBe(DEFAULT_DRAG_ICON);
    expect(options.icon.length).toBeGreaterThan(0);
  });

  it("uses the caller-supplied icon override", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"], { icon: "/tmp/preview.png" });

    const [options] = startDragMock.mock.calls[0];
    expect(options.icon).toBe("/tmp/preview.png");
  });

  it("treats an empty-string icon override as absent and falls back to the default", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"], { icon: "" });

    const [options] = startDragMock.mock.calls[0];
    expect(options.icon).toBe(DEFAULT_DRAG_ICON);
  });
});

describe("startFileDrag mode passthrough (CPE-1264)", () => {
  it("passes mode: 'copy' through unchanged", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"], { mode: "copy" });

    const [options] = startDragMock.mock.calls[0];
    expect(options.mode).toBe("copy");
  });

  it("passes mode: 'move' through unchanged", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"], { mode: "move" });

    const [options] = startDragMock.mock.calls[0];
    expect(options.mode).toBe("move");
  });

  it("leaves mode undefined when the caller doesn't specify one", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"]);

    const [options] = startDragMock.mock.calls[0];
    expect(options.mode).toBeUndefined();
  });
});

describe("startFileDrag onEvent bridge (CPE-1264)", () => {
  it("forwards the plugin's Dropped/Cancelled payload to opts.onEvent", async () => {
    setTauriBridge(true);
    startDragMock.mockImplementationOnce(async (_options, onEvent) => {
      onEvent?.({ result: "Dropped", cursorPos: { x: 12, y: 34 } });
    });
    const onEvent = vi.fn();

    await startFileDrag(["/a/one.txt"], { onEvent });

    expect(onEvent).toHaveBeenCalledWith({ result: "Dropped", cursorPos: { x: 12, y: 34 } });
  });

  it("never calls onEvent when the drag never starts (unavailable)", async () => {
    const onEvent = vi.fn();

    await startFileDrag([], { onEvent });

    expect(onEvent).not.toHaveBeenCalled();
    expect(startDragMock).not.toHaveBeenCalled();
  });
});

describe("startFileDrag graceful degradation (CPE-1264)", () => {
  it("resolves to unavailable/no-paths for an empty selection without calling the plugin", async () => {
    setTauriBridge(true);

    const result = await startFileDrag([]);

    expect(result).toEqual({ status: "unavailable", reason: "no-paths" });
    expect(startDragMock).not.toHaveBeenCalled();
  });

  it("resolves to unavailable/plugin-unavailable (never throws) when there's no Tauri bridge", async () => {
    setTauriBridge(false);
    startDragMock.mockRejectedValueOnce(new Error("__TAURI_INTERNALS__ is not defined"));

    const result = await startFileDrag(["/a/one.txt"]);

    expect(result).toEqual({ status: "unavailable", reason: "plugin-unavailable" });
  });

  it("resolves to status: error (still no throw) when the bridge exists but the plugin call rejects", async () => {
    setTauriBridge(true);
    const boom = new Error("OS refused the drag");
    startDragMock.mockRejectedValueOnce(boom);

    const result = await startFileDrag(["/a/one.txt"]);

    expect(result).toEqual({ status: "error", error: boom });
  });

  it("resolves to status: ok on a normal successful call", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    const result = await startFileDrag(["/a/one.txt"]);

    expect(result).toEqual({ status: "ok" });
  });
});
