// CPE-548: the busy-tracking invoke wrapper raises/clears the cursor around a Tauri call, passes args
// through, and propagates errors — while rawInvoke stays untracked.
import { describe, it, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";

// Hoisted so the mock factory can reference it (vi.mock is hoisted above imports).
const { coreInvoke } = vi.hoisted(() => ({ coreInvoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: coreInvoke }));

import { invoke, rawInvoke } from "./invoke";
import { busy, _resetBusy, SHOW_AFTER_MS } from "./busy";

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
  _resetBusy();
});

describe("busy-tracking invoke wrapper (CPE-548)", () => {
  it("raises the busy cursor for a long call and clears it on resolve", async () => {
    vi.useFakeTimers();
    const d = deferred<string>();
    coreInvoke.mockReturnValueOnce(d.promise);

    const p = invoke<string>("list_dir", { path: "/" });
    vi.advanceTimersByTime(SHOW_AFTER_MS);
    expect(get(busy)).toBe(true); // outlived the debounce → cursor shown

    d.resolve("ok");
    vi.useRealTimers();
    await expect(p).resolves.toBe("ok");
    expect(get(busy)).toBe(false); // cleared on resolve
  });

  it("clears the busy cursor and propagates the error when the call rejects", async () => {
    coreInvoke.mockRejectedValueOnce(new Error("boom"));
    await expect(invoke("bad_cmd")).rejects.toThrow("boom");
    expect(get(busy)).toBe(false); // released even on error — no stuck cursor
    expect(document.body.classList.contains("busy")).toBe(false);
  });

  it("passes the command + args through and returns the core result", async () => {
    coreInvoke.mockResolvedValueOnce(42);
    const v = await invoke<number>("answer", { q: 1 });
    expect(v).toBe(42);
    expect(coreInvoke).toHaveBeenCalledWith("answer", { q: 1 });
  });

  it("does not flash for a call faster than the debounce", async () => {
    coreInvoke.mockResolvedValueOnce("fast");
    await invoke("quick"); // resolves well under SHOW_AFTER_MS
    expect(get(busy)).toBe(false);
  });

  it("rawInvoke runs the command WITHOUT touching the busy cursor", async () => {
    vi.useFakeTimers();
    const d = deferred<string>();
    coreInvoke.mockReturnValueOnce(d.promise);

    const p = rawInvoke<string>("stream_logs");
    vi.advanceTimersByTime(SHOW_AFTER_MS * 2); // well past the debounce
    expect(get(busy)).toBe(false); // opt-out: never shown

    d.resolve("x");
    vi.useRealTimers();
    await p;
    expect(get(busy)).toBe(false);
  });
});
