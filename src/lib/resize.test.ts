import { describe, it, expect } from "vitest";
import { clampWidth } from "./resize";

describe("clampWidth", () => {
  it("returns the value when within range", () => {
    expect(clampWidth(300, 160, 480)).toBe(300);
  });
  it("clamps to the minimum (safe floor)", () => {
    expect(clampWidth(50, 160, 480)).toBe(160);
  });
  it("clamps to the maximum", () => {
    expect(clampWidth(9999, 160, 480)).toBe(480);
  });
});
