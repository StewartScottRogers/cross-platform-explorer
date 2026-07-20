import { describe, it, expect } from "vitest";
import { diffTrees, type CompareNode, type DiffNode } from "./treeDiff";

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
