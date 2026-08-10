/**
 * Component render tests for HomeView — the Favorites tab (CPE-338) and the
 * per-entry Recent remove control (CPE-341). These stand in for the GUI drive
 * that the native WebView2 window doesn't allow from the Nightshift harness:
 * they assert the favorites list actually renders and that the row controls
 * dispatch the right events with the right paths.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import HomeView from "./HomeView.svelte";
import type { Favorite, RecentFile, NetShare } from "../types";

// The component tree imports Tauri APIs transitively; stub them for jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

// HomeView persists its layout (CPE-573); isolate localStorage so tests don't inherit each other's state.
beforeEach(() => { try { localStorage.clear(); } catch { /* ignore */ } });

const favorites: Favorite[] = [
  { path: "/home/docs", name: "docs", is_dir: true },
  { path: "/home/notes.txt", name: "notes.txt", is_dir: false },
];
const recents: RecentFile[] = [
  { path: "/home/a.md", name: "a.md", opened: 2 },
  { path: "/home/b.md", name: "b.md", opened: 1 },
];

describe("HomeView layout persistence (CPE-573)", () => {
  it("restores the saved section tab on open", async () => {
    try { localStorage.clear(); } catch { /* ignore */ }
    localStorage.setItem("cpe.homeTab", "favorites");
    render(HomeView, { places: [], drives: [], pins: [], recents: [], favorites });
    // The Favorites tab is restored, so its items show without clicking a pill.
    expect(screen.getByText("docs")).toBeTruthy();
    expect(screen.getByText("notes.txt")).toBeTruthy();
  });
});

describe("HomeView Quick Access drive tile (CPE-1483)", () => {
  // Pins the CPE-1483 root-cause finding: given the exact props App.svelte/ExplorerPane.svelte feed
  // HomeView (`places` from `specialFolders`, `drives` from `listDrives`), the POSIX single-root drive
  // DOES render as a `.qa-card` with `.qa-sub` === "/" — the categorization/keyed-`{#each}` logic itself
  // is correct. The Linux gui-smoke gap this ticket investigated is therefore an environment-specific
  // (headless WebKitGTK-under-Xvfb CI) issue, not a code bug reproducible here — see the ticket's Notes.
  it("renders the POSIX root ('/') drive as a .qa-card, expanded by default", () => {
    const { container } = render(HomeView, {
      places: [],
      drives: [{ name: "File System", path: "/", kind: "drive" }],
      pins: [],
      recents: [],
      favorites: [],
    });
    const cards = Array.from(container.querySelectorAll(".qa-card"));
    expect(cards).toHaveLength(1);
    expect(cards[0].querySelector(".qa-sub")?.textContent).toBe("/");
  });

  it("renders a Windows drive tile alongside quick-access places without a key collision", () => {
    const { container } = render(HomeView, {
      places: [{ name: "Documents", path: "C:\\Users\\me\\Documents", kind: "documents" }],
      drives: [{ name: "Local Disk (C:)", path: "C:\\", kind: "drive" }],
      pins: [],
      recents: [],
      favorites: [],
    });
    const subs = Array.from(container.querySelectorAll(".qa-card .qa-sub")).map((el) => el.textContent);
    expect(subs).toEqual(["C:\\Users\\me\\Documents", "C:\\"]);
  });
});

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

describe("HomeView row right-click dispatches homeItemContext (CPE-1162)", () => {
  const recentFolders: RecentFile[] = [{ path: "/home/projects", name: "projects", opened: 2 }];

  it("a Recent row dispatches {path,is_dir:false,view:'recent'}", async () => {
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents, favorites: [] });
    const ctx = vi.fn();
    component.$on("homeItemContext", (e) => ctx(e.detail));

    const row = screen.getByText("a.md").closest("button")!;
    await fireEvent.contextMenu(row);

    expect(ctx).toHaveBeenCalledTimes(1);
    expect(ctx.mock.calls[0][0]).toMatchObject({ path: "/home/a.md", is_dir: false, view: "recent" });
  });

  it("a Favorites row carries the entry's own is_dir + view:'favorites'", async () => {
    localStorage.setItem("cpe.homeTab", "favorites");
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents: [], favorites });
    const ctx = vi.fn();
    component.$on("homeItemContext", (e) => ctx(e.detail));

    await fireEvent.contextMenu(screen.getByText("docs").closest("button")!);
    expect(ctx.mock.calls[0][0]).toMatchObject({ path: "/home/docs", is_dir: true, view: "favorites" });

    await fireEvent.contextMenu(screen.getByText("notes.txt").closest("button")!);
    expect(ctx.mock.calls[1][0]).toMatchObject({ path: "/home/notes.txt", is_dir: false, view: "favorites" });
  });

  it("a Folders row dispatches {is_dir:true,view:'folders'}", async () => {
    const { component } = render(HomeView, {
      places: [], drives: [], pins: [], recents: [], favorites: [], recentFolders,
    });
    const ctx = vi.fn();
    component.$on("homeItemContext", (e) => ctx(e.detail));

    await fireEvent.click(screen.getByRole("button", { name: /Folders/i }));
    await fireEvent.contextMenu(screen.getByText("projects").closest("button")!);
    expect(ctx.mock.calls[0][0]).toMatchObject({ path: "/home/projects", is_dir: true, view: "folders" });
  });

  it("the blank Home background dispatches NO homeItemContext", async () => {
    const { container, component } = render(HomeView, { places: [], drives: [], pins: [], recents, favorites: [] });
    const ctx = vi.fn();
    component.$on("homeItemContext", (e) => ctx(e.detail));

    await fireEvent.contextMenu(container.querySelector(".home")!);
    expect(ctx).not.toHaveBeenCalled();
  });
});

