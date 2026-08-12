/**
 * CPE-1621 F1/F2/F3 (review round 2): `closeAllConsoles()` in App.svelte must not claim a kill it
 * didn't perform. `sidecarCloseAllSessions()` silently no-ops (`Ok("nothing")`) whenever
 * `state.url` is `None` on the Rust side — after `sidecar_repair`, or whenever the console sidecar
 * crashed/exited without an explicit `sidecar_stop` — and the Agents leaves are a client-side store
 * that persists independently of the live connection (the CPE-461 reattach design), so a no-op with
 * leaves still showing is a normal, reachable state, not a corner case. These tests drive the real
 * `App.svelte` wiring (not a unit stub) and assert:
 *
 *   F2: `sidecar_close_all_sessions` is invoked BEFORE `sidecar_stop` — pinned by an ordering
 *       assertion on `invoke.mock.calls` so a future edit that swaps them fails a test, not just a
 *       silent regression (source-line order alone previously enforced nothing).
 *   F3: the three outcomes the review found untested —
 *       (a) `state.url == null` (backend returns "nothing") while a leaf is still showing → leaves
 *           must NOT clear, and the user must be told (not a swallowed `console.debug`).
 *       (b) the POST fails/times out (backend rejects) → same: leaves untouched, notice shown.
 *       (c) genuine success (backend returns "closed") → leaves DO clear (must not regress the
 *           happy path CPE-1621 shipped).
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { ingestSessionState, clearAgentSessions, currentSessions } from "./lib/agentSessions";
import { clearAgentSessionMetrics } from "./lib/agentSessionMetrics";
import type { DirEntry, Place } from "./lib/types";

const PROJ = "C:\\proj";

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
const drives: Place[] = [{ name: "Proj (C:)", path: PROJ, kind: "drive" }];

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
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

/** How `sidecar_close_all_sessions` behaves for this test: a value it returns, or "reject" to
 *  simulate the POST failing/timing out. */
let closeAllBehavior: "closed" | "nothing" | "reject" = "closed";

function mockBackend() {
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return args?.path === PROJ ? projListing : [];
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
      case "sidecar_close_all_sessions":
        if (closeAllBehavior === "reject") throw new Error("close-all: connection refused");
        return closeAllBehavior;
      case "sidecar_registry_ids": return [];
      default: return null;
    }
  });
}

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  invoke.mockReset();
  closeAllBehavior = "closed";
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

/** Boots the app AND captures the Agent Deck button reference before any session announces itself —
 *  its label text is exactly "Agent Deck" only while `$agentSessions` is empty; once a session is
 *  running a count badge is appended as a sibling node, which would break an exact-text query taken
 *  afterwards (same reasoning as `App.agentWatchPauseMetrics.test.ts`). The captured element stays
 *  valid for a later right-click regardless of what its text becomes. */
async function boot(): Promise<HTMLElement> {
  render(App);
  const projBtn = (await screen.findAllByText("Proj (C:)"))[0];
  await fireEvent.click(projBtn);
  await waitFor(() => expect(screen.getByText("a.txt")).toBeTruthy());
  return screen.findByText("Agent Deck");
}

/** Drive the real UI path: right-click the (already-captured) Agent Deck button, click "Close all
 *  consoles", then confirm through the CPE-1621 confirm dialog. */
async function triggerCloseAll(deckBtn: HTMLElement) {
  await fireEvent.contextMenu(deckBtn);
  const closeAllItem = await screen.findByText("Close all consoles");
  await fireEvent.click(closeAllItem);
  const confirmBtn = await screen.findByRole("button", { name: "Close all" });
  await fireEvent.click(confirmBtn);
}

describe("closeAllConsoles() honesty (CPE-1621 F1/F2/F3)", () => {
  it("F2: sidecar_close_all_sessions is invoked BEFORE sidecar_stop", async () => {
    const deckBtn = await boot();
    announce("s1", PROJ);
    await triggerCloseAll(deckBtn);
    await waitFor(() => expect(invoke.mock.calls.some(([cmd]) => cmd === "sidecar_stop")).toBe(true));

    const closeAllIdx = invoke.mock.calls.findIndex(([cmd]) => cmd === "sidecar_close_all_sessions");
    const stopIdx = invoke.mock.calls.findIndex(([cmd]) => cmd === "sidecar_stop");
    expect(closeAllIdx).toBeGreaterThanOrEqual(0);
    expect(stopIdx).toBeGreaterThanOrEqual(0);
    expect(closeAllIdx).toBeLessThan(stopIdx);
  });

  it("F3(c) genuine success: leaves clear when the backend reports \"closed\"", async () => {
    closeAllBehavior = "closed";
    const deckBtn = await boot();
    announce("s1", PROJ);
    expect(currentSessions().length).toBe(1);
    await triggerCloseAll(deckBtn);
    await waitFor(() => expect(currentSessions().length).toBe(0));
  });

  it('F3(a) no-URL no-op ("nothing") with a live leaf: leaves are NOT cleared and the user is told', async () => {
    closeAllBehavior = "nothing";
    const deckBtn = await boot();
    announce("s1", PROJ);
    expect(currentSessions().length).toBe(1);
    await triggerCloseAll(deckBtn);

    // The notice text must reach the user (not a swallowed console.debug).
    await waitFor(() => expect(screen.getByText(/Couldn't reach the console/)).toBeTruthy());
    // The leaf for s1 must still be present — the UI must not claim a kill it didn't perform.
    expect(currentSessions().length).toBe(1);
    expect(currentSessions()[0].sessionId).toBe("s1");
  });

  it("F3(b) the POST fails/times out: leaves are NOT cleared and the user is told", async () => {
    closeAllBehavior = "reject";
    const deckBtn = await boot();
    announce("s1", PROJ);
    expect(currentSessions().length).toBe(1);
    await triggerCloseAll(deckBtn);

    await waitFor(() => expect(screen.getByText(/Couldn't reach the console/)).toBeTruthy());
    expect(currentSessions().length).toBe(1);
    expect(currentSessions()[0].sessionId).toBe("s1");
  });

  it('F3 "nothing" with NO leaves at all is a genuine no-op: no notice, nothing to clear', async () => {
    closeAllBehavior = "nothing";
    const deckBtn = await boot();
    // No session announced — the deck is already empty.
    await triggerCloseAll(deckBtn);
    await waitFor(() => expect(invoke.mock.calls.some(([cmd]) => cmd === "sidecar_stop")).toBe(true));
    expect(screen.queryByText(/Couldn't reach the console/)).toBeNull();
    expect(currentSessions().length).toBe(0);
  });
});
