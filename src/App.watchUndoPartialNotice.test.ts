/**
 * CPE-1671: `undoFire` re-stats each recorded copy before deleting it (CPE-1666) and silently skips one
 * that has become a directory since fire time rather than trusting the stale record — but the ONLY trace
 * of that skip used to be a devtools `console.warn`. `App.svelte`'s `undoWatchFire` showed the exact same
 * `notice.watchUndone` ("Undid: {rule}") success toast whether or not anything was actually skipped, so a
 * real user had no way to know Undo left a copy behind. This is the UAT-found risk CPE-1666's own scope
 * section named: "a silent skip is arguably worse than the original bug — the user thinks all the files
 * were handled."
 *
 * This test drives the exact CPE-1666 scenario (a copy swapped for a directory between fire and Undo)
 * through the REAL App wiring — command palette → Watch Rules dialog → a live copy-rule fire → Undo — and
 * asserts on the RENDERED notice text, not on `undoFire`'s return value: the ticket's own acceptance
 * criteria requires the user-visible message to say something was skipped, not a plain success. Verified
 * RED against the pre-fix code (temporarily reverting `undoWatchFire` back to its unconditional
 * `notice.watchUndone` toast makes this assertion fail — see PR description).
 *
 * Isolated in its own file, one `it()` only: `folderWatch.ts`'s live-watch listener (`unlisten`) is a
 * MODULE-level singleton, so a second test in this file that also calls `startFolderWatch` would silently
 * reuse the first test's stale listener/closure instead of a fresh one bound to its own App instance —
 * exactly the reasoning `App.watchLiveGate.test.ts` already documents for the same shape of test.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { translate } from "./lib/i18n";
import type { Place } from "./lib/types";

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

// The copy the rule makes, and its exact on-disk path once the (fake) backend "runs" the copy action —
// `run_watch_actions` below appends the source file's own name to the rule's literal `dest`, mirroring
// the real backend's `do_copy_into` (never resolves onto an existing entry — see folderWatch.ts).
const COPY_PATH = "C:\\backup\\invoice.pdf";

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));

// Records every `listen(event, handler)` registration so the test can fire a synthetic backend
// `folder-watch` batch by calling the captured handler directly — same technique
// `App.watchLiveGate.test.ts` established for driving the live watcher without a real Tauri IPC bridge.
const listenHandlers = new Map<string, Set<(e: { payload: unknown }) => void>>();
function emitFake(event: string, payload: unknown): void {
  for (const h of listenHandlers.get(event) ?? []) h({ payload });
}

vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => `asset://${p}`, Channel }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    let set = listenHandlers.get(event);
    if (!set) { set = new Set(); listenHandlers.set(event, set); }
    set.add(handler);
    return () => set!.delete(handler);
  }),
}));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  listenHandlers.clear();
  Element.prototype.scrollIntoView = vi.fn();

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: [], filtered: 0 };
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage([]);
        return 0;
      }
      case "parent_dir": return null;
      case "entries_for_paths": return [];
      // Fire-time stat (source file) AND undo-time re-stat (CPE-1666, the copy). Distinguish by path:
      // the copy path reports as a DIRECTORY — simulating something having replaced it since the fire —
      // everything else (the source file) reports as a plain file.
      case "entry_info": {
        const path = args?.path as string;
        if (path === COPY_PATH) {
          return { name: "invoice.pdf", path, is_dir: true, size: 0, modified: 1_700_000_000_000 };
        }
        const name = path.split("\\").pop() ?? path;
        return { name, is_dir: false, size: 10, modified: 1_700_000_000_000 };
      }
      // The rule's fs-action executor: a copy resolves to `${dest}\${sourceName}`, same shape as the
      // real backend's `do_copy_into` (destination path is the recorded copy `undoFire` later re-stats).
      case "run_watch_actions": {
        const path = args?.path as string;
        const actions = args?.actions as { kind: string; resolved: string }[];
        const name = path.split("\\").pop() ?? path;
        return actions.map((a) => ({ path: `${a.resolved}\\${name}`, ok: true, error: "" }));
      }
      // Never actually reached for the swapped-directory copy — CPE-1666's skip must short-circuit
      // before this call. Included only so an accidental regression that removes the skip doesn't
      // silently "succeed" against an unmocked command (it would throw here, deliberately loud).
      case "delete_permanent": throw new Error("delete_permanent must not be called for a directory-swapped copy");
      default: return null;
    }
  });
});

/** Opens the command palette (Ctrl+Shift+P) and runs the "Watch rules…" command. */
async function openWatchRulesDialog(): Promise<void> {
  await fireEvent.keyDown(window, { key: "P", ctrlKey: true, shiftKey: true });
  const input = await screen.findByPlaceholderText(translate("en", "palette.placeholder"));
  await fireEvent.input(input, { target: { value: "watch rules" } });
  const row = await screen.findByText(translate("en", "palette.watchRules"));
  await fireEvent.click(row);
}

describe("folder-watch Undo notice (CPE-1671)", () => {
  it("shows a partial-undo notice — not the plain success toast — when a recorded copy has become a directory", async () => {
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    // --- Build a live copy rule, arm the watcher, and sync it into the App via the dialog's own UI. ---
    await openWatchRulesDialog();

    await fireEvent.input(await screen.findByLabelText("Rule name"), { target: { value: "Backup" } });
    await fireEvent.input(await screen.findByLabelText("Extensions"), { target: { value: "pdf" } });
    await fireEvent.change(await screen.findByLabelText("Action kind"), { target: { value: "copy" } });
    await fireEvent.input(await screen.findByLabelText("Action value"), { target: { value: "C:\\backup" } });
    await fireEvent.click(await screen.findByTestId("add-action-btn"));
    await fireEvent.click(await screen.findByTestId("add-rule-btn"));

    await fireEvent.input(await screen.findByLabelText("Watch folder"), { target: { value: "C:\\d" } });
    await fireEvent.click(await screen.findByTestId("add-folder-btn"));
    await fireEvent.click(await screen.findByTestId("watch-live-toggle"));

    // "Done" syncs the dialog's local rule list into App.svelte's own `watchRules` (what the live
    // listener actually reads when a batch lands) and closes the dialog.
    await fireEvent.click(await screen.findByTestId("done-btn"));

    // --- Fire the rule: a "created" .pdf lands in the watched folder. ---
    await waitFor(() => expect(listenHandlers.get("folder-watch")?.size ?? 0).toBeGreaterThan(0));
    emitFake("folder-watch", [{ path: "C:\\d\\invoice.pdf", kind: "created" }]);

    // --- Reopen the dialog and Undo the fire it just logged. ---
    await openWatchRulesDialog();
    const undoBtn = await screen.findByTestId("undo-btn", {}, { timeout: 3000 });
    await fireEvent.click(undoBtn);

    // The assertion this ticket exists for: the RENDERED notice must say something was skipped, not the
    // plain "Undid: Backup" success toast the pre-fix code always showed regardless of the skip.
    const expectedPartial = translate("en", "notice.watchUndonePartialOne", { rule: "Backup", count: 1 });
    await waitFor(() => expect(screen.getByText(expectedPartial)).toBeTruthy());

    // Negative control: the OLD plain-success message must NOT be what's shown.
    const plainSuccess = translate("en", "notice.watchUndone", { rule: "Backup" });
    expect(screen.queryByText(plainSuccess)).toBeNull();
  });
});
