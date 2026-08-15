/**
 * CPE-1712 review round 2 — coverage regression guard.
 *
 * Round 1 fixed `DetailsPane`'s name heading but left the "Path" row right below it rendering the raw
 * name/path — the two sitting side by side, one honest and one still lying. Every assertion here is on
 * `container.textContent`/`screen` text, per Evidence Rule "assert what the user SEES".
 */
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import DetailsPane from "./DetailsPane.svelte";
import type { DirEntry } from "../types";

// Built from a decimal code point, not a literal character — see filename.ts's own doc comment for why.
const RLO = String.fromCharCode(0x202e);

const entry = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "readme.md",
  path: "/x/readme.md",
  is_dir: false,
  size: 1024,
  modified: 0,
  extension: "md",
  hidden: false,
  is_symlink: false,
  ...over,
});

describe("DetailsPane — the 'Path' row (CPE-1712 round 2 blocker)", () => {
  it("escapes the Path row, not just the name heading, for a spoofed entry", () => {
    const spoofed = entry({ name: `${RLO}gnp.txt`, path: `/x/${RLO}gnp.txt` });
    const { container } = render(DetailsPane, { selected: [spoofed] });

    // The name heading (already covered pre-round-2) AND the Path row must both read safely.
    expect(screen.getByText("[RLO]gnp.txt")).toBeTruthy();
    expect(container.textContent).not.toContain("txt.png");
    // The exact probe the review used: the raw override survives nowhere in the rendered text.
    expect(container.textContent?.includes(RLO)).toBe(false);
  });

  it("still shows an ordinary path and a real Hebrew path untouched", () => {
    const { container } = render(DetailsPane, {
      selected: [entry({ name: "מסמך.txt", path: "/home/alice/מסמך.txt" })],
    });
    expect(container.textContent).toContain("מסמך.txt");
    expect(container.textContent).toContain("/home/alice/מסמך.txt");
  });
});
