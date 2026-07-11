import { describe, it, expect } from "vitest";
import { formatSize, friendlyError } from "./format";

describe("formatSize", () => {
  it("returns an empty string for zero bytes (directories)", () => {
    expect(formatSize(0)).toBe("");
  });

  it("formats bytes with no decimal place", () => {
    expect(formatSize(1)).toBe("1 B");
    expect(formatSize(999)).toBe("999 B");
  });

  it("rolls over to the next unit at exactly 1024", () => {
    expect(formatSize(1024)).toBe("1.0 KB");
    expect(formatSize(1023)).toBe("1023 B");
  });

  it("formats larger units with one decimal place", () => {
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(1024 * 1024)).toBe("1.0 MB");
    expect(formatSize(1024 * 1024 * 1024)).toBe("1.0 GB");
  });

  it("clamps at the largest known unit rather than inventing one", () => {
    const petabyte = 1024 ** 5;
    expect(formatSize(petabyte)).toContain("TB");
  });
});

describe("friendlyError", () => {
  it("maps Windows access-denied to a permission message", () => {
    expect(friendlyError("C:\\x: Access is denied. (os error 5)")).toBe(
      "Can't open this folder — permission denied.",
    );
  });

  it("maps Unix permission-denied to a permission message", () => {
    expect(friendlyError("/root: Permission denied (os error 13)")).toBe(
      "Can't open this folder — permission denied.",
    );
  });

  it("maps a missing path to a not-found message", () => {
    expect(friendlyError("/gone: No such file or directory (os error 2)")).toBe(
      "This folder no longer exists.",
    );
  });

  it("falls back to a generic message for unknown errors", () => {
    expect(friendlyError("something bizarre happened")).toBe(
      "Can't open this folder.",
    );
  });

  it("never leaks the raw error text", () => {
    const raw = "C:\\secret\\path: Access is denied. (os error 5)";
    expect(friendlyError(raw)).not.toContain("secret");
  });
});
