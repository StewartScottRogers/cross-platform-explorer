/**
 * CPE-1531 (epic CPE-1489) — the two "Add to Drop Stack" entry points wired on top of CPE-1530's
 * pure store (src/lib/dropStack.ts): a row context-menu item and a default Ctrl+Shift+D hotkey, both
 * calling `addToDropStack(selectedPaths, currentFolderPath)`. Mirrors `doCopy`/`doCut`'s pane-aware,
 * `hasSelection`-gated shape (App.svelte:3153 onward) — this ticket only wires the two entry points,
 * not the panel (CPE-1532) or the move/copy-all action (CPE-1533).
 *
 * `dropStack.ts`'s `store` is a module-level singleton (like `transfers.ts`), so it persists across
 * `render(App)` calls within this file — `clearDropStack()` in `beforeEach` resets it (and the
 * persisted `settings.dropStack` key) to a known-empty slate before every test, independent of whether
 * `initDropStack()`'s own once-only `started` guard has already fired in an earlier test.
 *
 * Same mounted-App-with-mocked-backend single-pane harness as App.clipboardPaneRouting.test.ts's
 * single-pane cases / App.paneBContextMenu.test.ts's context-menu-click pattern.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { clearDropStack, dropStackEntries } from "./lib/dropStack";
import { get } from "svelte/store";
import type { DirEntry, Place } from "./lib/types";

const file = (name: string, dir: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: false,
  size: 10,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "txt",
  hidden: false,
  is_symlink: false,
});

const PATH_A = "C:\\d";
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];
const entriesA: DirEntry[] = [file("alpha.txt", PATH_A), file("beta.txt", PATH_A)];

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
  clearDropStack();
  Element.prototype.scrollIntoView = vi.fn();
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: args.path === PATH_A ? entriesA : [], filtered: 0 };
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = args.path === PATH_A ? entriesA : [];
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "read_file_text": return "";
      default: return null;
    }
  });
});

/** Boot into pane A's own folder and wait for both rows to be visible. */
async function bootFolder() {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
  expect(screen.getByText("beta.txt")).toBeTruthy();
}

describe("App — \"Add to Drop Stack\" context-menu item + hotkey (CPE-1531)", () => {
  it("the context-menu item calls dropStack.add with the current selection and its source folder", async () => {
    await bootFolder();

    await fireEvent.click(screen.getByText("alpha.txt"));
    const row = screen.getByText("alpha.txt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(row);

    const menu = within(await screen.findByRole("menu"));
    const item = menu.getByText("Add to Drop Stack");
    await fireEvent.click(item);

    await waitFor(() => expect(get(dropStackEntries)).toHaveLength(1));
    const [entry] = get(dropStackEntries);
    expect(entry.path).toBe(`${PATH_A}\\alpha.txt`);
    expect(entry.addedFrom).toBe(PATH_A);
  });

  it("Ctrl+Shift+D triggers the same add action for the current selection", async () => {
    await bootFolder();

    await fireEvent.click(screen.getByText("beta.txt"));
    await fireEvent.keyDown(window, { key: "d", ctrlKey: true, shiftKey: true });

    await waitFor(() => expect(get(dropStackEntries)).toHaveLength(1));
    const [entry] = get(dropStackEntries);
    expect(entry.path).toBe(`${PATH_A}\\beta.txt`);
    expect(entry.addedFrom).toBe(PATH_A);
  });

  it("multi-selection: both entry points shelve every selected path", async () => {
    await bootFolder();

    await fireEvent.click(screen.getByText("alpha.txt"));
    await fireEvent.click(screen.getByText("beta.txt"), { ctrlKey: true });
    await fireEvent.keyDown(window, { key: "d", ctrlKey: true, shiftKey: true });

    await waitFor(() => expect(get(dropStackEntries)).toHaveLength(2));
    const paths = get(dropStackEntries).map((e) => e.path).sort();
    expect(paths).toEqual([`${PATH_A}\\alpha.txt`, `${PATH_A}\\beta.txt`].sort());
  });

  it("is a no-op with no selection: the hotkey adds nothing when nothing is selected", async () => {
    await bootFolder();

    // Fresh boot: nothing is selected yet (no click on any row).
    await fireEvent.keyDown(window, { key: "d", ctrlKey: true, shiftKey: true });

    // Give any (incorrect) async add a beat to land before asserting it didn't.
    await new Promise((r) => setTimeout(r, 20));
    expect(get(dropStackEntries)).toHaveLength(0);
  });
});
