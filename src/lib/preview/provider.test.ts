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

  it("falls back to metadata for folders, nothing, and unknown types", () => {
    expect(pickProvider(entry({ name: "dir", is_dir: true, extension: "" })).kind).toBe("none");
    expect(pickProvider(null).kind).toBe("none");
    expect(pickProvider(undefined).kind).toBe("none");
    expect(pickProvider(entry({ name: "a.qqq", extension: "qqq" })).kind).toBe("none");
  });
});
