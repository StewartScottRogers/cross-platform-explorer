/**
 * Component tests for the Agent Watch timeline drawer (CPE-400): it lists the session's activity
 * history and clicking an entry navigates to the change's containing folder. Also covers the
 * Replay tab / scrubber (CPE-1094).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import AgentTimeline from "./AgentTimeline.svelte";
import type { TimelineEntry } from "../agentActivity";
import { ingestDiff, clearDiffs } from "../agentDiffs";

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

describe("AgentTimeline Replay tab (CPE-1094)", () => {
  beforeEach(() => clearDiffs());
  afterEach(() => {
    clearDiffs();
    vi.useRealTimers();
  });

  it("shows a disabled/empty state when there's fewer than two entries to scrub", async () => {
    render(AgentTimeline, { entries: [entry({ id: 1 })], agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    expect(screen.getByText(/Not enough activity to replay/i)).toBeTruthy();
  });

  it("defaults to the end of the timeline and highlights the current entry", async () => {
    const entries = [
      entry({ id: 2, kind: "created", path: "Z:/repos/app/new.ts", at: 2000 }),
      entry({ id: 1, kind: "modified", path: "Z:/repos/app/src/main.rs", at: 1000 }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    // Defaults to the last entry (jump-to-end), so its filename shows as the "current" one.
    expect(screen.getAllByText("new.ts").length).toBeGreaterThan(0);
    const slider = screen.getByLabelText("Replay position") as HTMLInputElement;
    expect(slider.value).toBe("2000");
  });

  it("stepping back moves the current entry and its diff to the earlier moment", async () => {
    const entries = [
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/b.ts", at: 2000 }),
      entry({ id: 1, kind: "created", path: "Z:/repos/app/a.ts", at: 1000 }),
    ];
    ingestDiff([{ path: "Z:/repos/app/a.ts", before: "", after: "hello" }]);
    render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await fireEvent.click(screen.getByTitle("Step back"));
    const slider = screen.getByLabelText("Replay position") as HTMLInputElement;
    expect(slider.value).toBe("1000");
    expect(screen.getByText("hello")).toBeTruthy(); // a.ts's diff peek is now showing
  });

  it("badges a path edited more than once instead of silently showing the wrong-moment diff", async () => {
    const entries = [
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/x.ts", at: 2000 }),
      entry({ id: 1, kind: "created", path: "Z:/repos/app/x.ts", at: 1000 }),
    ];
    ingestDiff([{ path: "Z:/repos/app/x.ts", before: "v1", after: "v2" }]);
    render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    expect(screen.getByText(/content at this point not retained/i)).toBeTruthy();
  });

  it("does not badge a path that was only written once", async () => {
    const entries = [
      entry({ id: 2, kind: "created", path: "Z:/repos/app/y.ts", at: 2000 }),
      entry({ id: 1, kind: "modified", path: "Z:/repos/app/other.ts", at: 1000 }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    expect(screen.queryByText(/content at this point not retained/i)).toBeNull();
  });

  it("play advances through entries on a timer and stops at the end (no dangling interval)", async () => {
    vi.useFakeTimers();
    const entries = [
      entry({ id: 3, kind: "modified", path: "Z:/repos/app/c.ts", at: 3000 }),
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/b.ts", at: 2000 }),
      entry({ id: 1, kind: "created", path: "Z:/repos/app/a.ts", at: 1000 }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await fireEvent.click(screen.getByTitle("Jump to start"));
    const slider = screen.getByLabelText("Replay position") as HTMLInputElement;
    expect(slider.value).toBe("1000");

    await fireEvent.click(screen.getByTitle("Play"));
    await vi.advanceTimersByTimeAsync(400);
    expect(slider.value).toBe("2000");
    await vi.advanceTimersByTimeAsync(400);
    expect(slider.value).toBe("3000");
    // Reached the end — play should have stopped itself rather than spin forever.
    await vi.advanceTimersByTimeAsync(2000);
    expect(slider.value).toBe("3000");
    expect(screen.getByTitle("Play")).toBeTruthy(); // toggled back from "Pause"
  });

  it("clears the play interval on unmount so nothing keeps running after the drawer closes", async () => {
    vi.useFakeTimers();
    const clearSpy = vi.spyOn(global, "clearInterval");
    const entries = [
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/b.ts", at: 2000 }),
      entry({ id: 1, kind: "created", path: "Z:/repos/app/a.ts", at: 1000 }),
    ];
    const { unmount } = render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await fireEvent.click(screen.getByTitle("Jump to start"));
    await fireEvent.click(screen.getByTitle("Play"));
    const callsBeforeUnmount = clearSpy.mock.calls.length;
    unmount();
    expect(clearSpy.mock.calls.length).toBeGreaterThan(callsBeforeUnmount);
  });

  it("resets scrub position when the timeline empties (watch stops / clears)", async () => {
    const entries = [
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/b.ts", at: 2000 }),
      entry({ id: 1, kind: "created", path: "Z:/repos/app/a.ts", at: 1000 }),
    ];
    const { rerender } = render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    expect(screen.getByLabelText("Replay position")).toBeTruthy();

    await rerender({ entries: [], agentName: "Claude Code" });
    expect(screen.getByText(/Not enough activity to replay/i)).toBeTruthy();

    // A fresh session's timeline arrives — the scrubber should start clean, not carry over `t`.
    await rerender({ entries, agentName: "Claude Code" });
    const slider = screen.getByLabelText("Replay position") as HTMLInputElement;
    expect(slider.value).toBe("2000"); // back to jump-to-end default, not a stale mid-point
  });
});
