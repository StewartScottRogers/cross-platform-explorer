/**
 * CPE-1369: `selection` is a set of ROW INDICES into `visible`. When the list re-orders IN PLACE — a sort
 * (column header or CommandBar), a type/tag filter, or a streamed batch mid-selection — the indices used to
 * stay put while `visible` moved, so the highlight AND every op target (Delete/rename/copy/…) silently
 * jumped to a DIFFERENT file. This drives the real App: select a file, click the Size header to re-sort so
 * that file moves, and assert the selection still points at the SAME file (not whatever now sits at the old
 * index). Same mocked-Tauri harness as App.archiveNav.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const f = (name: string, size: number): DirEntry => ({
  name,
  path: `C:\\d\\${name}`,
  is_dir: false,
  size,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "txt",
  hidden: false,
  is_symlink: false,
});

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];
// Name-asc order: a, b, c. Size-asc order: b(10), c(50), a(100) — so selecting "c" then sorting by size
// MOVES it from index 2 to index 1, and index 2 becomes "a" — the exact stale-index trap.
const listing: DirEntry[] = [f("a.txt", 100), f("b.txt", 10), f("c.txt", 50)];

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => `asset://${p}`, Channel }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: listing, filtered: 0 };
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage(listing);
        return listing.length;
      }
      case "parent_dir": return null;
      case "read_file_text": return "";
      default: return null;
    }
  });
});

/** The single selected row's text, or "" — used to prove WHICH file the selection resolves to. */
function selectedRowText(): string {
  const rows = document.querySelectorAll(".row.selected");
  return rows.length === 1 ? (rows[0].textContent ?? "") : `#rows=${rows.length}`;
}

describe("App — selection follows the file across an in-place sort (CPE-1369)", () => {
  it("re-sorting keeps the selection on the same file, not the old row index", async () => {
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("c.txt")).toBeTruthy());

    // Select c.txt (index 2 under the default name sort).
    await fireEvent.click(screen.getByText("c.txt"));
    await waitFor(() => expect(selectedRowText()).toContain("c.txt"));

    // Click the Size column header → re-sort ascending → order becomes b, c, a (c moves 2 -> 1).
    await fireEvent.click(screen.getByText("Size"));

    // The selection must still be c.txt. Without the fix, the stale index {2} now highlights a.txt.
    await waitFor(() => expect(selectedRowText()).toContain("c.txt"));
    expect(selectedRowText()).not.toContain("a.txt");
  });
});
