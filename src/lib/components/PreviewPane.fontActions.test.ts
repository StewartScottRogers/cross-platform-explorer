/**
 * CPE-1586 (epic CPE-1568 slice 5) — the font provider's declared `actions` (`preview/provider.ts`),
 * rendered by PreviewPane's generic action bar (CPE-1570) and run end to end against `FontPreview.svelte`
 * (mounted internally by `PreviewPane` for the `font` provider kind). Mirrors
 * `PreviewPane.jwtActions.test.ts`'s recipe. jsdom has no `FontFace`, so `FontPreview` takes its
 * documented graceful-degradation branch (plain specimen, no actual font load) — exactly the same code
 * path a real font-parse failure takes, so the action-bar wiring is exercised the same way either way.
 * Metadata parsing itself (the sfnt `name`/`maxp` table reader) is unit-tested directly in
 * `preview/font.test.ts`; this file focuses on the glyph-grid → action-bar plumbing.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import PreviewPane from "./PreviewPane.svelte";
import { FONT_GLYPH_CANDIDATES, capGlyphs, codepointLabel } from "../preview/font";
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

function actionBarLabels(container: Element): (string | undefined)[] {
  const bar = container.querySelector('[data-testid="preview-action-bar"]')!;
  return [...bar.querySelectorAll("button")].map((b) => b.textContent?.trim());
}

beforeEach(() => {
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText: async () => {} },
    configurable: true,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("PreviewPane — font preview + action bar (CPE-1586)", () => {
  it("renders the specimen + a capped glyph grid for a .ttf file", async () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.ttf", path: "/fonts/a.ttf", extension: "ttf" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    await waitFor(() => expect(container.querySelector('[data-testid="font-preview"]')).toBeTruthy());
    const grid = container.querySelector('[data-testid="font-glyph-grid"]')!;
    const cells = grid.querySelectorAll("button");
    // Rendered cell count matches the same capping logic under unit test in preview/font.test.ts — never
    // the full candidate list unbounded (STREAMING.md/PURPOSE.md).
    expect(cells.length).toBe(capGlyphs(FONT_GLYPH_CANDIDATES).shown.length);
  });

  it("shows Copy glyph / Copy codepoint on the action bar as soon as a glyph is loaded (defaults to the first cell)", async () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.otf", path: "/fonts/a.otf", extension: "otf" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    expect(actionBarLabels(container)).toEqual(["Copy glyph", "Copy codepoint"]);
  });

  it("Copy glyph copies the selected glyph's character to the clipboard", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    render(PreviewPane, {
      entry: entry({ name: "a.woff", path: "/fonts/a.woff", extension: "woff" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Copy glyph/ }));
    await fireEvent.click(btn);

    // Default selection is the grid's first candidate codepoint.
    const first = FONT_GLYPH_CANDIDATES[0];
    expect(writeText).toHaveBeenCalledWith(String.fromCodePoint(first));
  });

  it("Copy codepoint copies the U+XXXX label of the selected glyph", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    render(PreviewPane, {
      entry: entry({ name: "a.woff2", path: "/fonts/a.woff2", extension: "woff2" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Copy codepoint/ }));
    await fireEvent.click(btn);

    const first = FONT_GLYPH_CANDIDATES[0];
    expect(writeText).toHaveBeenCalledWith(codepointLabel(first));
  });

  it("clicking a different glyph cell updates what Copy glyph/Copy codepoint report", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.ttf", path: "/fonts/a.ttf", extension: "ttf" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    await waitFor(() => expect(container.querySelector('[data-testid="font-glyph-grid"]')).toBeTruthy());
    const target = codepointLabel(0x41); // 'A'
    const cell = [...container.querySelectorAll('[data-testid="font-glyph-grid"] button')].find(
      (b) => b.getAttribute("title") === target,
    ) as HTMLElement;
    expect(cell).toBeTruthy();
    await fireEvent.click(cell);

    const copyGlyph = await waitFor(() => screen.getByRole("button", { name: /Copy glyph/ }));
    await fireEvent.click(copyGlyph);
    expect(writeText).toHaveBeenCalledWith("A");

    const copyCodepoint = screen.getByRole("button", { name: /Copy codepoint/ });
    await fireEvent.click(copyCodepoint);
    expect(writeText).toHaveBeenCalledWith("U+0041");
  });

  it("resets to the new file's default glyph when the previewed font changes", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    const { container, rerender } = render(PreviewPane, {
      entry: entry({ name: "a.ttf", path: "/fonts/a.ttf", extension: "ttf" }),
      assetUrl: (p: string) => `asset://${p}`,
    });
    await waitFor(() => expect(container.querySelector('[data-testid="font-glyph-grid"]')).toBeTruthy());

    // Select a non-default glyph on the first file.
    const cellA = [...container.querySelectorAll('[data-testid="font-glyph-grid"] button')].find(
      (b) => b.getAttribute("title") === codepointLabel(0x41),
    ) as HTMLElement;
    await fireEvent.click(cellA);

    // Switch to a different font file — the selection should reset to ITS first candidate, not carry
    // over the previous file's selected glyph.
    await rerender({
      entry: entry({ name: "b.otf", path: "/fonts/b.otf", extension: "otf" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const copyCodepoint = await waitFor(() => screen.getByRole("button", { name: /Copy codepoint/ }));
    await fireEvent.click(copyCodepoint);
    expect(writeText).toHaveBeenCalledWith(codepointLabel(FONT_GLYPH_CANDIDATES[0]));
  });
});
