import { describe, it, expect } from "vitest";
import { checksumMatches, normalizeDigest } from "./checksum";

const A = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

describe("checksum verify (CPE-413)", () => {
  it("normalizeDigest strips all whitespace and lowercases", () => {
    expect(normalizeDigest("  BA78 16BF\n8f01 ")).toBe("ba7816bf8f01");
  });

  it("matches case-insensitively and tolerant of whitespace", () => {
    expect(checksumMatches(A, A.toUpperCase())).toBe(true);
    expect(checksumMatches(A, `  ${A}  `)).toBe(true);
    // Some sites print the digest in space-separated groups.
    expect(checksumMatches(A, "ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad")).toBe(true);
  });

  it("reports a mismatch for a different digest", () => {
    expect(checksumMatches(A, A.replace(/.$/, "0"))).toBe(false);
  });

  it("is neutral (null) when either side is empty", () => {
    expect(checksumMatches(A, "")).toBeNull();
    expect(checksumMatches("", A)).toBeNull();
    expect(checksumMatches("", "")).toBeNull();
  });
});
