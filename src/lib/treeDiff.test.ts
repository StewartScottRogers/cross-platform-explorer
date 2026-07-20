import { describe, it, expect } from "vitest";
import {
  diffTrees,
  summarizeDiff,
  flattenDiff,
  type CompareNode,
  type DiffNode,
} from "./treeDiff";

const f = (name: string, size = 0, modified = 0): CompareNode => ({ name, isDir: false, size, modified });
const d = (name: string, children: CompareNode[] = []): CompareNode => ({ name, isDir: true, children });
const status = (nodes: DiffNode[], name: string) => nodes.find((n) => n.name === name)?.status;

describe("diffTrees (CPE-777)", () => {
  it("classifies added / removed / identical / changed files at one level", () => {
    const left = [f("same.txt", 10, 100), f("edited.txt", 10, 100), f("gone.txt", 5, 1)];
    const right = [f("same.txt", 10, 100), f("edited.txt", 20, 200), f("new.txt", 1, 1)];
    const diff = diffTrees(left, right);
    expect(status(diff, "same.txt")).toBe("identical");
    expect(status(diff, "edited.txt")).toBe("changed"); // size + mtime differ
    expect(status(diff, "gone.txt")).toBe("removed");
    expect(status(diff, "new.txt")).toBe("added");
  });

  it("treats a size-only or mtime-only change as changed", () => {
    expect(status(diffTrees([f("a", 10, 100)], [f("a", 11, 100)]), "a")).toBe("changed");
    expect(status(diffTrees([f("a", 10, 100)], [f("a", 10, 999)]), "a")).toBe("changed");
    expect(status(diffTrees([f("a", 10, 100)], [f("a", 10, 100)]), "a")).toBe("identical");
  });

  it("marks a dir identical when its whole subtree matches, changed when any descendant differs", () => {
    const left = [d("src", [f("a.ts", 1, 1), d("deep", [f("x", 2, 2)])])];
    const same = diffTrees(left, structuredClone(left));
    expect(status(same, "src")).toBe("identical");

    const right = [d("src", [f("a.ts", 1, 1), d("deep", [f("x", 9, 9)])])]; // deep/x changed
    const diff = diffTrees(left, right);
    expect(status(diff, "src")).toBe("changed");
    const src = diff.find((n) => n.name === "src")!;
    expect(status(src.children!, "deep")).toBe("changed");
    const deep = src.children!.find((n) => n.name === "deep")!;
    expect(status(deep.children!, "x")).toBe("changed");
  });

  it("marks an added/removed directory subtree recursively", () => {
    const diff = diffTrees([], [d("newdir", [f("inside", 1, 1)])]);
    expect(status(diff, "newdir")).toBe("added");
    const nd = diff.find((n) => n.name === "newdir")!;
    expect(nd.children!.every((c) => c.status === "added")).toBe(true);
  });

  it("flags a file-vs-dir type mismatch as changed", () => {
    expect(status(diffTrees([f("thing", 1, 1)], [d("thing", [])]), "thing")).toBe("changed");
  });

  it("orders dirs first then by name; empty vs empty is empty", () => {
    const diff = diffTrees([f("z.txt"), d("a-dir")], [f("z.txt"), d("a-dir")]);
    expect(diff.map((n) => n.name)).toEqual(["a-dir", "z.txt"]);
    expect(diffTrees([], [])).toEqual([]);
  });
});

describe("summarizeDiff (CPE-779)", () => {
  it("counts files by status, recursing into dirs (dirs not counted themselves)", () => {
    // left/right chosen to yield each status, including nested + a whole added subtree.
    const left = [
      f("same.txt", 1, 1),
      f("edited.txt", 1, 1),
      f("gone.txt", 1, 1),
      d("sub", [f("a.txt", 1, 1), f("b.txt", 1, 1)]),
    ];
    const right = [
      f("same.txt", 1, 1),
      f("edited.txt", 2, 2),
      d("sub", [f("a.txt", 1, 1), f("b.txt", 9, 9)]),
      d("fresh", [f("x.txt", 1, 1), f("y.txt", 1, 1)]), // whole added subtree → 2 added files
    ];
    const s = summarizeDiff(diffTrees(left, right));
    expect(s).toEqual({ added: 2, removed: 1, changed: 2, identical: 2 });
  });

  it("is all-zero for an empty diff", () => {
    expect(summarizeDiff([])).toEqual({ added: 0, removed: 0, changed: 0, identical: 0 });
  });

  it("counts a file↔dir type change and an empty added dir as single leaves", () => {
    // "x" is a file on the left, a dir on the right → diffTrees emits a childless `changed` node.
    const typeChange = summarizeDiff(diffTrees([f("x", 1, 1)], [d("x", [f("c", 1, 1)])]));
    expect(typeChange).toEqual({ added: 0, removed: 0, changed: 1, identical: 0 });
    // A brand-new empty folder counts as one added leaf (not zero).
    const emptyAdded = summarizeDiff(diffTrees([], [d("newdir")]));
    expect(emptyAdded).toEqual({ added: 1, removed: 0, changed: 0, identical: 0 });
  });
});

describe("flattenDiff (CPE-779)", () => {
  const tree = diffTrees(
    [d("dir", [f("deep.txt", 1, 1)]), f("root.txt", 1, 1)],
    [d("dir", [f("deep.txt", 2, 2)]), f("root.txt", 1, 1)],
  );

  it("flattens depth-first with depth, path, and hasChildren", () => {
    const rows = flattenDiff(tree);
    expect(rows.map((r) => [r.path, r.depth, r.hasChildren])).toEqual([
      ["dir", 0, true], // dirs first
      ["dir/deep.txt", 1, false],
      ["root.txt", 0, false],
    ]);
  });

  it("hides descendants of a collapsed dir (its own row stays)", () => {
    const rows = flattenDiff(tree, new Set(["dir"]));
    expect(rows.map((r) => r.path)).toEqual(["dir", "root.txt"]); // dir/deep.txt hidden
  });
});
