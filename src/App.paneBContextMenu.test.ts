/**
 * CPE-1377 — pane B's context menu was entirely inert (`on:rowContext`/`on:driveContext`/
 * `on:contextEmpty`/`on:homeItemContext` were never wired on its `<ExplorerPane>`), and even once wired,
 * a right-click doesn't focus a pane (only a plain click does — see App.svelte's `pane-col` `on:click`),
 * so `runAction`/`askDelete` reading the live `activePane` would silently act on whichever pane last had
 * a plain click instead of the one actually right-clicked. The fix threads `ctx.inPaneB` (set at
 * menu-OPEN time by `onRowContext`/etc., not focus time) through `runAction`'s new `pane = paneStateFor
 * (inPaneB)` and into `askDelete`'s new override parameter — mirroring CPE-1370's `activePaneState`/
 * `snapshotConfirmTarget` pattern for the keyboard path.
 *
 * F2 (already routed pane-aware since CPE-1370) also silently failed for pane B because `renamingPath`/
 * `renameValue` were never bound on pane B's `<ExplorerPane>` — there was nowhere for the editor to
 * render. Fixed with a dedicated `renamingPathB`/`renameValueB` pair.
 *
 * Same mounted-App-with-mocked-backend dual-pane harness as App.deleteSnapshot.test.ts / App.crossPaneDnd.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveDualPane, savePaneBPath } from "./lib/settings";
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
const PATH_B = "C:\\dB";
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];
const entriesA: DirEntry[] = [file("alpha.txt", PATH_A)];
const entriesB: DirEntry[] = [file("bravo.txt", PATH_B)];

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

let renameCalls: { path: string; args: Record<string, unknown> }[] = [];
let deleteToTrashCalls: string[][] = [];
let createDirCalls: { path: string; name: string }[] = [];

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  renameCalls = [];
  deleteToTrashCalls = [];
  createDirCalls = [];
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => (path === PATH_B ? entriesB : entriesA);
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
      case "rename_entry": {
        const path = args.path as string;
        renameCalls.push({ path, args });
        return `${path}.renamed`;
      }
      case "delete_to_trash": {
        const paths = args.paths as string[];
        deleteToTrashCalls.push(paths);
        return paths.map((p) => ({ path: p, ok: true, error: "" }));
      }
      case "create_dir": {
        const path = args.path as string;
        const name = args.name as string;
        createDirCalls.push({ path, name });
        return `${path}\\${name}`;
      }
      default: return null;
    }
  });
});

/** Boot straight into dual-pane, navigate pane A into its drive, wait for both panes to settle — same
 *  helper as App.deleteSnapshot.test.ts. Returns each pane's `.pane-col` wrapper. */
async function bootDualPane() {
  saveDualPane(true);
  savePaneBPath(PATH_B);
  render(App);
  await waitFor(() => expect(screen.getByText("bravo.txt")).toBeTruthy());
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
  const paneAWrap = screen.getByText("alpha.txt").closest(".pane-col") as HTMLElement;
  const paneBWrap = screen.getByText("bravo.txt").closest(".pane-col") as HTMLElement;
  return { paneAWrap, paneBWrap };
}

