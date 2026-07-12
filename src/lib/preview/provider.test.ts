import { describe, it, expect } from "vitest";
import { pickProvider } from "./provider";
import type { DirEntry } from "../types";

const entry = (over: Partial<DirEntry>): DirEntry => ({
  name: "x",
  path: "/x",
  is_dir: false,
  size: 1,
  modified: 0,
  extension: "",
  hidden: false,
  ...over,
});

describe("pickProvider", () => {
  it("picks the image provider for image files", () => {
    expect(pickProvider(entry({ name: "a.png", extension: "png" })).kind).toBe("image");
    expect(pickProvider(entry({ name: "a.jpg", extension: "jpg" })).kind).toBe("image");
  });

  it("picks markdown before text for .md files", () => {
    expect(pickProvider(entry({ name: "readme.md", extension: "md" })).kind).toBe("markdown");
    expect(pickProvider(entry({ name: "notes.markdown", extension: "markdown" })).kind).toBe("markdown");
  });

  it("picks the text provider for text and code files", () => {
    expect(pickProvider(entry({ name: "a.txt", extension: "txt" })).kind).toBe("text");
    expect(pickProvider(entry({ name: "a.ts", extension: "ts" })).kind).toBe("text");
    expect(pickProvider(entry({ name: "a.css", extension: "css" })).kind).toBe("text");
  });

  it("picks media and pdf providers by category (CPE-059 phase 3)", () => {
    expect(pickProvider(entry({ name: "a.mp3", extension: "mp3" })).kind).toBe("audio");
    expect(pickProvider(entry({ name: "a.mp4", extension: "mp4" })).kind).toBe("video");
    expect(pickProvider(entry({ name: "a.pdf", extension: "pdf" })).kind).toBe("pdf");
  });

  it("picks json/csv before the generic text provider", () => {
    expect(pickProvider(entry({ name: "a.json", extension: "json" })).kind).toBe("json");
    expect(pickProvider(entry({ name: "a.csv", extension: "csv" })).kind).toBe("csv");
  });

  it("picks the archive provider for .zip (CPE-064)", () => {
    expect(pickProvider(entry({ name: "a.zip", extension: "zip" })).kind).toBe("archive");
  });

  it("picks the archive provider for zip-family/tar/gzip (CPE-109/112/217)", () => {
    for (const ext of ["jar", "apk", "war", "ipa", "xpi", "tar", "tgz", "gz"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("archive");
    }
  });

  it("picks the tsv provider and marks it editable (CPE-083)", () => {
    const p = pickProvider(entry({ name: "a.tsv", extension: "tsv" }));
    expect(p.kind).toBe("tsv");
    expect(p.editable).toBe(true);
  });

  it("marks text-based kinds editable and binary/media kinds not (CPE-067)", () => {
    expect(pickProvider(entry({ name: "a.txt", extension: "txt" })).editable).toBe(true);
    expect(pickProvider(entry({ name: "a.md", extension: "md" })).editable).toBe(true);
    expect(pickProvider(entry({ name: "a.json", extension: "json" })).editable).toBe(true);
    expect(pickProvider(entry({ name: "a.csv", extension: "csv" })).editable).toBe(true);
    expect(pickProvider(entry({ name: "a.png", extension: "png" })).editable).toBe(false);
    expect(pickProvider(entry({ name: "a.mp3", extension: "mp3" })).editable).toBe(false);
    expect(pickProvider(entry({ name: "a.zip", extension: "zip" })).editable).toBe(false);
    expect(pickProvider(entry({ name: "dir", is_dir: true })).editable).toBe(false);
  });

  it("renders bitmap/vector/animated images via the image provider (CPE-095/096/098/100/103)", () => {
    for (const ext of ["gif", "svg", "avif", "ico", "bmp"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("image");
    }
  });

  it("plays WAV/FLAC audio and MKV/MOV video via the media providers (CPE-104/105/107/108)", () => {
    expect(pickProvider(entry({ name: "a.wav", extension: "wav" })).kind).toBe("audio");
    expect(pickProvider(entry({ name: "a.flac", extension: "flac" })).kind).toBe("audio");
    expect(pickProvider(entry({ name: "a.mkv", extension: "mkv" })).kind).toBe("video");
    expect(pickProvider(entry({ name: "a.mov", extension: "mov" })).kind).toBe("video");
  });

  it("previews HTML and Jupyter notebooks as editable source (CPE-078/114)", () => {
    const html = pickProvider(entry({ name: "a.html", extension: "html" }));
    expect(html.kind).toBe("text");
    expect(html.editable).toBe(true);
    const nb = pickProvider(entry({ name: "a.ipynb", extension: "ipynb" }));
    expect(nb.kind).toBe("text");
    expect(nb.editable).toBe(true);
  });

  it("previews binary formats as read-only info text (CPE-210/214/215/216/218)", () => {
    for (const ext of ["exe", "dll", "wasm", "torrent", "mid", "midi", "bin", "dat"]) {
      const p = pickProvider(entry({ name: `a.${ext}`, extension: ext }));
      expect(p.kind).toBe("info");
      expect(p.editable).toBe(false);
    }
  });

  it("previews office/ebook documents as extracted text (CPE-070/071/072/077)", () => {
    for (const ext of ["rtf", "docx", "odt", "epub"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("info");
    }
  });

  it("falls back to metadata for folders, nothing, and unknown types", () => {
    expect(pickProvider(entry({ name: "dir", is_dir: true, extension: "" })).kind).toBe("none");
    expect(pickProvider(null).kind).toBe("none");
    expect(pickProvider(undefined).kind).toBe("none");
    expect(pickProvider(entry({ name: "a.qqq", extension: "qqq" })).kind).toBe("none");
  });
});
