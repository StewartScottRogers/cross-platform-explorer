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
});
