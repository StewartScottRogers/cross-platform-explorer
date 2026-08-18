/**
 * CPE-1367: the file-type filter (`fileFilter`) is transient view state, not a persisted preference — so,
 * like its siblings `search`/`selectedTag`, it must clear on real navigation. It used to be a bare top-
 * level `let` that `loadPath` never reset, so a filter set in one folder/tab bled into every other tab and
 * new/duplicated tabs (the CPE-1366 class of leak: a top-level view-var loadPath forgot to reset), often
 * showing an empty pane. This drives the real App to prove navigation now clears the filter. Same mocked-
 * Tauri harness as App.archiveNav.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const entry = (name: string, path: string, extension: string): DirEntry => ({
  name,
  path,
  is_dir: false,
  size: 1024,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension,
  hidden: false,
  is_symlink: false,
});

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];
const listing: DirEntry[] = [entry("report.txt", "C:\\d\\report.txt", "txt")];

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

describe("App — file-type filter resets on navigation (CPE-1367)", () => {
  it("a filter set in a folder does not persist across a navigation", async () => {
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("report.txt")).toBeTruthy());

    // Apply the "Images" file-type filter — a .txt is not an image, so the row is filtered out.
    await fireEvent.click(screen.getByTitle("Filter by type"));
    await fireEvent.click(await screen.findByText("Images"));
    await waitFor(() => expect(screen.queryByText("report.txt")).toBeNull());

    // Navigate (re-enter the drive → a real loadPath). The filter must clear, so the .txt returns.
    await fireEvent.click(screen.getAllByText("Local Disk (C:)")[0]);
    await waitFor(() => expect(screen.getByText("report.txt")).toBeTruthy());
  });
});
