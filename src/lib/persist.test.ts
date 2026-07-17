import { describe, it, expect, beforeEach } from "vitest";
import { lsGet, lsSet, lsBool } from "./persist";

beforeEach(() => {
  try { localStorage.clear(); } catch { /* ignore */ }
});

describe("persist helpers", () => {
  it("round-trips a string; missing → null", () => {
    expect(lsGet("k")).toBeNull();
    lsSet("k", "v");
    expect(lsGet("k")).toBe("v");
  });

  it("lsBool reads '1'/'0' and falls back when absent", () => {
    expect(lsBool("b", true)).toBe(true); // absent → fallback
    expect(lsBool("b", false)).toBe(false);
    lsSet("b", "1");
    expect(lsBool("b", false)).toBe(true);
    lsSet("b", "0");
    expect(lsBool("b", true)).toBe(false);
  });
});
