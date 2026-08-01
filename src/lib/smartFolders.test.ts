import { describe, it, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";
import {
  parseSmartFolders,
  addSmartFolder,
  renameSmartFolder,
  removeSmartFolder,
  moveSmartFolder,
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

  // CPE-1231: reorder. Mirrors moveRule (colorRulesStore.ts) / moveMetaColumn (columns.ts).
  describe("moveSmartFolder (CPE-1231)", () => {
    const list: SmartFolder[] = [
      { id: "a", name: "A", tag: "t1" },
      { id: "b", name: "B", tag: "t2" },
      { id: "c", name: "C", tag: "t3" },
    ];

    it("moves an entry up (dir -1), swapping with its predecessor", () => {
      expect(moveSmartFolder(list, "b", -1).map((sf) => sf.id)).toEqual(["b", "a", "c"]);
    });

    it("moves an entry down (dir 1), swapping with its successor", () => {
      expect(moveSmartFolder(list, "b", 1).map((sf) => sf.id)).toEqual(["a", "c", "b"]);
    });

    it("is a no-op moving the first entry up", () => {
      expect(moveSmartFolder(list, "a", -1).map((sf) => sf.id)).toEqual(["a", "b", "c"]);
    });

    it("is a no-op moving the last entry down", () => {
      expect(moveSmartFolder(list, "c", 1).map((sf) => sf.id)).toEqual(["a", "b", "c"]);
    });

    it("is a no-op for an unknown id", () => {
      expect(moveSmartFolder(list, "zzz", -1)).toEqual(list);
    });

    it("does not mutate the input array", () => {
      const before = list.map((sf) => sf.id);
      moveSmartFolder(list, "b", -1);
      expect(list.map((sf) => sf.id)).toEqual(before);
    });
  });
});

// The module-level `smartFolders` writable store: init-from-storage, persist-on-change, and a
// re-init round-trip, mirroring savedSearchStore.test.ts's coverage of the same shape.
describe("smartFolders writable store (CPE-1231 reorder persistence)", () => {
  const KEY = "cpe.smartFolders";

  beforeEach(() => {
    localStorage.clear();
    vi.resetModules();
  });

  it("moveSaved persists the new order to localStorage", async () => {
    const mod = await import("./smartFolders");
    mod.saveSmartFolder("First", "t1");
    mod.saveSmartFolder("Second", "t2");
    let persisted = parseSmartFolders(localStorage.getItem(KEY));
    expect(persisted.map((sf) => sf.name)).toEqual(["First", "Second"]);
    const firstId = persisted[0].id;

    mod.moveSaved(firstId, 1); // push "First" down past "Second"
    persisted = parseSmartFolders(localStorage.getItem(KEY));
    expect(persisted.map((sf) => sf.name)).toEqual(["Second", "First"]);
  });

  it("moveSaved at a boundary is a no-op (order unchanged, still persisted)", async () => {
    const mod = await import("./smartFolders");
    mod.saveSmartFolder("First", "t1");
    mod.saveSmartFolder("Second", "t2");
    const persistedBefore = parseSmartFolders(localStorage.getItem(KEY));
    const firstId = persistedBefore[0].id;

    mod.moveSaved(firstId, -1); // already first — no-op
    const persistedAfter = parseSmartFolders(localStorage.getItem(KEY));
    expect(persistedAfter.map((sf) => sf.name)).toEqual(["First", "Second"]);
  });

  it("persistence round-trip: reordered list survives a fresh module load (simulated restart)", async () => {
    const mod = await import("./smartFolders");
    mod.saveSmartFolder("First", "t1");
    mod.saveSmartFolder("Second", "t2");
    const firstId = get(mod.smartFolders)[0].id;
    mod.moveSaved(firstId, 1);
    const before = get(mod.smartFolders);

    vi.resetModules();
    const { smartFolders: reloaded } = await import("./smartFolders");
    expect(get(reloaded)).toEqual(before);
    expect(get(reloaded).map((sf) => sf.name)).toEqual(["Second", "First"]);
  });
});
