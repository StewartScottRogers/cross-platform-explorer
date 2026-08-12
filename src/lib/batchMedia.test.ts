import { describe, it, expect } from "vitest";
import {
  confirmOverwriteJob,
  mediaOpLabel,
  opsToJob,
  overwritesInPlace,
  partitionEligible,
  progressPercent,
  canBatchTransform,
  sameFile,
  skipRows,
  templateEscapesDirectory,
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

describe("sameFile (CPE-1613)", () => {
  const WIN = "Win32";
  const MAC = "MacIntel";
  const LINUX = "Linux x86_64";

  it("the ticket's worked example: a case-only extension difference is platform-gated", () => {
    // plan() lower-cases only the extension, so "IMG_1.JPG" -> "IMG_1.jpg" — a different STRING but the
    // SAME FILE on Windows/macOS (case-insensitive filesystems), and a genuinely different possible file
    // on Linux (case-sensitive).
    expect(sameFile("/pics/IMG_1.JPG", "/pics/IMG_1.jpg", WIN)).toBe(true);
    expect(sameFile("/pics/IMG_1.JPG", "/pics/IMG_1.jpg", MAC)).toBe(true);
    expect(sameFile("/pics/IMG_1.JPG", "/pics/IMG_1.jpg", LINUX)).toBe(false);
  });

  it("trailing separators are ignored regardless of platform", () => {
    expect(sameFile("/pics/cat.jpg", "/pics/cat.jpg/", LINUX)).toBe(true);
    expect(sameFile("C:\\img\\p.png", "C:\\img\\p.png\\", WIN)).toBe(true);
  });

  it("resolves . and .. segments lexically", () => {
    expect(sameFile("/pics/x/../cat.jpg", "/pics/cat.jpg", LINUX)).toBe(true);
    expect(sameFile("/pics/./cat.jpg", "/pics/cat.jpg", LINUX)).toBe(true);
    expect(sameFile("/pics/a/b/../../cat.jpg", "/pics/cat.jpg", LINUX)).toBe(true);
    expect(sameFile("/pics/a/cat.jpg", "/pics/b/cat.jpg", LINUX)).toBe(false);
  });

  it("treats / and \\ as interchangeable separators", () => {
    expect(sameFile("C:\\img\\p.png", "C:/img/p.png", WIN)).toBe(true);
    expect(sameFile("/pics/a/cat.jpg", "\\pics\\a\\cat.jpg", LINUX)).toBe(true);
  });

  it("is reflexive and distinguishes genuinely different names", () => {
    expect(sameFile("/pics/cat.jpg", "/pics/cat.jpg", LINUX)).toBe(true);
    expect(sameFile("/pics/cat.jpg", "/pics/dog.jpg", LINUX)).toBe(false);
    expect(sameFile("/pics/cat.jpg", "/pics/cat.png", LINUX)).toBe(false);
  });

  it("does not fold case on Linux even for a directory-only case difference", () => {
    expect(sameFile("/PICS/cat.jpg", "/pics/cat.jpg", LINUX)).toBe(false);
    expect(sameFile("/PICS/cat.jpg", "/pics/cat.jpg", WIN)).toBe(true);
  });
});

describe("overwritesInPlace platform-gated case-only differences (CPE-1613)", () => {
  it("the worked example: 'IMG_1.JPG' -> 'IMG_1.jpg' via Convert is flagged in-place on Win/mac, not Linux", () => {
    const items = [{ input: "/pics/IMG_1.JPG", output: "/pics/IMG_1.jpg", summary: "convert→jpg" }];
    expect(overwritesInPlace(items, "Win32")).toEqual(items);
    expect(overwritesInPlace(items, "MacIntel")).toEqual(items);
    expect(overwritesInPlace(items, "Linux x86_64")).toEqual([]);
  });

  it("a genuinely different extension is never flagged, on any platform", () => {
    const items = [{ input: "/pics/cat.jpg", output: "/pics/cat.png", summary: "convert→png" }];
    for (const platform of ["Win32", "MacIntel", "Linux x86_64"]) {
      expect(overwritesInPlace(items, platform)).toEqual([]);
    }
  });
});

describe("templateEscapesDirectory (CPE-1623)", () => {
  it("rejects the ticket's exact traversal template", () => {
    expect(templateEscapesDirectory("..\\..\\cpe1613_traversal_victim\\important")).toBe(true);
  });

  it("rejects any path separator, forward or backward, and a whole-segment '..', on every platform", () => {
    for (const platform of ["Win32", "MacIntel", "Linux x86_64", ""]) {
      expect(templateEscapesDirectory("sub/name", platform)).toBe(true);
      expect(templateEscapesDirectory("sub\\name", platform)).toBe(true);
      expect(templateEscapesDirectory("..", platform)).toBe(true);
      expect(templateEscapesDirectory(" .. ", platform)).toBe(true); // whole segment once trimmed
      expect(templateEscapesDirectory("../x", platform)).toBe(true);
      expect(templateEscapesDirectory("..\\x", platform)).toBe(true);
      expect(templateEscapesDirectory("a/../../b", platform)).toBe(true);
      expect(templateEscapesDirectory("x/..", platform)).toBe(true);
    }
  });

  // CPE-1640: the colon half of the rule is Windows-only, and must stay in lockstep with
  // `crates/server/src/batch_media.rs`'s `colon_is_a_path_character()` (which gates on `cfg!(windows)`).
  it("rejects a colon on Windows only — it is an ordinary filename character on Linux/macOS (CPE-1640)", () => {
    for (const template of ["C:foo", "secrets:hidden", "10:30am-photo", "session:final"]) {
      // Windows: `:` is the drive separator AND the NTFS alternate-data-stream separator.
      expect(templateEscapesDirectory(template, "Win32")).toBe(true);
      expect(templateEscapesDirectory(template, "Windows NT 10.0; Win64; x64")).toBe(true);
      // Linux/macOS: an ordinary, legal filename character — refusing it was a pure false positive.
      expect(templateEscapesDirectory(template, "MacIntel")).toBe(false);
      expect(templateEscapesDirectory(template, "Linux x86_64")).toBe(false);
      // "Darwin" CONTAINS "win": a bare substring test would read macOS as Windows and re-introduce the
      // exact CPE-1640 false positive. Reachable because `defaultPlatform()` falls back to the userAgent
      // when `navigator.platform` is empty (reviewer nit, PR #848).
      expect(templateEscapesDirectory(template, "Darwin")).toBe(false);
      expect(templateEscapesDirectory(template, "Mozilla/5.0 (Macintosh; Darwin 23.0.0)")).toBe(false);
      // No navigator at all (a non-DOM test runner) reads as "not Windows": the direction that only ever
      // accepts a template the backend is still free to refuse, never the reverse.
      expect(templateEscapesDirectory(template, "")).toBe(false);
      // A separator alongside the colon is still refused everywhere — the colon gate can't mask one.
      expect(templateEscapesDirectory(`${template}/x`, "Linux x86_64")).toBe(true);
    }
  });

  it("accepts ordinary templates with no separators/colon or whole-segment traversal (UAT follow-up)", () => {
    // ".." inside an otherwise ordinary filename, with no separator anywhere, can never walk anywhere —
    // the ticket's own acceptance criterion: "ordinary rename templates (no separators) are unaffected".
    // Asserted on every platform: these are unaffected by CPE-1640's colon gate.
    for (const platform of ["Win32", "MacIntel", "Linux x86_64", ""]) {
      expect(templateEscapesDirectory("{stem}", platform)).toBe(false);
      expect(templateEscapesDirectory("{stem}-{n}", platform)).toBe(false);
      expect(templateEscapesDirectory("photo-{n}", platform)).toBe(false);
      expect(templateEscapesDirectory("vacation 2024", platform)).toBe(false);
      expect(templateEscapesDirectory("shot..final", platform)).toBe(false);
      expect(templateEscapesDirectory("v1..2", platform)).toBe(false);
      expect(templateEscapesDirectory("a..b", platform)).toBe(false);
      expect(templateEscapesDirectory("...", platform)).toBe(false);
    }
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
