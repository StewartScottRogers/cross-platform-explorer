import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import PreviewPane from "./PreviewPane.svelte";
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

describe("PreviewPane", () => {
  it("renders an <img> for an image entry using assetUrl", () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "pic.png", path: "C:\\d\\pic.png", extension: "png" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const img = container.querySelector("img.preview-img") as HTMLImageElement | null;
    expect(img).toBeTruthy();
    expect(img!.getAttribute("src")).toBe("asset://C:\\d\\pic.png");
    expect(img!.getAttribute("alt")).toBe("pic.png");
  });

  it("loads and renders text for a text entry", async () => {
    const loadText = vi.fn(async () => "hello from the file");
    render(PreviewPane, {
      entry: entry({ name: "a.txt", path: "C:\\d\\a.txt", extension: "txt" }),
      loadText,
    });

    await waitFor(() => expect(screen.getByText("hello from the file")).toBeTruthy());
    expect(loadText).toHaveBeenCalledWith("C:\\d\\a.txt");
  });

  it("shows an error state when the text load fails", async () => {
    const loadText = vi.fn(async () => {
      throw new Error("nope");
    });
    render(PreviewPane, {
      entry: entry({ name: "a.txt", path: "C:\\d\\a.txt", extension: "txt" }),
      loadText,
    });

    await waitFor(() => expect(screen.getByText(/Can't preview/i)).toBeTruthy());
  });

  it("renders an <audio> element for an audio entry", () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "song.mp3", path: "C:\\d\\song.mp3", extension: "mp3" }),
      assetUrl: (p: string) => `asset://${p}`,
    });
    const audio = container.querySelector("audio.preview-media");
    expect(audio).toBeTruthy();
    expect(audio!.getAttribute("src")).toBe("asset://C:\\d\\song.mp3");
  });

  it("renders a <video> element for a video entry", () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "clip.mp4", path: "C:\\d\\clip.mp4", extension: "mp4" }),
      assetUrl: (p: string) => `asset://${p}`,
    });
    expect(container.querySelector("video.preview-media")).toBeTruthy();
  });

  it("renders an <iframe> for a pdf entry", () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "doc.pdf", path: "C:\\d\\doc.pdf", extension: "pdf" }),
      assetUrl: (p: string) => `asset://${p}`,
    });
    const frame = container.querySelector("iframe.preview-pdf");
    expect(frame).toBeTruthy();
    expect(frame!.getAttribute("src")).toBe("asset://C:\\d\\doc.pdf");
  });

  it("pretty-prints JSON", async () => {
    const loadText = vi.fn(async () => '{"a":1}');
    const { container } = render(PreviewPane, {
      entry: entry({ name: "d.json", path: "C:\\d\\d.json", extension: "json" }),
      loadText,
    });
    await waitFor(() => {
      const pre = container.querySelector(".preview-text");
      expect(pre?.textContent).toContain('"a": 1'); // note the space — pretty-printed
    });
  });

  it("renders a CSV as a table", async () => {
    const loadText = vi.fn(async () => "a,b\n1,2");
    const { container } = render(PreviewPane, {
      entry: entry({ name: "d.csv", path: "C:\\d\\d.csv", extension: "csv" }),
      loadText,
    });
    await waitFor(() => {
      const cells = [...container.querySelectorAll(".preview-table td")].map((c) => c.textContent);
      expect(cells).toEqual(["a", "b", "1", "2"]);
    });
  });

  it("syntax-highlights a code file (CPE-065)", async () => {
    const loadText = vi.fn(async () => "const x = 1;");
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.ts", path: "C:\\d\\a.ts", extension: "ts" }),
      loadText,
    });
    await waitFor(() => {
      expect(container.querySelector(".preview-text code span")).toBeTruthy();
    });
  });

  it("renders markdown to HTML (CPE-065)", async () => {
    const loadText = vi.fn(async () => "# Heading\n\n**bold**");
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.md", path: "C:\\d\\a.md", extension: "md" }),
      loadText,
    });
    await waitFor(() => {
      const h1 = container.querySelector(".preview-markdown h1");
      expect(h1?.textContent).toBe("Heading");
      expect(container.querySelector(".preview-markdown strong")).toBeTruthy();
    });
  });

  it("sanitizes script in previewed markdown (CPE-065)", async () => {
    const loadText = vi.fn(async () => "# Safe\n\n<script>window.evil=1</script>");
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.md", path: "C:\\d\\a.md", extension: "md" }),
      loadText,
    });
    await waitFor(() => expect(container.querySelector(".preview-markdown h1")).toBeTruthy());
    expect(container.querySelector(".preview-markdown script")).toBeNull();
  });

  it("renders a TSV as a table (CPE-083)", async () => {
    const loadText = vi.fn(async () => "a\tb\n1\t2");
    const { container } = render(PreviewPane, {
      entry: entry({ name: "d.tsv", path: "C:\\d\\d.tsv", extension: "tsv" }),
      loadText,
    });
    await waitFor(() => {
      const cells = [...container.querySelectorAll(".preview-table td")].map((c) => c.textContent);
      expect(cells).toEqual(["a", "b", "1", "2"]);
    });
  });

  it("lists archive entries via loadEntries (CPE-064)", async () => {
    const loadEntries = vi.fn(async () => [
      { name: "hello.txt", size: 8, is_dir: false },
      { name: "sub/", size: 0, is_dir: true },
    ]);
    const { container } = render(PreviewPane, {
      entry: entry({ name: "b.zip", path: "C:\\d\\b.zip", extension: "zip" }),
      loadEntries,
    });

    await waitFor(() => {
      const firstCells = [...container.querySelectorAll(".preview-table tr td")].map(
        (c) => c.textContent,
      );
      expect(firstCells).toContain("hello.txt");
      expect(firstCells).toContain("sub/");
    });
    expect(loadEntries).toHaveBeenCalledWith("C:\\d\\b.zip");
  });

  it("shows an Edit button for an editable file and none for an image (CPE-068)", async () => {
    const { container: editableC } = render(PreviewPane, {
      entry: entry({ name: "a.txt", path: "C:\\d\\a.txt", extension: "txt" }),
      loadText: async () => "hello",
    });
    await waitFor(() =>
      expect([...editableC.querySelectorAll("button")].some((b) => b.textContent === "Edit")).toBe(true),
    );

    const { container: imageC } = render(PreviewPane, {
      entry: entry({ name: "p.png", path: "C:\\d\\p.png", extension: "png" }),
      assetUrl: (p: string) => `asset://${p}`,
    });
    expect([...imageC.querySelectorAll("button")].some((b) => b.textContent === "Edit")).toBe(false);
  });

  it("edits and saves via saveText (CPE-068)", async () => {
    const saveText = vi.fn(async () => {});
    const { container, getByText } = render(PreviewPane, {
      entry: entry({ name: "a.txt", path: "C:\\d\\a.txt", extension: "txt" }),
      loadText: async () => "original",
      saveText,
    });

    await waitFor(() => getByText("Edit"));
    await fireEvent.click(getByText("Edit"));

    const textarea = (await waitFor(() => {
      const el = container.querySelector("textarea.preview-editor");
      expect(el).toBeTruthy();
      return el as HTMLTextAreaElement;
    }));
    await fireEvent.input(textarea, { target: { value: "edited content" } });
    await fireEvent.click(getByText("Save"));

    await waitFor(() => expect(saveText).toHaveBeenCalledWith("C:\\d\\a.txt", "edited content"));
  });

  it("renders the fallback (no img/pre) for a folder", () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "dir", is_dir: true, extension: "" }),
    });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector(".preview-text")).toBeNull();
  });
});
