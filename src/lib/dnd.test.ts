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
    it("rejects a case-differing self-descendant drop (CPE-1379: case-insensitive filesystems)", () => {
      expect(isValidDrop(["C:\\Foo"], "C:\\FOO\\sub")).toBe(false); // same physical dir, differing case
      expect(isValidDrop(["C:/Foo"], "C:/FOO/sub")).toBe(false); // forward-slash variant
      expect(isValidDrop(["C:\\Foo"], "C:\\foo")).toBe(false); // itself, differing case
    });
    it("still allows a case-differing but genuinely unrelated folder", () => {
      expect(isValidDrop(["C:\\Foo"], "C:\\Bar")).toBe(true);
      expect(isValidDrop(["C:\\Foo"], "C:\\FOOBAR")).toBe(true); // segment-boundary check, not raw prefix match
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

  describe("hoverEffect (CPE-1372: cursor must match resolveEffect's actual drop decision)", () => {
    it("Ctrl forces copy, Shift forces move, regardless of volume", () => {
      expect(hoverEffect({ ctrlKey: true, shiftKey: false }, true)).toBe("copy");
      expect(hoverEffect({ ctrlKey: true, shiftKey: false }, false)).toBe("copy");
      expect(hoverEffect({ ctrlKey: false, shiftKey: true }, true)).toBe("move");
      expect(hoverEffect({ ctrlKey: false, shiftKey: true }, false)).toBe("move");
    });
    it("no modifier + same-volume shows move (e.g. C: to C:)", () => {
      expect(hoverEffect({ ctrlKey: false, shiftKey: false }, true)).toBe("move");
    });
    it("no modifier + cross-volume shows copy, matching the actual copy drop (e.g. C: to D:)", () => {
      expect(hoverEffect({ ctrlKey: false, shiftKey: false }, false)).toBe("copy");
    });
    it("unknown volume (no hover-time signal) defaults to move — the pre-CPE-1372 behaviour, not a regression to always-copy", () => {
      expect(hoverEffect({ ctrlKey: false, shiftKey: false }, null)).toBe("move");
      expect(hoverEffect({ ctrlKey: false, shiftKey: false })).toBe("move"); // sameVolume omitted entirely
    });
  });
});
