import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
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

  // CPE-1616 Visual Critic finding (must-fix): a real executed traceback embeds raw ANSI escape codes.
  // Confirms the end-to-end path (parseNotebook -> stripAnsi -> the rendered <pre>) never leaks the raw
  // escape garbage into what the user actually sees.
  it("renders an error traceback with ANSI escape codes stripped to readable text", async () => {
    const traceback = [
      "\x1b[0;31m---------------------------------------------------------------------------\x1b[0m",
      "\x1b[0;31mZeroDivisionError\x1b[0m                         Traceback (most recent call last)",
      "\x1b[0;32mCell \x1b[0;36mIn[3], line 1\x1b[0m",
      "\x1b[0;32m----> 1\x1b[0m \x1b[38;5;241;43m1\x1b[39;49m \x1b[38;5;241;43m/\x1b[39;49m \x1b[38;5;241;43m0\x1b[39;49m",
      "\x1b[0;31mZeroDivisionError\x1b[0m: division by zero",
    ];
    readFileTextMock.mockResolvedValueOnce(
      ok(
        JSON.stringify({
          cells: [
            {
              cell_type: "code",
              source: "1/0",
              outputs: [{ output_type: "error", ename: "ZeroDivisionError", evalue: "division by zero", traceback }],
            },
          ],
        }),
      ),
    );

    const { container } = render(NotebookPreview, { path: "/x/ansi-traceback.ipynb" });

    await waitFor(() => expect(container.querySelector('[data-testid="notebook-error-output"]')).toBeTruthy());
    const errOut = container.querySelector('[data-testid="notebook-error-output"]')!;
    // Not just "no raw ESC byte" (jsdom's textContent wouldn't show it anyway) — assert the specific
    // literal-garbage fragments the Visual Critic actually saw on screen are gone from the rendered text.
    expect(errOut.textContent).not.toContain("[0;31m");
    expect(errOut.textContent).not.toContain("[0;32m");
    expect(errOut.textContent).not.toContain("[38;5;241;43m");
    expect(errOut.textContent).not.toContain("[39;49m");
    expect(errOut.textContent).toContain("ZeroDivisionError");
    expect(errOut.textContent).toContain("Cell In[3], line 1");
    expect(errOut.textContent).toContain("1 / 0");
  });

  it("renders a stream output with ANSI colour codes stripped", async () => {
    readFileTextMock.mockResolvedValueOnce(
      ok(
        JSON.stringify({
          cells: [
            {
              cell_type: "code",
              source: "print('hi')",
              outputs: [{ output_type: "stream", name: "stdout", text: ["\x1b[32mgreen line\x1b[0m\n"] }],
            },
          ],
        }),
      ),
    );

    const { container } = render(NotebookPreview, { path: "/x/ansi-stream.ipynb" });

    await waitFor(() => expect(container.querySelector(".nb-stream")).toBeTruthy());
    expect(container.querySelector(".nb-stream")!.textContent).toBe("green line\n");
  });

  // CPE-1616 Visual Critic finding (must-fix): an unbounded long output used to push the rest of the
  // notebook off-screen. jsdom cannot see layout/scroll, so this only proves the DATA-integrity half of
  // the fix — every line of a long-but-uncapped output is still present in the DOM (nothing was silently
  // dropped) — not the visual bound itself. The visual claim (a real max-height + scroll region appears in
  // the browser) needs the screenshot/Visual-Critic pass, same as every other layout claim in this suite.
  it("keeps every line of a long (but uncapped) stream output in the DOM — bounding is a CSS/visual concern, not a data one", async () => {
    const lines = Array.from({ length: 300 }, (_, i) => `line ${i}\n`);
    readFileTextMock.mockResolvedValueOnce(
      ok(
        JSON.stringify({
          cells: [{ cell_type: "code", source: "x", outputs: [{ output_type: "stream", name: "stdout", text: lines }] }],
        }),
      ),
    );

    const { container } = render(NotebookPreview, { path: "/x/long-output.ipynb" });

    await waitFor(() => expect(container.querySelector(".nb-stream")).toBeTruthy());
    const text = container.querySelector(".nb-stream")!.textContent ?? "";
    expect(text).toContain("line 0\n");
    expect(text).toContain("line 299\n");
    // Well under MAX_OUTPUT_TEXT_CHARS (20,000), so the cap's own truncation note must NOT be the thing
    // bounding this — confirms the height bound is a separate, additional concern from the text-length cap.
    expect(container.querySelector('[data-testid="notebook-cell"]')?.textContent).not.toMatch(/output truncated/i);
  });

  // Structural guard (not a visual one — see the note above): asserts the CSS rule that bounds a long
  // output actually exists and applies to the right selectors, without depending on jsdom layout, which
  // can't compute a real max-height/scrollbar. This is the "test what is testable" half of the fix; a
  // human/Visual-Critic screenshot pass is still what confirms it looks right on screen.
  it("the .nb-output rule declares a bounded, scrollable region (structural CSS guard, not a visual proof)", () => {
    const src = readFileSync(join(__dirname, "NotebookPreview.svelte"), "utf8");
    const styleMatch = src.match(/<style>([\s\S]*)<\/style>/);
    expect(styleMatch, "NotebookPreview.svelte must have a <style> block").toBeTruthy();
    const css = styleMatch![1];
    const ruleMatch = css.match(/\.nb-output\s*\{([^}]*)\}/);
    expect(ruleMatch, ".nb-output rule not found in NotebookPreview.svelte").toBeTruthy();
    const rule = ruleMatch![1];
    expect(rule, ".nb-output must declare a bounded height").toMatch(/max-height\s*:/);
    expect(rule, ".nb-output must be its own scroll region, not silently clipped").toMatch(/overflow-y\s*:\s*auto/);
  });
});

