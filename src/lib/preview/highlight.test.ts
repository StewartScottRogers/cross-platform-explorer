import { describe, it, expect } from "vitest";
import { highlightCode, languageForExt } from "./highlight";

describe("languageForExt", () => {
  it("maps known code extensions", () => {
    expect(languageForExt("ts")).toBe("typescript");
    expect(languageForExt("rs")).toBe("rust");
    expect(languageForExt("py")).toBe("python");
  });
  it("returns null for unmapped extensions", () => {
    expect(languageForExt("txt")).toBeNull();
    expect(languageForExt("")).toBeNull();
  });
});

describe("highlightCode", () => {
  it("produces hljs token spans for a known language", () => {
    const html = highlightCode("const x = 1;", "ts");
    expect(html).toContain("hljs-");
  });

  it("escapes and does not add spans for an unmapped type", () => {
    const html = highlightCode("a < b && c > d", "txt");
    expect(html).toContain("&lt;");
    expect(html).toContain("&gt;");
    expect(html).not.toContain("hljs-");
  });

  it("never emits a raw unescaped angle bracket from the source", () => {
    const html = highlightCode("<script>evil()</script>", "txt");
    expect(html).not.toContain("<script>");
  });
});
