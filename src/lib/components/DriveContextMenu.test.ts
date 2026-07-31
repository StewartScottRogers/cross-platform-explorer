/**
 * CPE-1158 — right-clicking a disk/drive opens a folder-like context menu targeting the drive ROOT.
 *
 * Two layers are covered without a real WebView2 GUI:
 *   1. HomeView drive TILES dispatch `driveContext` with the drive's root path (and place tiles/blank
 *      Home DON'T), so ExplorerPane/App can open the menu — and the Home background stays menu-less.
 *   2. ContextMenu's `target: "drive"` branch renders Open / New ▸ (Folder|Text file) / Copy as path /
 *      Open in Terminal / Properties and dispatches the drive-* actions App routes to the drive root.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import HomeView from "./HomeView.svelte";
import ContextMenu from "./ContextMenu.svelte";
import type { Place } from "../types";

// The component tree imports Tauri APIs transitively; stub them for jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

// HomeView persists its layout (CPE-573); isolate localStorage so tests don't inherit each other's state.
beforeEach(() => { try { localStorage.clear(); } catch { /* ignore */ } });

const drives: Place[] = [
  { name: "Local Disk (C:)", path: "C:\\", kind: "drive" },
  { name: "Data (Z:)", path: "Z:\\", kind: "drive" },
];
const places: Place[] = [{ name: "Documents", path: "/home/docs", kind: "documents" }];

describe("HomeView drive tile right-click (CPE-1158)", () => {
  it("dispatches driveContext with the drive's root path + name", async () => {
    const { component } = render(HomeView, { places, drives, pins: [], recents: [], favorites: [] });
    const driveContext = vi.fn();
    component.$on("driveContext", (e) => driveContext(e.detail));

    // The drive tile shows its display name; right-click its card.
    const tile = screen.getByText("Data (Z:)").closest("button")!;
    await fireEvent.contextMenu(tile);

    expect(driveContext).toHaveBeenCalledTimes(1);
    expect(driveContext.mock.calls[0][0]).toMatchObject({ path: "Z:\\", name: "Data (Z:)" });
  });

  it("does NOT dispatch driveContext for a non-drive (place) tile", async () => {
    const { component } = render(HomeView, { places, drives, pins: [], recents: [], favorites: [] });
    const driveContext = vi.fn();
    component.$on("driveContext", (e) => driveContext(e.detail));

    const tile = screen.getByText("Documents").closest("button")!;
    await fireEvent.contextMenu(tile);

    expect(driveContext).not.toHaveBeenCalled();
  });
});

const driveMenu = {
  x: 10,
  y: 10,
  target: "drive" as const,
};

/** Open a submenu by its parent label and return the flyout's <menu> element. */
async function openSubmenu(label: string): Promise<HTMLElement> {
  const parent = screen.getByText(label).closest("button")!;
  await fireEvent.mouseEnter(parent.parentElement!); // wrapper .submenu handles hover-open
  const flyout = document.querySelector(".flyout") as HTMLElement;
  expect(flyout).toBeTruthy();
  return flyout;
}

describe("ContextMenu drive branch (CPE-1158)", () => {
  it("Open dispatches drive-open (navigates into the drive root)", async () => {
    const { component } = render(ContextMenu, { props: { ...driveMenu } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    await fireEvent.click(screen.getByText("Open"));
    expect(action).toHaveBeenCalledWith("drive-open");
  });

  it("New ▸ dispatches drive-new-folder / drive-new-file (create AT the drive root)", async () => {
    const { component } = render(ContextMenu, { props: { ...driveMenu } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Folder"));
    expect(action).toHaveBeenCalledWith("drive-new-folder");

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Text file"));
    expect(action).toHaveBeenCalledWith("drive-new-file");
  });

  it("Properties dispatches drive-properties (of the drive root)", async () => {
    const { component } = render(ContextMenu, { props: { ...driveMenu } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    await fireEvent.click(screen.getByText("Properties"));
    expect(action).toHaveBeenCalledWith("drive-properties");
  });

  it("offers Copy as path + Open in Terminal, and omits volume-nonsensical items (Rename/Delete/Cut)", async () => {
    const { component } = render(ContextMenu, { props: { ...driveMenu } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await fireEvent.click(screen.getByText("Copy as path"));
    expect(action).toHaveBeenCalledWith("drive-copy-path");
    await fireEvent.click(screen.getByText("Open in Terminal"));
    expect(action).toHaveBeenCalledWith("drive-terminal");

    // No rename/delete quick-row, no Duplicate — a drive isn't renamed/deleted like a file.
    expect(screen.queryByText("Duplicate")).toBeNull();
    expect(screen.queryByTitle(/Delete/)).toBeNull();
    expect(screen.queryByTitle(/Rename/)).toBeNull();
  });
});
