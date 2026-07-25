import { describe, it, expect } from "vitest";
import { parseSections, isOpen, toggled } from "./sidebarSections";

describe("sidebarSections (CPE-675)", () => {
  it("isOpen defaults to open when a section is unset", () => {
    expect(isOpen({}, "drives")).toBe(true);
    expect(isOpen({ drives: false }, "drives")).toBe(false);
    expect(isOpen({ drives: true }, "drives")).toBe(true);
  });

  it("toggled flips a section (unset → collapsed → open)", () => {
    expect(toggled({}, "explore")).toEqual({ explore: false }); // open by default → collapse
    expect(toggled({ explore: false }, "explore")).toEqual({ explore: true });
    expect(toggled({ explore: true }, "explore")).toEqual({ explore: false });
    // Other sections are untouched.
    expect(toggled({ drives: false }, "explore")).toEqual({ drives: false, explore: false });
  });

  it("parseSections tolerates malformed input and non-booleans", () => {
    expect(parseSections(null)).toEqual({});
    expect(parseSections("not json")).toEqual({});
    expect(parseSections("[]")).toEqual({});
    expect(parseSections('{"a":true,"b":"nope","c":false}')).toEqual({ a: true, c: false });
  });
});
