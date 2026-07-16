// CPE-482: busy tracker — ref-counting, debounce, error-safe release, body class toggle.
import { describe, it, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";
import { busy, beginBusy, withBusy, _resetBusy, SHOW_AFTER_MS } from "./busy";

beforeEach(() => {
  vi.useRealTimers();
  _resetBusy();
});

describe("busy tracker (CPE-482)", () => {
  it("shows only after the debounce, and clears when the last op ends", () => {
    vi.useFakeTimers();
    const end = beginBusy();
    expect(get(busy)).toBe(false); // not yet — under the debounce
    vi.advanceTimersByTime(SHOW_AFTER_MS);
    expect(get(busy)).toBe(true);
    expect(document.body.classList.contains("busy")).toBe(true);
    end();
    expect(get(busy)).toBe(false);
    expect(document.body.classList.contains("busy")).toBe(false);
  });

  it("does NOT flash for an operation shorter than the debounce", () => {
    vi.useFakeTimers();
    const end = beginBusy();
    vi.advanceTimersByTime(SHOW_AFTER_MS - 10);
    end(); // finished before the threshold
    vi.advanceTimersByTime(50);
    expect(get(busy)).toBe(false);
    expect(document.body.classList.contains("busy")).toBe(false);
  });

  it("ref-counts nested/concurrent operations (clears only on the last)", () => {
    vi.useFakeTimers();
    const a = beginBusy();
    const b = beginBusy();
    vi.advanceTimersByTime(SHOW_AFTER_MS);
    expect(get(busy)).toBe(true);
    a();
    expect(get(busy)).toBe(true); // b still running
    b();
    expect(get(busy)).toBe(false);
  });

  it("end() is idempotent — double-release doesn't underflow the count", () => {
    vi.useFakeTimers();
    const a = beginBusy();
    const b = beginBusy();
    vi.advanceTimersByTime(SHOW_AFTER_MS);
    a();
    a(); // no effect
    expect(get(busy)).toBe(true); // b still holds it
    b();
    expect(get(busy)).toBe(false);
  });

  it("withBusy clears the cursor even when the operation rejects", async () => {
    await expect(
      withBusy(async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");
    expect(get(busy)).toBe(false);
    expect(document.body.classList.contains("busy")).toBe(false);
  });

  it("withBusy returns the operation's value", async () => {
    const v = await withBusy(async () => 42);
    expect(v).toBe(42);
  });
});
