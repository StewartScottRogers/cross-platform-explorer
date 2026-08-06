/**
 * CPE-1350 — DICOM preview provider: `.dcm` routes through the `dicom` provider kind to
 * `loadDicomImageData` (backed by `read_dicom_image_data_url`) for the decoded pixel-data image,
 * plus `loadDicomTags` (backed by `read_dicom_tags`) for the curated tag list shown alongside it.
 * Mirrors the camera-RAW embedded-preview wiring (CPE-1349), but adds the tag list and its own
 * independent load/fallback: a missing/undecodable image (backend `Err`) must fall back to the
 * metadata slot rather than an error note, while the tags — read structurally, not from pixel
 * data — can still show even when the image decode fails.
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

describe("PreviewPane — DICOM preview (CPE-1350)", () => {
  it("requests the decoded image via loadDicomImageData and the tags via loadDicomTags, rendering both", async () => {
    const loadDicomImageData = vi.fn(async () => "data:image/png;base64,AAAA");
    const loadDicomTags = vi.fn(async () => [["PatientName", "Doe^Jane"], ["Modality", "OT"]] as [string, string][]);

    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.dcm", path: "/scans/a.dcm", extension: "dcm" }),
      loadDicomImageData,
      loadDicomTags,
    });

    await waitFor(() => expect(container.querySelector("img.preview-img")).toBeTruthy());
    expect(loadDicomImageData).toHaveBeenCalledWith("/scans/a.dcm");
    expect(loadDicomTags).toHaveBeenCalledWith("/scans/a.dcm");

    const img = container.querySelector("img.preview-img") as HTMLImageElement;
    expect(img.src).toBe("data:image/png;base64,AAAA");

    await waitFor(() => expect(container.querySelector('[data-testid="dicom-tags-section"]')).toBeTruthy());
    expect(container.textContent).toContain("PatientName");
    expect(container.textContent).toContain("Doe^Jane");
    expect(container.textContent).toContain("Modality");
    expect(container.textContent).toContain("OT");
  });

  it("falls back to the metadata slot (not an error note) when the pixel data can't be decoded, but still shows tags", async () => {
    const loadDicomImageData = vi.fn(async () => {
      throw new Error("unsupported transfer syntax");
    });
    const loadDicomTags = vi.fn(async () => [["PatientName", "Only^Name"]] as [string, string][]);

    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.dcm", path: "/scans/a.dcm", extension: "dcm" }),
      loadDicomImageData,
      loadDicomTags,
    });

    await waitFor(() => expect(loadDicomImageData).toHaveBeenCalled());
    expect(container.querySelector("img.preview-img")).toBeNull();

    await waitFor(() => expect(container.querySelector('[data-testid="dicom-tags-section"]')).toBeTruthy());
    expect(container.textContent).toContain("PatientName");
    expect(container.textContent).toContain("Only^Name");
  });

  it("omits the tags section entirely when the file can't be opened as DICOM at all", async () => {
    const loadDicomImageData = vi.fn(async () => {
      throw new Error("not a DICOM file");
    });
    const loadDicomTags = vi.fn(async () => {
      throw new Error("not a DICOM file");
    });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "bad.dcm", path: "/scans/bad.dcm", extension: "dcm" }),
      loadDicomImageData,
      loadDicomTags,
    });

    await waitFor(() => expect(loadDicomTags).toHaveBeenCalled());
    expect(container.querySelector('[data-testid="dicom-tags-section"]')).toBeNull();
  });

  it("drops a stale response when the selection changes mid-flight (generation guard)", async () => {
    let resolveFirst!: (v: string) => void;
    const loadDicomImageData = vi
      .fn()
      .mockImplementationOnce(() => new Promise<string>((resolve) => { resolveFirst = resolve; }))
      .mockResolvedValueOnce("data:image/png;base64,SECOND");
    const loadDicomTags = vi.fn(async () => [] as [string, string][]);

    const { container, rerender } = render(PreviewPane, {
      entry: entry({ name: "first.dcm", path: "/scans/first.dcm", extension: "dcm" }),
      loadDicomImageData,
      loadDicomTags,
    });
    await waitFor(() => expect(loadDicomImageData).toHaveBeenCalledWith("/scans/first.dcm"));

    await rerender({ entry: entry({ name: "second.dcm", path: "/scans/second.dcm", extension: "dcm" }) });
    await waitFor(() => expect(loadDicomImageData).toHaveBeenCalledWith("/scans/second.dcm"));
    await waitFor(() => expect(container.querySelector("img.preview-img")).toBeTruthy());

    // The superseded first request now resolves — it must NOT clobber the second file's preview.
    resolveFirst("data:image/png;base64,STALE");
    await new Promise((r) => setTimeout(r, 0));

    const img = container.querySelector("img.preview-img") as HTMLImageElement;
    expect(img.src).toBe("data:image/png;base64,SECOND");
  });
});
