/**
 * Component render tests for HomeView — the Favorites tab (CPE-338) and the
 * per-entry Recent remove control (CPE-341). These stand in for the GUI drive
 * that the native WebView2 window doesn't allow from the Nightshift harness:
 * they assert the favorites list actually renders and that the row controls
 * dispatch the right events with the right paths.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import HomeView from "./HomeView.svelte";
import type { Favorite, RecentFile } from "../types";

// The component tree imports Tauri APIs transitively; stub them for jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const favorites: Favorite[] = [
  { path: "/home/docs", name: "docs", is_dir: true },
  { path: "/home/notes.txt", name: "notes.txt", is_dir: false },
];
const recents: RecentFile[] = [
  { path: "/home/a.md", name: "a.md", opened: 2 },
  { path: "/home/b.md", name: "b.md", opened: 1 },
];

describe("HomeView Favorites tab (CPE-338)", () => {
  it("lists starred items and routes folder→navigate, file→openFile", async () => {
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents: [], favorites });
    const navigate = vi.fn();
    const openFile = vi.fn();
    component.$on("navigate", (e) => navigate(e.detail));
    component.$on("openFile", (e) => openFile(e.detail));

    await fireEvent.click(screen.getByRole("button", { name: /Favorites/i }));

    expect(screen.getByText("docs")).toBeTruthy();
    expect(screen.getByText("notes.txt")).toBeTruthy();

    await fireEvent.click(screen.getByText("docs"));
    expect(navigate).toHaveBeenCalledWith("/home/docs");

    await fireEvent.click(screen.getByText("notes.txt"));
    expect(openFile).toHaveBeenCalledWith("/home/notes.txt");
  });

  it("shows an empty state when there are no favorites", async () => {
    render(HomeView, { places: [], drives: [], pins: [], recents: [], favorites: [] });
    await fireEvent.click(screen.getByRole("button", { name: /Favorites/i }));
    expect(screen.getByText(/No favorites yet/i)).toBeTruthy();
  });
});

describe("HomeView Recent folders tab (CPE-342)", () => {
  const recentFolders: RecentFile[] = [
    { path: "/home/projects", name: "projects", opened: 2 },
    { path: "/home/music", name: "music", opened: 1 },
  ];

  it("lists visited folders and navigates on click", async () => {
    const { component } = render(HomeView, {
      places: [], drives: [], pins: [], recents: [], favorites: [], recentFolders,
    });
    const navigate = vi.fn();
    component.$on("navigate", (e) => navigate(e.detail));

    await fireEvent.click(screen.getByRole("button", { name: /Folders/i }));
    expect(screen.getByText("projects")).toBeTruthy();
    await fireEvent.click(screen.getByText("projects"));
    expect(navigate).toHaveBeenCalledWith("/home/projects");
  });

  it("removes one folder from the MRU via its ✕", async () => {
    const { component } = render(HomeView, {
      places: [], drives: [], pins: [], recents: [], favorites: [], recentFolders,
    });
    const removeRecentFolder = vi.fn();
    component.$on("removeRecentFolder", (e) => removeRecentFolder(e.detail));

    await fireEvent.click(screen.getByRole("button", { name: /Folders/i }));
    const buttons = screen.getAllByRole("button", { name: /Remove from Recent folders/i });
    await fireEvent.click(buttons[0]);
    expect(removeRecentFolder).toHaveBeenCalledWith("/home/projects");
  });
});

describe("HomeView Recent remove (CPE-341)", () => {
  it("dispatches removeRecent with just that row's path", async () => {
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents, favorites: [] });
    const removeRecent = vi.fn();
    component.$on("removeRecent", (e) => removeRecent(e.detail));

    const buttons = screen.getAllByRole("button", { name: /Remove from Recent/i });
    expect(buttons).toHaveLength(recents.length);
    await fireEvent.click(buttons[0]);
    expect(removeRecent).toHaveBeenCalledWith("/home/a.md");
  });
});
