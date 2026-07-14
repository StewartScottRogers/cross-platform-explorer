import { describe, it, expect } from "vitest";
import { canThumbnail, fitDimensions, thumbKey, ThumbCache } from "./thumbnails";

describe("canThumbnail (CPE-257)", () => {
  it("accepts common raster + svg extensions, case-insensitively", () => {
    for (const ext of ["jpg", "JPG", "jpeg", "png", "gif", "webp", "bmp", "avif", "svg"]) {
      expect(canThumbnail(ext)).toBe(true);
    }
  });
  it("rejects non-image extensions", () => {
    for (const ext of ["txt", "pdf", "mp4", "zip", "", "docx"]) {
      expect(canThumbnail(ext)).toBe(false);
    }
  });
});

describe("fitDimensions (CPE-257)", () => {
  it("scales the longer side down to the box, preserving aspect ratio", () => {
    expect(fitDimensions(200, 100, 96)).toEqual({ w: 96, h: 48 });
    expect(fitDimensions(100, 200, 96)).toEqual({ w: 48, h: 96 });
  });
  it("never upscales a small image", () => {
    expect(fitDimensions(30, 20, 96)).toEqual({ w: 30, h: 20 });
  });
  it("guards against zero / negative inputs", () => {
    expect(fitDimensions(0, 100, 96)).toEqual({ w: 0, h: 0 });
    expect(fitDimensions(100, 100, 0)).toEqual({ w: 0, h: 0 });
  });
  it("rounds and never collapses a tiny dimension below 1px", () => {
    expect(fitDimensions(1000, 1, 96)).toEqual({ w: 96, h: 1 });
  });
});

describe("thumbKey (CPE-257)", () => {
  it("ties the key to path AND mtime so an edit busts the cache", () => {
    expect(thumbKey("/a.png", 5)).not.toBe(thumbKey("/a.png", 6));
    expect(thumbKey("/a.png", 5)).toBe(thumbKey("/a.png", 5));
  });
  it("tolerates a null mtime", () => {
    expect(thumbKey("/a.png", null)).toBe(thumbKey("/a.png", 0));
  });
});

describe("ThumbCache LRU (CPE-257)", () => {
  it("stores and retrieves by key", () => {
    const c = new ThumbCache(3);
    c.set("a", "AA");
    expect(c.get("a")).toBe("AA");
    expect(c.has("a")).toBe(true);
  });

  it("evicts the least-recently-used entry past capacity", () => {
    const c = new ThumbCache(2);
    c.set("a", "A");
    c.set("b", "B");
    c.get("a"); // touch a → b is now LRU
    c.set("c", "C"); // evicts b
    expect(c.has("a")).toBe(true);
    expect(c.has("b")).toBe(false);
    expect(c.has("c")).toBe(true);
    expect(c.size).toBe(2);
  });
});
