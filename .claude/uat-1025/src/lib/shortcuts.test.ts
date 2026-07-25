import { describe, it, expect } from "vitest";
import { SHORTCUT_GROUPS } from "./shortcuts";

describe("SHORTCUT_GROUPS (CPE-339)", () => {
  it("has groups, each with a title and at least one item", () => {
    expect(SHORTCUT_GROUPS.length).toBeGreaterThan(0);
    for (const g of SHORTCUT_GROUPS) {
      expect(g.title.trim().length).toBeGreaterThan(0);
      expect(g.items.length).toBeGreaterThan(0);
    }
  });

  it("every entry has non-empty keys and description", () => {
    for (const g of SHORTCUT_GROUPS) {
      for (const s of g.items) {
        expect(s.keys.trim().length).toBeGreaterThan(0);
        expect(s.description.trim().length).toBeGreaterThan(0);
      }
    }
  });

  it("documents the marquee bindings the app actually honours", () => {
    const all = SHORTCUT_GROUPS.flatMap((g) => g.items.map((s) => s.keys));
    for (const key of ["F1", "F2", "F5", "Ctrl+C", "Ctrl+V", "Ctrl+L", "Alt+Enter"]) {
      expect(all).toContain(key);
    }
  });
});
