import { describe, it, expect } from "vitest";
import { namesList, detailList, csvList } from "./listing";
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

  it("csvList escapes cells, uses byte sizes + ISO modified, and blanks folder size", () => {
    const rows = [
      e({ name: 'weird,name".txt', size: 10, modified: 1_600_000_000_000 }),
      e({ name: "docs", is_dir: true, modified: 0 }), // 0 = no date → blank
    ];
    const out = csvList(rows).split("\n");
    expect(out[0]).toBe("Name,Size,Modified");
    expect(out[1]).toBe('"weird,name"".txt",10,2020-09-13T12:26:40.000Z');
    expect(out[2]).toBe("docs,,");
  });
});
