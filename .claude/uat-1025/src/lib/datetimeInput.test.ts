import { describe, it, expect } from "vitest";
import { msToLocalInput, localInputToMs } from "./datetimeInput";

describe("datetimeInput (CPE-786)", () => {
  it("round-trips a local wall-clock time through ms and back", () => {
    const ms = new Date(2026, 6, 20, 14, 30).getTime(); // 2026-07-20 14:30 local
    expect(msToLocalInput(ms)).toBe("2026-07-20T14:30");
    expect(localInputToMs("2026-07-20T14:30")).toBe(ms);
  });

  it("zero-pads month/day/hour/minute", () => {
    const ms = new Date(2026, 0, 3, 9, 5).getTime(); // 2026-01-03 09:05
    expect(msToLocalInput(ms)).toBe("2026-01-03T09:05");
  });

  it("treats null / invalid ms as an empty input", () => {
    expect(msToLocalInput(null)).toBe("");
    expect(msToLocalInput(undefined)).toBe("");
    expect(msToLocalInput(NaN)).toBe("");
  });

  it("parses an optional seconds field and rejects junk", () => {
    expect(localInputToMs("2026-07-20T14:30:45")).toBe(new Date(2026, 6, 20, 14, 30, 45).getTime());
    expect(localInputToMs("")).toBeNull();
    expect(localInputToMs("not a date")).toBeNull();
    expect(localInputToMs("2026-07-20")).toBeNull();
  });
});
