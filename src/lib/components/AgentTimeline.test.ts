/**
 * Component tests for the Agent Watch timeline drawer (CPE-400): it lists the session's activity
 * history and clicking an entry navigates to the change's containing folder. Also covers the
 * Replay tab / scrubber (CPE-1094).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import AgentTimeline from "./AgentTimeline.svelte";
import type { TimelineEntry } from "../agentActivity";
import type { AgentSession } from "../sidecar";
import { ingestDiff, clearDiffs } from "../agentDiffs";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
// Command-router `invoke` mock (RepoBrowser.test.ts's pattern) — `commands.replayLoad` (bindings.gen)
// routes through this same `invoke` (via `src/lib/invoke.ts`), so mocking it here drives the typed call.
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

/** Drain the `replay_load` await chain (mock → `withBusy` → the component's own `await`) plus a Svelte
 *  `tick()`, purely via microtasks — no `setTimeout`-based polling (`findBy*`/`waitFor`), which this file
 *  has seen flake against real-timer restoration after its earlier `vi.useFakeTimers()` tests. */
async function flushReplayLoad(): Promise<void> {
  for (let i = 0; i < 8; i++) await Promise.resolve();
  await tick();
}

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

describe("AgentTimeline reconstructed folder listing (CPE-1111, epic CPE-728 slice d)", () => {
  // Transport ticks (`t`) are driven off the `entries` prop's timestamp range, independent of the
  // durable `replay_load` events below — mirrors production, where the in-memory capped timeline and
  // the durable journal can differ.
  const transportEntries = [
    entry({ id: 1, kind: "created", path: "Z:/repos/app/root/file.txt", at: 100 }),
    entry({ id: 2, kind: "created", path: "Z:/repos/app/root/sub", at: 300 }),
  ];

  const ev = (ts: number, kind: string, path: string) => ({ ts, session: "sess-1", kind, path, actor: null, detail: null });
  const emptySummary = { total: 0, byKind: {}, sessions: [], firstAt: null, lastAt: null };

  beforeEach(() => invokeMock.mockReset());
  afterEach(() => vi.useRealTimers());

  it("loads replay data once on Replay-tab enter and renders the reconstructed listing at the scrub time", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd !== "replay_load") return undefined;
      return {
        replay: {
          events: [
            ev(100, "created", "Z:/repos/app/root/file.txt"),
            ev(200, "created", "Z:/repos/app/root/sub"),
            ev(300, "created", "Z:/repos/app/root/sub/leaf.txt"),
          ],
          bounds: [100, 300],
          summary: emptySummary,
        },
        baseline: null,
      };
    });

    render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();

    // Defaults to the end of the range (t=300): file.txt + sub (a direct child of root) show, but the
    // grandchild leaf.txt does not (children_at excludes anything not a DIRECT child of currentPath).
    // Both names also appear in the frozen event list / current-entry box below, so assert presence via
    // getAllByText rather than a single-match query.
    expect(screen.getAllByText("file.txt").length).toBeGreaterThan(0);
    expect(screen.getAllByText("sub").length).toBeGreaterThan(0);
    expect(screen.queryByText("leaf.txt")).toBeNull();
    expect(screen.getByText(/Reconstruction at scrub time \(read-only\)/i)).toBeTruthy();

    // CPE-1126: the Replay tab now ALSO lists checkpoints for `currentPath` on enter (checkpoint_list),
    // so assert replay_load fired exactly once specifically rather than a bare total-invoke count.
    const replayLoadCalls = invokeMock.mock.calls.filter(([cmd]: unknown[]) => cmd === "replay_load");
    expect(replayLoadCalls.length).toBe(1);
    expect(invokeMock).toHaveBeenCalledWith("replay_load", { session: "sess-1" });
  });

  it("re-derives the listing per scrub tick with no additional replay_load call", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd !== "replay_load") return undefined;
      return {
        replay: {
          events: [
            ev(100, "created", "Z:/repos/app/root/file.txt"),
            ev(300, "created", "Z:/repos/app/root/sub"),
          ],
          bounds: [100, 300],
          summary: emptySummary,
        },
        baseline: null,
      };
    });

    render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();
    expect(screen.getAllByText("file.txt").length).toBeGreaterThan(0);
    expect(screen.getAllByText("sub").length).toBeGreaterThan(0); // present at t=300

    invokeMock.mockClear();
    await fireEvent.click(screen.getByTitle("Step back")); // t: 300 -> 100 (only two ticks in transportEntries)
    await flushReplayLoad();

    expect(screen.queryByText("sub")).toBeNull(); // not created until ts=300
    expect(screen.getAllByText("file.txt").length).toBeGreaterThan(0); // created at ts=100, still present
    expect(invokeMock).not.toHaveBeenCalled(); // scrubbing never re-calls replay_load
  });

  it("falls back to the classic event list (never breaks the tab) when replay_load errors", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "replay_load") throw new Error("boom");
      return undefined;
    });

    render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();

    expect(invokeMock).toHaveBeenCalledWith("replay_load", { session: "sess-1" });
    // No reconstruction section, but the tab itself still renders fine (frozen event list + transport).
    expect(screen.queryByText(/Reconstruction at scrub time/i)).toBeNull();
    expect(screen.getByLabelText("Replay position")).toBeTruthy();
    expect(screen.getAllByText("sub").length).toBeGreaterThan(0); // frozen list entry, from `entries` prop
  });

  it("a session change (generation token) supersedes a slow in-flight load", async () => {
    let resolveSess1: (v: unknown) => void = () => {};
    let resolveSess2: (v: unknown) => void = () => {};
    const sess1Promise = new Promise((r) => { resolveSess1 = r; });
    const sess2Promise = new Promise((r) => { resolveSess2 = r; });
    invokeMock.mockImplementation(async (cmd: string, args: { session: string }) => {
      if (cmd !== "replay_load") return undefined;
      return args.session === "sess-1" ? sess1Promise : sess2Promise;
    });

    const { rerender } = render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();
    expect(invokeMock).toHaveBeenCalledWith("replay_load", { session: "sess-1" });

    // A different session is now watched before sess-1's slow load resolves.
    await rerender({ sessionId: "sess-2" });
    await flushReplayLoad();
    expect(invokeMock).toHaveBeenCalledWith("replay_load", { session: "sess-2" });

    resolveSess1({
      replay: { events: [ev(300, "created", "Z:/repos/app/root/from-sess1.txt")], bounds: [300, 300], summary: emptySummary },
      baseline: null,
    });
    await flushReplayLoad();
    expect(screen.queryByText("from-sess1.txt")).toBeNull(); // superseded — must never leak in

    resolveSess2({
      replay: { events: [ev(300, "created", "Z:/repos/app/root/from-sess2.txt")], bounds: [300, 300], summary: emptySummary },
      baseline: null,
    });
    await flushReplayLoad();
    expect(screen.getByText("from-sess2.txt")).toBeTruthy();
  });
});

