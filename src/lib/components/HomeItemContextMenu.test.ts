/**
 * CPE-1162 — the ContextMenu `target: "home-item"` branch: right-clicking a Home Recent/Favorites/
 * Folders row opens a file/folder-like menu targeting that row's real path.
 *
 * Covered here (no real WebView2 GUI needed):
 *   - the action set adapts to file vs folder (Open-in-new-tab + New ▸ only for folders);
 *   - the CRITICAL peculiarity: "Delete" (trashes the real file, `home-delete`) is a DIFFERENT action
 *     from the pointer-level "Remove from <view>" (`home-remove`) — distinct labels, distinct verbs;
 *   - the correct view-native remove label + "Clear all" (Recent only);
 *   - cross-view Add-to-Favorites / Pin (Recent/Folders) vs Remove-from-Favorites (Favorites);
 *   - a stale target disables the on-disk rows but keeps "Remove from <view>" enabled.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ContextMenu from "./ContextMenu.svelte";

// The component tree imports Tauri APIs transitively; stub them for jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

type HomeView = "recent" | "favorites" | "folders";
function renderHomeMenu(opts: { homeView: HomeView; homeIsDir: boolean; homeStale?: boolean }) {
  const { component } = render(ContextMenu, {
    props: { x: 10, y: 10, target: "home-item" as const, ...opts },
  });
  const action = vi.fn();
  component.$on("action", (e) => action(e.detail));
  return { action };
}

describe("ContextMenu home-item branch — the file/folder ops (CPE-1162)", () => {
  it("Open dispatches home-open; Copy / Copy as path / Rename / Reveal / Properties / Delete route to their home-* verbs", async () => {
    const { action } = renderHomeMenu({ homeView: "recent", homeIsDir: false });

    await fireEvent.click(screen.getByText("Open"));
    expect(action).toHaveBeenCalledWith("home-open");
    await fireEvent.click(screen.getByText("Copy", { selector: "button" }));
    expect(action).toHaveBeenCalledWith("home-copy");
    await fireEvent.click(screen.getByText("Copy as path"));
    expect(action).toHaveBeenCalledWith("home-copy-path");
    await fireEvent.click(screen.getByText("Rename…"));
    expect(action).toHaveBeenCalledWith("home-rename");
    await fireEvent.click(screen.getByText("Reveal in File Explorer"));
    expect(action).toHaveBeenCalledWith("home-reveal");
    await fireEvent.click(screen.getByText("Properties"));
    expect(action).toHaveBeenCalledWith("home-properties");
    await fireEvent.click(screen.getByText("Delete"));
    expect(action).toHaveBeenCalledWith("home-delete");
  });

  it("a FILE row omits Open-in-new-tab and New ▸ (folder-only)", () => {
    renderHomeMenu({ homeView: "recent", homeIsDir: false });
    expect(screen.queryByText("Open in new tab")).toBeNull();
    expect(screen.queryByText("New")).toBeNull();
  });

  it("a FOLDER row offers Open-in-new-tab (home-open-new-tab) and New ▸", async () => {
    const { action } = renderHomeMenu({ homeView: "folders", homeIsDir: true });
    await fireEvent.click(screen.getByText("Open in new tab"));
    expect(action).toHaveBeenCalledWith("home-open-new-tab");
    expect(screen.getByText("New")).toBeTruthy();
  });
});

describe("ContextMenu home-item — Delete (file) vs Remove-from-list (pointer) are DISTINCT (CPE-1162)", () => {
  it("both are present with different labels AND different actions", async () => {
    const { action } = renderHomeMenu({ homeView: "recent", homeIsDir: false });

    // The destructive real-file trash.
    const del = screen.getByText("Delete");
    // The list-management pointer prune — a different label, a different verb.
    const remove = screen.getByText("Remove from Recent");
    expect(del).toBeTruthy();
    expect(remove).toBeTruthy();

    await fireEvent.click(remove);
    expect(action).toHaveBeenCalledWith("home-remove");
    expect(action).not.toHaveBeenCalledWith("home-delete");
  });
});

describe("ContextMenu home-item — view-native remove label + Clear all (CPE-1162)", () => {
  it("Recent → 'Remove from Recent' + 'Clear all'", () => {
    renderHomeMenu({ homeView: "recent", homeIsDir: false });
    expect(screen.getByText("Remove from Recent")).toBeTruthy();
    expect(screen.getByText("Clear all")).toBeTruthy();
  });

  it("Folders → 'Remove from Recent folders', no Clear all", () => {
    renderHomeMenu({ homeView: "folders", homeIsDir: true });
    expect(screen.getByText("Remove from Recent folders")).toBeTruthy();
    expect(screen.queryByText("Clear all")).toBeNull();
  });

  it("Favorites → 'Remove from Favorites', no Clear all", () => {
    renderHomeMenu({ homeView: "favorites", homeIsDir: true });
    expect(screen.getByText("Remove from Favorites")).toBeTruthy();
    expect(screen.queryByText("Clear all")).toBeNull();
  });
});

describe("ContextMenu home-item — cross-view actions (CPE-1162)", () => {
  it("Recent/Folders offer Add to Favorites (home-favorite); a folder also offers Pin (home-pin)", async () => {
    const { action } = renderHomeMenu({ homeView: "folders", homeIsDir: true });
    await fireEvent.click(screen.getByText("Add to Favorites"));
    expect(action).toHaveBeenCalledWith("home-favorite");
    await fireEvent.click(screen.getByText("Pin to Quick access"));
    expect(action).toHaveBeenCalledWith("home-pin");
  });

  it("a Recent FILE offers Add to Favorites but NOT Pin (files aren't pinned to Quick access)", () => {
    renderHomeMenu({ homeView: "recent", homeIsDir: false });
    expect(screen.getByText("Add to Favorites")).toBeTruthy();
    expect(screen.queryByText("Pin to Quick access")).toBeNull();
  });

  it("Favorites rows do NOT offer Add to Favorites (their remove IS the cross-view action)", () => {
    renderHomeMenu({ homeView: "favorites", homeIsDir: false });
    expect(screen.queryByText("Add to Favorites")).toBeNull();
  });
});

describe("ContextMenu home-item — stale target (CPE-1162)", () => {
  it("disables the on-disk rows but keeps Remove-from-list enabled", () => {
    renderHomeMenu({ homeView: "recent", homeIsDir: false, homeStale: true });

    // On-disk actions disabled…
    expect((screen.getByText("Open").closest("button") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByText("Delete").closest("button") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByText("Rename…").closest("button") as HTMLButtonElement).disabled).toBe(true);
    // …but the pointer prune stays live so a dead entry can be removed.
    expect((screen.getByText("Remove from Recent").closest("button") as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByText("Clear all").closest("button") as HTMLButtonElement).disabled).toBe(false);
  });

  it("hides New ▸ entirely for a stale folder (can't create inside a folder that's gone)", () => {
    renderHomeMenu({ homeView: "folders", homeIsDir: true, homeStale: true });
    expect(screen.queryByText("New")).toBeNull();
    // Remove still available.
    expect((screen.getByText("Remove from Recent folders").closest("button") as HTMLButtonElement).disabled).toBe(false);
  });
});
