import { describe, it, expect } from "vitest";
import { applyNavIntent } from "./navMotion";
import { click, emptySelection, selectOnly, type Selection } from "./selection";
import type { NavIntent } from "./navMode";

function m(dir: "left" | "down" | "up" | "right" | "top" | "bottom", count = 1): NavIntent {
  return { kind: "motion", dir, count };
}

describe("applyNavIntent — normal mode, list layout", () => {
  const itemCount = 10;

  it("down moves the sole selection to the next index", () => {
    const sel = selectOnly(4);
    const next = applyNavIntent(m("down"), sel, itemCount, "normal", "list");
    expect([...next.indices]).toEqual([5]);
    expect(next.lead).toBe(5);
    expect(next.anchor).toBe(5);
  });

  it("up moves the sole selection to the previous index", () => {
    const sel = selectOnly(4);
    const next = applyNavIntent(m("up"), sel, itemCount, "normal", "list");
    expect([...next.indices]).toEqual([3]);
  });

  it("left/right are inert in list layout (no horizontal axis)", () => {
    const sel = selectOnly(4);
    expect([...applyNavIntent(m("left"), sel, itemCount, "normal", "list").indices]).toEqual([4]);
    expect([...applyNavIntent(m("right"), sel, itemCount, "normal", "list").indices]).toEqual([4]);
  });

  it("a count multiplies the single-step delta", () => {
    const sel = selectOnly(0);
    const next = applyNavIntent(m("down", 3), sel, itemCount, "normal", "list");
    expect([...next.indices]).toEqual([3]);
  });

  it("clamps at the top boundary instead of wrapping or throwing", () => {
    const sel = selectOnly(0);
    const next = applyNavIntent(m("up"), sel, itemCount, "normal", "list");
    expect([...next.indices]).toEqual([0]);
  });

  it("clamps at the bottom boundary instead of wrapping or throwing", () => {
    const sel = selectOnly(itemCount - 1);
    const next = applyNavIntent(m("down"), sel, itemCount, "normal", "list");
    expect([...next.indices]).toEqual([itemCount - 1]);
  });

  it("a large count clamps to the last index in one call", () => {
    const sel = selectOnly(0);
    const next = applyNavIntent(m("down", 999), sel, itemCount, "normal", "list");
    expect([...next.indices]).toEqual([itemCount - 1]);
  });

  it("top jumps straight to index 0 regardless of the current lead", () => {
    const sel = selectOnly(7);
    const next = applyNavIntent(m("top"), sel, itemCount, "normal", "list");
    expect([...next.indices]).toEqual([0]);
    expect(next.anchor).toBe(0);
    expect(next.lead).toBe(0);
  });

  it("bottom jumps straight to the last index regardless of the current lead", () => {
    const sel = selectOnly(2);
    const next = applyNavIntent(m("bottom"), sel, itemCount, "normal", "list");
    expect([...next.indices]).toEqual([itemCount - 1]);
  });

  it("a count on top/bottom does not change the absolute-jump target", () => {
    const sel = selectOnly(2);
    const next = applyNavIntent(m("top", 5), sel, itemCount, "normal", "list");
    expect([...next.indices]).toEqual([0]);
  });

  it("moving down from an empty selection lands on the first item, not the second", () => {
    const next = applyNavIntent(m("down"), emptySelection(), itemCount, "normal", "list");
    expect([...next.indices]).toEqual([0]);
  });

  it("moving up from an empty selection stays clamped at the first item", () => {
    const next = applyNavIntent(m("up"), emptySelection(), itemCount, "normal", "list");
    expect([...next.indices]).toEqual([0]);
  });
});

describe("applyNavIntent — normal mode, grid layout", () => {
  const itemCount = 12;
  const columns = 4;

  it("down moves a whole row (+columns)", () => {
    const sel = selectOnly(1);
    const next = applyNavIntent(m("down"), sel, itemCount, "normal", "grid", columns);
    expect([...next.indices]).toEqual([5]);
  });

  it("up moves a whole row (-columns)", () => {
    const sel = selectOnly(5);
    const next = applyNavIntent(m("up"), sel, itemCount, "normal", "grid", columns);
    expect([...next.indices]).toEqual([1]);
  });

  it("right moves one tile", () => {
    const sel = selectOnly(5);
    const next = applyNavIntent(m("right"), sel, itemCount, "normal", "grid", columns);
    expect([...next.indices]).toEqual([6]);
  });

  it("left moves one tile", () => {
    const sel = selectOnly(5);
    const next = applyNavIntent(m("left"), sel, itemCount, "normal", "grid", columns);
    expect([...next.indices]).toEqual([4]);
  });

  it("left from a row's first column lands on the previous row's last column (gridnav's flat-index wrap)", () => {
    const sel = selectOnly(4); // row 1, col 0
    const next = applyNavIntent(m("left"), sel, itemCount, "normal", "grid", columns);
    expect([...next.indices]).toEqual([3]); // row 0, col 3
  });

  it("right from a row's last column lands on the next row's first column", () => {
    const sel = selectOnly(3); // row 0, col 3
    const next = applyNavIntent(m("right"), sel, itemCount, "normal", "grid", columns);
    expect([...next.indices]).toEqual([4]); // row 1, col 0
  });
});

