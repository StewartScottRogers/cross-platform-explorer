// Pure split/join dialog logic (CPE-1509, parent CPE-1491): part-size preset/parsing and the
// part-file/manifest detection predicates that gate the "Split file…" / "Join parts…" context-menu rows.
// DOM/Tauri-free, mirroring certCreate.test.ts/vaultCreate.test.ts's split.
import { describe, it, expect } from "vitest";
import {
  PART_SIZE_PRESETS,
  parseCustomPartSize,
  isSplitManifestName,
  isSplitPartName,
  isSplitPartOrManifestName,
  canSplitFile,
  canJoinFile,
  manifestPathFor,
  defaultJoinOutputPath,
  guessOriginalName,
} from "./splitJoin";

describe("PART_SIZE_PRESETS", () => {
  it("offers the floppy/CD/FAT32 presets in bytes", () => {
    expect(PART_SIZE_PRESETS).toHaveLength(3);
    expect(PART_SIZE_PRESETS[0].bytes).toBe(1_474_560); // 1.44 MB floppy
    expect(PART_SIZE_PRESETS[1].bytes).toBe(650 * 1024 * 1024); // 650 MB CD
    expect(PART_SIZE_PRESETS[2].bytes).toBe(4 * 1024 * 1024 * 1024 - 1); // FAT32 max file size
  });
});

describe("parseCustomPartSize — MiB/GiB → bytes", () => {
  it("converts a positive MiB value to bytes", () => {
    expect(parseCustomPartSize(100, "MiB")).toBe(100 * 1024 * 1024);
  });

  it("converts a positive GiB value to bytes", () => {
    expect(parseCustomPartSize(2, "GiB")).toBe(2 * 1024 * 1024 * 1024);
  });

  it("rounds a fractional value to the nearest byte", () => {
    expect(parseCustomPartSize(1.5, "MiB")).toBe(Math.round(1.5 * 1024 * 1024));
  });

  it("rejects zero, negative, or non-finite values", () => {
    expect(parseCustomPartSize(0, "MiB")).toBeNull();
    expect(parseCustomPartSize(-5, "MiB")).toBeNull();
    expect(parseCustomPartSize(NaN, "MiB")).toBeNull();
    expect(parseCustomPartSize(Infinity, "GiB")).toBeNull();
  });
});

describe("isSplitManifestName", () => {
  it("matches a <name>.split-manifest.json file", () => {
    expect(isSplitManifestName("big.iso.split-manifest.json")).toBe(true);
  });

  it("rejects a bare manifest suffix with no stem", () => {
    expect(isSplitManifestName(".split-manifest.json")).toBe(false);
  });

  it("rejects an unrelated .json file", () => {
    expect(isSplitManifestName("config.json")).toBe(false);
  });
});

describe("isSplitPartName", () => {
  it("matches a 3-digit numbered part", () => {
    expect(isSplitPartName("big.iso.001")).toBe(true);
    expect(isSplitPartName("big.iso.042")).toBe(true);
  });

  it("matches a wider zero-padded part (1000+ parts)", () => {
    expect(isSplitPartName("big.iso.1000")).toBe(true);
  });

  it("rejects a non-numeric extension", () => {
    expect(isSplitPartName("report.pdf")).toBe(false);
    expect(isSplitPartName("archive.tar.gz")).toBe(false);
  });

  it("rejects a name with no extension or an empty stem", () => {
    expect(isSplitPartName("noext")).toBe(false);
    expect(isSplitPartName(".001")).toBe(false);
  });
});

describe("isSplitPartOrManifestName / canJoinFile", () => {
  it("accepts either a manifest or a numbered part", () => {
    expect(isSplitPartOrManifestName("big.iso.split-manifest.json")).toBe(true);
    expect(isSplitPartOrManifestName("big.iso.001")).toBe(true);
    expect(isSplitPartOrManifestName("plain.txt")).toBe(false);
  });

  it("canJoinFile requires a regular (non-directory) file", () => {
    expect(canJoinFile({ is_dir: false, name: "big.iso.001" })).toBe(true);
    expect(canJoinFile({ is_dir: true, name: "big.iso.001" })).toBe(false);
    expect(canJoinFile({ is_dir: false, name: "plain.txt" })).toBe(false);
  });
});

describe("canSplitFile", () => {
  it("accepts a non-empty regular file", () => {
    expect(canSplitFile({ is_dir: false, size: 1024 })).toBe(true);
  });

  it("rejects a folder", () => {
    expect(canSplitFile({ is_dir: true, size: 1024 })).toBe(false);
  });

  it("rejects an empty file", () => {
    expect(canSplitFile({ is_dir: false, size: 0 })).toBe(false);
  });
});

describe("manifestPathFor", () => {
  it("returns the same path when already pointed at the manifest", () => {
    expect(manifestPathFor("/out/big.iso.split-manifest.json")).toBe("/out/big.iso.split-manifest.json");
  });

  it("derives the manifest path from a numbered part, POSIX", () => {
    expect(manifestPathFor("/out/big.iso.001")).toBe("/out/big.iso.split-manifest.json");
  });

  it("derives the manifest path from a numbered part, Windows", () => {
    expect(manifestPathFor("C:\\out\\big.iso.001")).toBe("C:\\out\\big.iso.split-manifest.json");
  });
});

describe("defaultJoinOutputPath", () => {
  it("joins the manifest's originalName into the part's own folder", () => {
    expect(defaultJoinOutputPath("/out/big.iso.001", "big.iso")).toBe("/out/big.iso");
  });

  it("uses the Windows separator when the source folder is Windows-style", () => {
    expect(defaultJoinOutputPath("C:\\out\\big.iso.split-manifest.json", "big.iso")).toBe("C:\\out\\big.iso");
  });
});

describe("guessOriginalName — fallback default before the manifest is read", () => {
  it("strips the manifest suffix", () => {
    expect(guessOriginalName("big.iso.split-manifest.json")).toBe("big.iso");
  });

  it("strips a numbered part's trailing .NNN", () => {
    expect(guessOriginalName("big.iso.001")).toBe("big.iso");
  });

  it("returns the name unchanged when it's neither shape", () => {
    expect(guessOriginalName("plain.txt")).toBe("plain.txt");
  });
});
