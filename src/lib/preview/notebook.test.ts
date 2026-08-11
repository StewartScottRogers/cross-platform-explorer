import { describe, it, expect } from "vitest";
import {
  parseNotebook,
  stripAnsi,
  MAX_CELLS,
  MAX_CELL_SOURCE_CHARS,
  MAX_OUTPUTS_PER_CELL,
  MAX_OUTPUT_TEXT_CHARS,
  MAX_OUTPUT_IMAGE_CHARS,
  type NotebookOutputResult,
} from "./notebook";

// CPE-1616 (epic CPE-1568 slice 6): unit coverage for the pure notebook parser. A .ipynb is untrusted
// input, so most of this file exercises malformed/hostile shapes rather than the happy path — see the
// ticket's explicit robustness requirements (malformed JSON, missing cells array, huge notebooks, huge
// outputs) plus the "cap must bound work examined, not just work emitted" discipline from the module doc.

function nb(obj: unknown): string {
  return JSON.stringify(obj);
}

describe("parseNotebook — malformed / hostile input", () => {
  it("reports a clear error on invalid JSON, never throwing", () => {
    const result = parseNotebook("{not json");
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toMatch(/not valid json/i);
  });

  it("reports a clear error on truncated JSON", () => {
    const result = parseNotebook('{"cells": [{"cell_type": "code", "source": ["x = 1"');
    expect(result.ok).toBe(false);
  });

  it("reports a clear error when the JSON is valid but not an object (a bare array)", () => {
    const result = parseNotebook("[1, 2, 3]");
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toMatch(/not a notebook/i);
  });

  it("reports a clear error when the JSON is valid but not an object (a bare string)", () => {
    const result = parseNotebook('"hello"');
    expect(result.ok).toBe(false);
  });

  it("reports a clear error when cells is missing", () => {
    const result = parseNotebook(nb({ metadata: {}, nbformat: 4 }));
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toMatch(/cells/i);
  });

  it("reports a clear error when cells is present but not an array", () => {
    const result = parseNotebook(nb({ cells: "not an array", nbformat: 4 }));
    expect(result.ok).toBe(false);
  });

  it("never throws on an empty string", () => {
    expect(() => parseNotebook("")).not.toThrow();
    expect(parseNotebook("").ok).toBe(false);
  });

  it("tolerates an unknown/future nbformat version rather than rejecting it", () => {
    const result = parseNotebook(nb({ cells: [], nbformat: 999 }));
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.notebook.nbformat).toBe(999);
  });

  it("tolerates a missing nbformat field", () => {
    const result = parseNotebook(nb({ cells: [] }));
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.notebook.nbformat).toBeNull();
  });

  it("tolerates a cell that isn't an object (null, number, array) without throwing", () => {
    const result = parseNotebook(nb({ cells: [null, 42, ["x"], { cell_type: "code", source: "ok" }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells).toHaveLength(4);
    expect(result.notebook.cells[0].type).toBe("unknown");
    expect(result.notebook.cells[3].type).toBe("code");
    expect(result.notebook.cells[3].source).toBe("ok");
  });

  it("tolerates a cell with no source at all", () => {
    const result = parseNotebook(nb({ cells: [{ cell_type: "markdown" }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells[0].source).toBe("");
  });

  it("treats an unrecognized cell_type as 'unknown' rather than guessing", () => {
    const result = parseNotebook(nb({ cells: [{ cell_type: "future-cell-kind", source: "x" }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells[0].type).toBe("unknown");
  });
});

describe("parseNotebook — cell source normalization", () => {
  it("joins an array-of-lines source into one string", () => {
    const result = parseNotebook(nb({ cells: [{ cell_type: "code", source: ["a = 1\n", "b = 2\n"] }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells[0].source).toBe("a = 1\nb = 2\n");
  });

  it("accepts a plain string source unchanged", () => {
    const result = parseNotebook(nb({ cells: [{ cell_type: "markdown", source: "# Title" }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells[0].source).toBe("# Title");
  });

  it("drops non-string entries from an array source instead of throwing", () => {
    const result = parseNotebook(nb({ cells: [{ cell_type: "code", source: ["ok", 5, null, "!"] }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells[0].source).toBe("ok!");
  });
});

describe("parseNotebook — language detection", () => {
  it("uses metadata.kernelspec.language when present", () => {
    const result = parseNotebook(nb({ cells: [], metadata: { kernelspec: { language: "Julia" } } }));
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.notebook.language).toBe("julia");
  });

  it("falls back to metadata.language_info.name when kernelspec is absent", () => {
    const result = parseNotebook(nb({ cells: [], metadata: { language_info: { name: "R" } } }));
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.notebook.language).toBe("r");
  });

  it("falls back to python when no language metadata is present at all", () => {
    const result = parseNotebook(nb({ cells: [] }));
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.notebook.language).toBe("python");
  });
});

describe("parseNotebook — cell-count cap bounds work, not just output", () => {
  it("caps rendering at MAX_CELLS and reports the real total", () => {
    const cells = Array.from({ length: MAX_CELLS + 50 }, (_, i) => ({
      cell_type: "code",
      source: `x = ${i}`,
    }));
    const result = parseNotebook(nb({ cells }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells).toHaveLength(MAX_CELLS);
    expect(result.notebook.totalCells).toBe(MAX_CELLS + 50);
    expect(result.notebook.cellsCapped).toBe(true);
  });

  it("does not cap a notebook at or under the limit", () => {
    const cells = Array.from({ length: MAX_CELLS }, () => ({ cell_type: "raw", source: "x" }));
    const result = parseNotebook(nb({ cells }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells).toHaveLength(MAX_CELLS);
    expect(result.notebook.cellsCapped).toBe(false);
  });

  // A getter-based "must never be read" proof (the obvious way to assert laziness) can't survive
  // `parseNotebook`'s string-in API: building the fixture requires JSON.stringify-ing the cells array
  // first, and JSON.stringify itself visits (and would trigger) every getter while serializing — long
  // before parseNotebook or its cap ever runs. So this instead asserts the cap's actual observable
  // contract at scale: cells/outputs FAR beyond the cap (tens of thousands of them) never inflate parse
  // time, which would happen if the code mapped over the full array before slicing (the "cap counts items
  // emitted, not items examined" bug this module's own doc comment calls out). notebook.ts's
  // `.slice(0, MAX_CELLS)` BEFORE `.map(parseCell)` (and the equivalent for outputs) is what keeps this
  // fast; a regression back to "map-then-slice" would still pass the count assertions above but show up
  // here as a large, easily-noticed slowdown.
  it("a notebook with tens of thousands of cells still parses quickly (cap bounds work done, not just cells shown)", () => {
    const cells = Array.from({ length: 40_000 }, (_, i) => ({ cell_type: "code", source: `x = ${i}` }));
    const raw = nb({ cells });
    const start = performance.now();
    const result = parseNotebook(raw);
    const elapsed = performance.now() - start;
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells).toHaveLength(MAX_CELLS);
    expect(result.notebook.totalCells).toBe(40_000);
    // Generous bound (only MAX_CELLS cells are ever actually parsed) — this is a coarse regression guard,
    // not a tight perf assertion, to avoid flaking on a slow CI runner.
    expect(elapsed).toBeLessThan(2000);
  });
});

describe("parseNotebook — per-cell source cap bounds a single pathological cell", () => {
  it("truncates a single giant cell's source and flags it", () => {
    const huge = "x".repeat(MAX_CELL_SOURCE_CHARS + 1000);
    const result = parseNotebook(nb({ cells: [{ cell_type: "code", source: huge }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells[0].source).toHaveLength(MAX_CELL_SOURCE_CHARS);
    expect(result.notebook.cells[0].sourceTruncated).toBe(true);
  });

  it("does not flag a normal-sized cell as truncated", () => {
    const result = parseNotebook(nb({ cells: [{ cell_type: "code", source: "print('hi')" }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells[0].sourceTruncated).toBe(false);
  });
});

describe("parseNotebook — outputs", () => {
  it("parses a stream output", () => {
    const result = parseNotebook(
      nb({ cells: [{ cell_type: "code", source: "print(1)", outputs: [{ output_type: "stream", name: "stdout", text: ["1\n"] }] }] }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0];
    expect(out.kind).toBe("stream");
    if (out.kind === "stream") {
      expect(out.name).toBe("stdout");
      expect(out.text).toBe("1\n");
    }
  });

  it("parses an execute_result with text/plain", () => {
    const result = parseNotebook(
      nb({
        cells: [
          {
            cell_type: "code",
            source: "1 + 1",
            outputs: [{ output_type: "execute_result", data: { "text/plain": ["2"] } }],
          },
        ],
      }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0] as NotebookOutputResult;
    expect(out.kind).toBe("result");
    expect(out.text).toBe("2");
    expect(out.imageDataUrl).toBeNull();
  });

  it("renders image/png as a data: URL", () => {
    const b64 = Buffer.from("fake-png-bytes").toString("base64");
    const result = parseNotebook(
      nb({
        cells: [
          {
            cell_type: "code",
            source: "plot()",
            outputs: [{ output_type: "display_data", data: { "image/png": b64 } }],
          },
        ],
      }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0] as NotebookOutputResult;
    expect(out.imageDataUrl).toBe(`data:image/png;base64,${b64}`);
    expect(out.imageOmitted).toBe(false);
  });

  it("omits an oversized image/png rather than building a giant data: URL", () => {
    const hugeB64 = "A".repeat(MAX_OUTPUT_IMAGE_CHARS + 1000);
    const result = parseNotebook(
      nb({ cells: [{ cell_type: "code", source: "x", outputs: [{ output_type: "display_data", data: { "image/png": hugeB64 } }] }] }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0] as NotebookOutputResult;
    expect(out.imageDataUrl).toBeNull();
    expect(out.imageOmitted).toBe(true);
  });

  it("reports unsupported MIME types honestly instead of half-rendering them", () => {
    const result = parseNotebook(
      nb({
        cells: [
          {
            cell_type: "code",
            source: "x",
            outputs: [{ output_type: "execute_result", data: { "text/html": "<b>hi</b>", "application/json": { a: 1 } } }],
          },
        ],
      }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0] as NotebookOutputResult;
    expect(out.otherMimeTypes.sort()).toEqual(["application/json", "text/html"]);
  });

  it("parses an error output with traceback", () => {
    const result = parseNotebook(
      nb({
        cells: [
          {
            cell_type: "code",
            source: "1/0",
            outputs: [{ output_type: "error", ename: "ZeroDivisionError", evalue: "division by zero", traceback: ["line1", "line2"] }],
          },
        ],
      }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0];
    expect(out.kind).toBe("error");
    if (out.kind === "error") {
      expect(out.ename).toBe("ZeroDivisionError");
      expect(out.traceback).toBe("line1\nline2");
    }
  });

  it("truncates enormous output text and flags it", () => {
    const hugeText = "y".repeat(MAX_OUTPUT_TEXT_CHARS + 500);
    const result = parseNotebook(
      nb({ cells: [{ cell_type: "code", source: "x", outputs: [{ output_type: "stream", name: "stdout", text: hugeText }] }] }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0];
    expect(out.kind).toBe("stream");
    if (out.kind === "stream") {
      expect(out.text).toHaveLength(MAX_OUTPUT_TEXT_CHARS);
      expect(out.truncated).toBe(true);
    }
  });

  it("caps outputs-per-cell at MAX_OUTPUTS_PER_CELL and reports the real total", () => {
    const outputs = Array.from({ length: MAX_OUTPUTS_PER_CELL + 10 }, (_, i) => ({
      output_type: "stream",
      name: "stdout",
      text: `${i}\n`,
    }));
    const result = parseNotebook(nb({ cells: [{ cell_type: "code", source: "x", outputs }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const cell = result.notebook.cells[0];
    expect(cell.outputs).toHaveLength(MAX_OUTPUTS_PER_CELL);
    expect(cell.outputsTotal).toBe(MAX_OUTPUTS_PER_CELL + 10);
    expect(cell.outputsCapped).toBe(true);
  });

  it("a cell with tens of thousands of outputs still parses quickly (cap bounds work done, not just outputs shown)", () => {
    // Same rationale as the cell-count version above: a getter-based "never read" proof can't survive
    // the JSON.stringify fixture-building step, so this asserts the cap's real effect at scale instead.
    const outputs = Array.from({ length: 40_000 }, (_, i) => ({ output_type: "stream", name: "stdout", text: `${i}\n` }));
    const raw = nb({ cells: [{ cell_type: "code", source: "x", outputs }] });
    const start = performance.now();
    const result = parseNotebook(raw);
    const elapsed = performance.now() - start;
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells[0].outputs).toHaveLength(MAX_OUTPUTS_PER_CELL);
    expect(result.notebook.cells[0].outputsTotal).toBe(40_000);
    expect(elapsed).toBeLessThan(2000);
  });

  it("ignores an unknown output_type rather than guessing at its shape", () => {
    const result = parseNotebook(nb({ cells: [{ cell_type: "code", source: "x", outputs: [{ output_type: "widget_state" }] }] }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.notebook.cells[0].outputs).toHaveLength(0);
  });

  it("a non-code cell with an outputs array is untouched (outputs only apply to code cells in practice, but this still must not throw)", () => {
    const result = parseNotebook(nb({ cells: [{ cell_type: "markdown", source: "# hi", outputs: [{ output_type: "stream", text: "x" }] }] }));
    expect(result.ok).toBe(true);
  });
});

// CPE-1616 Visual Critic finding (must-fix): a real Jupyter traceback/stream is routinely colourised with
// raw ANSI escape codes by the kernel (IPython's exception formatter, colorama, tqdm, …). Unstripped, the
// view renders literal garbage like `[0;31m` interleaved with the real message — confirmed in both themes
// at every width. `stripAnsi` (and its use inside `parseOutput`) must remove these from stream text,
// text/plain results, and error tracebacks, since that's what reaches the `<pre>` in NotebookPreview.svelte.
describe("stripAnsi", () => {
  it("removes a single SGR colour-code sequence", () => {
    expect(stripAnsi("\x1b[0;31mred text\x1b[0m")).toBe("red text");
  });

  it("removes multi-parameter SGR sequences (256-colour / truecolor-style codes)", () => {
    // Real Jupyter output uses these for syntax-highlighted tracebacks, e.g. `\x1b[38;5;241;43m1\x1b[39;49m`.
    expect(stripAnsi("\x1b[38;5;241;43m1\x1b[39;49m")).toBe("1");
  });

  it("leaves plain text with no escape codes untouched", () => {
    expect(stripAnsi("ZeroDivisionError: division by zero")).toBe("ZeroDivisionError: division by zero");
  });

  it("leaves an empty string untouched and never throws", () => {
    expect(() => stripAnsi("")).not.toThrow();
    expect(stripAnsi("")).toBe("");
  });

  it("strips a real-shaped multi-line Jupyter traceback down to readable text", () => {
    // Shaped like an actual `python -c "1/0"` traceback captured under IPython's colourised formatter —
    // the exact kind of fragment the Visual Critic saw on screen as literal `[0;31m` garbage.
    const raw = [
      "\x1b[0;31m---------------------------------------------------------------------------\x1b[0m",
      "\x1b[0;31mZeroDivisionError\x1b[0m                         Traceback (most recent call last)",
      "\x1b[0;32mCell \x1b[0;36mIn[3], line 1\x1b[0m",
      "\x1b[0;32m----> 1\x1b[0m \x1b[38;5;241;43m1\x1b[39;49m \x1b[38;5;241;43m/\x1b[39;49m \x1b[38;5;241;43m0\x1b[39;49m",
      "\x1b[0;31mZeroDivisionError\x1b[0m: division by zero",
    ].join("\n");
    const cleaned = stripAnsi(raw);
    expect(cleaned).not.toMatch(/\x1b/);
    expect(cleaned).not.toContain("[0;31m");
    expect(cleaned).not.toContain("[38;5;241;43m");
    expect(cleaned).toContain("ZeroDivisionError");
    expect(cleaned).toContain("division by zero");
    expect(cleaned).toContain("Cell In[3], line 1");
    expect(cleaned).toContain("1 / 0");
  });
});

describe("parseNotebook — ANSI escape codes are stripped from every text-bearing output kind", () => {
  it("strips ANSI codes from a stream output's text", () => {
    const result = parseNotebook(
      nb({
        cells: [
          {
            cell_type: "code",
            source: "print('hi')",
            outputs: [{ output_type: "stream", name: "stdout", text: ["\x1b[32mhi\x1b[0m\n"] }],
          },
        ],
      }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0];
    expect(out.kind).toBe("stream");
    if (out.kind === "stream") {
      expect(out.text).toBe("hi\n");
      expect(out.text).not.toContain("\x1b");
    }
  });

  it("strips ANSI codes from an execute_result's text/plain", () => {
    const result = parseNotebook(
      nb({
        cells: [
          {
            cell_type: "code",
            source: "x",
            outputs: [{ output_type: "execute_result", data: { "text/plain": ["\x1b[1m2\x1b[0m"] } }],
          },
        ],
      }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0] as NotebookOutputResult;
    expect(out.text).toBe("2");
  });

  it("strips a real-shaped ANSI traceback from an error output, leaving the message readable", () => {
    const traceback = [
      "\x1b[0;31m---------------------------------------------------------------------------\x1b[0m",
      "\x1b[0;31mZeroDivisionError\x1b[0m                         Traceback (most recent call last)",
      "\x1b[0;32mCell \x1b[0;36mIn[3], line 1\x1b[0m",
      "\x1b[0;32m----> 1\x1b[0m \x1b[38;5;241;43m1\x1b[39;49m \x1b[38;5;241;43m/\x1b[39;49m \x1b[38;5;241;43m0\x1b[39;49m",
      "\x1b[0;31mZeroDivisionError\x1b[0m: division by zero",
    ];
    const result = parseNotebook(
      nb({
        cells: [
          {
            cell_type: "code",
            source: "1/0",
            outputs: [{ output_type: "error", ename: "ZeroDivisionError", evalue: "division by zero", traceback }],
          },
        ],
      }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0];
    expect(out.kind).toBe("error");
    if (out.kind === "error") {
      expect(out.traceback).not.toMatch(/\x1b/);
      expect(out.traceback).not.toContain("[0;31m");
      expect(out.traceback).toContain("ZeroDivisionError");
      expect(out.traceback).toContain("Cell In[3], line 1");
      expect(out.traceback).toContain("1 / 0");
    }
  });

  it("still truncates and flags an enormous ANSI-laden traceback at MAX_OUTPUT_TEXT_CHARS", () => {
    // The colour codes are stripped BEFORE the length cap is applied, so the cap reflects the length of
    // what's actually rendered, not the raw (longer, escape-code-inflated) source text.
    const hugeTraceback = "\x1b[0;31m".repeat(1) + "y".repeat(MAX_OUTPUT_TEXT_CHARS + 500) + "\x1b[0m";
    const result = parseNotebook(
      nb({
        cells: [
          {
            cell_type: "code",
            source: "x",
            outputs: [{ output_type: "error", ename: "E", evalue: "v", traceback: [hugeTraceback] }],
          },
        ],
      }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.notebook.cells[0].outputs[0];
    expect(out.kind).toBe("error");
    if (out.kind === "error") {
      expect(out.traceback).toHaveLength(MAX_OUTPUT_TEXT_CHARS);
      expect(out.truncated).toBe(true);
      expect(out.traceback).not.toContain("\x1b");
    }
  });
});
