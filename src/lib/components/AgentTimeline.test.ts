/**
 * Component tests for the Agent Watch timeline drawer (CPE-400): it lists the session's activity
 * history and clicking an entry navigates to the change's containing folder. Also covers the
 * Replay tab / scrubber (CPE-1094).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import AgentTimeline from "./AgentTimeline.svelte";
import type { TimelineEntry } from "../agentActivity";
import type { AgentSession } from "../sidecar";
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

  it("defaults to 1x speed and plays at the base ~400ms cadence", async () => {
    vi.useFakeTimers();
    const entries = [
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/b.ts", at: 2000 }),
      entry({ id: 1, kind: "created", path: "Z:/repos/app/a.ts", at: 1000 }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    expect(screen.getByRole("button", { name: "1×" }).classList.contains("active")).toBe(true);

    await fireEvent.click(screen.getByTitle("Jump to start"));
    await fireEvent.click(screen.getByTitle("Play"));
    const slider = screen.getByLabelText("Replay position") as HTMLInputElement;
    await vi.advanceTimersByTimeAsync(399);
    expect(slider.value).toBe("1000"); // not yet — cadence hasn't elapsed
    await vi.advanceTimersByTimeAsync(1);
    expect(slider.value).toBe("2000");
  });

  it("selecting 4x speed while playing restarts the interval at a quarter of the base cadence, without leaking a timer", async () => {
    vi.useFakeTimers();
    const clearSpy = vi.spyOn(global, "clearInterval");
    const entries = [
      entry({ id: 3, kind: "modified", path: "Z:/repos/app/c.ts", at: 3000 }),
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/b.ts", at: 2000 }),
      entry({ id: 1, kind: "created", path: "Z:/repos/app/a.ts", at: 1000 }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await fireEvent.click(screen.getByTitle("Jump to start"));
    await fireEvent.click(screen.getByTitle("Play"));

    const callsBeforeSpeedChange = clearSpy.mock.calls.length;
    await fireEvent.click(screen.getByRole("button", { name: "4×" }));
    // Switching speed mid-play clears the old interval before starting the new one (no leak).
    expect(clearSpy.mock.calls.length).toBe(callsBeforeSpeedChange + 1);
    expect(screen.getByRole("button", { name: "4×" }).classList.contains("active")).toBe(true);

    const slider = screen.getByLabelText("Replay position") as HTMLInputElement;
    // 400ms / 4 = 100ms per step at 4x.
    await vi.advanceTimersByTimeAsync(100);
    expect(slider.value).toBe("2000");
    await vi.advanceTimersByTimeAsync(100);
    expect(slider.value).toBe("3000");
    // Reached the end — still self-stops, same as at 1x.
    await vi.advanceTimersByTimeAsync(1000);
    expect(slider.value).toBe("3000");
    expect(screen.getByTitle("Play")).toBeTruthy();
  });

  it("choosing a speed while paused does not start playback, only remembers the choice", async () => {
    vi.useFakeTimers();
    const entries = [
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/b.ts", at: 2000 }),
      entry({ id: 1, kind: "created", path: "Z:/repos/app/a.ts", at: 1000 }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await fireEvent.click(screen.getByTitle("Jump to start"));

    await fireEvent.click(screen.getByRole("button", { name: "2×" }));
    expect(screen.getByRole("button", { name: "2×" }).classList.contains("active")).toBe(true);
    await vi.advanceTimersByTimeAsync(1000);
    const slider = screen.getByLabelText("Replay position") as HTMLInputElement;
    expect(slider.value).toBe("1000"); // still at start — no interval running while paused
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

describe("AgentTimeline Radar tab (CPE-1100)", () => {
  const sessions: AgentSession[] = [
    {
      sessionId: "sess-1",
      agentId: "claude-code",
      agentName: "Claude Code",
      provider: "anthropic",
      model: "claude",
      cwd: "/repo",
    },
  ];

  it("shows a clean empty state when nothing overlaps", async () => {
    render(AgentTimeline, { entries: [entry({ id: 1 })], agentName: "Claude Code" });
    await fireEvent.click(screen.getByRole("tab", { name: "Radar" }));
    expect(screen.getByText(/No overlapping activity/i)).toBeTruthy();
  });

  it("lists an overlap with its path, friendly actor pills, and navigates on click", async () => {
    const entries = [
      entry({ id: 1, kind: "modified", path: "Z:/repos/app/shared.ts", at: 1_000, actor: "sess-1" }),
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/shared.ts", at: 1_500, actor: "user" }),
    ];
    const { component } = render(AgentTimeline, { entries, agentName: "Claude Code", sessions });
    const navigate = vi.fn();
    component.$on("navigate", (e) => navigate(e.detail));
    await fireEvent.click(screen.getByRole("tab", { name: "Radar" }));

    expect(screen.getByText("shared.ts")).toBeTruthy();
    expect(screen.getByText("Claude Code")).toBeTruthy(); // sess-1 resolved via `sessions`
    expect(screen.getByText("You")).toBeTruthy(); // "user" -> "You"

    await fireEvent.click(screen.getByText("shared.ts"));
    expect(navigate).toHaveBeenCalledWith("Z:/repos/app"); // containing folder
  });

  it("shows the unresolved-actor hedge note only when an overlap includes 'unknown'", async () => {
    const entries = [
      entry({ id: 1, kind: "modified", path: "Z:/repos/app/a.ts", at: 1_000, actor: "sess-1" }),
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/a.ts", at: 1_500, actor: "unknown" }),
      entry({ id: 3, kind: "modified", path: "Z:/repos/app/b.ts", at: 2_000, actor: "sess-1" }),
      entry({ id: 4, kind: "modified", path: "Z:/repos/app/b.ts", at: 2_500, actor: "user" }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code", sessions });
    await fireEvent.click(screen.getByRole("tab", { name: "Radar" }));
    expect(screen.getAllByText(/unresolved actor/i)).toHaveLength(1); // only a.ts's overlap qualifies
  });

  it("never shows an overlap for a single actor touching a path repeatedly", async () => {
    const entries = [
      entry({ id: 1, kind: "modified", path: "Z:/repos/app/solo.ts", at: 1_000, actor: "sess-1" }),
      entry({ id: 2, kind: "modified", path: "Z:/repos/app/solo.ts", at: 1_500, actor: "sess-1" }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code", sessions });
    await fireEvent.click(screen.getByRole("tab", { name: "Radar" }));
    expect(screen.getByText(/No overlapping activity/i)).toBeTruthy();
  });
});
