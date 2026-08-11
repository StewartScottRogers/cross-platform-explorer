import { describe, it, expect } from "vitest";
import {
  confirmOverwriteJob,
  mediaOpLabel,
  opsToJob,
  overwritesInPlace,
  partitionEligible,
  progressPercent,
  canBatchTransform,
  skipRows,
  uniqueParentDirs,
} from "./batchMedia";
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
  it("wraps an empty op list (no ops chosen yet), always starting confirmed_overwrite false", () => {
    expect(opsToJob([], true)).toEqual({ ops: [], non_destructive: true, confirmed_overwrite: false });
  });

  it("preserves op order and the non-destructive flag", () => {
    const ops: MediaOp[] = [{ op: "resize", max_px: 800 }, { op: "strip_metadata" }];
    expect(opsToJob(ops, false)).toEqual({ ops, non_destructive: false, confirmed_overwrite: false });
  });
});

describe("confirmOverwriteJob (CPE-1599)", () => {
  it("returns a copy of the job with confirmed_overwrite flipped to true, leaving the input untouched", () => {
    const job = opsToJob([{ op: "compress", quality: 80 }], false);
    const confirmed = confirmOverwriteJob(job);
    expect(confirmed).toEqual({ ...job, confirmed_overwrite: true });
    expect(job.confirmed_overwrite).toBe(false); // the original job object is not mutated
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

describe("overwritesInPlace (CPE-1590)", () => {
  it("returns only the planned items whose output equals their input", () => {
    const items = [
      { input: "/pics/a.jpg", output: "/pics/a-1024.jpg", summary: "resize" }, // non-destructive: distinct
      { input: "/pics/b.jpg", output: "/pics/b.jpg", summary: "compress q80" }, // overwrite-mode: same path
      { input: "/pics/c.jpg", output: "/pics/c.jpg", summary: "strip-metadata" }, // overwrite-mode: same path
    ];
    expect(overwritesInPlace(items)).toEqual([
      { input: "/pics/b.jpg", output: "/pics/b.jpg", summary: "compress q80" },
      { input: "/pics/c.jpg", output: "/pics/c.jpg", summary: "strip-metadata" },
    ]);
  });

  it("is empty when every planned output differs from its input (the safe/default path)", () => {
    const items = [
      { input: "/pics/a.jpg", output: "/pics/a-1024.jpg", summary: "resize" },
      { input: "/pics/b.png", output: "/pics/b.webp", summary: "convert" },
    ];
    expect(overwritesInPlace(items)).toEqual([]);
  });

  it("is empty for an empty plan", () => {
    expect(overwritesInPlace([])).toEqual([]);
  });
});

describe("uniqueParentDirs (CPE-1590)", () => {
  it("dedupes parent directories, first-seen order, cross-platform separators", () => {
    expect(
      uniqueParentDirs(["/pics/a.jpg", "/pics/b.jpg", "C:\\photos\\c.jpg", "/pics/d.jpg", "C:\\photos\\e.jpg"]),
    ).toEqual(["/pics", "C:\\photos"]);
  });

  it("is empty for an empty input", () => {
    expect(uniqueParentDirs([])).toEqual([]);
  });

  it("drops a root-level file's empty parent rather than pushing an empty string", () => {
    // parentDir("/a.jpg") -> "/" (POSIX root case), which IS kept — only a truly empty result is dropped.
    expect(uniqueParentDirs(["/a.jpg"])).toEqual(["/"]);
  });
});
