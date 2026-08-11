import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import NotebookPreview from "./NotebookPreview.svelte";

// CPE-1616 (epic CPE-1568 slice 6): jsdom render-spec for the notebook preview, wiring the generic
// `read_file_text` backend command into a standalone component (same mocking recipe as
// CertPreview.test.ts/JwtPreview.test.ts: mock `../bindings.gen`'s `commands` object). jsdom can't see
// layout, so these assert text/DOM content and robustness paths only — never a visual verdict.

const { readFileTextMock } = vi.hoisted(() => ({ readFileTextMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { readFileText: readFileTextMock },
}));

function ok(text: string) {
  return { status: "ok" as const, data: text };
}

const REAL_NOTEBOOK = JSON.stringify({
  nbformat: 4,
  nbformat_minor: 5,
  metadata: { kernelspec: { language: "python" } },
  cells: [
    { cell_type: "markdown", source: ["# Title\n", "\n", "Some **bold** text."] },
    {
      cell_type: "code",
      execution_count: 1,
      source: ["print('hi')\n", "1 + 1"],
      outputs: [
        { output_type: "stream", name: "stdout", text: ["hi\n"] },
        { output_type: "execute_result", execution_count: 1, data: { "text/plain": ["2"] } },
      ],
    },
    {
      cell_type: "code",
      execution_count: 2,
      source: "plot()",
      outputs: [{ output_type: "display_data", data: { "image/png": Buffer.from("fake-png").toString("base64") } }],
    },
    {
      cell_type: "code",
      execution_count: 3,
      source: "1 / 0",
      outputs: [{ output_type: "error", ename: "ZeroDivisionError", evalue: "division by zero", traceback: ["Traceback...", "ZeroDivisionError: division by zero"] }],
    },
    { cell_type: "raw", source: "plain raw text" },
  ],
});

beforeEach(() => {
  readFileTextMock.mockReset();
});

describe("NotebookPreview (CPE-1616)", () => {
  it("renders markdown, code, stream/result/image/error outputs, and raw cells in order", async () => {
    readFileTextMock.mockResolvedValueOnce(ok(REAL_NOTEBOOK));

    const { container } = render(NotebookPreview, { path: "/x/demo.ipynb" });

    await waitFor(() => expect(container.querySelectorAll('[data-testid="notebook-cell"]').length).toBe(5));
    expect(readFileTextMock).toHaveBeenCalledWith("/x/demo.ipynb", expect.any(Number));

    // Markdown cell rendered as real HTML (not raw source text).
    expect(container.querySelector(".nb-markdown h1")).toBeTruthy();
    expect(container.querySelector(".nb-markdown strong")?.textContent).toBe("bold");

    // Code cell: stream output + text/plain result.
    expect(container.textContent).toContain("hi");
    expect(container.textContent).toContain("2");
    expect(container.querySelector('[data-cell-type="code"] .nb-exec-count')?.textContent).toContain("In [1]");

    // Image output rendered as an <img> with a data: URL.
    const img = container.querySelector(".nb-output-image") as HTMLImageElement | null;
    expect(img).toBeTruthy();
    expect(img!.src).toMatch(/^data:image\/png;base64,/);

    // Error output styled distinctly and shows the traceback.
    const errOut = container.querySelector('[data-testid="notebook-error-output"]');
    expect(errOut).toBeTruthy();
    expect(errOut!.textContent).toContain("ZeroDivisionError");
    expect(errOut!.textContent).toContain("division by zero");

    // Raw cell: plain text, not markdown-rendered or highlighted.
    expect(container.querySelector(".nb-raw")?.textContent).toBe("plain raw text");
  });

  it("degrades to raw text with a clear reason for a malformed/non-notebook JSON file, never a blank pane", async () => {
    readFileTextMock.mockResolvedValueOnce(ok('{"just": "some json", "not": "a notebook"}'));

    const { container } = render(NotebookPreview, { path: "/x/fake.ipynb" });

    await waitFor(() => expect(container.querySelector('[data-testid="notebook-parse-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="notebook-parse-error"]')!.textContent).toMatch(/cells/i);
    // Degrades to the raw file content instead of a blank pane.
    const fallback = container.querySelector('[data-testid="notebook-raw-fallback"]');
    expect(fallback).toBeTruthy();
    expect(fallback!.textContent).toContain("some json");
  });

  it("degrades cleanly for truncated JSON without crashing", async () => {
    readFileTextMock.mockResolvedValueOnce(ok('{"cells": [{"cell_type": "code", "source": ["x = 1"'));

    const { container } = render(NotebookPreview, { path: "/x/truncated.ipynb" });

    await waitFor(() => expect(container.querySelector('[data-testid="notebook-parse-error"]')).toBeTruthy());
  });

  it("shows a load-error state when the invoke call itself fails, without crashing", async () => {
    readFileTextMock.mockRejectedValueOnce(new Error("File is too large to preview"));

    const { container } = render(NotebookPreview, { path: "/x/huge.ipynb" });

    await waitFor(() => expect(container.querySelector('[data-testid="notebook-load-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="notebook-load-error"]')!.textContent).toContain("too large");
  });

  it("shows an honest 'showing N of M' note for a notebook with hundreds of cells, and stays capped", async () => {
    const cells = Array.from({ length: 400 }, (_, i) => ({ cell_type: "raw", source: `cell ${i}` }));
    readFileTextMock.mockResolvedValueOnce(ok(JSON.stringify({ cells })));

    const { container } = render(NotebookPreview, { path: "/x/huge-cell-count.ipynb" });

    await waitFor(() => expect(container.querySelector('[data-testid="notebook-cells-capped"]')).toBeTruthy());
    expect(container.querySelectorAll('[data-testid="notebook-cell"]').length).toBeLessThan(400);
    expect(container.querySelector('[data-testid="notebook-cells-capped"]')!.textContent).toMatch(/of 400/);
  });

  it("tolerates a cell with no source without crashing", async () => {
    readFileTextMock.mockResolvedValueOnce(ok(JSON.stringify({ cells: [{ cell_type: "markdown" }] })));

    const { container } = render(NotebookPreview, { path: "/x/empty-source.ipynb" });

    await waitFor(() => expect(container.querySelector('[data-testid="notebook-cell"]')).toBeTruthy());
  });

  it("reports unsupported output MIME types honestly instead of half-rendering them", async () => {
    readFileTextMock.mockResolvedValueOnce(
      ok(
        JSON.stringify({
          cells: [
            {
              cell_type: "code",
              source: "x",
              outputs: [{ output_type: "execute_result", data: { "text/html": "<b>hi</b>" } }],
            },
          ],
        }),
      ),
    );

    const { container } = render(NotebookPreview, { path: "/x/rich-output.ipynb" });

    await waitFor(() => expect(container.textContent).toMatch(/not shown/i));
    expect(container.textContent).toContain("text/html");
  });
});
