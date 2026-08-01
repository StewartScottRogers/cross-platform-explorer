/**
 * Integration test: opening a saved STRUCTURED search from the sidebar (CPE-1229, epic CPE-978) actually
 * runs `evaluateSavedSearch` over a real recursive scan and shows the filtered result — not just that the
 * pure evaluator works in isolation (already covered by savedSearch.test.ts), but that App.svelte's own
 * open-evaluator wiring (`scan_tree` IPC → `flattenTree` → `evaluateSavedSearch` → the file-list pane)
 * is actually connected end to end, mirroring App.test.ts's precedent for catching wiring-only bugs.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { savedSearches, addSavedSearch } from "./lib/savedSearchStore";
import type { Place } from "./lib/types";
import type { TreeNode } from "./lib/bindings.gen";

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

// A tree scan_tree would return under "C:\\d": one matching .md file, one non-matching .txt file, and a
// nested subfolder with a second matching .md file — proves the open-evaluator both filters AND recurses.
const scannedTree: TreeNode[] = [
  { name: "keep.md", isDir: false, size: 10, modified: 1_700_000_000_000 },
  { name: "skip.txt", isDir: false, size: 10, modified: 1_700_000_000_000 },
  {
    name: "sub",
    isDir: true,
    children: [{ name: "also-keep.md", isDir: false, size: 5, modified: 1_700_000_000_000 }],
  },
];

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

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  savedSearches.set([]); // the store is a module singleton across this file's tests — start clean
  Element.prototype.scrollIntoView = vi.fn();

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return [];
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage([]);
        return 0;
      }
      case "parent_dir": return null;
      case "scan_tree": return scannedTree;
      default: return null;
    }
  });
});

describe("App — opening a saved structured search (CPE-1229 open-evaluator)", () => {
  it("scans the search's captured root and shows only the matching entries, recursively", async () => {
    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");

    render(App);

    // Let the app's own async startup (restore-last-session / loadPath(HOME)) settle first — otherwise
    // it can resolve AFTER our click and stomp the just-opened saved search back to null (it resets
    // `structuredSearch` on every real-folder navigation, same as the tag smart folder it mirrors).
    await screen.findAllByText("Local Disk (C:)");

    const row = await screen.findByText("Markdown docs");
    await fireEvent.click(row);

    // The evaluator ran against a REAL scan_tree call, scoped to the captured root.
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("scan_tree", { path: "C:\\d", maxDepth: 12 });
    });

    // Matches (including the nested one) show; the non-matching file does not.
    await waitFor(() => {
      expect(screen.getByText("keep.md")).toBeTruthy();
      expect(screen.getByText("also-keep.md")).toBeTruthy();
    });
    expect(screen.queryByText("skip.txt")).toBeNull();
  });

  it("right-clicking, renaming, then removing the saved search updates the sidebar live", async () => {
    addSavedSearch("Temp search", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    const row = await screen.findByText("Temp search");
    await fireEvent.contextMenu(row);

    const renameInput = await screen.findByLabelText("Rename…");
    await fireEvent.input(renameInput, { target: { value: "Renamed search" } });
    await fireEvent.keyDown(renameInput, { key: "Enter" });

    expect(await screen.findByText("Renamed search")).toBeTruthy();

    await fireEvent.contextMenu(screen.getByText("Renamed search"));
    await fireEvent.click(await screen.findByText("Delete"));

    await waitFor(() => {
      expect(screen.queryByText("Renamed search")).toBeNull();
    });
  });
});