describe("App — pane B's context menu (CPE-1377)", () => {
  it("right-clicking a pane-B row opens the context menu FOR that row, without needing pane B to be the active pane first", async () => {
    const { paneAWrap } = await bootDualPane();
    // Pane A is (still) the active pane — clicking it, then its row, leaves `activePane` at 0. Pre-fix,
    // this is exactly the scenario that made a pane-B right-click either do nothing (no handler wired)
    // or — had a naive fix just wired the same pane-A-only handler — silently select/act on pane A's
    // alpha.txt instead of the bravo.txt actually clicked.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(screen.getByText("alpha.txt"));

    const bravoRow = screen.getByText("bravo.txt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);

    // Scoped to the popup itself — `CommandBar`'s toolbar has its own, always-rendered "Rename (F2)"
    // icon button with the same title, driven by pane A's `selectionCount`, which would otherwise match
    // too and mask a bug in this assertion.
    const menu = within(await screen.findByRole("menu"));
    // The menu opened at all (rowContext now wired) AND selected bravo.txt in pane B's OWN selection —
    // proven by the quick-row "Rename" icon being enabled (title text changes when selectionCount !== 1,
    // and pane A's alpha.txt is also still selected, so a pane-agnostic single global `selection` would
    // show 2 selected / a disabled Rename here).
    const renameBtn = menu.getByTitle("Rename (F2)");
    expect((renameBtn as HTMLButtonElement).disabled).toBe(false);
    // Pane A's row is untouched — still shows selected (not cleared by the pane-B right-click).
    expect(screen.getByText("alpha.txt").closest(".row")?.className).toContain("selected");
  });

  it("'Copy as path' from a pane-B context menu copies pane B's path, not pane A's — even while pane A is the active pane", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    const { paneAWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(screen.getByText("alpha.txt")); // pane A has its own, different selection

    const bravoRow = screen.getByText("bravo.txt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Copy as path"));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith(`"${PATH_B}\\bravo.txt"`));
    expect(writeText).not.toHaveBeenCalledWith(expect.stringContaining("alpha.txt"));
  });

  it("'Rename' from a pane-B context menu commits against pane B's file, and the editor renders in pane B", async () => {
    const { paneBWrap } = await bootDualPane();
    const bravoRow = screen.getByText("bravo.txt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByTitle("Rename (F2)"));

    const input = await screen.findByDisplayValue("bravo.txt");
    // The editor is inside pane B's own wrapper, not pane A's (proves `renamingPathB`, not the shared
    // pane-A `renamingPath`, was set — before the fix there was nowhere for it to render at all).
    expect(paneBWrap.contains(input)).toBe(true);

    await fireEvent.input(input, { target: { value: "bravo2.txt" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(renameCalls.length).toBe(1));
    expect(renameCalls[0].path).toBe(`${PATH_B}\\bravo.txt`);
  });

  it("F2 on a selected pane-B row opens a committable inline editor in pane B (was previously inert)", async () => {
    const { paneBWrap } = await bootDualPane();
    await fireEvent.click(paneBWrap); // make pane B the active pane (CPE-1370's F2 routing needs this)
    await fireEvent.click(screen.getByText("bravo.txt"));
    await fireEvent.keyDown(window, { key: "F2" });

    const input = await screen.findByDisplayValue("bravo.txt");
    expect(paneBWrap.contains(input)).toBe(true);

    await fireEvent.input(input, { target: { value: "renamed.txt" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      const call = renameCalls[0];
      expect(call).toBeTruthy();
      expect(call.path).toBe(`${PATH_B}\\bravo.txt`);
      expect((call.args as { newName: string }).newName).toBe("renamed.txt");
    });
  });

  it("Delete from a pane-B context menu targets pane B's file — snapshot-safe like CPE-1370's confirm-delete, never the live `activePane` (App.deleteSnapshot.test.ts)", async () => {
    const { paneAWrap } = await bootDualPane();
    // Pane A stays the "active" pane throughout (never clicked into pane B) — the pre-fix bug: reading
    // live `activePane` for a context-menu delete would target pane A's alpha.txt instead of the
    // bravo.txt the menu was actually opened over and the user clicked Delete for.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(screen.getByText("alpha.txt"));

    const bravoRow = screen.getByText("bravo.txt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByTitle("Delete (Del)"));

    await waitFor(() => expect(deleteToTrashCalls.length).toBe(1));
    expect(deleteToTrashCalls[0]).toEqual([`${PATH_B}\\bravo.txt`]); // NOT alpha.txt
  });

  it("New ▸ Folder from pane B's EMPTY-AREA context menu creates inside pane B's folder, not pane A's (Reviewer follow-up)", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    // Pane A stays the "active" pane throughout — the pre-fix bug: `newFolder()` always defaulted to
    // `currentPath` (pane A), regardless of which pane's empty area was actually right-clicked.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(screen.getByText("alpha.txt"));

    // Right-click BLANK space inside pane B's populated row list (not a row itself, which has its own
    // `stopPropagation`ing handler) — this is FileList's `emptyContext`/ExplorerPane's `paneContext`
    // catch-all, both of which fire pane B's `on:contextEmpty` with `inPaneB: true`.
    const paneBRows = paneBWrap.querySelector(".rows") as HTMLElement;
    await fireEvent.contextMenu(paneBRows);

    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("New")); // opens the New ▸ submenu
    await fireEvent.click(menu.getByText("Folder"));

    await waitFor(() => expect(createDirCalls.length).toBe(1));
    expect(createDirCalls[0].path).toBe(PATH_B); // NOT PATH_A
  });
});
