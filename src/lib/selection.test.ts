import { describe, it, expect } from "vitest";
import {
  emptySelection,
  click,
  selectAll,
  selectOnly,
  moveLead,
  selectedIndices,
  selectedCount,
  isSelected,
  remapByPath,
} from "./selection";

describe("selection", () => {
  it("starts empty", () => {
    const s = emptySelection();
    expect(selectedCount(s)).toBe(0);
    expect(s.anchor).toBe(-1);
  });

  it("plain click selects exactly one item and sets the anchor", () => {
    let s = click(emptySelection(), 3);
    expect(selectedIndices(s)).toEqual([3]);
    expect(s.anchor).toBe(3);

    s = click(s, 5);
    expect(selectedIndices(s)).toEqual([5]);
  });

  it("ctrl+click toggles items in and out", () => {
    let s = click(emptySelection(), 1);
    s = click(s, 3, { ctrl: true });
    s = click(s, 5, { ctrl: true });
    expect(selectedIndices(s)).toEqual([1, 3, 5]);

    s = click(s, 3, { ctrl: true }); // toggle off
    expect(selectedIndices(s)).toEqual([1, 5]);
    expect(isSelected(s, 3)).toBe(false);
  });

  it("shift+click selects the contiguous range from the anchor", () => {
    let s = click(emptySelection(), 2);
    s = click(s, 5, { shift: true });
    expect(selectedIndices(s)).toEqual([2, 3, 4, 5]);
  });

  it("shift+click works backwards too", () => {
    let s = click(emptySelection(), 5);
    s = click(s, 2, { shift: true });
    expect(selectedIndices(s)).toEqual([2, 3, 4, 5]);
  });

  it("shift+click replaces the previous range rather than accumulating", () => {
    let s = click(emptySelection(), 2);
    s = click(s, 5, { shift: true });
    s = click(s, 3, { shift: true }); // re-drag from the same anchor
    expect(selectedIndices(s)).toEqual([2, 3]);
  });

  it("ctrl+shift+click extends the existing selection with a range", () => {
    let s = click(emptySelection(), 0);
    s = click(s, 4, { ctrl: true }); // anchor moves to 4
    s = click(s, 6, { ctrl: true, shift: true });
    expect(selectedIndices(s)).toEqual([0, 4, 5, 6]);
  });

  it("selects all and clears", () => {
    const s = selectAll(4);
    expect(selectedIndices(s)).toEqual([0, 1, 2, 3]);
    expect(selectedCount(emptySelection())).toBe(0);
  });

  it("selectAll on an empty list stays empty", () => {
    expect(selectedCount(selectAll(0))).toBe(0);
  });

  it("moveLead walks the list and clamps at both ends", () => {
    let s = selectOnly(0);
    s = moveLead(s, 1, 3);
    expect(selectedIndices(s)).toEqual([1]);

    s = moveLead(s, 5, 3); // past the end
    expect(selectedIndices(s)).toEqual([2]);

    s = moveLead(s, -99, 3); // past the start
    expect(selectedIndices(s)).toEqual([0]);
  });

  it("shift+arrow extends the selection from the anchor", () => {
    let s = selectOnly(1);
    s = moveLead(s, 1, 5, true);
    s = moveLead(s, 1, 5, true);
    expect(selectedIndices(s)).toEqual([1, 2, 3]);
  });

  it("moveLead on an empty list yields an empty selection", () => {
    expect(selectedCount(moveLead(emptySelection(), 1, 0))).toBe(0);
  });

  it("remaps the selection by path after a re-sort", () => {
    // "b" and "c" were selected; the list is then reversed.
    const s = remapByPath(
      ["/b", "/c"],
      [{ path: "/c" }, { path: "/b" }, { path: "/a" }],
    );
    expect(selectedIndices(s)).toEqual([0, 1]);
  });

  it("drops paths that no longer exist when remapping", () => {
    const s = remapByPath(["/gone", "/a"], [{ path: "/a" }, { path: "/b" }]);
    expect(selectedIndices(s)).toEqual([0]);
  });

  it("remapping with nothing left yields an empty selection", () => {
    const s = remapByPath(["/gone"], [{ path: "/a" }]);
    expect(selectedCount(s)).toBe(0);
    expect(s.anchor).toBe(-1);
  });
});
