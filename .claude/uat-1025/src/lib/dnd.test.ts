import { describe, it, expect } from "vitest";
import { isValidDrop, resolveEffect, hoverEffect } from "./dnd";

describe("dnd shared model (CPE-669)", () => {
  describe("isValidDrop", () => {
    it("rejects an empty drag or empty dest", () => {
      expect(isValidDrop([], "C:/a")).toBe(false);
      expect(isValidDrop(["C:/a"], "")).toBe(false);
    });
    it("rejects dropping onto a dragged path or its descendant", () => {
      expect(isValidDrop(["C:/a/folder"], "C:/a/folder")).toBe(false); // itself
      expect(isValidDrop(["C:/a/folder"], "C:/a/folder/sub")).toBe(false); // descendant
    });
    it("allows an unrelated folder, normalizing separators/trailing slash", () => {
      expect(isValidDrop(["C:/a/folder"], "C:/b")).toBe(true);
      expect(isValidDrop(["C:\\a\\folder"], "C:/a/folder2")).toBe(true); // sibling, not descendant
      expect(isValidDrop(["C:/a/folder/"], "C:/a/folder")).toBe(false); // trailing slash still itself
    });
  });

  describe("resolveEffect (OS convention + modifier override)", () => {
    it("Ctrl forces copy, Shift forces move, regardless of volume", () => {
      expect(resolveEffect({ ctrlKey: true, shiftKey: false }, true)).toBe("copy");
      expect(resolveEffect({ ctrlKey: false, shiftKey: true }, false)).toBe("move");
    });
    it("without a modifier, same-volume moves and cross-volume copies", () => {
      expect(resolveEffect({ ctrlKey: false, shiftKey: false }, true)).toBe("move");
      expect(resolveEffect({ ctrlKey: false, shiftKey: false }, false)).toBe("copy");
    });
    it("unknown volume defaults to copy (safe: never loses the source)", () => {
      expect(resolveEffect({ ctrlKey: false, shiftKey: false }, null)).toBe("copy");
    });
  });

  describe("hoverEffect", () => {
    it("is modifier-driven and defaults to move", () => {
      expect(hoverEffect({ ctrlKey: true, shiftKey: false })).toBe("copy");
      expect(hoverEffect({ ctrlKey: false, shiftKey: true })).toBe("move");
      expect(hoverEffect({ ctrlKey: false, shiftKey: false })).toBe("move");
    });
  });
});
