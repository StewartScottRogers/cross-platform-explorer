import { describe, it, expect } from "vitest";
import {
  parseSmartFolders,
  addSmartFolder,
  renameSmartFolder,
  removeSmartFolder,
  smartFolderPaths,
  type SmartFolder,
} from "./smartFolders";
import type { TagStore } from "./tags";

describe("smartFolders (CPE-667)", () => {
  const store: TagStore = {
    "C:/a/invoice1.pdf": { tags: ["invoice", "2026"], label: "" },
    "C:/b/invoice2.pdf": { tags: ["invoice"], label: "red" },
    "C:/c/photo.jpg": { tags: ["2026"], label: "" },
    "C:/d/notes.txt": { tags: [], label: "" },
  };

  it("smartFolderPaths returns matching paths, sorted", () => {
    const sf: SmartFolder = { id: "1", name: "Invoices", tag: "invoice" };
    expect(smartFolderPaths(store, sf)).toEqual(["C:/a/invoice1.pdf", "C:/b/invoice2.pdf"]);
    expect(smartFolderPaths(store, { id: "2", name: "y2026", tag: "2026" })).toEqual([
      "C:/a/invoice1.pdf",
      "C:/c/photo.jpg",
    ]);
    expect(smartFolderPaths(store, { id: "3", name: "none", tag: "missing" })).toEqual([]);
  });

  it("smartFolderPaths is safe on empty/blank input", () => {
    expect(smartFolderPaths({}, { id: "1", name: "x", tag: "invoice" })).toEqual([]);
    expect(smartFolderPaths(store, { id: "1", name: "x", tag: "" })).toEqual([]);
  });

  it("addSmartFolder appends, trims, and dedupes by name+tag", () => {
    let list: SmartFolder[] = [];
    list = addSmartFolder(list, "  Invoices  ", " invoice ");
    expect(list).toHaveLength(1);
    expect(list[0]).toMatchObject({ name: "Invoices", tag: "invoice" });
    // Same name + tag → no duplicate.
    list = addSmartFolder(list, "Invoices", "invoice");
    expect(list).toHaveLength(1);
    // Empty name or tag → no-op.
    expect(addSmartFolder(list, "", "x")).toHaveLength(1);
    expect(addSmartFolder(list, "y", "")).toHaveLength(1);
  });

  it("renameSmartFolder renames by id, ignoring empty/unknown", () => {
    const list: SmartFolder[] = [{ id: "a", name: "Old", tag: "t" }];
    expect(renameSmartFolder(list, "a", "New")[0].name).toBe("New");
    expect(renameSmartFolder(list, "a", "  ")[0].name).toBe("Old"); // blank ignored
    expect(renameSmartFolder(list, "zzz", "New")[0].name).toBe("Old"); // unknown id
  });

  it("removeSmartFolder drops by id", () => {
    const list: SmartFolder[] = [
      { id: "a", name: "A", tag: "t1" },
      { id: "b", name: "B", tag: "t2" },
    ];
    expect(removeSmartFolder(list, "a")).toEqual([{ id: "b", name: "B", tag: "t2" }]);
    expect(removeSmartFolder(list, "zzz")).toHaveLength(2);
  });

  it("parseSmartFolders tolerates malformed JSON and entries", () => {
    expect(parseSmartFolders(null)).toEqual([]);
    expect(parseSmartFolders("not json")).toEqual([]);
    expect(parseSmartFolders("{}")).toEqual([]);
    expect(parseSmartFolders('[{"id":"a","name":"A","tag":"t"},{"bad":true},42]')).toEqual([
      { id: "a", name: "A", tag: "t" },
    ]);
  });
});
