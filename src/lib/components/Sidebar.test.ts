/**
 * Component render tests for the Sidebar's Agents section (Agent Watch, CPE-397): a running
 * coding-agent session surfaces in the left pane and its row navigates the explorer to the
 * agent's Project folder. Stands in for the WebView2 GUI the headless harness can't drive.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import Sidebar from "./Sidebar.svelte";
import type { AgentSession } from "../sidecar";
import type { SavedSearch } from "../savedSearch";

// The component tree imports Tauri APIs transitively; stub for jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const session = (over: Partial<AgentSession> = {}): AgentSession => ({
  sessionId: "s1",
  agentId: "claude",
  agentName: "Claude Code",
  provider: "openrouter",
  model: "sonnet",
  cwd: "Z:/repos/cross-platform-explorer/src-tauri",
  ...over,
});

describe("Sidebar Agents section (CPE-397)", () => {
  it("shows no Agents section when nothing is running", () => {
    render(Sidebar, { places: [], drives: [], favorites: [], sessions: [] });
    expect(screen.queryByText("Agents")).toBeNull();
  });

  it("lists a running agent with its Project folder and navigates there on click", async () => {
    const { component } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      sessions: [session()],
    });
    const navigate = vi.fn();
    component.$on("navigate", (e) => navigate(e.detail));

    expect(screen.getByText("Agents")).toBeTruthy();
    expect(screen.getByText("Claude Code")).toBeTruthy();
    expect(screen.getByText("src-tauri")).toBeTruthy(); // folder basename subtitle

    await fireEvent.click(screen.getByText("Claude Code"));
    expect(navigate).toHaveBeenCalledWith("Z:/repos/cross-platform-explorer/src-tauri");
  });

  it("lists multiple sessions, keyed independently", () => {
    render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      sessions: [session(), session({ sessionId: "s2", agentName: "Aider", cwd: "/home/api" })],
    });
    expect(screen.getByText("Claude Code")).toBeTruthy();
    expect(screen.getByText("Aider")).toBeTruthy();
    expect(screen.getByText("api")).toBeTruthy();
  });

  it("shows a session-identity chip + short model on each leaf (CPE-490)", () => {
    const { container } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      sessions: [session({ sessionId: "s2", model: "anthropic/claude-sonnet-5" })],
    });
    const chip = container.querySelector(".agent-chip") as HTMLElement;
    expect(chip).toBeTruthy();
    expect(chip.textContent).toBe("2"); // number derived from the id
    expect(chip.style.background).not.toBe(""); // deterministic colour applied
    expect(screen.getByText(/claude-sonnet-5/)).toBeTruthy(); // shortened model in the label
  });

  it("right-clicking a leaf opens the menu targeting that session (CPE-489)", async () => {
    const { component, container } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      sessions: [session({ sessionId: "s2", agentName: "Aider" })],
    });
    const agentMenu = vi.fn();
    component.$on("agentMenu", (e) => agentMenu(e.detail));

    const leaf = container.querySelector(".agent-item") as HTMLElement;
    await fireEvent.contextMenu(leaf);
    expect(agentMenu).toHaveBeenCalledOnce();
    expect(agentMenu.mock.calls[0][0].sessionId).toBe("s2");
    expect(agentMenu.mock.calls[0][0].sessionLabel).toMatch(/Aider/);
  });
});

describe("Sidebar Saved Searches section (CPE-1229)", () => {
  const savedSearch = (over: Partial<SavedSearch> = {}): SavedSearch => ({
    id: "ss1",
    name: "Big PNGs",
    conditions: [{ kind: "ext", exts: ["png"] }],
    match: "all",
    root: "Z:\\repos\\project",
    ...over,
  });

  it("shows no Saved Searches section when none are saved", () => {
    render(Sidebar, { places: [], drives: [], favorites: [], savedSearches: [] });
    expect(screen.queryByText("Saved Searches")).toBeNull();
  });

  it("lists a saved search and opens it on click", async () => {
    const { component } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      savedSearches: [savedSearch()],
    });
    const opened: SavedSearch[] = [];
    component.$on("openSavedSearch", (e) => opened.push(e.detail));

    expect(screen.getByText("Saved Searches")).toBeTruthy();
    expect(screen.getByText("Big PNGs")).toBeTruthy();

    await fireEvent.click(screen.getByText("Big PNGs"));
    expect(opened).toEqual([savedSearch()]);
  });

  it("highlights the currently-open saved search", () => {
    const { container } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      savedSearches: [savedSearch(), savedSearch({ id: "ss2", name: "Old logs" })],
      activeSavedSearch: "ss2",
    });
    const items = container.querySelectorAll(".nav-children .fav-item");
    const active = Array.from(items).find((el) => el.classList.contains("active")) as HTMLElement;
    expect(active?.textContent).toContain("Old logs");
  });

  it("right-clicking a saved search opens its menu targeting that id", async () => {
    const { component, container } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      savedSearches: [savedSearch()],
    });
    const menu = vi.fn();
    component.$on("savedSearchMenu", (e) => menu(e.detail));

    const row = Array.from(container.querySelectorAll(".nav-children .fav-item")).find((el) =>
      el.textContent?.includes("Big PNGs"),
    ) as HTMLElement;
    await fireEvent.contextMenu(row);
    expect(menu).toHaveBeenCalledOnce();
    expect(menu.mock.calls[0][0]).toMatchObject({ id: "ss1", name: "Big PNGs" });
  });

  it("keeps the Saved Searches section distinct from the tag-only Smart Folders section", async () => {
    render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      savedSearches: [savedSearch()],
      smartFolders: [{ id: "sf1", name: "Screenshots", tag: "screenshot" }],
    });
    expect(screen.getByText("Smart Folders")).toBeTruthy();
    expect(screen.getByText("Saved Searches")).toBeTruthy();
    expect(screen.getByText("Screenshots")).toBeTruthy();
    expect(screen.getByText("Big PNGs")).toBeTruthy();
  });
});

describe("Sidebar drive usage bars (CPE-406)", () => {
  const drive = { name: "Local Disk (C:)", path: "C:\\", kind: "drive" };
  it("renders a usage bar + free label under a drive when usage is known", () => {
    const { container } = render(Sidebar, {
      places: [],
      drives: [drive],
      favorites: [],
      driveUsage: { "C:\\": { free: 50 * 1024 ** 3, total: 200 * 1024 ** 3 } },
    });
    const fill = container.querySelector(".drive-bar-fill") as HTMLElement;
    expect(fill).toBeTruthy();
    expect(fill.style.width).toBe("75%"); // 150/200 used
    expect(screen.getByText(/50.0 GB free/)).toBeTruthy();
  });

  it("flags a nearly-full drive as full", () => {
    const { container } = render(Sidebar, {
      places: [],
      drives: [drive],
      favorites: [],
      driveUsage: { "C:\\": { free: 2 * 1024 ** 3, total: 200 * 1024 ** 3 } },
    });
    expect(container.querySelector(".drive-bar-fill.full")).toBeTruthy(); // <5% free
  });

  it("shows no bar when usage is absent (off means off)", () => {
    const { container } = render(Sidebar, { places: [], drives: [drive], favorites: [], driveUsage: {} });
    expect(container.querySelector(".drive-bar")).toBeNull();
  });
});

describe("Sidebar Network section (CPE-1516: permanent top-level section)", () => {
  it("always renders the Network header, even with zero connections and zero shares", () => {
    render(Sidebar, { places: [], drives: [], favorites: [], connections: [], networkShares: [] });
    expect(screen.getByText("Network")).toBeTruthy();
  });

  it("shows the empty-state '+ Add a connection' control and hint when there's nothing saved", () => {
    render(Sidebar, { places: [], drives: [], favorites: [], connections: [], networkShares: [] });
    expect(screen.getByText("＋ Add a connection")).toBeTruthy();
    expect(screen.getByText(/No connections yet/)).toBeTruthy();
  });

  it("dispatches networkAdd from the empty-state control", async () => {
    const { component } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      connections: [],
      networkShares: [],
    });
    const added = vi.fn();
    component.$on("networkAdd", (e) => added(e.detail));

    await fireEvent.click(screen.getByText("＋ Add a connection"));
    expect(added).toHaveBeenCalledOnce();
  });

  it("no longer shows a 'Network…' row under Explore (removed — the section itself is now permanent)", () => {
    render(Sidebar, { places: [], drives: [], favorites: [], connections: [], networkShares: [] });
    expect(screen.queryByText("Network…")).toBeNull();
  });

  it("shows saved-connection and OS-share rows, and hides the empty-state control, once there's something to show", () => {
    render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      connections: [
        { name: "prod", scheme: "sftp", host: "host.example.com", port: 22, user: "deploy", auth: { kind: "password" } },
      ],
      networkShares: [{ name: "backups (Y:)", path: "Y:\\", kind: "mapped" }],
    });
    expect(screen.getByText("prod")).toBeTruthy();
    expect(screen.getByText("backups (Y:)")).toBeTruthy();
    expect(screen.queryByText("＋ Add a connection")).toBeNull();
    expect(screen.queryByText(/No connections yet/)).toBeNull();
  });

  it("clicking a saved connection dispatches networkConnect", async () => {
    const conn = { name: "prod", scheme: "sftp", host: "host.example.com", port: 22, user: "deploy", auth: { kind: "password" as const } };
    const { component } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      connections: [conn],
      networkShares: [],
    });
    const connected = vi.fn();
    component.$on("networkConnect", (e) => connected(e.detail));

    await fireEvent.click(screen.getByText("prod"));
    expect(connected).toHaveBeenCalledWith(conn);
  });
});

describe("Sidebar 'Discovered on your network' tier (CPE-1519)", () => {
  it("renders nothing extra when discoveredShares is empty (the common non-Windows case)", () => {
    render(Sidebar, { places: [], drives: [], favorites: [], connections: [], networkShares: [], discoveredShares: [] });
    expect(screen.getByText("＋ Add a connection")).toBeTruthy();
    expect(screen.getByText(/No connections yet/)).toBeTruthy();
  });

  it("shows a discovered row and hides the empty state, even with no connections/OS shares", () => {
    render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      connections: [],
      networkShares: [],
      discoveredShares: [{ name: "Media", path: "\\\\qnap\\media", kind: "discovered" }],
    });
    expect(screen.getByText("Media")).toBeTruthy();
    expect(screen.queryByText("＋ Add a connection")).toBeNull();
    expect(screen.queryByText(/No connections yet/)).toBeNull();
  });

  it("dedupes a discovered share against a saved connection's host (tier 1)", () => {
    render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      connections: [
        { name: "prod", scheme: "sftp", host: "qnap", port: 22, user: "deploy", auth: { kind: "password" } },
      ],
      networkShares: [],
      discoveredShares: [{ name: "Media", path: "\\\\qnap\\media", kind: "discovered" }],
    });
    expect(screen.queryByText("Media")).toBeNull();
  });

  it("dedupes a discovered share against an already-mapped OS share (tier 2)", () => {
    render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      connections: [],
      networkShares: [{ name: "\\\\qnap\\media (Z:)", path: "Z:\\", kind: "mapped" }],
      discoveredShares: [{ name: "Media", path: "\\\\qnap\\media", kind: "discovered" }],
    });
    expect(screen.queryByText("Media")).toBeNull();
    // The mapped tier-2 row for the same share is still shown.
    expect(screen.getByText("\\\\qnap\\media (Z:)")).toBeTruthy();
  });

  it("clicking a discovered row dispatches networkAdd pre-filled with scheme smb, host, and path", async () => {
    const { component } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      connections: [],
      networkShares: [],
      discoveredShares: [{ name: "Media", path: "\\\\qnap\\media", kind: "discovered" }],
    });
    const added = vi.fn();
    component.$on("networkAdd", (e) => added(e.detail));

    await fireEvent.click(screen.getByText("Media"));
    expect(added).toHaveBeenCalledOnce();
    const detail = added.mock.calls[0][0];
    expect(detail.prefill).toMatchObject({ scheme: "smb", host: "qnap", path: "/media" });
  });

  it("CPE-1524: gates the ＋Add affordance on an mDNS nfs:// row — visible, disabled, no networkAdd on click", async () => {
    const { component } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      connections: [],
      networkShares: [],
      discoveredShares: [{ name: "nas", path: "nfs://nas.local", kind: "discovered" }],
    });
    const added = vi.fn();
    component.$on("networkAdd", (e) => added(e.detail));

    const row = screen.getByText("nas").closest("button") as HTMLButtonElement;
    expect(row).toBeTruthy();
    expect(row.disabled).toBe(true);
    expect(row.title).toMatch(/NFS isn't supported yet/);

    await fireEvent.click(row);
    expect(added).not.toHaveBeenCalled();
  });

  it("a savable mDNS row (e.g. sftp://) keeps the ＋Add affordance enabled and dispatches networkAdd", async () => {
    const { component } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      connections: [],
      networkShares: [],
      discoveredShares: [{ name: "nas-sftp", path: "sftp://nas.local", kind: "discovered" }],
    });
    const added = vi.fn();
    component.$on("networkAdd", (e) => added(e.detail));

    const row = screen.getByText("nas-sftp").closest("button") as HTMLButtonElement;
    expect(row.disabled).toBe(false);

    await fireEvent.click(row);
    expect(added).toHaveBeenCalledOnce();
    const detail = added.mock.calls[0][0];
    expect(detail.prefill).toMatchObject({ scheme: "sftp", host: "nas.local" });
  });
});

describe("Sidebar Trash section (CPE-1560, epic CPE-1486)", () => {
  it("always renders the Trash header, regardless of platform gate", () => {
    render(Sidebar, { places: [], drives: [], favorites: [] });
    expect(screen.getByText("Trash")).toBeTruthy();
  });

  it("shows a clickable Open Trash row when canBrowseTrash is true (Windows/Linux, the default)", () => {
    render(Sidebar, { places: [], drives: [], favorites: [], canBrowseTrash: true });
    const row = screen.getByText("Open Trash").closest("button") as HTMLButtonElement;
    expect(row).toBeTruthy();
    expect(row.disabled).toBe(false);
  });

  it("dispatches openTrash when the Open Trash row is clicked", async () => {
    const { component } = render(Sidebar, { places: [], drives: [], favorites: [], canBrowseTrash: true });
    const opened = vi.fn();
    component.$on("openTrash", opened);

    await fireEvent.click(screen.getByText("Open Trash"));
    expect(opened).toHaveBeenCalledOnce();
  });

  it("shows an inert Finder message instead of Open Trash when canBrowseTrash is false (macOS)", async () => {
    const { component } = render(Sidebar, { places: [], drives: [], favorites: [], canBrowseTrash: false });
    expect(screen.queryByText("Open Trash")).toBeNull();
    const row = screen.getByText("Open Finder's Trash instead").closest("button") as HTMLButtonElement;
    expect(row).toBeTruthy();
    expect(row.disabled).toBe(true);

    const opened = vi.fn();
    component.$on("openTrash", opened);
    await fireEvent.click(row);
    expect(opened).not.toHaveBeenCalled();
  });

  it("collapses and expands independently of other sections (generic sidebarSections store)", async () => {
    const { container } = render(Sidebar, { places: [], drives: [], favorites: [], canBrowseTrash: true });
    expect(screen.getByText("Open Trash")).toBeTruthy();

    const twisty = container.querySelector('[data-section-id="trash"] .twisty') as HTMLButtonElement;
    await fireEvent.click(twisty);
    expect(screen.queryByText("Open Trash")).toBeNull();

    await fireEvent.click(twisty);
    expect(screen.getByText("Open Trash")).toBeTruthy();
  });
});

describe("Sidebar density (CPE-1528)", () => {
  it("does not apply the compact class when density is comfortable (default)", () => {
    const { container } = render(Sidebar, { places: [], drives: [], favorites: [] });
    expect(container.querySelector(".navigation-pane")?.classList.contains("compact")).toBe(false);
  });

  it("applies the compact class to the root .navigation-pane when density is compact", () => {
    const { container } = render(Sidebar, {
      places: [],
      drives: [],
      favorites: [],
      density: "compact",
    });
    expect(container.querySelector(".navigation-pane")?.classList.contains("compact")).toBe(true);
  });
});
