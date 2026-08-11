/**
 * Integration test (CPE-1626 — decouple metrics flush from watch teardown). `App.replayGuards.test.ts`
 * and `lib/agentSessions.test.ts` / `lib/agentSessionMetrics.test.ts` already cover the pure logic in
 * isolation; THIS file drives the real wiring in `App.svelte`'s `reconcileAgentWatch` — a real navigation
 * in and out of a watched agent's folder — and asserts on the actual backend calls it makes
 * (`agent_watch_start` / `agent_watch_stop` / `metrics_record`), proving:
 *
 *   1. A session never navigated into stays fully unarmed (CPE-1606's original boundary).
 *   2. Navigating away from a still-running watched session's folder disarms its watcher (stops the
 *      `notify` watch) WITHOUT flushing a metrics row — a pause, not an end (CPE-1626's fix). Navigating
 *      back in re-arms the SAME session, and its activity from both before and after the pause ends up in
 *      exactly ONE `metrics_record` row once the session actually ends — nothing fragmented, duplicated,
 *      or silently dropped.
 *   3. Two sessions sharing one cwd are both armed together when you navigate into it (CPE-1625,
 *      must not regress).
 *
 * Isolated in its own file (module-level singleton stores in agentSessions.ts/agentSessionMetrics.ts/
 * agentDiffs.ts persist across `it()`s in one file — see `App.replayGuards.test.ts`'s use of
 * `clearAgentSessions()`/`clearActivity()` in beforeEach/afterEach for the same pattern) so a fresh test
 * run starts from a clean slate every time.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { ingestSessionState, clearAgentSessions } from "./lib/agentSessions";
import { clearAgentSessionMetrics } from "./lib/agentSessionMetrics";
import { ingestDiff } from "./lib/agentDiffs";
import type { DirEntry, Place } from "./lib/types";

const PROJ = "C:\\proj";
const OTHER = "D:\\other";
const NEVER = "E:\\never-visited";

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
const otherListing = [file(OTHER, "z.txt")];
const drives: Place[] = [
  { name: "Proj (C:)", path: PROJ, kind: "drive" },
  { name: "Other (D:)", path: OTHER, kind: "drive" },
];

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
// Same reasoning as App.replayGuards.test.ts: arming a real watch calls `initAgentActivity`/
// `initAgentDiffs`/`initAgentCost`, each of which wraps the real `listen` — mock it to a no-op so it
// behaves like "the platform is present but nothing has fired yet" in jsdom.
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

function listingFor(path: unknown): DirEntry[] {
  if (path === PROJ) return projListing;
  if (path === OTHER) return otherListing;
  return [];
}

function mockBackend() {
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return listingFor(args?.path);
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args?.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "agent_watch_start": return null;
      case "agent_watch_stop": return null;
      case "agent_watch_stop_all": return null;
      case "metrics_record": return null;
      default: return null;
    }
  });
}

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
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

function end(sessionId: string) {
  ingestSessionState(`session:${JSON.stringify({ event: "ended", sessionId })}`);
}

const watchStartCallsFor = (id: string) =>
  invoke.mock.calls.filter(([cmd, a]) => cmd === "agent_watch_start" && (a as Record<string, unknown>)?.sessionId === id);
const watchStopCallsFor = (id: string) =>
  invoke.mock.calls.filter(([cmd, a]) => cmd === "agent_watch_stop" && (a as Record<string, unknown>)?.sessionId === id);
const metricsRecordCalls = () => invoke.mock.calls.filter(([cmd]) => cmd === "metrics_record");

async function boot() {
  render(App);
  const projBtn = (await screen.findAllByText("Proj (C:)"))[0];
  await fireEvent.click(projBtn);
  await waitFor(() => expect(screen.getByText("a.txt")).toBeTruthy());
}

describe("Agent Watch: pause vs end (CPE-1626 wiring)", () => {
  it("a session never navigated into stays fully unarmed (CPE-1606 boundary, must not regress)", async () => {
    await boot();
    announce("s-never", NEVER);
    // Give any pending reconcile a tick to run.
    await new Promise((r) => setTimeout(r, 50));
    expect(watchStartCallsFor("s-never")).toHaveLength(0);
  });

  it("navigate-away pauses (disarms, no flush); navigate-back resumes; ending later flushes ONE complete row", async () => {
    await boot();

    announce("s1", PROJ);
    await waitFor(() => expect(watchStartCallsFor("s1")).toHaveLength(1)); // armed: inside its folder

    // Pre-pause activity, attributed to s1.
    ingestDiff([{ path: `${PROJ}\\a.txt`, before: "", after: "hello", actor: "s1" }]);

    // Navigate away to an unrelated folder while s1 is still running.
    const otherBtn = (await screen.findAllByText("Other (D:)"))[0];
    await fireEvent.click(otherBtn);
    await waitFor(() => expect(screen.getByText("z.txt")).toBeTruthy());
    await waitFor(() => expect(watchStopCallsFor("s1")).toHaveLength(1)); // disarmed (paused)

    // Must NOT have flushed a metrics row just because the watcher stopped — s1 is still running.
    expect(metricsRecordCalls()).toHaveLength(0);

    // Navigate back into s1's folder: resumes (re-arms) the SAME session.
    const projBtn = (await screen.findAllByText("Proj (C:)"))[0];
    await fireEvent.click(projBtn);
    await waitFor(() => expect(screen.getByText("a.txt")).toBeTruthy());
    await waitFor(() => expect(watchStartCallsFor("s1")).toHaveLength(2)); // re-armed

    // Post-pause activity, same session.
    ingestDiff([{ path: `${PROJ}\\b.txt`, before: "", after: "post-pause world", actor: "s1" }]);

    // The session genuinely ends now.
    end("s1");

    await waitFor(() => expect(metricsRecordCalls()).toHaveLength(1));
    const rec = metricsRecordCalls()[0][1] as { rec: Record<string, unknown> };
    // Both the pre-pause AND post-pause edits are present in the ONE persisted row — nothing dropped.
    expect(rec.rec.editCount).toBe(2);
  });

  it("CPE-1625: two sessions sharing the same cwd are both armed together (must not regress)", async () => {
    await boot();
    announce("s1", PROJ);
    announce("s2", PROJ);
    await waitFor(() => expect(watchStartCallsFor("s1")).toHaveLength(1));
    await waitFor(() => expect(watchStartCallsFor("s2")).toHaveLength(1));
  });
});
