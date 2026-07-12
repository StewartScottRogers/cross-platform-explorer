import { describe, it, expect } from "vitest";
import { highlightCode, languageForExt, languageForName } from "./highlight";

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

  it("maps the common-bundle languages (CPE-120..178)", () => {
    expect(languageForExt("c")).toBe("c");
    expect(languageForExt("cpp")).toBe("cpp");
    expect(languageForExt("go")).toBe("go");
    expect(languageForExt("java")).toBe("java");
    expect(languageForExt("kt")).toBe("kotlin");
    expect(languageForExt("swift")).toBe("swift");
    expect(languageForExt("rb")).toBe("ruby");
    expect(languageForExt("php")).toBe("php");
    expect(languageForExt("scss")).toBe("scss");
    expect(languageForExt("graphql")).toBe("graphql");
  });
});

describe("highlightCode", () => {
  it("produces hljs token spans for a known language", () => {
    const html = highlightCode("const x = 1;", "ts");
    expect(html).toContain("hljs-");
  });

  it("highlights newly added languages (Go, Ruby)", () => {
    expect(highlightCode("package main\nfunc main() {}", "go")).toContain("hljs-");
    expect(highlightCode("def hi; puts 'x'; end", "rb")).toContain("hljs-");
  });

  it("highlights individually-registered grammars (Scala, Haskell, Elixir, Nix)", () => {
    expect(highlightCode("object A { def f = 1 }", "scala")).toContain("hljs-");
    expect(highlightCode("main = putStrLn \"hi\"", "hs")).toContain("hljs-");
    expect(highlightCode("defmodule A do end", "ex")).toContain("hljs-");
    expect(highlightCode('let msg = "hi"; in msg', "nix")).toContain("hljs-");
  });

  it("resolves language from special filenames, then extension (CPE-164/166/200)", () => {
    expect(languageForName("Dockerfile")).toBe("dockerfile");
    expect(languageForName("Makefile")).toBe("makefile");
    expect(languageForName("CMakeLists.txt")).toBe("cmake");
    expect(languageForName(".gitconfig")).toBe("ini");
    expect(languageForName("app.go")).toBe("go"); // falls through to extension
    expect(languageForName("notes.txt")).toBeNull();
  });

  it("maps extensions that share a registered grammar", () => {
    expect(languageForExt("edn")).toBe("clojure");
    expect(languageForExt("ron")).toBe("rust");
    expect(languageForExt("xslt")).toBe("xml");
    expect(languageForExt("json5")).toBe("json");
    expect(languageForExt("gradle")).toBe("groovy");
    expect(languageForExt("hx")).toBe("haxe");
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
