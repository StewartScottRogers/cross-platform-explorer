/**
 * Component render tests for the Sidebar's Agents section (Agent Watch, CPE-397): a running
 * coding-agent session surfaces in the left pane and its row navigates the explorer to the
 * agent's Project folder. Stands in for the WebView2 GUI the headless harness can't drive.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import Sidebar from "./Sidebar.svelte";
import type { AgentSession } from "../sidecar";

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
