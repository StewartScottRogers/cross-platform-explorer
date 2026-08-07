/**
 * CPE-1410: regression pin for archive-inside-virtual-view nesting. `enterArchive()` (App.svelte) never
 * clears `smartFolder`/`structuredSearch` — only `openSmartFolder`/`openStructuredSearch`/`loadPath` clear
 * `archive`. So double-clicking a .zip from INSIDE a smart-folder or saved-search listing stacks `archive`
 * ON TOP of the still-set virtual view: `archiveOverride` wins over `smartOverride` in
 * `ExplorerPane.svelte` (`baseEntries = archiveOverride ?? smartOverride ?? entries`), and `exitArchive()`
 * (only clears `archive`) falls back to the still-set virtual view rather than the plain real-folder
 * listing. This is believed CORRECT by design but was untested — smart-folder tests and archive tests
 * never intersected — so a future edit to `openSmartFolder`/`enterArchive`/`exitArchive` could silently
 * break the stacking/unstacking without any test catching it.
 *
 * This test exercises the structured-search flavor of the virtual view (same `smartOverride` mechanism
 * a tag smart folder uses — see `App.smartFolderLiveRefresh.test.ts` for the sibling tag-folder harness).
 * The real folder `C:\d` is mocked to list NOTHING (`list_dir`/`list_dir_stream` return `[]`), while the
 * structured search matches one entry, "bundle.zip", via `scan_tree`. That asymmetry is what makes the
 * post-exit assertion meaningful: if `exitArchive()` regressed to fall back to the plain folder instead of
 * the structured search, "bundle.zip" would NOT reappear (the real `C:\d` is empty).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { savedSearches, addSavedSearch } from "./lib/savedSearchStore";
import type { Place } from "./lib/types";
import type { TreeNode } from "./lib/bindings.gen";

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

// The structured search's captured root (C:\d) contains one matching file when scanned recursively.
const scannedTree: TreeNode[] = [
  { name: "bundle.zip", isDir: false, size: 100, modified: 1_700_000_000_000 },
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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  savedSearches.set([]);
  Element.prototype.scrollIntoView = vi.fn();

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      // The real C:\d is deliberately EMPTY so a fallback-to-plain-folder regression is visible: if exit
      // ever lands on the plain folder instead of the structured search, "bundle.zip" won't reappear.
      case "list_dir": return [];
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage([]);
        return 0;
      }
      case "parent_dir": return null;
      case "scan_tree": return scannedTree;
      case "read_archive_entries": return [{ name: "inside.txt", size: 5, is_dir: false }];
      case "read_file_text": return "";
      default: return null;
    }
  });
});

describe("App — archive entered from inside a structured search stays stacked on it (CPE-1410)", () => {
  it("archive overlays the virtual view, then exitArchive() falls back to it (not the plain folder)", async () => {
    addSavedSearch("Zips", [{ kind: "ext", exts: ["zip"] }], "all", "C:\\d");
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    // The `.rows` container is the file-list body — scoping row lookups to it (rather than the whole
    // document) avoids false matches against the breadcrumb, which also renders the current zip/search
    // name as text (see step 2/3 below).
    const rows = () => within(document.querySelector(".rows") as HTMLElement);

    // 1. Open the structured search — its virtual listing is active (breadcrumb's current crumb is the
    //    search's own name, driven by `structuredSearch` per App.svelte's `folderName`/`crumbs`), and its
    //    one match renders in the file list.
    await fireEvent.click(await screen.findByText("Zips"));
    await waitFor(() => expect(rows().getByText("bundle.zip")).toBeTruthy());
    expect(document.querySelector(".crumb.current")?.textContent).toBe("Zips");

    // 2. Enter the archive from within the structured-search listing. `structuredSearch` is NOT cleared
    //    (enterArchive() only sets `archive`) — but `archiveOverride` wins over `smartOverride` in
    //    ExplorerPane (`baseEntries = archiveOverride ?? smartOverride ?? entries`), so the archive's
    //    inner contents overlay the search results in the file list: "inside.txt" now shows there,
    //    "bundle.zip" is no longer a row (it's still in `structuredSearchEntries` underneath, just not
    //    the rendered list — only the breadcrumb still shows the zip's own name here, which is expected).
    await fireEvent.dblClick(rows().getByText("bundle.zip"));
    await waitFor(() => expect(rows().getByText("inside.txt")).toBeTruthy());
    expect(rows().queryByText("bundle.zip")).toBeNull();

    // 3. Exit the archive via the toolbar's Up action — at the archive root (`inner === ""`) `goUp()`
    //    calls `exitArchive()`, which clears only `archive`. Since `structuredSearch` was never cleared,
    //    the view falls back to it: "bundle.zip" reappears in the file list AND the breadcrumb goes back
    //    to the search's own name. If this regressed to fall back to the plain real folder instead,
    //    "bundle.zip" would NOT reappear (C:\d is mocked empty above).
    await fireEvent.click(screen.getByTitle(/Alt\+Up/));
    await waitFor(() => expect(rows().getByText("bundle.zip")).toBeTruthy());
    expect(rows().queryByText("inside.txt")).toBeNull();
    expect(document.querySelector(".crumb.current")?.textContent).toBe("Zips");
  });
});
