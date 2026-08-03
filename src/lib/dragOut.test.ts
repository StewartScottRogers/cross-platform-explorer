// CPE-1264: dragOut.ts is pure plumbing (nothing calls it yet — see the module header). These tests cover
// the wrapper's own contract: paths -> item mapping, icon resolution (default vs override), mode
// passthrough, the onEvent bridge, and — the whole point of wrapping the plugin — that it degrades to a
// clear "unavailable" result instead of throwing when there's no Tauri IPC bridge (mirrors how
// invoke.test.ts mocks `@tauri-apps/api/core`).
//
// CPE-1269: added a `@tauri-apps/api/path` mock so `resolveDragIcon()`'s success/failure/caching paths
// have direct unit coverage, plus tests asserting the hardening itself — a failed resolution must never
// reach the plugin as a relative `icon`. Every test gets a FRESH module instance (`vi.resetModules()` +
// dynamic `import("./dragOut")` in `beforeEach`) because `resolveDragIcon` caches its result in
// module-level state; without a reset, whichever test runs first to resolve successfully would poison the
// cache for every test after it.
import { describe, it, expect, beforeEach, vi } from "vitest";

const { startDragMock } = vi.hoisted(() => ({ startDragMock: vi.fn() }));
vi.mock("@crabnebula/tauri-plugin-drag", () => ({
  startDrag: startDragMock,
}));

const { resolveResourceMock } = vi.hoisted(() => ({ resolveResourceMock: vi.fn() }));
vi.mock("@tauri-apps/api/path", () => ({
  resolveResource: resolveResourceMock,
}));

function setTauriBridge(present: boolean) {
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  if (present) {
    w.__TAURI_INTERNALS__ = {};
  } else {
    delete w.__TAURI_INTERNALS__;
  }
}

let startFileDrag: typeof import("./dragOut").startFileDrag;
let resolveDragIcon: typeof import("./dragOut").resolveDragIcon;
let DEFAULT_DRAG_ICON: typeof import("./dragOut").DEFAULT_DRAG_ICON;
let isTauriEnv: typeof import("./dragOut").isTauriEnv;

beforeEach(async () => {
  vi.clearAllMocks();
  vi.resetModules();
  setTauriBridge(false);
  // Default: no Tauri bridge, so resolveResource rejects like it would outside a real webview — matches
  // the pre-existing tests below that never configure it explicitly. Tests that care about the success
  // path override this with `resolveResourceMock.mockResolvedValueOnce(...)`.
  resolveResourceMock.mockRejectedValue(new Error("no Tauri IPC bridge"));

  const mod = await import("./dragOut");
  startFileDrag = mod.startFileDrag;
  resolveDragIcon = mod.resolveDragIcon;
  DEFAULT_DRAG_ICON = mod.DEFAULT_DRAG_ICON;
  isTauriEnv = mod.isTauriEnv;
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

describe("resolveDragIcon (CPE-1269)", () => {
  it("resolves DEFAULT_DRAG_ICON to an absolute path via resolveResource and returns it", async () => {
    resolveResourceMock.mockResolvedValueOnce("/abs/resources/icons/icon.png");

    const result = await resolveDragIcon();

    expect(resolveResourceMock).toHaveBeenCalledWith(DEFAULT_DRAG_ICON);
    expect(result).toBe("/abs/resources/icons/icon.png");
  });

  it("caches a successful resolution — a second call doesn't hit resolveResource again", async () => {
    resolveResourceMock.mockResolvedValueOnce("/abs/resources/icons/icon.png");

    const first = await resolveDragIcon();
    const second = await resolveDragIcon();

    expect(first).toBe("/abs/resources/icons/icon.png");
    expect(second).toBe("/abs/resources/icons/icon.png");
    expect(resolveResourceMock).toHaveBeenCalledTimes(1);
  });

  it("returns null (never the relative DEFAULT_DRAG_ICON) when resolveResource rejects", async () => {
    resolveResourceMock.mockRejectedValueOnce(new Error("resource not found"));

    const result = await resolveDragIcon();

    expect(result).toBeNull();
    expect(result).not.toBe(DEFAULT_DRAG_ICON);
  });

  it("returns null when resolveResource resolves to an empty string", async () => {
    resolveResourceMock.mockResolvedValueOnce("");

    const result = await resolveDragIcon();

    expect(result).toBeNull();
  });

  it("does not cache a failed resolution — a later successful call still resolves (cache-only-on-success)", async () => {
    resolveResourceMock.mockRejectedValueOnce(new Error("transient failure"));
    resolveResourceMock.mockResolvedValueOnce("/abs/resources/icons/icon.png");

    const failed = await resolveDragIcon();
    const succeeded = await resolveDragIcon();

    expect(failed).toBeNull();
    expect(succeeded).toBe("/abs/resources/icons/icon.png");
    expect(resolveResourceMock).toHaveBeenCalledTimes(2);
  });
});

describe("startFileDrag icon resolution (CPE-1264/CPE-1269)", () => {
  it("uses the caller-supplied icon override", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"], { icon: "/tmp/preview.png" });

    const [options] = startDragMock.mock.calls[0];
    expect(options.icon).toBe("/tmp/preview.png");
  });

  it("resolves the bundled default to an absolute path when no icon is given and resolution succeeds", async () => {
    setTauriBridge(true);
    resolveResourceMock.mockResolvedValueOnce("/abs/resources/icons/icon.png");
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"]);

    const [options] = startDragMock.mock.calls[0];
    expect(options.icon).toBe("/abs/resources/icons/icon.png");
  });

  it("CPE-1269: omits icon (never sends the relative DEFAULT_DRAG_ICON) when no icon is given and resolution fails", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"]);

    const [options] = startDragMock.mock.calls[0];
    expect(options.icon).toBeUndefined();
    expect(options.icon).not.toBe(DEFAULT_DRAG_ICON);
  });

  it("CPE-1269: treats an empty-string icon override as absent and falls through to resolution (still no relative icon on failure)", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValueOnce(undefined);

    await startFileDrag(["/a/one.txt"], { icon: "" });

    const [options] = startDragMock.mock.calls[0];
    expect(options.icon).toBeUndefined();
    expect(options.icon).not.toBe(DEFAULT_DRAG_ICON);
  });

  it("CPE-1269: never passes a non-absolute icon to the plugin across a resolved-then-failed-then-resolved sequence", async () => {
    setTauriBridge(true);
    startDragMock.mockResolvedValue(undefined);

    // 1) Resolution fails outright — must omit icon, never the relative default.
    await startFileDrag(["/a/one.txt"]);
    const first = startDragMock.mock.calls[0][0];
    expect(first.icon === undefined || isAbsolute(first.icon)).toBe(true);

    // 2) A later successful resolution — icon must be the absolute resolved path.
    resolveResourceMock.mockResolvedValueOnce("/abs/resources/icons/icon.png");
    await startFileDrag(["/a/two.txt"]);
    const second = startDragMock.mock.calls[1][0];
    expect(second.icon).toBe("/abs/resources/icons/icon.png");
    expect(isAbsolute(second.icon)).toBe(true);
  });
});

/** Minimal cross-platform absolute-path check for test assertions only (POSIX `/...` or Windows drive
 *  letter `C:\...` / `C:/...`). */
function isAbsolute(p: string): boolean {
  return /^\//.test(p) || /^[A-Za-z]:[\\/]/.test(p);
}

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
