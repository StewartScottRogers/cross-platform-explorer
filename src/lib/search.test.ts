import { describe, it, expect } from "vitest";
import { matchesQuery } from "./search";

describe("matchesQuery", () => {
  it("matches everything for an empty query", () => {
    expect(matchesQuery("anything.txt", "")).toBe(true);
    expect(matchesQuery("anything.txt", "   ")).toBe(true);
  });

  it("does a case-insensitive substring match for plain queries", () => {
    expect(matchesQuery("Report-2026.pdf", "report")).toBe(true);
    expect(matchesQuery("Report-2026.pdf", "2026")).toBe(true);
    expect(matchesQuery("Report-2026.pdf", "budget")).toBe(false);
  });

  it("treats * as any run of characters (anchored full match)", () => {
    expect(matchesQuery("notes.txt", "*.txt")).toBe(true);
    expect(matchesQuery("a.txtx", "*.txt")).toBe(false); // anchored, not substring
    expect(matchesQuery("archive.tar.gz", "*.gz")).toBe(true);
    expect(matchesQuery("img_2026.jpg", "img_*")).toBe(true);
  });

  it("treats ? as exactly one character", () => {
    expect(matchesQuery("report1.md", "report?.md")).toBe(true);
    expect(matchesQuery("reportA.md", "report?.md")).toBe(true);
    expect(matchesQuery("report10.md", "report?.md")).toBe(false);
    expect(matchesQuery("report.md", "report?.md")).toBe(false);
  });

  it("is case-insensitive for wildcard queries", () => {
    expect(matchesQuery("PHOTO.JPG", "*.jpg")).toBe(true);
  });

  it("treats regex metacharacters in the query literally", () => {
    expect(matchesQuery("a+b.txt", "a+b.*")).toBe(true);
    expect(matchesQuery("axb.txt", "a+b.*")).toBe(false); // '+' is literal, not "one or more"
    expect(matchesQuery("file(1).txt", "file(1)*")).toBe(true);
  });
});
