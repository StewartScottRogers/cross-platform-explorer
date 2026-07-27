/**
 * Integration test (CPE-1112 rework, filed after independent review + UAT both found the SAME
 * data-loss hole): while Replay mode's read-only overlay is showing in the main file pane, no
 * keyboard shortcut / command-palette action may act on a LIVE file. The original bug was that
 * `ExplorerPane`'s FileList-level no-ops (open/context/drop/commitRename/...) only covered actions
 * that route THROUGH FileList's dispatched events — App.svelte's own mutators (Ctrl+V paste,
 * Ctrl+Shift+N new-folder, Delete, Enter-to-open) read `selectedEntries` or run their own guarded
 * functions directly, bypassing FileList entirely, and `selectedEntries` used to be derived by
 * indexing into the LIVE `visible` array with an index that meant something different in the
 * overlay's row order/set — a real wrong-file mutation path.
 *
 * `replayOverlay.test.ts` and `AgentTimeline.test.ts` already prove the pure derivation + the
 * component-local dispatch are correct; THIS file drives the full integrated path a real user takes —
 * a real watched agent session, a real Replay-tab load, the real "Show in file pane" toggle — and
 * proves the fix at the App-level boundary where the leak actually was.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { ingestSessionState, clearAgentSessions } from "./lib/agentSessions";
import { ingestActivity, clearActivity } from "./lib/agentActivity";
import type { DirEntry, Place } from "./lib/types";

const file = (name: string, extension: string): DirEntry => ({
  name,
  path: `C:\\d\\${name}`,
  is_dir: false,
  size: 1024,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension,
  hidden: false,
});

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];
const listing = [file("alpha.md", "md"), file("beta.txt", "txt")];

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
// This is the first test file to actually arm a live watch (via `ingestSessionState`), which makes
// `reconcileAgentWatch` (App.svelte) call `initAgentActivity`/`initAgentDiffs`/`initAgentCost` — each
// wraps the REAL `@tauri-apps/api/event.listen`, which needs the Tauri IPC bridge
// (`window.__TAURI_INTERNALS__`) that doesn't exist in jsdom. Mock it to a no-op listener so arming a
// watch here behaves like "the platform is present but nothing has fired yet", exactly like the other
// Tauri plugin mocks above.
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const replayEv = (ts: number, kind: string, path: string) => ({
  ts,
  session: "sess-1",
  kind,
  path,
  actor: null,
  detail: null,
});

function mockBackend() {
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return listing;
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage(listing);
        return listing.length;
      }
      case "parent_dir": return null;
      case "agent_watch_start": return null;
      case "agent_watch_stop": return null;
      case "replay_load":
        return {
          replay: {
            events: [
              replayEv(100, "created", "C:\\d\\alpha.md"),
              replayEv(200, "created", "C:\\d\\beta.txt"),
            ],
            bounds: [100, 200],
            summary: { total: 2, byKind: {}, sessions: ["sess-1"], firstAt: 100, lastAt: 200 },
          },
          baseline: null,
        };
      // Guarded mutators — the assertions below check these never get called while the overlay is
      // showing; the return values here only matter for the "leaving Replay mode restores it" case.
      case "create_dir": return `${args?.path}\\New folder`;
      case "delete_to_trash":
      case "delete_permanent":
        return (args?.paths as string[]).map((p) => ({ path: p, ok: true, error: "" }));
      case "copy_entries":
      case "move_entries":
        return (args?.paths as string[]).map((p) => ({ path: `${p} (x)`, ok: true, error: "" }));
      case "open_external": return null;
      case "read_file_text": return "FILE PREVIEW CONTENT";
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
  clearActivity();
});

afterEach(() => {
  clearAgentSessions();
  clearActivity();
});

function announceSession() {
  ingestSessionState(
    `session:${JSON.stringify({
      event: "started",
      sessionId: "sess-1",
      agentId: "claude-code",
      agentName: "Claude Code",
      provider: "anthropic",
      model: "claude",
      cwd: "C:\\d",
    })}`,
  );
  // At least two timeline entries so the Replay tab's scrubber (`sliderRange`) is enabled.
  ingestActivity([{ kind: "created", path: "C:\\d\\alpha.md" }], 1000);
  ingestActivity([{ kind: "created", path: "C:\\d\\beta.txt" }], 2000);
}

/** Render App, navigate into C:\d, arm a watched agent session on that folder, open the Activity
 *  drawer, load the Replay tab, and check "Show in file pane" — the full path a real user takes to
 *  get the overlay showing. Resolves once the "Replay mode" banner is visible in the main pane. */
