import { describe, it, expect } from "vitest";
import { namesList, detailList } from "./listing";
import type { DirEntry } from "./types";

const e = (over: Partial<DirEntry>): DirEntry => ({
  name: "x", path: "/x", is_dir: false, size: 0, modified: 0, extension: "", hidden: false, ...over,
});

describe("copy folder listing (CPE-422)", () => {
  const entries = [
    e({ name: "readme.md", size: 2048 }),
    e({ name: "src", is_dir: true }),
    e({ name: "empty.txt", size: 0 }),
  ];

  it("namesList is one name per line, in order", () => {
    expect(namesList(entries)).toBe("readme.md\nsrc\nempty.txt");
    expect(namesList([])).toBe("");
  });

  it("detailList is a Name\\tSize table with a header; folders show <folder>", () => {
    expect(detailList(entries)).toBe(
      ["Name\tSize", "readme.md\t2.0 KB", "src\t<folder>", "empty.txt\t0 B"].join("\n"),
    );
    expect(detailList([])).toBe("Name\tSize");
  });
});
