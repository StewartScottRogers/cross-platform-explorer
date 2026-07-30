import { describe, it, expect } from "vitest";
import { moveActive, volumeIdForRoot, resolveBuildRoot, extensionOf } from "./instantSearch";

describe("moveActive (CPE-1139)", () => {
  it("wraps forward and backward within the list", () => {
    expect(moveActive(0, 1, 3)).toBe(1);
    expect(moveActive(2, 1, 3)).toBe(0); // wraps past the end
    expect(moveActive(0, -1, 3)).toBe(2); // wraps past the start
  });

  it("returns 0 for an empty list", () => {
    expect(moveActive(0, 1, 0)).toBe(0);
    expect(moveActive(5, -1, 0)).toBe(0);
  });

  it("is a no-op for a single-item list", () => {
    expect(moveActive(0, 1, 1)).toBe(0);
    expect(moveActive(0, -1, 1)).toBe(0);
  });
});

describe("volumeIdForRoot (CPE-1139)", () => {
  it("is deterministic for the same root", () => {
    expect(volumeIdForRoot("Z:\\")).toBe(volumeIdForRoot("Z:\\"));
  });

  it("is case-insensitive (D:\\ and d:\\ collide, same drive)", () => {
    expect(volumeIdForRoot("D:\\")).toBe(volumeIdForRoot("d:\\"));
  });

  it("differs across distinct roots (no accidental collision for common drive letters)", () => {
    const ids = new Set(["C:\\", "D:\\", "Z:\\", "/"].map(volumeIdForRoot));
    expect(ids.size).toBe(4);
  });

  it("always yields a non-negative safe integer", () => {
    const id = volumeIdForRoot("E:\\some\\path");
    expect(Number.isSafeInteger(id)).toBe(true);
    expect(id).toBeGreaterThanOrEqual(0);
  });
});

describe("resolveBuildRoot (CPE-1139)", () => {
  it("uses the drive owning the current folder when one is open", () => {
    expect(resolveBuildRoot("Z:\\repos\\cross-platform-explorer", null)).toBe("Z:\\");
  });

  it("falls back to the home directory's drive when there is no current folder (Home screen)", () => {
    expect(resolveBuildRoot("", "C:\\Users\\stew")).toBe("C:\\");
  });

  it("is empty when neither a current folder nor a home directory is known", () => {
    expect(resolveBuildRoot("", null)).toBe("");
  });
});

describe("extensionOf (CPE-1139)", () => {
  it("lowercases a normal extension", () => {
    expect(extensionOf("Report.PDF")).toBe("pdf");
  });

  it("is empty for an extensionless name", () => {
    expect(extensionOf("README")).toBe("");
  });

  it("is empty for a dotfile with no further extension", () => {
    expect(extensionOf(".gitignore")).toBe("");
  });

  it("uses the last dot for a multi-dot name", () => {
    expect(extensionOf("archive.tar.gz")).toBe("gz");
  });

  it("is empty for a trailing dot with nothing after it", () => {
    expect(extensionOf("file.")).toBe("");
  });
});
