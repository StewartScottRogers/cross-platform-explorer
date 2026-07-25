import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import PropertiesDialog from "./PropertiesDialog.svelte";
import type { DirEntry } from "../types";

// Mock the Tauri bridge: entry_info returns metadata, hash_file returns a fixed digest (CPE-412).
const invoke = vi.fn(async (cmd: string) => {
  if (cmd === "entry_info")
    return { name: "a.txt", path: "/a.txt", is_dir: false, size: 3, modified: 0, created: 0, readonly: false, hidden: false };
  if (cmd === "hash_file") return "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
  if (cmd === "text_stats") return { lines: 2, words: 3, chars: 16, bytes: 16 };
  return null;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...(a as [string])) }));

const file = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "a.txt", path: "/a.txt", is_dir: false, size: 3, modified: 0, extension: "txt", hidden: false, ...over,
});
const img = (over: Partial<DirEntry> = {}): DirEntry => file({ name: "p.png", path: "/p.png", extension: "png", ...over });

describe("PropertiesDialog — SHA-256 checksum (CPE-412)", () => {
  beforeEach(() => invoke.mockClear());

  it("does not hash automatically; Compute triggers hash_file and shows the digest", async () => {
    render(PropertiesDialog, { entries: [file()] });
    // The row starts with a Compute button — no automatic hashing.
    const compute = await screen.findByText("Compute");
    expect(invoke).not.toHaveBeenCalledWith("hash_file", expect.anything());

    await fireEvent.click(compute);
    await waitFor(() =>
      expect(screen.getByText("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")).toBeTruthy(),
    );
    expect(invoke).toHaveBeenCalledWith("hash_file", { path: "/a.txt" });
  });

  it("shows no checksum row for a folder", async () => {
    render(PropertiesDialog, { entries: [file({ is_dir: true, name: "d", path: "/d", extension: "" })] });
    await waitFor(() => expect(screen.queryByText("SHA-256")).toBeNull());
  });

  it("verifies against a pasted expected hash — Match then No match (CPE-413)", async () => {
    const digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    render(PropertiesDialog, { entries: [file()] });
    await fireEvent.click(await screen.findByText("Compute"));
    const input = await screen.findByPlaceholderText("Paste expected hash to verify");
    // Correct (uppercased + spaced) → Match.
    await fireEvent.input(input, { target: { value: `  ${digest.toUpperCase()} ` } });
    await waitFor(() => expect(screen.getByText("✓ Match")).toBeTruthy());
    // Wrong → No match.
    await fireEvent.input(input, { target: { value: "deadbeef" } });
    await waitFor(() => expect(screen.getByText("✗ No match")).toBeTruthy());
  });

  it("text stats (CPE-414): offered for a text file, Count shows the counts", async () => {
    render(PropertiesDialog, { entries: [file()] });
    await fireEvent.click(await screen.findByText("Count"));
    await waitFor(() => expect(screen.getByText(/2 lines · 3 words · 16 characters/)).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("text_stats", { path: "/a.txt" });
  });

  it("text stats: NOT offered for a non-text (image) file", async () => {
    render(PropertiesDialog, { entries: [img()] });
    await screen.findByText("SHA-256"); // dialog rendered
    expect(screen.queryByText("Contents")).toBeNull();
  });
});
