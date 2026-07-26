import { describe, it, expect } from "vitest";
import { mediaOpLabel, opsToJob, partitionEligible, progressPercent } from "./batchMedia";
import type { MediaOp } from "./bindings.gen";

describe("mediaOpLabel", () => {
  it("formats every op kind as a short one-line pill label", () => {
    expect(mediaOpLabel({ op: "resize", max_px: 1024 })).toBe("Resize 1024px");
    expect(mediaOpLabel({ op: "convert", to_ext: "webp" })).toBe("Convert → webp");
    expect(mediaOpLabel({ op: "rotate", degrees: 90 })).toBe("Rotate 90°");
    expect(mediaOpLabel({ op: "flip", horizontal: true })).toBe("Flip horizontal");
    expect(mediaOpLabel({ op: "flip", horizontal: false })).toBe("Flip vertical");
    expect(mediaOpLabel({ op: "rename", template: "{stem}-x" })).toBe('Rename "{stem}-x"');
    expect(mediaOpLabel({ op: "strip_metadata" })).toBe("Strip metadata");
  });
});

describe("opsToJob", () => {
  it("wraps an empty op list (no ops chosen yet)", () => {
    expect(opsToJob([], true)).toEqual({ ops: [], non_destructive: true });
  });

  it("preserves op order and the non-destructive flag", () => {
    const ops: MediaOp[] = [{ op: "resize", max_px: 800 }, { op: "strip_metadata" }];
    expect(opsToJob(ops, false)).toEqual({ ops, non_destructive: false });
  });
});

describe("partitionEligible", () => {
  it("keeps only image files, dropping folders and non-image extensions", () => {
    const entries = [
      { name: "a.jpg", is_dir: false },
      { name: "notes.txt", is_dir: false },
      { name: "pics", is_dir: true },
      { name: "b.PNG", is_dir: false }, // case-insensitive match
      { name: "archive.zip", is_dir: false },
    ];
    const { eligible, skipped } = partitionEligible(entries);
    expect(eligible.map((e) => e.name)).toEqual(["a.jpg", "b.PNG"]);
    expect(skipped).toBe(3);
  });

  it("reports zero skipped for an all-image selection", () => {
    const entries = [{ name: "a.jpg", is_dir: false }, { name: "b.png", is_dir: false }];
    const { eligible, skipped } = partitionEligible(entries);
    expect(eligible).toHaveLength(2);
    expect(skipped).toBe(0);
  });

  it("handles an empty selection", () => {
    expect(partitionEligible([])).toEqual({ eligible: [], skipped: 0 });
  });
});

describe("progressPercent", () => {
  it("is NaN-safe for a zero or not-yet-known total", () => {
    expect(progressPercent(0, 0)).toBe(0);
    expect(progressPercent(5, 0)).toBe(0);
    expect(progressPercent(NaN, 10)).toBe(0);
    expect(progressPercent(3, NaN)).toBe(0);
  });

  it("computes a rounded percent, clamped to [0,100]", () => {
    expect(progressPercent(1, 3)).toBe(33);
    expect(progressPercent(3, 3)).toBe(100);
    expect(progressPercent(4, 3)).toBe(100); // never overshoot past done
    expect(progressPercent(-1, 3)).toBe(0); // never go negative
  });
});