describe("applyNavIntent — visual mode extends the range from the anchor", () => {
  const itemCount = 10;

  it("down extends the range forward, keeping the original anchor", () => {
    const sel: Selection = click(emptySelection(), 2); // anchor=2, lead=2
    const next = applyNavIntent(m("down"), sel, itemCount, "visual", "list");
    expect(next.anchor).toBe(2);
    expect(next.lead).toBe(3);
    expect([...next.indices].sort((a, b) => a - b)).toEqual([2, 3]);
  });

  it("repeated down motions keep growing the range from the same anchor", () => {
    let sel: Selection = click(emptySelection(), 2);
    sel = applyNavIntent(m("down"), sel, itemCount, "visual", "list");
    sel = applyNavIntent(m("down"), sel, itemCount, "visual", "list");
    expect(sel.anchor).toBe(2);
    expect(sel.lead).toBe(4);
    expect([...sel.indices].sort((a, b) => a - b)).toEqual([2, 3, 4]);
  });

  it("up extends the range backward from the anchor", () => {
    const sel: Selection = click(emptySelection(), 5);
    const next = applyNavIntent(m("up"), sel, itemCount, "visual", "list");
    expect(next.anchor).toBe(5);
    expect(next.lead).toBe(4);
    expect([...next.indices].sort((a, b) => a - b)).toEqual([4, 5]);
  });

  it("a count extends the range by several rows in one call", () => {
    const sel: Selection = click(emptySelection(), 0);
    const next = applyNavIntent(m("down", 3), sel, itemCount, "visual", "list");
    expect(next.anchor).toBe(0);
    expect(next.lead).toBe(3);
    expect([...next.indices].sort((a, b) => a - b)).toEqual([0, 1, 2, 3]);
  });

  it("top extends the range up to index 0 from the anchor", () => {
    const sel: Selection = click(emptySelection(), 5);
    const next = applyNavIntent(m("top"), sel, itemCount, "visual", "list");
    expect(next.anchor).toBe(5);
    expect(next.lead).toBe(0);
    expect([...next.indices].sort((a, b) => a - b)).toEqual([0, 1, 2, 3, 4, 5]);
  });

  it("bottom extends the range down to the last index from the anchor", () => {
    const sel: Selection = click(emptySelection(), 7);
    const next = applyNavIntent(m("bottom"), sel, itemCount, "visual", "list");
    expect(next.anchor).toBe(7);
    expect(next.lead).toBe(itemCount - 1);
    expect([...next.indices].sort((a, b) => a - b)).toEqual([7, 8, 9]);
  });

  it("clamps at the boundary in visual mode too, instead of wrapping", () => {
    const sel: Selection = click(emptySelection(), 0);
    const next = applyNavIntent(m("up"), sel, itemCount, "visual", "list");
    expect(next.anchor).toBe(0);
    expect(next.lead).toBe(0);
    expect([...next.indices]).toEqual([0]);
  });
});

describe("applyNavIntent — out-of-scope input is a no-op", () => {
  it("itemCount === 0 returns the same selection reference unchanged", () => {
    const sel = emptySelection();
    const next = applyNavIntent(m("down"), sel, 0, "normal", "list");
    expect(next).toBe(sel);
  });

  it("a non-motion intent returns the same selection reference unchanged", () => {
    const sel = selectOnly(3);
    const nonMotionIntents: NavIntent[] = [
      { kind: "none" },
      { kind: "enterVisual" },
      { kind: "exitVisual" },
      { kind: "op", op: "yank" },
      { kind: "op", op: "delete" },
      { kind: "op", op: "paste" },
      { kind: "startFilter" },
      { kind: "startCommand" },
    ];
    for (const intent of nonMotionIntents) {
      expect(applyNavIntent(intent, sel, 10, "normal", "list")).toBe(sel);
      expect(applyNavIntent(intent, sel, 10, "visual", "list")).toBe(sel);
    }
  });
});
