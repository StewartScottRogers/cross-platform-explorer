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
  selectIndices,
  invertSelection,
  pickActivePane,
  snapshotConfirmTarget,
} from "./selection";
import { pageDelta } from "./gridnav";

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

  it("builds a selection from explicit indices", () => {
    const s = selectIndices([3, 1, 4]);
    expect(selectedIndices(s)).toEqual([1, 3, 4]);
    expect(s.anchor).toBe(1);
    expect(s.lead).toBe(4);
  });

  it("selectIndices ignores negatives and empties cleanly", () => {
    expect(selectedCount(selectIndices([-1, -2]))).toBe(0);
    expect(selectIndices([]).anchor).toBe(-1);
  });

  it("inverts the selection across the visible rows", () => {
    const s = selectIndices([0, 2]); // of 5 rows
    const inv = invertSelection(s, 5);
    expect(selectedIndices(inv)).toEqual([1, 3, 4]);
  });

  it("inverting an empty selection selects everything", () => {
    const inv = invertSelection(emptySelection(), 3);
    expect(selectedIndices(inv)).toEqual([0, 1, 2]);
  });

  it("inverting a full selection selects nothing", () => {
    const inv = invertSelection(selectAll(4), 4);
    expect(selectedCount(inv)).toBe(0);
  });

  it("handles a very large index array without a stack overflow (CPE-696)", () => {
    // Regression: Math.min(...clean) threw RangeError on big folders; both "Select all of this type"
    // and "Invert selection" feed ~N indices through selectIndices.
    const n = 200_000;
    const idx = Array.from({ length: n }, (_, i) => i);
    let s!: ReturnType<typeof selectIndices>;
    expect(() => (s = selectIndices(idx))).not.toThrow();
    expect(s.anchor).toBe(0);
    expect(s.lead).toBe(n - 1);
    expect(selectedCount(s)).toBe(n);
  });

  it("inverts across a large folder without throwing (CPE-696)", () => {
    const inv = invertSelection(selectIndices([0, 5]), 150_000);
    expect(selectedCount(inv)).toBe(150_000 - 2);
    expect(inv.anchor).toBe(1); // lowest not-previously-selected index
    expect(inv.lead).toBe(150_000 - 1);
  });

  // ---- CPE-1370: dual-pane active-pane routing ----

  describe("pickActivePane (CPE-1370)", () => {
    it("single-pane (dualPane off) always resolves to pane A, whatever activePane says", () => {
      expect(pickActivePane(false, 0, "A", "B")).toBe("A");
      expect(pickActivePane(false, 1, "A", "B")).toBe("A");
    });

    it("dual-pane with pane A active resolves to pane A", () => {
      expect(pickActivePane(true, 0, "A", "B")).toBe("A");
    });

    it("dual-pane with pane B active resolves to pane B", () => {
      expect(pickActivePane(true, 1, "A", "B")).toBe("B");
    });

    it("works over selection-shaped state, not just primitives", () => {
      const selA = selectOnly(2);
      const selB = selectOnly(7);
      expect(pickActivePane(true, 1, selA, selB)).toBe(selB);
      expect(pickActivePane(true, 0, selA, selB)).toBe(selA);
    });
  });

  // ---- CPE-1370 review: a confirm-gated action's target is frozen at confirm-open time, so a later
  // ---- pane switch (while the dialog is open) can't retarget an already-confirmed delete (data loss).

  describe("snapshotConfirmTarget (CPE-1370 review — delete-target snapshot)", () => {
    it("captures inPaneB and the paths of the given entries", () => {
      const entries = [{ path: "/b/one.txt" }, { path: "/b/two.txt" }];
      const target = snapshotConfirmTarget(true, entries);
      expect(target).toEqual({ inPaneB: true, paths: ["/b/one.txt", "/b/two.txt"] });
    });

    it("captures pane A the same way", () => {
      const target = snapshotConfirmTarget(false, [{ path: "/a/one.txt" }]);
      expect(target).toEqual({ inPaneB: false, paths: ["/a/one.txt"] });
    });

    it("empty selection snapshots to an empty paths array", () => {
      expect(snapshotConfirmTarget(true, [])).toEqual({ inPaneB: true, paths: [] });
    });

    // The actual data-loss bug: a confirm dialog stays open while `activePane` can still change
    // underneath it. Prove the snapshot is immune to that by mutating the SOURCE data the caller would
    // naturally still be holding a reference to, after the snapshot was taken — the snapshot must not
    // observe it (it copies the paths array, not the pane's live `selectedEntries` reference).
    it("is a frozen copy — mutating the source entries afterward doesn't change the snapshot", () => {
      const liveSelectedEntries = [{ path: "/b/bravo.txt" }];
      const target = snapshotConfirmTarget(true, liveSelectedEntries);

      // Simulate "activePane flipped and pane A's selection is now what `selectedEntries` points at" —
      // i.e. exactly what would happen between askDelete capturing pane B and a user Tab-ing to pane A
      // before clicking "confirm", if doDelete were (wrongly) reading live state instead of `target`.
      liveSelectedEntries.push({ path: "/b/should-not-appear.txt" });
      liveSelectedEntries[0].path = "/a/alpha.txt"; // even an in-place mutation of the first entry

      expect(target).toEqual({ inPaneB: true, paths: ["/b/bravo.txt"] });
    });

    it("two snapshots from the same pane don't alias each other's paths array", () => {
      const entries = [{ path: "/b/bravo.txt" }];
      const t1 = snapshotConfirmTarget(true, entries);
      const t2 = snapshotConfirmTarget(true, entries);
      t1.paths.push("/b/mutated-in-t1.txt");
      expect(t2.paths).toEqual(["/b/bravo.txt"]); // t2 unaffected by mutating t1's array
    });
  });

  // ---- CPE-1373: bulk selections keep the lead (and scroll position) put ----

  describe("bulk selections don't yank the lead to a far row (CPE-1373)", () => {
    it("selectAll keeps the current lead when one is given", () => {
      const s = selectAll(1000, 3);
      expect(s.lead).toBe(3); // NOT 999
      expect(selectedCount(s)).toBe(1000); // selection itself is still everything
    });

    it("selectAll falls back to the last row when no lead is given (back-compat)", () => {
      expect(selectAll(1000).lead).toBe(999);
    });

    it("selectAll falls back to the last row when the given lead is out of range", () => {
      expect(selectAll(10, 50).lead).toBe(9);
      expect(selectAll(10, -1).lead).toBe(9);
    });

    it("selectIndices keeps the given lead instead of the max selected index", () => {
      const s = selectIndices([1, 3, 4, 9], 3);
      expect(s.lead).toBe(3); // NOT 9
      expect(selectedIndices(s)).toEqual([1, 3, 4, 9]); // selection unaffected
    });

    it("selectIndices without a keepLead still defaults to the max index (back-compat)", () => {
      expect(selectIndices([1, 3, 4, 9]).lead).toBe(9);
    });

    it("invertSelection (Invert / Select-all-of-type's underlying call) keeps the given lead", () => {
      // Top of a 1000-row folder, row 0 selected/led — Ctrl+Alt+I / Invert flips everything else on
      // but shouldn't yank the viewport to row 999.
      const before = selectOnly(0);
      const inv = invertSelection(before, 1000, before.lead);
      expect(inv.lead).toBe(0); // NOT 999
      expect(selectedCount(inv)).toBe(999); // rows 1..999 — row 0 stays excluded
    });

    it("invertSelection without a keepLead still defaults to the max index (back-compat, CPE-696)", () => {
      const inv = invertSelection(selectIndices([0, 5]), 150_000);
      expect(inv.lead).toBe(150_000 - 1);
    });

    it("a keepLead outside the inverted set is still honoured — lead tracks focus, not membership", () => {
      // Row 2 was selected (so it's excluded from the inversion) but was also the lead; the lead should
      // stay on row 2 rather than jumping to whatever the new selection's max index is.
      const before = selectOnly(2);
      const inv = invertSelection(before, 5, before.lead);
      expect(selectedIndices(inv)).toEqual([0, 1, 3, 4]); // row 2 correctly excluded
      expect(inv.lead).toBe(2); // but the lead stays put
    });
  });

  // ---- CPE-1374: PageUp / PageDown move the lead by a viewport, grid-aware ----

  describe("PageUp/PageDown lead movement via moveLead + pageDelta (CPE-1374)", () => {
    it("PageDown in a single-column list moves the lead by a full page", () => {
      let s = selectOnly(0);
      s = moveLead(s, pageDelta("PageDown", 1, 10), 100, false);
      expect(selectedIndices(s)).toEqual([10]);
    });

    it("PageUp in a single-column list moves the lead back by a full page", () => {
      let s = selectOnly(50);
      s = moveLead(s, pageDelta("PageUp", 1, 10), 100, false);
      expect(selectedIndices(s)).toEqual([40]);
    });

    it("PageUp at the top of the list clamps to row 0", () => {
      let s = selectOnly(3);
      s = moveLead(s, pageDelta("PageUp", 1, 10), 100, false);
      expect(selectedIndices(s)).toEqual([0]);
    });

    it("PageDown at the bottom of the list clamps to the last row", () => {
      let s = selectOnly(95);
      s = moveLead(s, pageDelta("PageDown", 1, 10), 100, false);
      expect(selectedIndices(s)).toEqual([99]);
    });

    it("is grid-aware: a page in a 4-column grid moves by rowsPerPage * cols", () => {
      let s = selectOnly(0);
      s = moveLead(s, pageDelta("PageDown", 4, 3), 100, false); // 3 rows of 4 cols = 12 tiles
      expect(selectedIndices(s)).toEqual([12]);
    });

    it("Shift+PageDown extends the selection from the anchor, same as Shift+Arrow", () => {
      let s = selectOnly(5);
      s = moveLead(s, pageDelta("PageDown", 1, 10), 100, true);
      expect(s.anchor).toBe(5);
      expect(s.lead).toBe(15);
      expect(selectedIndices(s)).toEqual(Array.from({ length: 11 }, (_, i) => i + 5)); // 5..15 inclusive
    });

    it("Shift+PageUp extends upward from the anchor and clamps at row 0", () => {
      let s = selectOnly(4);
      s = moveLead(s, pageDelta("PageUp", 1, 10), 100, true);
      expect(s.anchor).toBe(4);
      expect(s.lead).toBe(0);
      expect(selectedIndices(s)).toEqual([0, 1, 2, 3, 4]);
    });
  });
});
