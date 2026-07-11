import { describe, it, expect } from "vitest";
import { uniqueName } from "./naming";

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
