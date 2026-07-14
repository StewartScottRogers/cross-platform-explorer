import { describe, it, expect } from "vitest";
import { matchesGlob } from "./glob";

describe("matchesGlob (CPE-360)", () => {
  it("* matches any run including empty", () => {
    expect(matchesGlob("a.txt", "*.txt")).toBe(true);
    expect(matchesGlob(".txt", "*.txt")).toBe(true);
    expect(matchesGlob("a.md", "*.txt")).toBe(false);
    expect(matchesGlob("anything", "*")).toBe(true);
  });

  it("? matches exactly one character", () => {
    expect(matchesGlob("file1.log", "file?.log")).toBe(true);
    expect(matchesGlob("file12.log", "file?.log")).toBe(false);
    expect(matchesGlob("file.log", "file?.log")).toBe(false);
  });

  it("is case-insensitive", () => {
    expect(matchesGlob("README.MD", "*.md")).toBe(true);
  });

  it("treats other regex specials as literals", () => {
    expect(matchesGlob("a.b.txt", "a.b.txt")).toBe(true);
    expect(matchesGlob("axbxtxt", "a.b.txt")).toBe(false); // '.' is literal, not any-char
    expect(matchesGlob("report(1).pdf", "report(1).pdf")).toBe(true);
  });

  it("an empty/blank pattern matches nothing", () => {
    expect(matchesGlob("a.txt", "")).toBe(false);
    expect(matchesGlob("a.txt", "   ")).toBe(false);
  });
});
