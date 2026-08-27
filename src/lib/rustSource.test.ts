import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { stripRustComments, rustStringLiteralAfter, rustStrSliceAfter } from "./rustSource";

/**
 * CPE-1950. `stripRustComments` / `rustStringLiteralAfter` came out of
 * `components/MacroRunConfirm.test.ts` (which still exercises them end-to-end against the real
 * `fsutil.rs`); `rustStrSliceAfter` is new here. These are the unit-level pins, including the
 * adversarial shapes that motivated the comment stripping in the first place.
 */
describe("stripRustComments", () => {
  it("blanks a line comment but keeps every offset", () => {
    const src = 'let a = 1; // let a = 2;\nlet b = 3;';
    const out = stripRustComments(src);
    expect(out.length).toBe(src.length);
    expect(out).not.toContain("let a = 2");
    expect(out).toContain("let b = 3");
  });

  it("blanks a block comment", () => {
    expect(stripRustComments("a /* hidden */ b")).toBe("a              b");
  });

  it("leaves a // inside a string literal alone", () => {
    expect(stripRustComments('let u = "https://example.com/x";')).toBe(
      'let u = "https://example.com/x";',
    );
  });
});

describe("rustStringLiteralAfter", () => {
  it("resolves \\\" and \\\\", () => {
    expect(rustStringLiteralAfter('x("a \\"q\\" b\\\\c")', 0)).toBe('a "q" b\\c');
  });

  it("swallows a backslash-at-end-of-line continuation AND the next line's indentation", () => {
    // The part a naive join gets wrong: without this the leading spaces land inside the string.
    const src = 'x("one \\\n                 two")';
    expect(rustStringLiteralAfter(src, 0)).toBe("one two");
  });
});

describe("rustStrSliceAfter", () => {
  const CONST = 'pub const T: &[&str] = &["macos", "windows", "linux"];';

  it("reads every element of a &[&str] slice literal", () => {
    expect(rustStrSliceAfter(CONST, "pub const T")).toEqual(["macos", "windows", "linux"]);
  });

  it("reads a slice written across several lines", () => {
    const multi = ["pub const T: &[&str] = &[", '    "a",', '    "b",', "];"].join("\n");
    expect(rustStrSliceAfter(multi, "pub const T")).toEqual(["a", "b"]);
  });

  it("throws — loudly — when the anchor is gone, rather than deriving an empty list", () => {
    // A renamed const must red. An empty derived list would match nothing and pass vacuously, which
    // is the failure mode CPE-1932 calls out: enumerate, don't recall.
    expect(() => rustStrSliceAfter(CONST, "pub const RENAMED")).toThrow(/anchor not found/);
  });

  it("a comment quoting the OLD list cannot be mistaken for the real one (stripped first)", () => {
    const hostile = [
      '// Was: pub const T: &[&str] = &["stale"];',
      'pub const T: &[&str] = &["current"];',
    ].join("\n");
    // Raw source: the comment's copy comes first and wins -- the silent-wrong-value class.
    expect(rustStrSliceAfter(hostile, "pub const T")).toEqual(["stale"]);
    // Stripped first (what every caller does): the real declaration is what is read.
    expect(rustStrSliceAfter(stripRustComments(hostile), "pub const T")).toEqual(["current"]);
  });

  it("reads the real TAURI_PLATFORM_TOKENS out of the shipped guard", () => {
    const src = stripRustComments(
      readFileSync(
        join(process.cwd(), "crates", "updater-verify", "src", "platform_config_guard.rs"),
        "utf8",
      ),
    );
    expect(rustStrSliceAfter(src, "pub const TAURI_PLATFORM_TOKENS")).toEqual([
      "macos",
      "windows",
      "linux",
      "android",
      "ios",
    ]);
  });
});
