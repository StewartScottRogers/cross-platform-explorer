import { describe, it, expect } from "vitest";
import { squarify, type TreemapItem } from "./treemap";

const items = (...sizes: number[]): TreemapItem[] =>
  sizes.map((size, i) => ({ key: `k${i}`, size }));

describe("squarify", () => {
  it("returns no tiles for empty input or a degenerate rect", () => {
    expect(squarify([], 0, 0, 100, 100)).toEqual([]);
    expect(squarify(items(1, 2), 0, 0, 0, 100)).toEqual([]);
    expect(squarify(items(1, 2), 0, 0, 100, -5)).toEqual([]);
  });

  it("drops zero/negative sizes, one tile per positive item", () => {
    const tiles = squarify(items(4, 0, 2, -1), 0, 0, 60, 40);
    expect(tiles).toHaveLength(2);
    expect(tiles.map((t) => t.key).sort()).toEqual(["k0", "k2"]);
  });

  it("a single item fills the whole rectangle", () => {
    const [t] = squarify(items(10), 5, 7, 30, 20);
    expect(t).toMatchObject({ key: "k0", x: 5, y: 7, w: 30, h: 20 });
  });

  it("tiles fully cover the rect (area conserved) with no tile out of bounds", () => {
    const w = 200;
    const h = 120;
    const tiles = squarify(items(50, 30, 20, 10, 8, 8, 4, 2), 0, 0, w, h);
    const area = tiles.reduce((s, t) => s + t.w * t.h, 0);
    expect(area).toBeCloseTo(w * h, 3);
    for (const t of tiles) {
      expect(t.w).toBeGreaterThan(0);
      expect(t.h).toBeGreaterThan(0);
      expect(t.x).toBeGreaterThanOrEqual(-1e-6);
      expect(t.y).toBeGreaterThanOrEqual(-1e-6);
      expect(t.x + t.w).toBeLessThanOrEqual(w + 1e-6);
      expect(t.y + t.h).toBeLessThanOrEqual(h + 1e-6);
    }
  });

  it("tile area is proportional to size", () => {
    const tiles = squarify(items(30, 10), 0, 0, 100, 100);
    const byKey = Object.fromEntries(tiles.map((t) => [t.key, t.w * t.h]));
    // k0 is 3x the size of k1 -> ~3x the area.
    expect(byKey.k0 / byKey.k1).toBeCloseTo(3, 4);
  });

  it("does not overlap tiles (pairwise) for a typical set", () => {
    const tiles = squarify(items(40, 25, 20, 15, 12, 8), 0, 0, 160, 90);
    const overlap = (a: (typeof tiles)[0], b: (typeof tiles)[0]) =>
      a.x < b.x + b.w - 1e-6 &&
      b.x < a.x + a.w - 1e-6 &&
      a.y < b.y + b.h - 1e-6 &&
      b.y < a.y + a.h - 1e-6;
    for (let i = 0; i < tiles.length; i++)
      for (let j = i + 1; j < tiles.length; j++)
        expect(overlap(tiles[i], tiles[j])).toBe(false);
  });
});
