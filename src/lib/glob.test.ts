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

  it("matches ANY of several comma-separated patterns (CPE-571)", () => {
    expect(matchesGlob("photo.jpg", "*.jpg, *.png")).toBe(true);
    expect(matchesGlob("icon.png", "*.jpg, *.png")).toBe(true);
    expect(matchesGlob("notes.txt", "*.jpg, *.png")).toBe(false);
    // blanks between commas are ignored; a wholly-blank list still matches nothing.
    expect(matchesGlob("a.txt", "*.txt, , ")).toBe(true);
    expect(matchesGlob("a.txt", " , ")).toBe(false);
  });

  it("supports !-prefixed exclusions (CPE-578)", () => {
    // include minus exclude
    expect(matchesGlob("app.js", "*.js, !*.min.js")).toBe(true);
    expect(matchesGlob("app.min.js", "*.js, !*.min.js")).toBe(false);
    // only exclusions ⇒ everything except them
    expect(matchesGlob("a.txt", "!*.tmp")).toBe(true);
    expect(matchesGlob("a.tmp", "!*.tmp")).toBe(false);
    // a bare "!" (no pattern after) is ignored, not a match-all exclude
    expect(matchesGlob("a.txt", "*.txt, !")).toBe(true);
  });
});
