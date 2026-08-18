/**
 * CPE-1643 regression (leak 1 of 2): `unlistenDiffs` / `unlistenCost` are armed alongside
 * `unlistenActivity` in the agent-watch reconcile block (`App.svelte` ~L1476-1492, all three via
 * `initAgentActivity`/`initAgentDiffs`/`initAgentCost`), but `onDestroy` only ever tore down
 * `unlistenActivity` — so a watch still armed at component-destroy time left two event listeners
 * registered past the component's life. Found by the CPE-1633 worker sweeping `App.svelte` for the same
 * leak shape and asked to report rather than expand its diff.
 *
 * Isolated in its own file (rather than sharing `App.agentWatchPauseMetrics.test.ts`) purely to keep the
 * `listen()` mock specific to this test's needs: it must return a DISTINCT, per-event teardown spy (so
 * "fs-diff's unlisten ran" can be told apart from "fs-activity's unlisten ran"), which is stricter than
 * that file's shared no-op teardown.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { ingestSessionState, clearAgentSessions } from "./lib/agentSessions";
import { clearAgentSessionMetrics } from "./lib/agentSessionMetrics";
import type { DirEntry, Place } from "./lib/types";

const PROJ = "C:\\proj";
const drives: Place[] = [{ name: "Proj (C:)", path: PROJ, kind: "drive" }];

const file = (dir: string, name: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: false,
  size: 10,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "txt",
  hidden: false,
  is_symlink: false,
});
const projListing = [file(PROJ, "a.txt")];

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => `asset://${p}`, Channel }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

// Records, PER EVENT NAME, a fresh teardown spy for every `listen(event, handler)` registration — so a
// test can assert whether THAT SPECIFIC event's teardown ran on destroy, not just "some listener, some
// unlisten".
const teardownsByEvent = new Map<string, ReturnType<typeof vi.fn>[]>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string) => {
    const teardown = vi.fn();
    const arr = teardownsByEvent.get(event) ?? [];
    arr.push(teardown);
    teardownsByEvent.set(event, arr);
    return teardown;
  }),
}));

function mockBackend() {
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: args?.path === PROJ ? projListing : [], filtered: 0 };
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        const data = args?.path === PROJ ? projListing : [];
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "agent_watch_start": return null;
      case "agent_watch_stop": return null;
      case "agent_watch_stop_all": return null;
      case "metrics_record": return null;
      case "sidecar_stop": return null;
      case "sidecar_close_all_sessions": return null;
      case "sidecar_registry_ids": return [];
      default: return null;
    }
  });
}

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  teardownsByEvent.clear();
  invoke.mockReset();
  mockBackend();
  clearAgentSessions();
  clearAgentSessionMetrics();
});

afterEach(() => {
  clearAgentSessions();
  clearAgentSessionMetrics();
});

function announce(sessionId: string, cwd: string) {
  ingestSessionState(
    `session:${JSON.stringify({
      event: "started",
      sessionId,
      agentId: "claude-code",
      agentName: "Claude Code",
      provider: "anthropic",
      model: "claude",
      cwd,
    })}`,
  );
}

const watchStartCallsFor = (id: string) =>
  invoke.mock.calls.filter(([cmd, a]) => cmd === "agent_watch_start" && (a as Record<string, unknown>)?.sessionId === id);

describe("onDestroy releases the agent-watch diff/cost listeners even while a watch is still armed (CPE-1643)", () => {
  it("unlistens ai-console://fs-diff and ai-console://agent-cost on destroy, not just fs-activity", async () => {
    render(App);
    const projBtn = (await screen.findAllByText("Proj (C:)"))[0];
    await fireEvent.click(projBtn);
    await waitFor(() => expect(screen.getByText("a.txt")).toBeTruthy());

    // Arm a watch on the current folder — this is the reconcile block that starts ALL THREE shared
    // listeners together (`initAgentActivity`/`initAgentDiffs`/`initAgentCost`, App.svelte ~L1477-1479).
    announce("s1", PROJ);
    await waitFor(() => expect(watchStartCallsFor("s1")).toHaveLength(1));
    // The listener-arm block runs AFTER the `agent_watch_start` invoke call within the same
    // `reconcileAgentWatch` pass — wait directly on the thing under test (all three `listen()`
    // registrations) rather than inferring readiness from an earlier step in that same pass.
    await waitFor(() => expect(teardownsByEvent.get("ai-console://agent-cost")).toHaveLength(1));

    const activityTeardowns = teardownsByEvent.get("ai-console://fs-activity") ?? [];
    const diffTeardowns = teardownsByEvent.get("ai-console://fs-diff") ?? [];
    const costTeardowns = teardownsByEvent.get("ai-console://agent-cost") ?? [];
    // Sanity: all three really did get armed together — otherwise this test would trivially pass.
    expect(activityTeardowns).toHaveLength(1);
    expect(diffTeardowns).toHaveLength(1);
    expect(costTeardowns).toHaveLength(1);

    // Destroy the component WITHOUT disarming the watch first (no navigate-away, no session end) —
    // mirrors a window close mid-session, or @testing-library/svelte's own afterEach(cleanup()).
    cleanup();

    // Control: fs-activity's teardown already worked pre-CPE-1643 — proves the harness itself is sound
    // (this assertion would pass even against the pre-fix code, unlike the two below).
    expect(activityTeardowns[0]).toHaveBeenCalledOnce();
    // The actual regression: these two were never reached by onDestroy before this ticket's fix. Verified
    // as a real negative control by temporarily removing the `unlistenDiffs?.(); unlistenCost?.();` lines
    // this ticket added to `onDestroy` and re-running this file — both assertions below failed
    // (`expected [] to be called` / spy never invoked) while the fs-activity control above kept passing.
    expect(diffTeardowns[0]).toHaveBeenCalledOnce();
    expect(costTeardowns[0]).toHaveBeenCalledOnce();
  });
});
