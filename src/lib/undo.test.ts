import { describe, it, expect } from "vitest";
import { pushUndo, popUndo, canUndo, peekLabel, invert } from "./undo";
import type { UndoEntry } from "./undo";

const ren = (from: string, to: string): UndoEntry => ({
  kind: "rename",
  moves: [{ from, to }],
  label: `Rename to ${to}`,
});

describe("undo stack", () => {
  it("starts unable to undo", () => {
    expect(canUndo([])).toBe(false);
    expect(peekLabel([])).toBe("");
  });

  it("pushes the newest entry to the front", () => {
    let s: UndoEntry[] = [];
    s = pushUndo(s, ren("/a", "/b"));
    s = pushUndo(s, ren("/c", "/d"));
    expect(peekLabel(s)).toBe("Rename to /d");
    expect(canUndo(s)).toBe(true);
  });

  it("pops the most recent entry", () => {
    let s: UndoEntry[] = [];
    s = pushUndo(s, ren("/a", "/b"));
    s = pushUndo(s, ren("/c", "/d"));

    const { entry, rest } = popUndo(s);
    expect(entry?.moves[0].to).toBe("/d");
    expect(rest).toHaveLength(1);
    expect(peekLabel(rest)).toBe("Rename to /b");
  });

  it("popping an empty stack is safe", () => {
    const { entry, rest } = popUndo([]);
    expect(entry).toBeNull();
    expect(rest).toEqual([]);
  });

  it("is bounded so it cannot grow without limit", () => {
    let s: UndoEntry[] = [];
    for (let i = 0; i < 60; i++) s = pushUndo(s, ren(`/a${i}`, `/b${i}`));
    expect(s).toHaveLength(25);
    expect(peekLabel(s)).toBe("Rename to /b59"); // newest kept
  });

  it("inverts an entry by swapping from and to", () => {
    const e: UndoEntry = {
      kind: "move",
      moves: [
        { from: "/src/1.txt", to: "/dst/1.txt" },
        { from: "/src/2.txt", to: "/dst/2.txt" },
      ],
      label: "Move 2 items",
    };
    expect(invert(e)).toEqual([
      { from: "/dst/1.txt", to: "/src/1.txt" },
      { from: "/dst/2.txt", to: "/src/2.txt" },
    ]);
  });
});
