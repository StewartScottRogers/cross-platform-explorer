import { describe, it, expect } from "vitest";
import { ZIP_FAMILY_EXTS, ARCHIVE_EXTS, EXTRACT_EXTS, ARCHIVE_SAFETY_EXTS } from "./archiveExts";

describe("archive extension sets (CPE-1181)", () => {
  it("ARCHIVE_EXTS (browsable) includes the zip family plus tar/gz/tgz/7z/iso", () => {
    for (const e of ZIP_FAMILY_EXTS) expect(ARCHIVE_EXTS.has(e)).toBe(true);
    for (const e of ["tar", "gz", "tgz", "7z", "iso"]) expect(ARCHIVE_EXTS.has(e)).toBe(true);
  });

  it("EXTRACT_EXTS (backend-unpackable) is the zip family plus tar/gz/tgz/7z", () => {
    for (const e of ZIP_FAMILY_EXTS) expect(EXTRACT_EXTS.has(e)).toBe(true);
    for (const e of ["tar", "gz", "tgz", "7z"]) expect(EXTRACT_EXTS.has(e)).toBe(true);
  });

  it("iso is browsable but NOT extractable (backend has no ISO extractor — CPE-1181 regression guard)", () => {
    expect(ARCHIVE_EXTS.has("iso")).toBe(true);
    expect(EXTRACT_EXTS.has("iso")).toBe(false);
  });

  it("EXTRACT_EXTS never silently inherits a browse-only format from ARCHIVE_EXTS", () => {
    // Every extractable ext must be genuinely unpackable — i.e. present in the zip family or the
    // explicit tar/gz set — never merely because it's browsable. Guards the exact bug where adding a
    // browse-only ext to ARCHIVE_EXTS leaked an Extract action.
    const unpackable = new Set([...ZIP_FAMILY_EXTS, "tar", "gz", "tgz", "7z"]);
    for (const e of EXTRACT_EXTS) expect(unpackable.has(e)).toBe(true);
  });

  it("ARCHIVE_SAFETY_EXTS (analyze_archive_safety-eligible) is exactly the zip family (CPE-1318)", () => {
    // The backend scan opens the file with the `zip` crate directly — tar/gz/tgz/7z/iso are NOT zip
    // containers, so offering "Check archive safety…" for them would silently score a "0 entries
    // scanned" report that looks safe but was never actually analyzed. Regression guard mirroring the
    // EXTRACT_EXTS/CPE-1181 discipline above.
    expect(ARCHIVE_SAFETY_EXTS).toEqual(ZIP_FAMILY_EXTS);
    for (const e of ["tar", "gz", "tgz", "7z", "iso"]) expect(ARCHIVE_SAFETY_EXTS.has(e)).toBe(false);
  });
});
