import { describe, it, expect } from "vitest";
import { lineDiff } from "./lineDiff";

const ops = (o: string, n: string) => lineDiff(o, n).rows.map((r) => `${r.op[0]}:${r.text}`);

describe("lineDiff (CPE-779)", () => {
  it("marks a modified middle line as del then add, keeping the surroundings same", () => {
    const r = lineDiff("a\nb\nc", "a\nx\nc");
    expect(r.rows.map((x) => `${x.op}:${x.text}`)).toEqual(["same:a", "del:b", "add:x", "same:c"]);
    expect(r.added).toBe(1);
    expect(r.removed).toBe(1);
  });

  it("is all-same for identical text (no adds/removes)", () => {
    const r = lineDiff("one\ntwo\nthree", "one\ntwo\nthree");
    expect(r.added).toBe(0);
    expect(r.removed).toBe(0);
    expect(r.rows.every((x) => x.op === "same")).toBe(true);
  });

  it("handles pure insertion and pure deletion", () => {
    expect(ops("a\nc", "a\nb\nc")).toEqual(["s:a", "a:b", "s:c"]); // inserted b
    expect(ops("a\nb\nc", "a\nc")).toEqual(["s:a", "d:b", "s:c"]); // deleted b
  });

  it("appends trailing adds / leading dels via the LCS backbone", () => {
    expect(ops("x", "x\ny\nz")).toEqual(["s:x", "a:y", "a:z"]);
    expect(ops("x\ny\nz", "z")).toEqual(["d:x", "d:y", "s:z"]);
  });

  it("handles empty inputs", () => {
    expect(lineDiff("", "").rows).toEqual([]);
    expect(lineDiff("", "a\nb").rows.map((r) => r.op)).toEqual(["add", "add"]);
    expect(lineDiff("a\nb", "").rows.map((r) => r.op)).toEqual(["del", "del"]);
  });
});
