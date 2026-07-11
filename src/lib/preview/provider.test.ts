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
    expect(pickProvider(entry({ name: "a.json", extension: "json" })).kind).toBe("text");
  });

  it("falls back to metadata for folders, nothing, and unknown types", () => {
    expect(pickProvider(entry({ name: "dir", is_dir: true, extension: "" })).kind).toBe("none");
    expect(pickProvider(null).kind).toBe("none");
    expect(pickProvider(undefined).kind).toBe("none");
    expect(pickProvider(entry({ name: "a.qqq", extension: "qqq" })).kind).toBe("none");
  });
});