describe("AgentTimeline Replay-mode file-pane overlay (CPE-1112, epic CPE-728 slice e)", () => {
  // Same fixtures as the in-drawer reconstruction suite above — this feature reuses that exact data,
  // just also mirrors it out via the `replayOverlay` event instead of only rendering it in the drawer.
  const transportEntries = [
    entry({ id: 1, kind: "created", path: "Z:/repos/app/root/file.txt", at: 100 }),
    entry({ id: 2, kind: "created", path: "Z:/repos/app/root/sub", at: 300 }),
  ];
  const ev = (ts: number, kind: string, path: string) => ({ ts, session: "sess-1", kind, path, actor: null, detail: null });
  const emptySummary = { total: 0, byKind: {}, sessions: [], firstAt: null, lastAt: null };

  const mockReplayLoad = () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd !== "replay_load") return undefined;
      return {
        replay: {
          events: [ev(100, "created", "Z:/repos/app/root/file.txt"), ev(300, "created", "Z:/repos/app/root/sub")],
          bounds: [100, 300],
          summary: emptySummary,
        },
        baseline: null,
      };
    });
  };

  beforeEach(() => invokeMock.mockReset());
  afterEach(() => vi.useRealTimers());

  it("off by default: opening Replay with a loaded reconstruction never fires a non-null overlay", async () => {
    mockReplayLoad();
    const { component } = render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    const overlay = vi.fn();
    component.$on("replayOverlay", (e) => overlay(e.detail));

    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();

    expect(screen.getByLabelText("Show in file pane")).toBeTruthy();
    // Every dispatch so far (mount + every reactive recompute) has been null — the toggle is off.
    expect(overlay.mock.calls.every((call) => call[0] === null)).toBe(true);
    expect(overlay).toHaveBeenCalled(); // sanity: the handler IS wired and firing
  });

  it("checking the toggle dispatches the same reconstruction the drawer shows, DirEntry-shaped", async () => {
    mockReplayLoad();
    const { component } = render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    const overlay = vi.fn();
    component.$on("replayOverlay", (e) => overlay(e.detail));

    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();
    overlay.mockClear();

    await fireEvent.click(screen.getByLabelText("Show in file pane"));

    const last = overlay.mock.calls.at(-1)?.[0];
    expect(last).not.toBeNull();
    const names = (last as { name: string }[]).map((e) => e.name).sort();
    expect(names).toEqual(["file.txt", "sub"]); // t defaults to the end (300): both direct children present
    const sub = (last as { name: string; is_dir: boolean; path: string }[]).find((e) => e.name === "sub");
    expect(sub?.is_dir).toBe(false); // no grandchild under it in this fixture — not distinguishable as a dir
    expect(sub?.path).toBe("Z:/repos/app/root/sub");
  });

  it("unchecking the toggle restores the live pane (dispatches null again)", async () => {
    mockReplayLoad();
    const { component } = render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    const overlay = vi.fn();
    component.$on("replayOverlay", (e) => overlay(e.detail));

    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();
    const toggle = screen.getByLabelText("Show in file pane") as HTMLInputElement;
    await fireEvent.click(toggle);
    expect(overlay.mock.calls.at(-1)?.[0]).not.toBeNull();

    await fireEvent.click(toggle); // uncheck
    expect(overlay.mock.calls.at(-1)?.[0]).toBeNull();
  });

  it("leaving the Replay tab restores the live pane even without touching the toggle", async () => {
    mockReplayLoad();
    const { component } = render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    const overlay = vi.fn();
    component.$on("replayOverlay", (e) => overlay(e.detail));

    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();
    await fireEvent.click(screen.getByLabelText("Show in file pane"));
    expect(overlay.mock.calls.at(-1)?.[0]).not.toBeNull();

    await fireEvent.click(screen.getByRole("tab", { name: "Live" }));
    expect(overlay.mock.calls.at(-1)?.[0]).toBeNull();
  });

  it("unmounting the drawer mid-overlay restores the live pane (no dangling reconstruction)", async () => {
    mockReplayLoad();
    const { component, unmount } = render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    const overlay = vi.fn();
    component.$on("replayOverlay", (e) => overlay(e.detail));

    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();
    await fireEvent.click(screen.getByLabelText("Show in file pane"));
    expect(overlay.mock.calls.at(-1)?.[0]).not.toBeNull();

    unmount();
    expect(overlay.mock.calls.at(-1)?.[0]).toBeNull(); // the drawer's own teardown sends the restore
  });

  it("never mutates the `entries` prop it was given while the overlay is toggled on and off", async () => {
    mockReplayLoad();
    const snapshot = transportEntries.map((e) => ({ ...e }));
    const { component } = render(AgentTimeline, {
      entries: transportEntries,
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "Z:/repos/app/root",
    });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();
    const toggle = screen.getByLabelText("Show in file pane");
    await fireEvent.click(toggle);
    await fireEvent.click(screen.getByTitle("Step back"));
    await fireEvent.click(toggle);

    expect(transportEntries).toEqual(snapshot); // the source timeline is exactly as it was handed in
    void component;
  });
});

