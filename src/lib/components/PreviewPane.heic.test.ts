/**
 * CPE-1351 — HEIC/HEIF platform-API preview provider: heic/heif/hif route through the `heic`
 * provider kind to the `loadHeicImageData` prop (backed by `read_heic_preview_data_url`), never
 * through the tiff/psd `loadImageData` path. A decode failure (backend `Err` — commonly a missing
 * OS HEIF codec on Windows) must fall back to the metadata slot rather than the "can't preview"
 * error note — this suite asserts that divergence and the generation guard.
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

describe("PreviewPane — HEIC/HEIF platform-API preview (CPE-1351)", () => {
  it("requests the decode via loadHeicImageData (not loadImageData) and renders it", async () => {
    const loadHeicImageData = vi.fn(async () => "data:image/png;base64,AAAA");
    const loadImageData = vi.fn(async () => "");

    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.heic", path: "/photos/a.heic", extension: "heic" }),
      loadHeicImageData,
      loadImageData,
    });

    await waitFor(() => expect(container.querySelector("img.preview-img")).toBeTruthy());
    expect(loadHeicImageData).toHaveBeenCalledWith("/photos/a.heic");
    expect(loadImageData).not.toHaveBeenCalled();

    const img = container.querySelector("img.preview-img") as HTMLImageElement;
    expect(img.src).toBe("data:image/png;base64,AAAA");
  });

  it("routes .heif and .hif through the same heic path", async () => {
    for (const [ext, path] of [["heif", "/photos/b.heif"], ["hif", "/photos/c.hif"]] as const) {
      const loadHeicImageData = vi.fn(async () => "data:image/png;base64,BBBB");

      const { container } = render(PreviewPane, {
        entry: entry({ name: `x.${ext}`, path, extension: ext }),
        loadHeicImageData,
      });

      await waitFor(() => expect(loadHeicImageData).toHaveBeenCalledWith(path));
      await waitFor(() => expect(container.querySelector("img.preview-img")).toBeTruthy());
    }
  });

  it("falls back to the metadata slot (not the error note) when the platform can't decode (no HEIF codec)", async () => {
    const loadHeicImageData = vi.fn(async () => {
      throw new Error("HEIC decode unavailable: install the HEIF Image Extensions");
    });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.heic", path: "/photos/a.heic", extension: "heic" }),
      loadHeicImageData,
    });

    await waitFor(() => expect(loadHeicImageData).toHaveBeenCalled());
    await waitFor(() => {
      expect(container.textContent).not.toContain("Can't preview this image.");
    });
    expect(container.querySelector("img.preview-img")).toBeNull();
  });

  it("drops a stale response when the selection changes mid-flight (generation guard)", async () => {
    let resolveFirst!: (v: string) => void;
    const loadHeicImageData = vi
      .fn()
      .mockImplementationOnce(() => new Promise<string>((resolve) => { resolveFirst = resolve; }))
      .mockResolvedValueOnce("data:image/png;base64,SECOND");

    const { container, rerender } = render(PreviewPane, {
      entry: entry({ name: "first.heic", path: "/photos/first.heic", extension: "heic" }),
      loadHeicImageData,
    });
    await waitFor(() => expect(loadHeicImageData).toHaveBeenCalledWith("/photos/first.heic"));

    await rerender({ entry: entry({ name: "second.heic", path: "/photos/second.heic", extension: "heic" }) });
    await waitFor(() => expect(loadHeicImageData).toHaveBeenCalledWith("/photos/second.heic"));
    await waitFor(() => expect(container.querySelector("img.preview-img")).toBeTruthy());

    // The superseded first request now resolves — it must NOT clobber the second file's preview.
    resolveFirst("data:image/png;base64,STALE");
    await new Promise((r) => setTimeout(r, 0));

    const img = container.querySelector("img.preview-img") as HTMLImageElement;
    expect(img.src).toBe("data:image/png;base64,SECOND");
  });
});