async function enterReplayOverlay() {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

  announceSession();
  await waitFor(() => expect(screen.getByTitle("Show the full activity log")).toBeTruthy());
  await fireEvent.click(screen.getByTitle("Show the full activity log"));
  await fireEvent.click(await screen.findByRole("tab", { name: "Replay" }));
  await waitFor(() => expect(screen.getByText(/Reconstruction at scrub time \(read-only\)/i)).toBeTruthy());

  await fireEvent.click(screen.getByLabelText("Show in file pane"));
  await waitFor(() => expect(screen.getByText(/Replay mode — read-only reconstruction/i)).toBeTruthy());
}

describe("Replay-mode overlay: integrated read-only guards (CPE-1112 rework)", () => {
  it("clicking an overlay row never selects a live file (no '#N selected' status)", async () => {
    await enterReplayOverlay();
    const rows = document.querySelectorAll(".rows .row");
    expect(rows.length).toBeGreaterThan(0); // sanity: the overlay actually rendered rows to click
    await fireEvent.click(rows[0]);
    expect(screen.queryByText(/\d+ selected/)).toBeNull();
  });

  it("Delete never deletes a live file while the overlay is showing", async () => {
    await enterReplayOverlay();
    await fireEvent.click(document.querySelectorAll(".rows .row")[0]); // try to "select" an overlay row
    invoke.mockClear();
    await fireEvent.keyDown(window, { key: "Delete" });
    expect(invoke).not.toHaveBeenCalledWith("delete_to_trash", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("delete_permanent", expect.anything());
  });

  it("Ctrl+V is blocked with the read-only notice, even with a non-empty clipboard", async () => {
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    // Copy a REAL live file BEFORE entering Replay mode, so the clipboard is non-empty — proves the
    // block below is Replay mode's own guard firing, not the pre-existing "nothing to paste" no-op.
    await fireEvent.click(screen.getByText("alpha.md"));
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    await waitFor(() => expect(screen.getByText(/^Copied 1 item/)).toBeTruthy());

    announceSession();
    await waitFor(() => expect(screen.getByTitle("Show the full activity log")).toBeTruthy());
    await fireEvent.click(screen.getByTitle("Show the full activity log"));
    await fireEvent.click(await screen.findByRole("tab", { name: "Replay" }));
    await waitFor(() => expect(screen.getByText(/Reconstruction at scrub time \(read-only\)/i)).toBeTruthy());
    await fireEvent.click(screen.getByLabelText("Show in file pane"));
    await waitFor(() => expect(screen.getByText(/Replay mode — read-only reconstruction/i)).toBeTruthy());

    invoke.mockClear();
    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });
    await waitFor(() => expect(screen.getByText(/read-only Replay-mode reconstruction/i)).toBeTruthy());
    expect(invoke).not.toHaveBeenCalledWith("copy_entries", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("move_entries", expect.anything());
  });

  it("Ctrl+Shift+N (new folder) is blocked with the read-only notice", async () => {
    await enterReplayOverlay();
    invoke.mockClear();
    await fireEvent.keyDown(window, { key: "N", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(screen.getByText(/read-only Replay-mode reconstruction/i)).toBeTruthy());
    expect(invoke).not.toHaveBeenCalledWith("create_dir", expect.anything());
  });

  it("Enter never opens/navigates a live file while the overlay is showing", async () => {
    await enterReplayOverlay();
    invoke.mockClear();
    // Dispatched on `document.body` (not `window`) so `event.target` is a real Element — matches how a
    // real keydown reaches `<svelte:window>` (target = the focused element, bubbling up to window);
    // App's Enter handler calls `target?.closest(".row")`, which only an Element supports.
    await fireEvent.keyDown(document.body, { key: "Enter" });
    expect(invoke).not.toHaveBeenCalledWith("open_external", expect.anything());
    // Still showing the overlay — Enter-to-open never navigated anywhere either.
    expect(screen.getByText(/Replay mode — read-only reconstruction/i)).toBeTruthy();
  });

  it("leaving Replay mode restores full functionality (new-folder works again)", async () => {
    await enterReplayOverlay();
    await fireEvent.click(screen.getByLabelText("Show in file pane")); // uncheck
    await waitFor(() => expect(screen.queryByText(/Replay mode — read-only reconstruction/i)).toBeNull());
    // Scoped to the main file pane's row container (`.rows`) specifically — the drawer's own
    // reconstruction/frozen-event lists also render "alpha.md" text, so an unscoped query is ambiguous
    // while the drawer stays open.
    await waitFor(() =>
      expect(within(document.querySelector(".rows") as HTMLElement).getByText("alpha.md")).toBeTruthy(),
    );

    invoke.mockClear();
    await fireEvent.keyDown(window, { key: "N", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_dir", expect.anything()));
  });
});
