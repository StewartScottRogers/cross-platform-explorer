import { describe, it, expect } from "vitest";
import { maskPngToDataUrl, bboxRectPercent, zoomToBBox, clampZoom, formatPercentDifferent } from "./imageDiffView";
import { base64ToBytes } from "./terminalClient";

describe("maskPngToDataUrl (CPE-1508)", () => {
  it("wraps the byte array in a data:image/png;base64 URL that round-trips", () => {
    const bytes = [137, 80, 78, 71, 13, 10, 26, 10]; // PNG magic header
    const url = maskPngToDataUrl(bytes);
    expect(url.startsWith("data:image/png;base64,")).toBe(true);
    const b64 = url.slice("data:image/png;base64,".length);
    expect(Array.from(base64ToBytes(b64))).toEqual(bytes);
  });

  it("handles an empty mask", () => {
    expect(maskPngToDataUrl([])).toBe("data:image/png;base64,");
  });

  it("handles a large mask without blowing the call stack (chunked encode)", () => {
    const bytes = Array.from({ length: 200_000 }, (_, i) => i % 256);
    const url = maskPngToDataUrl(bytes);
    const b64 = url.slice("data:image/png;base64,".length);
    expect(Array.from(base64ToBytes(b64))).toEqual(bytes);
  });
});

describe("bboxRectPercent (CPE-1508)", () => {
  it("returns null when there's no bbox", () => {
    expect(bboxRectPercent(null, 100, 100)).toBeNull();
    expect(bboxRectPercent(undefined, 100, 100)).toBeNull();
  });

  it("returns null for a zero-extent canvas", () => {
    expect(bboxRectPercent({ x: 0, y: 0, width: 10, height: 10 }, 0, 100)).toBeNull();
    expect(bboxRectPercent({ x: 0, y: 0, width: 10, height: 10 }, 100, 0)).toBeNull();
  });

  it("converts a bbox to CSS percentages of the canvas", () => {
    const rect = bboxRectPercent({ x: 25, y: 50, width: 50, height: 25 }, 100, 100);
    expect(rect).toEqual({ left: 25, top: 50, width: 50, height: 25 });
  });

  it("scales correctly for a non-square canvas", () => {
    const rect = bboxRectPercent({ x: 0, y: 0, width: 200, height: 40 }, 400, 200);
    expect(rect).toEqual({ left: 0, top: 0, width: 50, height: 20 });
  });
});

describe("zoomToBBox (CPE-1508)", () => {
  it("falls back to identity for a degenerate bbox or canvas", () => {
    expect(zoomToBBox({ x: 0, y: 0, width: 0, height: 10 }, 100, 100, 400, 400)).toEqual({ zoom: 1, panX: 0, panY: 0 });
    expect(zoomToBBox({ x: 0, y: 0, width: 10, height: 10 }, 0, 100, 400, 400)).toEqual({ zoom: 1, panX: 0, panY: 0 });
    expect(zoomToBBox({ x: 0, y: 0, width: 10, height: 10 }, 100, 100, 0, 400)).toEqual({ zoom: 1, panX: 0, panY: 0 });
  });

  it("computes a zoom that fits a small bbox with padding, centered in the viewport", () => {
    // A 10x10 bbox at the canvas center, viewport 400x400: fitZoom = 400/(10*1.4) = ~28.6, clamped to 8.
    const { zoom, panX, panY } = zoomToBBox({ x: 45, y: 45, width: 10, height: 10 }, 100, 100, 400, 400);
    expect(zoom).toBe(8);
    // bbox center is (50, 50); pan centers it: viewportWidth/(2*zoom) - cx = 400/16 - 50 = -25.
    expect(panX).toBeCloseTo(-25);
    expect(panY).toBeCloseTo(-25);
  });

  it("never zooms below 1x even for a huge bbox", () => {
    const { zoom } = zoomToBBox({ x: 0, y: 0, width: 1000, height: 1000 }, 1000, 1000, 400, 400);
    expect(zoom).toBe(1);
  });

  it("respects a custom padding factor", () => {
    const tight = zoomToBBox({ x: 0, y: 0, width: 100, height: 100 }, 1000, 1000, 400, 400, 1.0);
    const padded = zoomToBBox({ x: 0, y: 0, width: 100, height: 100 }, 1000, 1000, 400, 400, 2.0);
    expect(tight.zoom).toBeGreaterThan(padded.zoom);
  });
});

describe("clampZoom (CPE-1508)", () => {
  it("clamps to [1, 8]", () => {
    expect(clampZoom(0.2)).toBe(1);
    expect(clampZoom(4)).toBe(4);
    expect(clampZoom(50)).toBe(8);
  });
});

describe("formatPercentDifferent (CPE-1508)", () => {
  it("formats with one decimal place", () => {
    expect(formatPercentDifferent(0)).toBe("0.0%");
    expect(formatPercentDifferent(3.14159)).toBe("3.1%");
    expect(formatPercentDifferent(100)).toBe("100.0%");
  });
});