describe("HomeView Shared tab — network shares (CPE-1163)", () => {
  const shared: NetShare[] = [
    { name: "\\\\server\\share (Z:)", path: "Z:\\", kind: "mapped" },
    { name: "projects", path: "\\\\nas\\projects", kind: "user" },
  ];

  it("requests a load when the Shared tab is opened, and renders the rows", async () => {
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents: [], favorites: [], shared });
    const loadShared = vi.fn();
    component.$on("loadShared", () => loadShared());

    await fireEvent.click(screen.getByRole("button", { name: /Shared/i }));
    expect(loadShared).toHaveBeenCalled();
    expect(screen.getByText(/\\\\server\\share \(Z:\)/)).toBeTruthy();
    expect(screen.getByText("projects")).toBeTruthy();
  });

  it("restores the Shared tab on open and shows an empty state when there are none", async () => {
    // (The mount-time `loadShared` dispatch fires before a post-render `$on` can attach, so it's not
    // asserted here; the tab-open dispatch is covered above. This verifies the restored-tab render.)
    localStorage.setItem("cpe.homeTab", "shared");
    render(HomeView, { places: [], drives: [], pins: [], recents: [], favorites: [], shared: [] });
    expect(screen.getByText(/No network locations/i)).toBeTruthy();
  });

  it("a Shared row right-click dispatches {view:'shared', kind}", async () => {
    localStorage.setItem("cpe.homeTab", "shared");
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents: [], favorites: [], shared });
    const ctx = vi.fn();
    component.$on("homeItemContext", (e) => ctx(e.detail));

    await fireEvent.contextMenu(screen.getByText("projects").closest("button")!);
    expect(ctx.mock.calls[0][0]).toMatchObject({ path: "\\\\nas\\projects", is_dir: true, view: "shared", kind: "user" });
  });

  it("the ✕ on a user-added row dispatches removeNetworkLocation with that path", async () => {
    localStorage.setItem("cpe.homeTab", "shared");
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents: [], favorites: [], shared });
    const remove = vi.fn();
    component.$on("removeNetworkLocation", (e) => remove(e.detail));

    // Only the user-added row carries a remove ✕ (mapped drives use Disconnect from the menu).
    const buttons = screen.getAllByRole("button", { name: /Remove network location/i });
    expect(buttons).toHaveLength(1);
    await fireEvent.click(buttons[0]);
    expect(remove).toHaveBeenCalledWith("\\\\nas\\projects");
  });

  it("＋ Add network location reveals an input that dispatches addNetworkLocation on submit", async () => {
    localStorage.setItem("cpe.homeTab", "shared");
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents: [], favorites: [], shared: [] });
    const add = vi.fn();
    component.$on("addNetworkLocation", (e) => add(e.detail));

    await fireEvent.click(screen.getByRole("button", { name: /Add network location/i }));
    const input = screen.getByPlaceholderText(/server|smb:/i) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "\\\\host\\pub" } });
    await fireEvent.submit(input.closest("form")!);
    expect(add).toHaveBeenCalledWith("\\\\host\\pub");
  });
});

describe("HomeView Recent single-click selects for preview, double-click opens (CPE-1132)", () => {
  it("a single click dispatches select with the row's path and does NOT open it", async () => {
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents, favorites: [] });
    const select = vi.fn();
    const openFile = vi.fn();
    component.$on("select", (e) => select(e.detail));
    component.$on("openFile", (e) => openFile(e.detail));

    await fireEvent.click(screen.getByText("a.md"));

    expect(select).toHaveBeenCalledWith("/home/a.md");
    expect(openFile).not.toHaveBeenCalled();
  });

  it("a double click still dispatches openFile with the row's path", async () => {
    const { component } = render(HomeView, { places: [], drives: [], pins: [], recents, favorites: [] });
    const openFile = vi.fn();
    component.$on("openFile", (e) => openFile(e.detail));

    await fireEvent.dblClick(screen.getByText("b.md"));

    expect(openFile).toHaveBeenCalledWith("/home/b.md");
  });
});
