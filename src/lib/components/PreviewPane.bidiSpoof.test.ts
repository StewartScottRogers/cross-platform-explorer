/**
 * CPE-1757 round 2 — `<img alt={entry.name}>` was raw across every image preview kind (image/
 * decoded-image/raw-image/heic/dicom). `alt` is a render position (visible on a broken image, and to a
 * screen reader) that round 1's file-level scope never reached: PreviewPane wasn't in the guard's
 * `COVERED_FILES` domain at all. `src`/`assetUrl(entry.path)` deliberately stay raw — the fetch needs the
 * real bytes, same boundary QuickLook's `<img src>` relies on — only `alt` needed the escape.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import PreviewPane from "./PreviewPane.svelte";
import type { DirEntry } from "../types";

const invoke = vi.fn((_cmd: string, _args?: unknown): Promise<unknown> => Promise.resolve(undefined));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));

// Built from a decimal code point, not a literal character — see filename.ts's own doc comment for why.
const RLO = String.fromCharCode(0x202e);

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

beforeEach(() => invoke.mockClear());

describe("PreviewPane — bidi/format-character escape on an image's alt text (CPE-1757)", () => {
  it("escapes an override in a native-image preview's alt text, leaving src raw for the fetch", async () => {
    const spoofed = entry({ name: `${RLO}gnp.jpg`, path: `/photos/${RLO}gnp.jpg`, extension: "jpg" });
    const { container } = render(PreviewPane, { entry: spoofed, assetUrl: (p: string) => `asset://${p}` });

    await waitFor(() => expect(container.querySelector("img.preview-img")).toBeTruthy());
    const img = container.querySelector("img.preview-img") as HTMLImageElement;
    expect(img.getAttribute("alt")).toBe("[RLO]gnp.jpg");
    expect(img.getAttribute("src")).toBe(`asset://${spoofed.path}`); // raw path — the fetch needs real bytes
    expect(container.textContent).not.toContain("jpg.png");
  });
});
