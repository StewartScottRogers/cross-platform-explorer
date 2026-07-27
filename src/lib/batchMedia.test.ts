import { describe, it, expect } from "vitest";
import { mediaOpLabel, opsToJob, partitionEligible, progressPercent, canBatchTransform, skipRows } from "./batchMedia";
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
    expect(mediaOpLabel({ op: "compress", quality: 80 })).toBe("Compress q80");
    expect(mediaOpLabel({ op: "watermark", image: "", position: "bottom_right", opacity: 80 })).toBe(
      "Watermark (none)",
    );
    expect(
      mediaOpLabel({ op: "watermark", image: "C:\\assets\\logo.png", position: "top_left", opacity: 40 }),
    ).toBe("Watermark logo.png top_left 40%");
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

  it("excludes decode-only formats the encoder can't write (e.g. avif)", () => {
    // avif shows a thumbnail (decodes) but batch_transform can't ENCODE it, so it must be pre-filtered
    // rather than sent to the backend to fail per-file.
    const entries = [
      { name: "photo.avif", is_dir: false },
      { name: "keep.webp", is_dir: false },
    ];
    const { eligible, skipped } = partitionEligible(entries);
    expect(eligible.map((e) => e.name)).toEqual(["keep.webp"]);
    expect(skipped).toBe(1);
  });
});

describe("canBatchTransform", () => {
  it("accepts exactly the encoder-writable extensions, case-insensitively", () => {
    for (const ok of ["a.png", "a.jpg", "a.jpeg", "a.gif", "a.webp", "a.bmp", "a.tif", "a.tiff", "A.PNG"]) {
      expect(canBatchTransform(ok)).toBe(true);
    }
  });

  it("rejects decode-only / non-image / extensionless names", () => {
    for (const no of ["a.avif", "a.heic", "a.psd", "a.svg", "notes.txt", "archive.zip", "README"]) {
      expect(canBatchTransform(no)).toBe(false);
    }
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

describe("skipRows (CPE-1115)", () => {
  it("maps skipped [path, reason] pairs to basename + reason rows", () => {
    const rows = skipRows({
      skipped: [
        ["Z:\\pics\\photo.jpg", "not a valid image"],
        ["/home/me/pics/broken.png", "unexpected EOF"],
      ],
    });
    expect(rows).toEqual([
      { name: "photo.jpg", reason: "not a valid image" },
      { name: "broken.png", reason: "unexpected EOF" },
    ]);
  });

  it("is empty for a clean report and keeps a path with no separators", () => {
    expect(skipRows({ skipped: [] })).toEqual([]);
    expect(skipRows({ skipped: [["bare.gif", "why"]] })).toEqual([{ name: "bare.gif", reason: "why" }]);
  });
});
