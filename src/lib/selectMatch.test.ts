import { describe, it, expect } from "vitest";
import { selectMatching, sameExtensionAs } from "./selectMatch";
import type { Condition } from "./colorRules";
import type { DirEntry } from "./types";

const NOW = 1_700_000_000_000;
const f = (name: string, over: Partial<DirEntry> = {}): DirEntry =>
  ({ name, path: "/x/" + name, is_dir: false, size: 10, modified: NOW, ...(over as object) }) as DirEntry;
const dir = (name: string): DirEntry => f(name, { is_dir: true });

describe("selectMatching (CPE-780)", () => {
  const entries = [f("a.png"), f("b.txt"), f("c.PNG"), dir("sub"), f("big.bin", { size: 9999 })];

  it("selects indices matching a Condition (reusing matchesCondition)", () => {
    const png: Condition = { kind: "ext", exts: ["png"] };
    expect(selectMatching(entries, png, NOW)).toEqual([0, 2]); // a.png, c.PNG (case-insensitive)
    expect(selectMatching(entries, { kind: "isDir", value: true }, NOW)).toEqual([3]);
    expect(selectMatching(entries, { kind: "size", min: 1000 }, NOW)).toEqual([4]);
  });

  it("returns [] when nothing matches or entries are empty", () => {
    expect(selectMatching(entries, { kind: "ext", exts: ["gif"] }, NOW)).toEqual([]);
    expect(selectMatching([], { kind: "isDir", value: false }, NOW)).toEqual([]);
  });
});

describe("sameExtensionAs (CPE-780)", () => {
  const entries = [f("a.png"), f("b.txt"), f("c.png"), f("d.txt"), dir("e.png"), f("noext")];

  it("extends to all files sharing an extension with any seed file", () => {
    expect(sameExtensionAs(entries, [0])).toEqual([0, 2]); // seed a.png → all .png files (dir e.png ignored)
    expect(sameExtensionAs(entries, [0, 1])).toEqual([0, 1, 2, 3]); // png + txt
  });

  it("ignores dirs, extension-less, and out-of-range seed indices; empty when no usable seed", () => {
    expect(sameExtensionAs(entries, [4])).toEqual([]); // seed is a dir
    expect(sameExtensionAs(entries, [5])).toEqual([]); // seed has no extension
    expect(sameExtensionAs(entries, [99])).toEqual([]); // out of range
  });
});
