/**
 * CPE-1376 — pane B was a second-class citizen for display props: it never got `search`/`fileFilter`
 * (always showed the unfiltered listing), `showFolderSizes`/`folderSizes`/`on:needSizes` (no recursive
 * size column), `cutPaths` (no dimmed cut styling), or its own tag filter (`selectedTag`/`on:filterTag`
 * always drove pane A only, even when pane B was the active pane). This drives the real dual-pane App —
 * same mounted-App-with-mocked-backend harness as App.deleteSnapshot.test.ts / App.crossPaneDnd.test.ts
 * — proving pane B's `visible` rows actually respect each of these now that they're wired.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveDualPane, savePaneBPath, saveShowFolderSizes } from "./lib/settings";
import { setEntryTags } from "./lib/tags";
import type { DirEntry, Place } from "./lib/types";

const file = (name: string, dir: string, extension = "txt"): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: false,
  size: 10,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension,
  hidden: false,
  is_symlink: false,
});

const folder = (name: string, dir: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: true,
  size: 0,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "",
  hidden: false,
  is_symlink: false,
});

const PATH_A = "C:\\d";
const PATH_B = "C:\\dB";
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];

let diskA: DirEntry[] = [];
let diskB: DirEntry[] = [];
let dirSizeCalls: string[] = [];

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
  dirSizeCalls = [];
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => (path === PATH_B ? diskB : diskA);
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: listingFor(args.path), filtered: 0 };
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "dir_size": {
        dirSizeCalls.push(args.path as string);
        return 123456;
      }
      case "set_tags": {
        const path = args.path as string;
        const tagList = (args.tags as string[] | undefined) ?? [];
        const label = (args.label as string | undefined) ?? "";
        return { [path]: { tags: tagList, label } };
      }
      default: return null;
    }
  });
});

/** Boot straight into dual-pane, navigate pane A into its drive, wait for both panes to settle. */
async function bootDualPane() {
  saveDualPane(true);
  savePaneBPath(PATH_B);
  render(App);
  await waitFor(() => expect(screen.getByText(diskB[0].name)).toBeTruthy());
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText(diskA[0].name)).toBeTruthy());
}

describe("App — pane B search + file-type filter (CPE-1376)", () => {
  beforeEach(() => {
    diskA = [file("alpha.txt", PATH_A)];
    diskB = [file("bravo.txt", PATH_B), file("cover.jpg", PATH_B, "jpg")];
  });

  it("a free-text search narrows pane B's visible rows", async () => {
    await bootDualPane();
    expect(screen.getByText("bravo.txt")).toBeTruthy();
    expect(screen.getByText("cover.jpg")).toBeTruthy();

    const searchInput = document.querySelector(".search input") as HTMLInputElement;
    await fireEvent.input(searchInput, { target: { value: "bravo" } });

    await waitFor(() => expect(screen.queryByText("cover.jpg")).toBeNull());
    expect(screen.getByText("bravo.txt")).toBeTruthy();
  });

  it("the file-type filter narrows pane B's visible rows", async () => {
    await bootDualPane();
    await fireEvent.click(screen.getByTitle("Filter by type"));
    await fireEvent.click(await screen.findByText("Images"));

    await waitFor(() => expect(screen.queryByText("bravo.txt")).toBeNull());
    expect(screen.getByText("cover.jpg")).toBeTruthy();
  });
});

describe("App — pane B recursive folder sizes (CPE-1376)", () => {
  beforeEach(() => {
    diskA = [file("alpha.txt", PATH_A)];
    diskB = [folder("bigfolder", PATH_B)];
  });

  it("wires on:needSizes for pane B and renders the fetched size on its row", async () => {
    saveShowFolderSizes(true);
    await bootDualPane();

    await waitFor(() => expect(dirSizeCalls).toContain(`${PATH_B}\\bigfolder`));
    // formatSize(123456) — asserting the resolved size actually painted onto pane B's row (not just
    // that the backend call fired), proving `folderSizes`/`showFolderSizes` are truly wired through.
    await waitFor(() => expect(screen.queryByText("…")).toBeNull());
    const row = screen.getByText("bigfolder").closest(".row") as HTMLElement;
    expect(row.textContent).not.toContain("—");
  });
});

describe("App — pane B cut-highlight (CPE-1376)", () => {
  it("dims pane B's row for a path that's staged in the (shared) cut clipboard", async () => {
    diskA = [file("shared.txt", PATH_A)];
    diskB = diskA; // mirror pane B onto the SAME folder so the SAME path is a row in both panes
    saveDualPane(true);
    savePaneBPath(PATH_A);
    render(App);

    // Pane B auto-restores to its persisted folder on mount — it alone shows "shared.txt" until pane A
    // is also navigated there below.
    await waitFor(() => expect(screen.getAllByText("shared.txt").length).toBe(1));
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getAllByText("shared.txt").length).toBe(2));

    // Cut it via pane A (pane A is still the active pane here — neither `.pane-col` wrapper was
    // clicked, so Ctrl+X routes to pane A per CPE-1380's `activePaneState()` — see
    // App.clipboardPaneRouting.test.ts for the pane-B-active routing itself) — the resulting `cutPaths`
    // is a single shared clipboard, so pane B's row for the SAME path must dim too, proving `{cutPaths}`
    // actually reached pane B's ExplorerPane.
    const rows = screen.getAllByText("shared.txt").map((el) => el.closest(".row") as HTMLElement);
    await fireEvent.click(rows[0]);
    await fireEvent.keyDown(window, { key: "x", ctrlKey: true });

    await waitFor(() => {
      const dimmed = screen.getAllByText("shared.txt").map((el) => el.closest(".row") as HTMLElement);
      expect(dimmed.every((r) => r.className.includes("cut"))).toBe(true);
    });
  });
});

describe("App — pane B has its own tag filter (CPE-1376)", () => {
  it("filtering by a tag while pane B is active only narrows pane B, leaving pane A untouched", async () => {
    diskA = [file("alpha.txt", PATH_A)];
    diskB = [file("tagged.txt", PATH_B), file("other.txt", PATH_B)];
    await bootDualPane();

    // Tag AFTER mount: initTags() runs once on mount and would otherwise stomp a pre-mount tag back to
    // the (mocked, empty) backend store (same technique as App.smartFolderLiveRefresh.test.ts).
    await setEntryTags(`${PATH_B}\\tagged.txt`, ["invoice"], "");

    // Make pane B the active pane, then click the SIDEBAR's "invoice" tag entry — disambiguated from
    // "tagged.txt"'s own per-row tag chip (same text, different element) via its distinctive title.
    const paneBWrap = screen.getByText("tagged.txt").closest(".pane-col") as HTMLElement;
    await fireEvent.click(paneBWrap);
    const tagChip = await screen.findByTitle(/click to filter/);
    await fireEvent.click(tagChip);

    await waitFor(() => expect(screen.queryByText("other.txt")).toBeNull());
    expect(screen.getByText("tagged.txt")).toBeTruthy();
    // Pane A has nothing tagged "invoice" and its OWN `selectedTag` was never touched — it must still
    // show its full listing, proving the filter is per-pane (`selectedTagB`), not global.
    expect(screen.getByText("alpha.txt")).toBeTruthy();
  });
});