describe("AgentTimeline checkpoint restore panel — two-step revert confirm gate (CPE-1150, epic CPE-732)", () => {
  // SAFETY-CRITICAL regression cover for CPE-1126's restore panel: a revert overwrites/deletes files
  // with no Recycle Bin, so it must never fire on a single click. "Revert to this checkpoint…" only
  // ARMS a confirm (sets cpConfirming); `commands.checkpointRevert` fires ONLY from the separate
  // "Yes, revert" click. Mirrors CheckpointDialog.test.ts's gate coverage. Routes through the same
  // core-`invoke` mock the reconstruction suites use — the typed `commands.*` client dispatches to it.
  const CP = { manifest_id: "cp-1", label: "before refactor", ts: 1500 };
  const PREVIEW = {
    creates: 1,
    overwrites: 2,
    deletes: 0,
    bytes_written: 4096,
    total: 3,
    drift_count: 0,
    drift_paths: [] as string[],
  };
  const OUTCOME = { applied: 3, skipped: [] as unknown[] };
  const emptySummary = { total: 0, byKind: {}, sessions: [], firstAt: null, lastAt: null };
  const CURRENT_PATH = "Z:/work/proj";

  /** Two entries so `sliderRange` yields a scrubbable range (markers only render when a range exists). */
  const scrubEntries = () => [
    entry({ id: 2, kind: "modified", path: "Z:/work/proj/b.ts", at: 2000 }),
    entry({ id: 1, kind: "created", path: "Z:/work/proj/a.ts", at: 1000 }),
  ];

  /** Default happy-path mock: Replay-tab enter loads reconstruction + one in-range checkpoint; the
   *  marker click previews the revert; the confirm's "Yes" reverts. Individual tests can re-mock. */
  const mockCheckpointCommands = () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "replay_load":
          return { replay: { events: [], bounds: [1000, 2000], summary: emptySummary }, baseline: null };
        case "checkpoint_list":
          return [CP];
        case "checkpoint_preview_revert":
          return PREVIEW;
        case "checkpoint_revert":
          return OUTCOME;
        default:
          return undefined;
      }
    });
  };

  /** Mount, open the Replay tab, click the checkpoint marker, and settle the revert preview so the
   *  restore panel's "Revert to this checkpoint…" button is enabled (not `revertPreviewLoading`). */
  async function mountAndOpenRestorePanel(props: Record<string, unknown> = {}) {
    const r = render(AgentTimeline, {
      entries: scrubEntries(),
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: CURRENT_PATH,
      ...props,
    });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad(); // drains replay_load + checkpoint_list (both fire on tab enter)
    await fireEvent.click(screen.getByTestId(`checkpoint-marker-${CP.manifest_id}`));
    await flushReplayLoad(); // drains checkpoint_preview_revert so the arm button is enabled
    return r;
  }

  beforeEach(() => invokeMock.mockReset());
  afterEach(() => vi.useRealTimers());

  it("arming: clicking 'Revert to this checkpoint…' does NOT revert — it only shows the confirm", async () => {
    mockCheckpointCommands();
    await mountAndOpenRestorePanel();

    // Precondition: the restore panel is open with its (armed-first) revert button, no confirm yet.
    expect(screen.getByTestId("checkpoint-restore-panel")).toBeTruthy();
    expect(screen.queryByTestId("checkpoint-confirm-revert")).toBeNull();

    await fireEvent.click(screen.getByTestId("checkpoint-revert-btn"));

    // The gate: arming must NOT invoke the destructive command — it only reveals the confirm.
    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert", expect.anything());
    expect(screen.getByTestId("checkpoint-confirm-revert")).toBeTruthy();
    expect(screen.getByTestId("checkpoint-confirm-yes")).toBeTruthy();
  });

  it("confirming: checkpoint_revert fires only after 'Yes, revert', with (currentPath, manifest_id)", async () => {
    mockCheckpointCommands();
    await mountAndOpenRestorePanel();

    await fireEvent.click(screen.getByTestId("checkpoint-revert-btn")); // arm
    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert", expect.anything());

    await fireEvent.click(screen.getByTestId("checkpoint-confirm-yes")); // confirm
    await flushReplayLoad();
    await flushReplayLoad(); // settle doCheckpointRevert's post-revert list/preview refresh too

    // The ONLY place checkpointRevert is invoked — with the panel's folder + selected manifest id.
    expect(invokeMock).toHaveBeenCalledWith("checkpoint_revert", {
      root: CURRENT_PATH,
      manifestId: CP.manifest_id,
    });
    // The outcome renders, confirming the revert completed through the component (synchronous read —
    // `findBy*`/`waitFor` are known to flake in this file against real-timer restoration, see the
    // `flushReplayLoad` note above).
    expect(screen.getByTestId("checkpoint-outcome")).toBeTruthy();
  });

  it("cancelling: 'Cancel' dismisses the confirm and leaves checkpoint_revert uncalled", async () => {
    mockCheckpointCommands();
    await mountAndOpenRestorePanel();

    await fireEvent.click(screen.getByTestId("checkpoint-revert-btn")); // arm
    expect(screen.getByTestId("checkpoint-confirm-revert")).toBeTruthy();

    await fireEvent.click(screen.getByTestId("checkpoint-confirm-cancel"));

    expect(screen.queryByTestId("checkpoint-confirm-revert")).toBeNull(); // dismissed
    expect(screen.getByTestId("checkpoint-revert-btn")).toBeTruthy(); // back to the armed-first button
    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert", expect.anything());
  });

  it("empty currentPath: no marker/restore panel surfaces, so the destructive path is unreachable", async () => {
    mockCheckpointCommands();
    render(AgentTimeline, {
      entries: scrubEntries(),
      agentName: "Claude Code",
      sessionId: "sess-1",
      currentPath: "", // no folder → checkpoints never load (the load guard requires a path)
    });
    await fireEvent.click(screen.getByRole("tab", { name: "Replay" }));
    await flushReplayLoad();

    // With no folder there is nothing to revert TO: no checkpoint list, no markers, no restore panel.
    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_list", expect.anything());
    expect(screen.queryByTestId("checkpoint-markers")).toBeNull();
    expect(screen.queryByTestId("checkpoint-restore-panel")).toBeNull();
    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert", expect.anything());
  });

  it("confirm message names the selected checkpoint and warns the revert cannot be undone", async () => {
    mockCheckpointCommands();
    await mountAndOpenRestorePanel();

    await fireEvent.click(screen.getByTestId("checkpoint-revert-btn")); // arm

    const confirm = screen.getByTestId("checkpoint-confirm-revert");
    expect(confirm.textContent).toContain(CP.label); // "before refactor"
    expect(confirm.textContent).toContain(CURRENT_PATH); // the folder being overwritten
    expect(confirm.textContent).toMatch(/cannot be undone/i);
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

describe("AgentTimeline Radar tab — competing renames (CPE-1118)", () => {
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

  it("renders a divergence with a badge, filename, and actor pills, and navigates on click", async () => {
    const entries = [
      entry({
        id: 1,
        kind: "renamed",
        path: "Z:/repos/app/alice.ts",
        at: 1_000,
        actor: "sess-1",
        from: "Z:/repos/app/shared.ts",
        to: "Z:/repos/app/alice.ts",
      }),
      entry({
        id: 2,
        kind: "renamed",
        path: "Z:/repos/app/bob.ts",
        at: 1_500,
        actor: "user",
        from: "Z:/repos/app/shared.ts",
        to: "Z:/repos/app/bob.ts",
      }),
    ];
    const { component } = render(AgentTimeline, { entries, agentName: "Claude Code", sessions });
    const navigate = vi.fn();
    component.$on("navigate", (e) => navigate(e.detail));
    await fireEvent.click(screen.getByRole("tab", { name: "Radar" }));

    expect(screen.getByText("Competing renames")).toBeTruthy();
    expect(screen.getByText("diverged")).toBeTruthy();
    expect(screen.getByText("shared.ts")).toBeTruthy(); // badge shows the shared `from` path
    expect(screen.getByText("Claude Code")).toBeTruthy(); // sess-1 resolved via `sessions`
    expect(screen.getByText("You")).toBeTruthy(); // "user" -> "You"
    expect(screen.getByText(/different names/i)).toBeTruthy();

    await fireEvent.click(screen.getByText("shared.ts"));
    expect(navigate).toHaveBeenCalledWith("Z:/repos/app"); // containing folder of the shared `from`
  });

  it("renders a collision with a badge and actor pills", async () => {
    const entries = [
      entry({
        id: 1,
        kind: "renamed",
        path: "Z:/repos/app/final.ts",
        at: 1_000,
        actor: "sess-1",
        from: "Z:/repos/app/a.ts",
        to: "Z:/repos/app/final.ts",
      }),
      entry({
        id: 2,
        kind: "renamed",
        path: "Z:/repos/app/final.ts",
        at: 1_500,
        actor: "user",
        from: "Z:/repos/app/b.ts",
        to: "Z:/repos/app/final.ts",
      }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code", sessions });
    await fireEvent.click(screen.getByRole("tab", { name: "Radar" }));

    // Two actors touching the same `to` path within the overlap window also qualifies as an
    // activity overlap (agentConflicts.ts) — both sections legitimately render "final.ts" here,
    // so assert the count rather than a single match.
    expect(screen.getByText("collided")).toBeTruthy();
    expect(screen.getAllByText("final.ts").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/same name/i)).toBeTruthy();
  });

  it("shows no 'Competing renames' section and no rows when there are no rename conflicts", async () => {
    const entries = [
      entry({ id: 1, kind: "renamed", path: "Z:/repos/app/solo.ts", at: 1_000, actor: "sess-1", from: "Z:/repos/app/old.ts", to: "Z:/repos/app/solo.ts" }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code", sessions });
    await fireEvent.click(screen.getByRole("tab", { name: "Radar" }));

    expect(screen.queryByText("Competing renames")).toBeNull();
    expect(screen.getByText(/No overlapping activity/i)).toBeTruthy();
  });

  it("does not flag a same-actor rename or a from===to no-op as a competing rename", async () => {
    const entries = [
      entry({ id: 1, kind: "renamed", path: "Z:/repos/app/v1.ts", at: 1_000, actor: "sess-1", from: "Z:/repos/app/shared.ts", to: "Z:/repos/app/v1.ts" }),
      entry({ id: 2, kind: "renamed", path: "Z:/repos/app/v2.ts", at: 1_500, actor: "sess-1", from: "Z:/repos/app/shared.ts", to: "Z:/repos/app/v2.ts" }),
      entry({ id: 3, kind: "renamed", path: "Z:/repos/app/same.ts", at: 2_000, actor: "user", from: "Z:/repos/app/same.ts", to: "Z:/repos/app/same.ts" }),
    ];
    render(AgentTimeline, { entries, agentName: "Claude Code", sessions });
    await fireEvent.click(screen.getByRole("tab", { name: "Radar" }));

    expect(screen.queryByText("Competing renames")).toBeNull();
    expect(screen.getByText(/No overlapping activity/i)).toBeTruthy();
  });
});
