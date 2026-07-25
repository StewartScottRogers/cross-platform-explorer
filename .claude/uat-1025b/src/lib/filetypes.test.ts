import { describe, it, expect } from "vitest";
import { categoryOf, typeName, sameTypeIndices, matchesFileFilter, isImage, CATEGORY_BY_EXT, TYPE_NAME_BY_EXT } from "./filetypes";

const file = (extension: string) => ({ is_dir: false, extension });
const folder = { is_dir: true, extension: "" };

describe("isImage (CPE-643)", () => {
  it("is true for the thumbnailable raster extensions, case-insensitively", () => {
    for (const ext of ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "avif"]) {
      expect(isImage(`photo.${ext}`)).toBe(true);
      expect(isImage(`PHOTO.${ext.toUpperCase()}`)).toBe(true);
    }
  });
  it("uses the last extension of a multi-dot name", () => {
    expect(isImage("archive.tar.png")).toBe(true);
    expect(isImage("archive.png.tar")).toBe(false);
  });
  it("is false for non-image, vector, and unsupported formats", () => {
    for (const name of ["notes.txt", "clip.mp4", "logo.svg", "favicon.ico", "raw.heic", "song.mp3"]) {
      expect(isImage(name)).toBe(false);
    }
  });
  it("is false for extensionless names, dotfiles, and trailing dots", () => {
    expect(isImage("README")).toBe(false);
    expect(isImage(".png")).toBe(false);
    expect(isImage("photo.")).toBe(false);
    expect(isImage("")).toBe(false);
  });
});

describe("matchesFileFilter (CPE-358)", () => {
  it("'all' matches everything (incl. folders and unknown)", () => {
    expect(matchesFileFilter(folder, "all")).toBe(true);
    expect(matchesFileFilter(file("png"), "all")).toBe(true);
    expect(matchesFileFilter(file("xyz"), "all")).toBe(true);
  });
  it("filters to a single category", () => {
    expect(matchesFileFilter(file("png"), "image")).toBe(true);
    expect(matchesFileFilter(file("txt"), "image")).toBe(false);
    expect(matchesFileFilter(folder, "folder")).toBe(true);
    expect(matchesFileFilter(file("png"), "folder")).toBe(false);
  });
  it("groups broad categories (Documents spans pdf/doc/spreadsheet/text)", () => {
    expect(matchesFileFilter(file("pdf"), "document")).toBe(true);
    expect(matchesFileFilter(file("docx"), "document")).toBe(true);
    expect(matchesFileFilter(file("xlsx"), "document")).toBe(true);
    expect(matchesFileFilter(file("png"), "document")).toBe(false);
  });
  it("an unknown filter key matches everything", () => {
    expect(matchesFileFilter(file("png"), "bogus")).toBe(true);
  });
});

describe("common extensions (CPE-559 / CPE-563)", () => {
  it("classifies newly-added common formats (icon-only categories with correct previews)", () => {
    expect(categoryOf(file("psd"))).toBe("image");
    expect(categoryOf(file("epub"))).toBe("document");
    expect(categoryOf(file("mobi"))).toBe("document");
    expect(categoryOf(file("iso"))).toBe("archive");
    expect(categoryOf(file("dmg"))).toBe("archive");
    expect(categoryOf(file("cab"))).toBe("archive");
    expect(categoryOf(file("appimage"))).toBe("executable");
  });
  it("gives them readable type names", () => {
    expect(typeName(file("psd"))).toBe("Photoshop image");
    expect(typeName(file("epub"))).toBe("EPUB e-book");
    expect(typeName(file("iso"))).toBe("Disc image");
    expect(typeName(file("appimage"))).toBe("AppImage application");
  });
  it("leaves non-web audio/video formats uncategorised so their preview stays read-only info (CPE-563)", () => {
    // mid/midi/wma/mpeg etc. must NOT become audio/video — that would break their info preview.
    for (const ext of ["mid", "midi", "wma", "aiff", "mpeg", "3gp", "mts"]) {
      expect(categoryOf(file(ext))).toBe("unknown");
    }
  });
});

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

  it("classifies languages & templates as code (CPE-146/150/158/180/181/183/184/185)", () => {
    for (const ext of ["cbl", "cob", "cpy", "zig", "sol", "vue", "astro", "pug", "ejs", "liquid"]) {
      expect(categoryOf(file(ext))).toBe("code");
    }
    expect(typeName(file("zig"))).toBe("Zig source");
    expect(typeName(file("vue"))).toBe("Vue component");
  });

  it("classifies text-based data/comms formats as code (CPE-079/092/093/106/119/202..213)", () => {
    for (const ext of ["dot", "gv", "puml", "mmd", "fasta", "fa", "wkt", "eml", "ics", "vcf", "srt", "vtt", "pem", "crt", "reg"]) {
      expect(categoryOf(file(ext))).toBe("code");
    }
    expect(typeName(file("eml"))).toBe("Email message");
    expect(typeName(file("vcf"))).toBe("vCard contact");
    expect(typeName(file("reg"))).toBe("Registry export");
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

describe("sameTypeIndices", () => {
  const entries = [
    file("jpg"), // 0
    folder, // 1
    file("png"), // 2
    file("jpg"), // 3
    file(""), // 4 (extensionless)
  ];

  it("returns indices of non-dir entries sharing the extension", () => {
    expect(sameTypeIndices(entries, "jpg")).toEqual([0, 3]);
  });

  it("never matches directories", () => {
    // "" would match the extensionless file (4) but not the folder (1).
    expect(sameTypeIndices(entries, "")).toEqual([4]);
  });

  it("returns empty when nothing matches", () => {
    expect(sameTypeIndices(entries, "gif")).toEqual([]);
  });
});
