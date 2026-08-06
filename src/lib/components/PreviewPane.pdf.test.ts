/**
 * CPE-1357 — PDF preview crash resilience: `.pdf` renders via a raw WebView2 `<iframe>` (the webview
 * has no native <embed>-style plugin the app controls), and a malformed/empty PDF handed to WebView2's
 * built-in PDF viewer can crash the whole app. Before the iframe's `src` is ever set, the pane calls a
 * backend structural-validity check (`loadPdfValidity`, backed by `read_pdf_validity` ->
 * `cpe_server::media_meta_read::pdf_validity`) — a rejected check falls back to the metadata slot
 * (mirrors the raw-image/dicom/heic Err -> fallback pattern) instead of ever reaching the iframe.
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

describe("PreviewPane — PDF preview crash resilience (CPE-1357)", () => {
  it("renders the iframe with the asset URL once the validity check passes", async () => {
    const loadPdfValidity = vi.fn(async () => 2);
    const assetUrl = (p: string) => `asset://${p}`;

    const { container } = render(PreviewPane, {
      entry: entry({ name: "doc.pdf", path: "/docs/doc.pdf", extension: "pdf" }),
      loadPdfValidity,
      assetUrl,
    });

    await waitFor(() => expect(loadPdfValidity).toHaveBeenCalledWith("/docs/doc.pdf"));
    await waitFor(() => expect(container.querySelector("iframe.preview-pdf")).toBeTruthy());

    const iframe = container.querySelector("iframe.preview-pdf") as HTMLIFrameElement;
    expect(iframe.getAttribute("src")).toBe("asset:///docs/doc.pdf");
    // CPE-1362: the iframe must NOT carry a `sandbox` attribute — WebView2/Chromium render PDFs via the
    // MimeHandlerView plugin (the built-in PDF viewer), which a sandboxed iframe disables, leaving the
    // pane blank on valid PDFs. Crash-safety comes from the validity gate (this branch is only reached
    // once `loadPdfValidity` resolves) plus the load-timeout/on:error fallback, not from the sandbox.
    expect(iframe.getAttribute("sandbox")).toBeNull();
  });

  it("falls back to the metadata slot (no iframe at all) when the validity check rejects — malformed/empty PDF", async () => {
    const loadPdfValidity = vi.fn(async () => {
      throw new Error("no startxref: unresolvable cross-reference table");
    });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "broken.pdf", path: "/docs/broken.pdf", extension: "pdf" }),
      loadPdfValidity,
    });

    await waitFor(() => expect(loadPdfValidity).toHaveBeenCalledWith("/docs/broken.pdf"));
    await waitFor(() => expect(container.querySelector("iframe.preview-pdf")).toBeNull());
  });

  it("falls back when the validity check resolves a zero-page rejection message (backend Err path)", async () => {
    const loadPdfValidity = vi.fn(async () => {
      throw new Error("PDF declares zero pages");
    });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "empty.pdf", path: "/docs/empty.pdf", extension: "pdf" }),
      loadPdfValidity,
    });

    await waitFor(() => expect(loadPdfValidity).toHaveBeenCalled());
    expect(container.querySelector("iframe.preview-pdf")).toBeNull();
  });

  it("drops a stale response when the selection changes mid-flight (generation guard)", async () => {
    let resolveFirst!: (v: number | null) => void;
    const loadPdfValidity = vi
      .fn()
      .mockImplementationOnce(() => new Promise<number | null>((resolve) => { resolveFirst = resolve; }))
      .mockResolvedValueOnce(1);

    const { container, rerender } = render(PreviewPane, {
      entry: entry({ name: "first.pdf", path: "/docs/first.pdf", extension: "pdf" }),
      loadPdfValidity,
    });
    await waitFor(() => expect(loadPdfValidity).toHaveBeenCalledWith("/docs/first.pdf"));
    expect(container.querySelector("iframe.preview-pdf")).toBeNull(); // still loading

    await rerender({ entry: entry({ name: "second.pdf", path: "/docs/second.pdf", extension: "pdf" }) });
    await waitFor(() => expect(loadPdfValidity).toHaveBeenCalledWith("/docs/second.pdf"));
    await waitFor(() => expect(container.querySelector("iframe.preview-pdf")).toBeTruthy());

    // The superseded first (still-pending) request now resolves — it must NOT resurrect a fallback
    // state or otherwise clobber the second file's already-rendered preview.
    resolveFirst(2);
    await new Promise((r) => setTimeout(r, 0));

    expect(container.querySelector("iframe.preview-pdf")).toBeTruthy();
  });
});
