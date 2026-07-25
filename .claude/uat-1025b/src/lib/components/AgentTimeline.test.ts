/**
 * Component tests for the Agent Watch timeline drawer (CPE-400): it lists the session's activity
 * history and clicking an entry navigates to the change's containing folder.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import AgentTimeline from "./AgentTimeline.svelte";
import type { TimelineEntry } from "../agentActivity";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const entry = (over: Partial<TimelineEntry> = {}): TimelineEntry => ({
  id: 1,
  kind: "modified",
  path: "Z:/repos/app/src/main.rs",
  at: new Date(2026, 6, 14, 22, 0, 0).getTime(),
  ...over,
});

describe("AgentTimeline (CPE-400)", () => {
  it("shows an empty state when there is no activity", () => {
    render(AgentTimeline, { entries: [], agentName: "Claude Code" });
    expect(screen.getByText(/No activity yet/i)).toBeTruthy();
  });

  it("lists entries newest-first with kind + filename, and navigates to the folder on click", async () => {
    const entries = [
      entry({ id: 2, kind: "created", path: "Z:/repos/app/new.ts" }),
      entry({ id: 1, kind: "modified", path: "Z:/repos/app/src/main.rs" }),
    ];
    const { component } = render(AgentTimeline, { entries, agentName: "Claude Code" });
    const navigate = vi.fn();
    component.$on("navigate", (e) => navigate(e.detail));

    expect(screen.getByText("new.ts")).toBeTruthy();
    expect(screen.getByText("main.rs")).toBeTruthy();
    expect(screen.getByText("new")).toBeTruthy(); // created badge label

    await fireEvent.click(screen.getByText("main.rs"));
    expect(navigate).toHaveBeenCalledWith("Z:/repos/app/src"); // containing folder
  });

  it("dispatches close from the header button", async () => {
    const { component } = render(AgentTimeline, { entries: [], agentName: "A" });
    const close = vi.fn();
    component.$on("close", close);
    await fireEvent.click(screen.getByTitle("Close"));
    expect(close).toHaveBeenCalled();
  });
});
