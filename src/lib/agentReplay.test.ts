import { describe, it, expect } from "vitest";
import {
  sliderRange,
  sliderFraction,
  entriesUpTo,
  currentEntry,
  chronological,
  nextTimestamp,
  prevTimestamp,
  isMultiplyEdited,
  cadenceForSpeed,
  checkpointMarkers,
} from "./agentReplay";
import type { TimelineEntry } from "./agentActivity";
import type { Checkpoint } from "./bindings.gen";

const cp = (manifest_id: string, ts: number, label = ""): Checkpoint => ({ manifest_id, ts, label });

const e = (id: number, path: string, at: number, kind: TimelineEntry["kind"] = "modified"): TimelineEntry => ({
  id,
  kind,
  path,
  at,
});

describe("agentReplay pure helpers (CPE-1094)", () => {
  describe("sliderRange", () => {
    it("is null for an empty timeline", () => {
      expect(sliderRange([])).toBeNull();
    });

    it("is null for a single-entry timeline (disabled/empty state)", () => {
      expect(sliderRange([e(1, "/a", 100)])).toBeNull();
    });

    it("spans min..max at across entries regardless of array order", () => {
      const tl = [e(3, "/c", 300), e(1, "/a", 100), e(2, "/b", 200)];
      expect(sliderRange(tl)).toEqual({ firstAt: 100, lastAt: 300 });
    });

    it("is a degenerate (equal) range when every entry shares one timestamp", () => {
      const tl = [e(1, "/a", 500), e(2, "/b", 500), e(3, "/c", 500)];
      expect(sliderRange(tl)).toEqual({ firstAt: 500, lastAt: 500 });
    });
  });

  describe("sliderFraction", () => {
    it("computes a 0..1 fraction across a normal range", () => {
      const range = { firstAt: 0, lastAt: 200 };
      expect(sliderFraction(range, 0)).toBe(0);
      expect(sliderFraction(range, 200)).toBe(1);
      expect(sliderFraction(range, 50)).toBe(0.25);
    });

    it("never returns NaN for a degenerate (firstAt === lastAt) range", () => {
      const range = { firstAt: 500, lastAt: 500 };
      expect(sliderFraction(range, 500)).toBe(0);
      expect(Number.isNaN(sliderFraction(range, 500))).toBe(false);
    });

    it("clamps out-of-range t to [0, 1]", () => {
      const range = { firstAt: 100, lastAt: 200 };
      expect(sliderFraction(range, 0)).toBe(0);
      expect(sliderFraction(range, 999)).toBe(1);
    });
  });

  describe("entriesUpTo", () => {
    const tl = [e(3, "/c", 300), e(2, "/b", 200), e(1, "/a", 100)];

    it("returns an empty array for t before the first entry", () => {
      expect(entriesUpTo(tl, 0)).toEqual([]);
    });

    it("includes entries exactly at the boundary (at <= t)", () => {
      expect(entriesUpTo(tl, 200).map((x) => x.id)).toEqual([2, 1]);
    });

    it("returns everything for t at or after the last entry", () => {
      expect(entriesUpTo(tl, 300).map((x) => x.id)).toEqual([3, 2, 1]);
      expect(entriesUpTo(tl, 9999).map((x) => x.id)).toEqual([3, 2, 1]);
    });

    it("preserves the input array's order (no re-sort)", () => {
      const shuffled = [e(1, "/a", 100), e(3, "/c", 300), e(2, "/b", 200)];
      expect(entriesUpTo(shuffled, 300).map((x) => x.id)).toEqual([1, 3, 2]);
    });
  });

  describe("currentEntry", () => {
    it("is null for an empty timeline", () => {
      expect(currentEntry([], 100)).toBeNull();
    });

    it("is null when t is before every entry", () => {
      expect(currentEntry([e(1, "/a", 100)], 0)).toBeNull();
    });

    it("picks the single entry for a single-entry timeline at/after it", () => {
      expect(currentEntry([e(1, "/a", 100)], 100)?.id).toBe(1);
      expect(currentEntry([e(1, "/a", 100)], 500)?.id).toBe(1);
    });

    it("picks the most recent entry at or before t", () => {
      const tl = [e(1, "/a", 100), e(2, "/b", 200), e(3, "/c", 300)];
      expect(currentEntry(tl, 250)?.id).toBe(2);
      expect(currentEntry(tl, 300)?.id).toBe(3);
    });

    it("breaks same-timestamp ties on the higher id (later within the batch)", () => {
      const tl = [e(1, "/a", 500), e(2, "/b", 500), e(3, "/c", 500)];
      expect(currentEntry(tl, 500)?.id).toBe(3);
    });
  });

  describe("chronological / nextTimestamp / prevTimestamp", () => {
    const tl = [e(3, "/c", 300), e(1, "/a", 100), e(2, "/b", 200)];

    it("sorts oldest-first", () => {
      expect(chronological(tl).map((x) => x.at)).toEqual([100, 200, 300]);
    });

    it("nextTimestamp advances to the next distinct at, null past the end", () => {
      expect(nextTimestamp(tl, 0)).toBe(100);
      expect(nextTimestamp(tl, 100)).toBe(200);
      expect(nextTimestamp(tl, 300)).toBeNull();
    });

    it("prevTimestamp retreats to the previous distinct at, null before the start", () => {
      expect(prevTimestamp(tl, 300)).toBe(200);
      expect(prevTimestamp(tl, 100)).toBeNull();
      expect(prevTimestamp(tl, 0)).toBeNull();
    });

    it("next/prevTimestamp collapse a batch sharing one timestamp to a single step", () => {
      const batch = [e(1, "/a", 500), e(2, "/b", 500), e(3, "/c", 500)];
      expect(nextTimestamp(batch, 0)).toBe(500);
      expect(nextTimestamp(batch, 500)).toBeNull();
      expect(prevTimestamp(batch, 500)).toBeNull();
    });
  });

  describe("isMultiplyEdited", () => {
    it("is false when a path was written only once", () => {
      const tl = [e(1, "/a", 100, "created")];
      expect(isMultiplyEdited(tl, "/a")).toBe(false);
    });

    it("is true when a path was written more than once (create + modify, or 2x modify)", () => {
      const tl = [e(1, "/a", 100, "created"), e(2, "/a", 200, "modified")];
      expect(isMultiplyEdited(tl, "/a")).toBe(true);
    });

    it("ignores reads/renames/removes when counting writes", () => {
      const tl = [e(1, "/a", 100, "created"), e(2, "/a", 200, "read"), e(3, "/a", 300, "renamed")];
      expect(isMultiplyEdited(tl, "/a")).toBe(false);
    });

    it("is false for a path with no entries at all", () => {
      expect(isMultiplyEdited([], "/missing")).toBe(false);
    });
  });

  describe("cadenceForSpeed (CPE-1104)", () => {
    it("returns the base cadence unchanged at 1x (default)", () => {
      expect(cadenceForSpeed(400, 1)).toBe(400);
    });

    it("halves the period at 2x and quarters it at 4x (faster playback)", () => {
      expect(cadenceForSpeed(400, 2)).toBe(200);
      expect(cadenceForSpeed(400, 4)).toBe(100);
    });

    it("doubles the period at 0.5x (slower playback)", () => {
      expect(cadenceForSpeed(400, 0.5)).toBe(800);
    });

    it("is division-safe: a non-positive speed falls back to the base cadence rather than NaN/Infinity", () => {
      expect(cadenceForSpeed(400, 0)).toBe(400);
      expect(cadenceForSpeed(400, -1)).toBe(400);
      expect(Number.isFinite(cadenceForSpeed(400, 0))).toBe(true);
    });
  });

  describe("checkpointMarkers (CPE-1126)", () => {
    it("is empty for a null range", () => {
      expect(checkpointMarkers(null, [cp("m1", 150)])).toEqual([]);
    });

    it("is empty when there are no checkpoints", () => {
      expect(checkpointMarkers({ firstAt: 100, lastAt: 300 }, [])).toEqual([]);
    });

    it("positions an in-range checkpoint by the same math as sliderFraction", () => {
      const range = { firstAt: 100, lastAt: 300 };
      const [m] = checkpointMarkers(range, [cp("m1", 200)]);
      expect(m.fraction).toBe(0.5);
      expect(m.inRange).toBe(true);
      expect(m.cp.manifest_id).toBe("m1");
    });

    it("marks the exact endpoints as in-range at fractions 0 and 1", () => {
      const range = { firstAt: 100, lastAt: 300 };
      const markers = checkpointMarkers(range, [cp("start", 100), cp("end", 300)]);
      expect(markers[0]).toMatchObject({ fraction: 0, inRange: true });
      expect(markers[1]).toMatchObject({ fraction: 1, inRange: true });
    });

    it("clamps a checkpoint BEFORE the window to fraction 0 and flags it out-of-range", () => {
      const range = { firstAt: 100, lastAt: 300 };
      const [m] = checkpointMarkers(range, [cp("early", 50)]);
      expect(m.fraction).toBe(0);
      expect(m.inRange).toBe(false);
    });

    it("clamps a checkpoint AFTER the window to fraction 1 and flags it out-of-range", () => {
      const range = { firstAt: 100, lastAt: 300 };
      const [m] = checkpointMarkers(range, [cp("late", 999)]);
      expect(m.fraction).toBe(1);
      expect(m.inRange).toBe(false);
    });

    it("never divides by zero on a degenerate (firstAt === lastAt) range", () => {
      const range = { firstAt: 500, lastAt: 500 };
      const markers = checkpointMarkers(range, [cp("on", 500), cp("off", 400)]);
      expect(markers[0]).toMatchObject({ fraction: 0, inRange: true });
      expect(markers[1]).toMatchObject({ fraction: 0, inRange: false });
      expect(markers.every((m) => Number.isFinite(m.fraction))).toBe(true);
    });

    it("preserves input order and maps every checkpoint", () => {
      const range = { firstAt: 0, lastAt: 400 };
      const markers = checkpointMarkers(range, [cp("a", 400), cp("b", 0), cp("c", 200)]);
      expect(markers.map((m) => m.cp.manifest_id)).toEqual(["a", "b", "c"]);
      expect(markers.map((m) => m.fraction)).toEqual([1, 0, 0.5]);
    });
  });
});
