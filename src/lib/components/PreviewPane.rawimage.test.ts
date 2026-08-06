/**
 * CPE-1349 — camera-RAW embedded-preview provider: cr2/nef/arw route through the `raw-image`
 * provider kind to the `loadRawImageData` prop (backed by `read_raw_preview_data_url`), never
 * through the tiff/psd `loadImageData` (`read_image_data_url`) path. Mirrors the decoded-image
 * loader wiring, but a missing embedded preview (backend `Err`) must fall back to the metadata
 * slot rather than the "can't preview" error note — this suite asserts that divergence.
 */
import { describe, it, expect, vi } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
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
  is_symlink: false,
  ...over,
});

describe("PreviewPane — camera-RAW embedded preview (CPE-1349)", () => {
  it("requests the embedded preview via loadRawImageData (not loadImageData) and renders it", async () => {
    const loadRawImageData = vi.fn(async () => "data:image/jpeg;base64,AAAA");
    const loadImageData = vi.fn(async () => "");

    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.cr2", path: "/photos/a.cr2", extension: "cr2" }),
      loadRawImageData,
      loadImageData,
    });

    await waitFor(() => expect(container.querySelector("img.preview-img")).toBeTruthy());
    expect(loadRawImageData).toHaveBeenCalledWith("/photos/a.cr2");
    expect(loadImageData).not.toHaveBeenCalled();

    const img = container.querySelector("img.preview-img") as HTMLImageElement;
    expect(img.src).toBe("data:image/jpeg;base64,AAAA");
  });

  it("routes .nef and .arw through the same raw-image path", async () => {
    for (const [ext, path] of [["nef", "/photos/b.nef"], ["arw", "/photos/c.arw"]] as const) {
      const loadRawImageData = vi.fn(async () => "data:image/jpeg;base64,BBBB");

      const { container } = render(PreviewPane, {
        entry: entry({ name: `x.${ext}`, path, extension: ext }),
        loadRawImageData,
      });

      await waitFor(() => expect(loadRawImageData).toHaveBeenCalledWith(path));
      await waitFor(() => expect(container.querySelector("img.preview-img")).toBeTruthy());
    }
  });

  it("falls back to the metadata slot (not the tiff/psd error note) when there is no embedded preview", async () => {
    const loadRawImageData = vi.fn(async () => {
      throw new Error("no embedded JPEG preview found in this raw file");
    });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.cr2", path: "/photos/a.cr2", extension: "cr2" }),
      loadRawImageData,
    });

    await waitFor(() => expect(loadRawImageData).toHaveBeenCalled());
    await waitFor(() => {
      // Never renders the tiff/psd "can't preview" note for a RAW miss — the pane must not break;
      // it defers to the (here empty) metadata slot instead.
      expect(container.textContent).not.toContain("Can't preview this image.");
    });
    expect(container.querySelector("img.preview-img")).toBeNull();
  });

  it("drops a stale response when the selection changes mid-flight (generation guard)", async () => {
    let resolveFirst!: (v: string) => void;
    const loadRawImageData = vi
      .fn()
      .mockImplementationOnce(() => new Promise<string>((resolve) => { resolveFirst = resolve; }))
      .mockResolvedValueOnce("data:image/jpeg;base64,SECOND");

    const { container, rerender } = render(PreviewPane, {
      entry: entry({ name: "first.cr2", path: "/photos/first.cr2", extension: "cr2" }),
      loadRawImageData,
    });
    await waitFor(() => expect(loadRawImageData).toHaveBeenCalledWith("/photos/first.cr2"));

    await rerender({ entry: entry({ name: "second.cr2", path: "/photos/second.cr2", extension: "cr2" }) });
    await waitFor(() => expect(loadRawImageData).toHaveBeenCalledWith("/photos/second.cr2"));
    await waitFor(() => expect(container.querySelector("img.preview-img")).toBeTruthy());

    // The superseded first request now resolves — it must NOT clobber the second file's preview.
    resolveFirst("data:image/jpeg;base64,STALE");
    await new Promise((r) => setTimeout(r, 0));

    const img = container.querySelector("img.preview-img") as HTMLImageElement;
    expect(img.src).toBe("data:image/jpeg;base64,SECOND");
  });
});
