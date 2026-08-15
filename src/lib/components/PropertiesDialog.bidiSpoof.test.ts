/**
 * CPE-1712 review round 2 — coverage regression guard.
 *
 * PropertiesDialog was named explicitly as a not-optional blocker: the whole point of this dialog is
 * "what IS this file?", so a raw name/location here is the exact deception this ticket exists to stop.
 * Mocking strategy mirrors `PropertiesDialog.test.ts`'s own.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import PropertiesDialog from "./PropertiesDialog.svelte";
import type { DirEntry } from "../types";

// Built from a decimal code point, not a literal character — see filename.ts's own doc comment for why.
const RLO = String.fromCharCode(0x202e);

const invoke = vi.fn(async (cmd: string, args?: { path?: string }) => {
  if (cmd === "entry_info") {
    return { name: `${RLO}gnp.txt`, path: args?.path ?? "", is_dir: false, size: 3, modified: 0, created: 0, readonly: false, hidden: false };
  }
  if (cmd === "inspect_file") return { encoding: "UTF-8", line_endings: "LF", file_type: null, type_mismatch: null, architecture: null };
  if (cmd === "native_tags_name") return "NTFS alternate data streams";
  if (cmd === "native_tags_pull") return {};
  return null;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...(a as [string])) }));

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

beforeEach(() => invoke.mockClear());

describe("PropertiesDialog — name AND location (CPE-1712 round 2 blocker)", () => {
  it("escapes both the file-name heading and the Location row", async () => {
    const spoofed: DirEntry = entry({ name: `${RLO}gnp.txt`, path: `/x/${RLO}gnp.txt` });
    const { container } = render(PropertiesDialog, { entries: [spoofed] });
    await waitFor(() => expect(container.querySelector(".fname")).toBeTruthy());

    expect(container.querySelector(".fname")?.textContent).toBe("[RLO]gnp.txt");
    expect(container.querySelector(".path")?.textContent).toBe("/x/[RLO]gnp.txt");
    expect(container.textContent).not.toContain("txt.png");
  });
});
