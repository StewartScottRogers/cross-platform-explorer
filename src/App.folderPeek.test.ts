/**
 * CPE-1426: the preview pane doubles as a folder browser when a DIRECTORY is highlighted in the main
 * list — a one-level "peek" (reusing the same `list_dir_stream` the main list itself streams from), and
 * clicking a subfolder inside that peek drives the main pane's real `navigate()` + selects the clicked
 * row, so a folder tree can be walked entirely from the preview pane (Miller-columns / Finder
 * column-view feel). Same mocked-Tauri harness as App.selectionReorder.test.ts.
 *
 * Fixture tree:
 *   C:\d                     -> [docs/, readme.txt]
 *   C:\d\docs                -> [sub/, note.txt]
 *   C:\d\docs\sub             -> [] (empty — exercises the "empty folder" note)
 *   C:\d\baddir               -> list_dir_stream rejects (exercises the "can't open" note)
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const MODIFIED = new Date(2026, 6, 10, 15, 0).getTime();

function dir(name: string, parent: string): DirEntry {
  return {
    name,
    path: `${parent}\\${name}`,
    is_dir: true,
    size: 0,
    modified: MODIFIED,
    extension: "",
    hidden: false,
    is_symlink: false,
  };
}

function file(name: string, parent: string, ext: string): DirEntry {
  return {
    name,
    path: `${parent}\\${name}`,
    is_dir: false,
    size: 42,
    modified: MODIFIED,
    extension: ext,
    hidden: false,
    is_symlink: false,
  };
}

const ROOT = "C:\\d";
const DOCS = `${ROOT}\\docs`;
const SUB = `${DOCS}\\sub`;
const BADDIR = `${ROOT}\\baddir`;

const rootEntries = [dir("docs", ROOT), dir("baddir", ROOT), file("readme.txt", ROOT, "txt")];
const docsEntries = [dir("sub", DOCS), file("note.txt", DOCS, "txt")];
const subEntries: DirEntry[] = [];

const listingByPath: Record<string, DirEntry[]> = {
  [ROOT]: rootEntries,
  [DOCS]: docsEntries,
  [SUB]: subEntries,
};

const drives: Place[] = [{ name: "Local Disk (C:)", path: ROOT, kind: "drive" }];

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
      case "list_dir": {
        const p = args.path as string;
        if (p === BADDIR) throw new Error("access denied");
        return { entries: listingByPath[p] ?? [], filtered: 0 };
      }
      case "list_dir_stream": {
        const p = args.path as string;
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        if (p === BADDIR) throw new Error("access denied");
        const entries = listingByPath[p] ?? [];
        if (entries.length) ch.onmessage(entries);
        return entries.length;
      }
      case "cancel_dir_stream": return null;
      case "parent_dir": return null;
      case "read_file_text": return "hello";
      default: return null;
    }
  });
});

/** The single selected row's text, or a diagnostic when it's not exactly one. */
function selectedRowText(): string {
  const rows = document.querySelectorAll(".row.selected");
  return rows.length === 1 ? (rows[0].textContent ?? "") : `#rows=${rows.length}`;
}

async function openRoot() {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("docs")).toBeTruthy());
}

describe("App — preview pane folder peek (CPE-1426)", () => {
  it("selecting a folder shows its streamed contents in the preview pane", async () => {
    await openRoot();

    await fireEvent.click(screen.getByText("docs"));

    // The peek is debounced (~150ms) — wait for it to settle and stream "docs"'s own contents.
    await waitFor(() => expect(document.querySelector(".folder-browser")).toBeTruthy());
    await waitFor(() => expect(screen.getByText("sub")).toBeTruthy());
    expect(screen.getByText("note.txt")).toBeTruthy();
  });

  it("clicking a subfolder in the peek navigates the main pane (parent + selected child) and re-points the preview", async () => {
    await openRoot();
    await fireEvent.click(screen.getByText("docs"));
    await waitFor(() => expect(screen.getByText("sub")).toBeTruthy());

    // "sub" only exists inside the peek right now (the main list is still showing root) — this click
    // targets the peek row.
    await fireEvent.click(screen.getByText("sub"));

    // The main pane descended into "docs" (its file now shows) and landed with "sub" selected.
    await waitFor(() => expect(screen.getByText("note.txt")).toBeTruthy());
    await waitFor(() => expect(selectedRowText()).toContain("sub"));
    expect(screen.queryByText("readme.txt")).toBeNull(); // left root's listing

    // The preview re-pointed to "sub"'s own (empty) peek rather than staying on "docs".
    await waitFor(() => expect(screen.getByText("This folder is empty")).toBeTruthy());
  });

  it("a file selection still shows the normal file preview, not the folder browser", async () => {
    await openRoot();

    await fireEvent.click(screen.getByText("readme.txt"));

    await waitFor(() => expect(selectedRowText()).toContain("readme.txt"));
    expect(document.querySelector(".folder-browser")).toBeNull();
  });

  it("an empty folder shows an empty note (never an error dialog)", async () => {
    await openRoot();
    await fireEvent.click(screen.getByText("docs"));
    await waitFor(() => expect(screen.getByText("sub")).toBeTruthy());
    await fireEvent.click(screen.getByText("sub")); // descend into docs with sub selected (empty)

    await waitFor(() => expect(screen.getByText("This folder is empty")).toBeTruthy());
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it("an inaccessible folder shows a can't-open note (never an error dialog)", async () => {
    await openRoot();

    await fireEvent.click(screen.getByText("baddir"));

    await waitFor(() => expect(screen.getByText("Can't open this folder.")).toBeTruthy());
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
});
