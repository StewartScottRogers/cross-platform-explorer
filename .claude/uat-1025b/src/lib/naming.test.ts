import { describe, it, expect } from "vitest";
import { uniqueName, uniqueNameWithExt } from "./naming";

describe("uniqueName", () => {
  it("returns the base name when nothing collides", () => {
    expect(uniqueName("New folder", [])).toBe("New folder");
    expect(uniqueName("New folder", ["Documents", "Photos"])).toBe("New folder");
  });

  it("appends (2) when the base is taken", () => {
    expect(uniqueName("New folder", ["New folder"])).toBe("New folder (2)");
  });

  it("keeps incrementing past consecutive collisions", () => {
    expect(
      uniqueName("New folder", ["New folder", "New folder (2)", "New folder (3)"]),
    ).toBe("New folder (4)");
  });

  it("fills the lowest available gap", () => {
    // (2) is free even though (3) exists — Explorer picks the lowest.
    expect(uniqueName("New folder", ["New folder", "New folder (3)"])).toBe(
      "New folder (2)",
    );
  });

  it("matches names case-insensitively", () => {
    expect(uniqueName("New folder", ["NEW FOLDER"])).toBe("New folder (2)");
  });

  it("works for arbitrary base names", () => {
    expect(uniqueName("Untitled", ["Untitled", "untitled (2)"])).toBe("Untitled (3)");
  });

  it("ignores unrelated names when numbering", () => {
    expect(uniqueName("New folder", ["report.txt", "New folder"])).toBe(
      "New folder (2)",
    );
  });
});

describe("uniqueNameWithExt", () => {
  it("returns base+ext when nothing collides", () => {
    expect(uniqueNameWithExt("report", ".zip", [])).toBe("report.zip");
  });

  it("numbers before the extension, not after", () => {
    expect(uniqueNameWithExt("report", ".zip", ["report.zip"])).toBe(
      "report (2).zip",
    );
  });

  it("keeps incrementing past consecutive collisions", () => {
    expect(
      uniqueNameWithExt("report", ".zip", [
        "report.zip",
        "report (2).zip",
        "report (3).zip",
      ]),
    ).toBe("report (4).zip");
  });

  it("matches case-insensitively", () => {
    expect(uniqueNameWithExt("Archive", ".zip", ["ARCHIVE.ZIP"])).toBe(
      "Archive (2).zip",
    );
  });
});
