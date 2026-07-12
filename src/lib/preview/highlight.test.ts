import { describe, it, expect } from "vitest";
import {
  highlightForFile,
  languageForExt,
  languageForName,
  ensureLanguage,
} from "./highlight";

describe("languageForExt", () => {
  it("maps known code extensions (loader exists)", () => {
    expect(languageForExt("ts")).toBe("typescript");
    expect(languageForExt("rs")).toBe("rust");
    expect(languageForExt("py")).toBe("python");
  });
  it("returns null for unmapped extensions", () => {
    expect(languageForExt("qqq")).toBeNull();
    expect(languageForExt("")).toBeNull();
  });
  it("maps the many added languages", () => {
    for (const [ext, lang] of [
      ["c", "c"], ["cpp", "cpp"], ["go", "go"], ["java", "java"], ["kt", "kotlin"],
      ["swift", "swift"], ["rb", "ruby"], ["php", "php"], ["scss", "scss"],
      ["graphql", "graphql"], ["scala", "scala"], ["hs", "haskell"], ["hx", "haxe"],
    ] as const) {
      expect(languageForExt(ext)).toBe(lang);
    }
  });
  it("maps extensions that share a grammar", () => {
    expect(languageForExt("edn")).toBe("clojure");
    expect(languageForExt("ron")).toBe("rust");
    expect(languageForExt("xslt")).toBe("xml");
    expect(languageForExt("json5")).toBe("json");
    expect(languageForExt("gradle")).toBe("groovy");
  });
  it("maps XML/JSON data formats to their grammars (CPE-094/206/207/208/211)", () => {
    expect(languageForExt("geojson")).toBe("json");
    expect(languageForExt("gpx")).toBe("xml");
    expect(languageForExt("kml")).toBe("xml");
    expect(languageForExt("musicxml")).toBe("xml");
    expect(languageForExt("plist")).toBe("xml");
  });
});

describe("languageForName", () => {
  it("resolves special filenames first, then extension", () => {
    expect(languageForName("Dockerfile")).toBe("dockerfile");
    expect(languageForName("Makefile")).toBe("makefile");
    expect(languageForName("CMakeLists.txt")).toBe("cmake");
    expect(languageForName(".gitconfig")).toBe("ini");
    expect(languageForName("app.go")).toBe("go");
    expect(languageForName("notes.txt")).toBeNull();
  });
});

describe("highlightForFile (lazy grammars)", () => {
  it("escapes plain text before/without a grammar", () => {
    const html = highlightForFile("a < b && c > d", "notes.txt");
    expect(html).toContain("&lt;");
    expect(html).toContain("&gt;");
    expect(html).not.toContain("hljs-");
  });

  it("never emits a raw script tag from the source", () => {
    expect(highlightForFile("<script>evil()</script>", "notes.txt")).not.toContain("<script>");
  });

  it("highlights once the grammar is ensured (TS, Go, Ruby, Scala, Nix)", async () => {
    await ensureLanguage("typescript");
    expect(highlightForFile("const x = 1;", "a.ts")).toContain("hljs-");
    await ensureLanguage("go");
    expect(highlightForFile("package main\nfunc main() {}", "a.go")).toContain("hljs-");
    await ensureLanguage("ruby");
    expect(highlightForFile("def hi; puts 'x'; end", "a.rb")).toContain("hljs-");
    await ensureLanguage("scala");
    expect(highlightForFile("object A { def f = 1 }", "a.scala")).toContain("hljs-");
    await ensureLanguage("nix");
    expect(highlightForFile('let msg = "hi"; in msg', "a.nix")).toContain("hljs-");
  });
});
