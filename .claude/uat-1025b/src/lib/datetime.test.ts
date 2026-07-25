import { describe, it, expect } from "vitest";
import { formatDate } from "./datetime";

// Build a local-time timestamp so the assertions don't depend on the TZ the
// tests happen to run in (CI is UTC, the dev machine is not).
const at = (y: number, m: number, d: number, h: number, min: number) =>
  new Date(y, m - 1, d, h, min).getTime();

describe("formatDate", () => {
  it("returns empty string when there is no timestamp", () => {
    expect(formatDate(null)).toBe("");
  });

  it("formats an afternoon time with PM", () => {
    expect(formatDate(at(2026, 7, 10, 15, 0))).toBe("7/10/2026 3:00 PM");
  });

  it("formats a morning time with AM and zero-padded minutes", () => {
    expect(formatDate(at(2026, 7, 8, 8, 28))).toBe("7/8/2026 8:28 AM");
  });

  it("renders midnight as 12 AM, not 0 AM", () => {
    expect(formatDate(at(2026, 1, 1, 0, 5))).toBe("1/1/2026 12:05 AM");
  });

  it("renders noon as 12 PM, not 0 PM", () => {
    expect(formatDate(at(2026, 1, 1, 12, 0))).toBe("1/1/2026 12:00 PM");
  });

  it("returns empty string for an invalid timestamp", () => {
    expect(formatDate(NaN)).toBe("");
  });
});
