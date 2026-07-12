import { describe, it, expect } from "vitest";
import { categoryOf, typeName, CATEGORY_BY_EXT, TYPE_NAME_BY_EXT } from "./filetypes";

const file = (extension: string) => ({ is_dir: false, extension });
const folder = { is_dir: true, extension: "" };

describe("categoryOf", () => {
  it("classifies folders", () => {
    expect(categoryOf(folder)).toBe("folder");
  });

  it("classifies common categories", () => {
    expect(categoryOf(file("png"))).toBe("image");
    expect(categoryOf(file("docx"))).toBe("document");
    expect(categoryOf(file("xlsx"))).toBe("spreadsheet");
    expect(categoryOf(file("pdf"))).toBe("pdf");
    expect(categoryOf(file("rs"))).toBe("code");
    expect(categoryOf(file("zip"))).toBe("archive");
    expect(categoryOf(file("mp3"))).toBe("audio");
    expect(categoryOf(file("mp4"))).toBe("video");
    expect(categoryOf(file("md"))).toBe("text");
  });

  it("falls back to unknown for unrecognised extensions", () => {
    expect(categoryOf(file("qqq"))).toBe("unknown");
    expect(categoryOf(file(""))).toBe("unknown");
  });

  it("classifies the newly added common extensions (CPE-048)", () => {
    expect(categoryOf(file("heic"))).toBe("image");
    expect(categoryOf(file("avif"))).toBe("image");
    expect(categoryOf(file("aac"))).toBe("audio");
    expect(categoryOf(file("opus"))).toBe("audio");
    expect(categoryOf(file("wmv"))).toBe("video");
    expect(categoryOf(file("m4v"))).toBe("video");
    expect(categoryOf(file("xz"))).toBe("archive");
    expect(categoryOf(file("tgz"))).toBe("archive");
    expect(categoryOf(file("mjs"))).toBe("code");
    expect(categoryOf(file("cjs"))).toBe("code");
  });

  it("classifies the common-bundle languages as code (CPE-120..178)", () => {
    for (const ext of ["c", "cpp", "go", "java", "kt", "swift", "rb", "php", "lua", "r", "scss", "less", "graphql"]) {
      expect(categoryOf(file(ext))).toBe("code");
    }
  });

  it("classifies XML/JSON data formats as code (CPE-082/094/206/207/208/211)", () => {
    for (const ext of ["xml", "geojson", "gpx", "kml", "musicxml", "plist"]) {
      expect(categoryOf(file(ext))).toBe("code");
    }
  });

  it("classifies config/infra formats as code (CPE-080/081/191/192/193)", () => {
    for (const ext of ["yaml", "yml", "toml", "tf", "hcl", "tfvars", "dhall", "jsonnet", "libsonnet"]) {
      expect(categoryOf(file(ext))).toBe("code");
    }
  });

  it("classifies markup/doc formats as code (CPE-073..076/188/189/190)", () => {
    for (const ext of ["tex", "rst", "adoc", "asciidoc", "org", "mdx", "textile", "bib"]) {
      expect(categoryOf(file(ext))).toBe("code");
    }
    expect(typeName(file("tex"))).toBe("LaTeX source");
    expect(typeName(file("bib"))).toBe("BibTeX bibliography");
  });

  it("classifies .editorconfig by name (CPE-199)", () => {
    expect(categoryOf({ is_dir: false, extension: "", name: ".editorconfig" })).toBe("code");
  });

  it("classifies well-known code files by name (CPE-164/166/200)", () => {
    expect(categoryOf({ is_dir: false, extension: "", name: "Dockerfile" })).toBe("code");
    expect(categoryOf({ is_dir: false, extension: "", name: "Makefile" })).toBe("code");
    expect(categoryOf({ is_dir: false, extension: "", name: ".gitignore" })).toBe("code");
    expect(categoryOf({ is_dir: false, extension: "", name: "randomfile" })).toBe("unknown");
  });

  it("classifies executables and installers (CPE-047)", () => {
    expect(categoryOf(file("exe"))).toBe("executable");
    expect(categoryOf(file("msi"))).toBe("executable");
    expect(categoryOf(file("dll"))).toBe("executable");
  });
});

describe("table consistency (CPE-047)", () => {
  it("every named type also has an icon category", () => {
    const missing = Object.keys(TYPE_NAME_BY_EXT).filter(
      (ext) => !(ext in CATEGORY_BY_EXT),
    );
    expect(missing).toEqual([]);
  });
});

describe("typeName", () => {
  it("names folders like Explorer does", () => {
    expect(typeName(folder)).toBe("File folder");
  });

  it("gives friendly names to known types", () => {
    expect(typeName(file("png"))).toBe("PNG image");
    expect(typeName(file("md"))).toBe("Markdown file");
    expect(typeName(file("slnx"))).toBe("Visual Studio Solution");
  });

  it("falls back to 'EXT File' for unknown extensions", () => {
    expect(typeName(file("qqq"))).toBe("QQQ File");
  });

  it("calls an extensionless file just 'File'", () => {
    expect(typeName(file(""))).toBe("File");
  });

  it("names XML/JSON data formats (CPE-082/094/206/207/208/211)", () => {
    expect(typeName(file("geojson"))).toBe("GeoJSON file");
    expect(typeName(file("gpx"))).toBe("GPX GPS track");
    expect(typeName(file("kml"))).toBe("KML geographic data");
    expect(typeName(file("musicxml"))).toBe("MusicXML score");
    expect(typeName(file("plist"))).toBe("Apple property list");
  });

  it("names the newly added common extensions (CPE-048)", () => {
    expect(typeName(file("heic"))).toBe("HEIC image");
    expect(typeName(file("aac"))).toBe("AAC audio");
    expect(typeName(file("wmv"))).toBe("Windows Media Video");
    expect(typeName(file("zst"))).toBe("Zstandard archive");
    expect(typeName(file("mjs"))).toBe("JavaScript module");
  });
});
