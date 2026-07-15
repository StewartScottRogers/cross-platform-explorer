import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import PropertiesDialog from "./PropertiesDialog.svelte";
import type { DirEntry } from "../types";

// Mock the Tauri bridge: entry_info returns metadata, hash_file returns a fixed digest (CPE-412).
const invoke = vi.fn(async (cmd: string) => {
  if (cmd === "entry_info")
    return { name: "a.txt", path: "/a.txt", is_dir: false, size: 3, modified: 0, created: 0, readonly: false, hidden: false };
  if (cmd === "hash_file") return "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
  return null;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...(a as [string])) }));

const file = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "a.txt", path: "/a.txt", is_dir: false, size: 3, modified: 0, extension: "txt", hidden: false, ...over,
});

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
});
