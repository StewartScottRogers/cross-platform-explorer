import { describe, it, expect } from "vitest";
import { categoryOf, typeName } from "./filetypes";

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
});
