import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
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

  it("renders the fallback (no img/pre) for a folder", () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "dir", is_dir: true, extension: "" }),
    });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector(".preview-text")).toBeNull();
  });
});
