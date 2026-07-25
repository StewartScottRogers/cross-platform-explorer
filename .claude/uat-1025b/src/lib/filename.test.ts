import { describe, it, expect } from "vitest";
import { validateFileName } from "./filename";

describe("validateFileName", () => {
  it("accepts ordinary names", () => {
    expect(validateFileName("report.txt")).toBeNull();
    expect(validateFileName("My Folder")).toBeNull();
    expect(validateFileName("data-2026_v2.tar.gz")).toBeNull();
  });

  it("rejects empty or whitespace-only names", () => {
    expect(validateFileName("")).toMatch(/empty/i);
    expect(validateFileName("   ")).toMatch(/empty/i);
  });

  it("rejects each illegal character", () => {
    for (const ch of ['<', '>', ':', '"', "/", "\\", "|", "?", "*"]) {
      expect(validateFileName(`bad${ch}name`)).toMatch(/can't contain/i);
    }
  });

  it("rejects control characters", () => {
    const withCtrl = `bad${String.fromCharCode(1)}name`;
    expect(validateFileName(withCtrl)).toMatch(/control character/i);
  });

  it("rejects a trailing space or period", () => {
    expect(validateFileName("name ")).toMatch(/space or a period/i);
    expect(validateFileName("name.")).toMatch(/space or a period/i);
  });

  it("rejects Windows reserved device names, any casing, with or without extension", () => {
    expect(validateFileName("CON")).toMatch(/reserved/i);
    expect(validateFileName("nul")).toMatch(/reserved/i);
    expect(validateFileName("Com1.txt")).toMatch(/reserved/i);
    expect(validateFileName("LPT9")).toMatch(/reserved/i);
  });

  it("does not treat reserved-name substrings as reserved", () => {
    expect(validateFileName("console.log")).toBeNull();
    expect(validateFileName("connection")).toBeNull();
  });
});