// CPE-1616 Visual Critic finding (must-fix): dark-theme traceback contrast measured 4.41:1 — under the
// 4.5:1 AA floor for normal text — because the traceback's actual background is
// `color-mix(in srgb, var(--danger) 8%, var(--surface))` (NotebookPreview.svelte's `.nb-error-output`
// rule), not plain `--surface`. src/app.css.dark-contrast.test.ts already guards `--danger` vs plain
// `--bg`/`--surface`, but that pairing doesn't reflect this component's actual mixed background, so this
// is a dedicated guard tied to the real formula this component uses.
//
// CPE-1616 correction: an earlier revision "fixed" this by nudging the SHARED dark `--danger` token
// itself (`--pal-dark-red-400`), which silently regressed an unrelated, already-failing surface app-wide
// (white text on solid --danger buttons/pills/fills — tracked separately as CPE-1632) while softening the
// app-wide destructive red. That shared-token move was reverted; `--danger` is back to its original
// #ff6659. `.nb-error-output`'s text colour now reads a dedicated `--danger-on-tint` token (src/app.css)
// that resolves to `--danger` everywhere except dark theme, where it resolves to a slightly lighter red
// used ONLY by this component — so this guard now measures `--danger-on-tint` against a background mixed
// from the (reverted) `--danger`, exactly matching what `.nb-error-output` actually renders. Same inline
// WCAG contrast math as app.css.dark-contrast.test.ts / app.css.hc-contrast.test.ts (no shared util exists
// yet — matches that file-local convention) and reads the live hex values out of app.css so it can never
// silently drift.
describe("NotebookPreview traceback contrast against its real tinted background (CPE-1616)", () => {
  const appCss = readFileSync(join(__dirname, "..", "..", "app.css"), "utf8");

  function hexToRgb(hex: string): [number, number, number] {
    let h = hex.replace("#", "");
    if (h.length === 3) h = h.split("").map((c) => c + c).join("");
    return [parseInt(h.substring(0, 2), 16), parseInt(h.substring(2, 4), 16), parseInt(h.substring(4, 6), 16)];
  }
  function relativeLuminance(hex: string): number {
    const [r, g, b] = hexToRgb(hex).map((c) => {
      const s = c / 255;
      return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  }
  function contrastRatio(hexA: string, hexB: string): number {
    const lA = relativeLuminance(hexA);
    const lB = relativeLuminance(hexB);
    const lighter = Math.max(lA, lB);
    const darker = Math.min(lA, lB);
    return (lighter + 0.05) / (darker + 0.05);
  }
  /** Mirrors CSS `color-mix(in srgb, colorA pctA%, colorB)` for two opaque sRGB hex colours. */
  function mix(hexA: string, pctA: number, hexB: string): string {
    const a = hexToRgb(hexA);
    const b = hexToRgb(hexB);
    const rgb = [0, 1, 2].map((i) => Math.round(a[i] * pctA + b[i] * (1 - pctA)));
    return "#" + rgb.map((x) => x.toString(16).padStart(2, "0")).join("");
  }

  /** Resolves a token declared within a given `:root[data-theme="X"]` block down to its concrete hex
   *  literal in app.css, following `var(--...)` indirection until it lands on a literal — one hop for
   *  most tokens (semantic -> palette, e.g. `--danger` -> `--pal-dark-red-400` -> `#ff6659`), two for
   *  `--danger-on-tint` in light theme (`--danger-on-tint` -> `--danger` -> `--pal-red-600` -> a hex),
   *  since light theme just aliases it back to the plain `--danger` token. */
  function resolveToHex(varName: string, block: string, depth = 0): string {
    expect(depth, `${varName}: too much indirection resolving to a hex literal`).toBeLessThan(5);
    const declMatch = block.match(new RegExp(`${varName}\\s*:\\s*([^;]+);`));
    expect(declMatch, `${varName} not declared in this theme block`).toBeTruthy();
    const value = declMatch![1].trim();
    const hexMatch = value.match(/^#[0-9a-fA-F]{6}$/);
    if (hexMatch) return value;
    const varMatch = value.match(/^var\((--[a-zA-Z0-9-]+)\)$/);
    expect(varMatch, `${varName}'s value "${value}" is neither a hex literal nor a var() reference`).toBeTruthy();
    const nextName = varMatch![1];
    // Prefer resolving within the SAME theme block first (handles semantic -> semantic aliasing, e.g.
    // --danger-on-tint -> --danger in the light block); fall back to a bare palette hex declared
    // elsewhere in app.css (the palette layer) for the common semantic -> palette case.
    if (new RegExp(`${nextName}\\s*:`).test(block)) return resolveToHex(nextName, block, depth + 1);
    const paletteMatch = appCss.match(new RegExp(`${nextName}\\s*:\\s*(#[0-9a-fA-F]{6})`));
    expect(paletteMatch, `${nextName} not found as a hex literal in app.css`).toBeTruthy();
    return paletteMatch![1];
  }

  function extractHex(varName: string, themeAttr: string): string {
    const blockMatch = appCss.match(new RegExp(`:root\\[data-theme="${themeAttr}"\\]\\s*\\{([^}]*)\\}`));
    expect(blockMatch, `:root[data-theme="${themeAttr}"] block not found in app.css`).toBeTruthy();
    return resolveToHex(varName, blockMatch![1]);
  }

  it("--danger-on-tint on the .nb-error-output's actual 8%-mixed background clears 4.5:1 AA in dark theme", () => {
    const danger = extractHex("--danger", "dark"); // the mixed background is color-mix'd from --danger
    const surface = extractHex("--surface", "dark");
    const textColor = extractHex("--danger-on-tint", "dark");
    const tracebackBg = mix(danger, 0.08, surface); // matches color-mix(in srgb, var(--danger) 8%, var(--surface))
    const ratio = contrastRatio(textColor, tracebackBg);
    // eslint-disable-next-line no-console -- the ticket asks for the measured ratio to be reported.
    console.log(
      `NotebookPreview dark-theme traceback contrast: ${ratio.toFixed(2)}:1 (--danger-on-tint ${textColor} on ${tracebackBg}, mixed from --danger ${danger})`,
    );
    expect(ratio).toBeGreaterThanOrEqual(4.5);
  });

  it("--danger-on-tint on the .nb-error-output's actual 8%-mixed background clears 4.5:1 AA in light theme (unchanged by this fix)", () => {
    const danger = extractHex("--danger", "light");
    const surface = extractHex("--surface", "light");
    const textColor = extractHex("--danger-on-tint", "light");
    const tracebackBg = mix(danger, 0.08, surface);
    const ratio = contrastRatio(textColor, tracebackBg);
    // eslint-disable-next-line no-console -- the ticket asks for the measured ratio to be reported.
    console.log(
      `NotebookPreview light-theme traceback contrast: ${ratio.toFixed(2)}:1 (--danger-on-tint ${textColor} on ${tracebackBg}, mixed from --danger ${danger})`,
    );
    expect(ratio).toBeGreaterThanOrEqual(4.5);
  });
});
