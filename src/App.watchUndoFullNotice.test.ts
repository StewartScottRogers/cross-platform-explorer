/**
 * CPE-1671 companion to `App.watchUndoPartialNotice.test.ts`: the acceptance criterion that a FULLY
 * successful undo must keep today's plain "Undid: {rule}" toast unchanged — no new noise on the common
 * path. Same real-App-wiring approach (command palette → Watch Rules dialog → live copy-rule fire →
 * Undo), but every recorded copy re-stats as a plain file, so `undoFire` reports nothing skipped and
 * `undoWatchFire` must fall back to the exact pre-existing success message.
 *
 * Isolated in its own file for the same reason as its partial-case sibling: `folderWatch.ts`'s live-watch
 * listener (`unlisten`) is a MODULE-level singleton, so a second `it()` in the same file that also calls
 * `startFolderWatch` would reuse the first test's stale listener instead of a fresh one.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { translate } from "./lib/i18n";
import type { Place } from "./lib/types";

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));

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

const deletePermanentCalls: string[][] = [];

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  listenHandlers.clear();
  deletePermanentCalls.length = 0;
  Element.prototype.scrollIntoView = vi.fn();

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return [];
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage([]);
        return 0;
      }
      case "parent_dir": return null;
      case "entries_for_paths": return [];
      // Every path (fire-time source stat AND undo-time re-stat) reports as a plain file — nothing to
      // skip, the common case.
      case "entry_info": {
        const path = args?.path as string;
        const name = path.split("\\").pop() ?? path;
        return { name, is_dir: false, size: 10, modified: 1_700_000_000_000 };
      }
      case "run_watch_actions": {
        const path = args?.path as string;
        const actions = args?.actions as { kind: string; resolved: string }[];
        const name = path.split("\\").pop() ?? path;
        return actions.map((a) => ({ path: `${a.resolved}\\${name}`, ok: true, error: "" }));
      }
      case "delete_permanent": {
        const paths = args?.paths as string[];
        deletePermanentCalls.push(paths);
        return paths.map((p) => ({ path: p, ok: true, error: "" }));
      }
      default: return null;
    }
  });
});

async function openWatchRulesDialog(): Promise<void> {
  await fireEvent.keyDown(window, { key: "P", ctrlKey: true, shiftKey: true });
  const input = await screen.findByPlaceholderText(translate("en", "palette.placeholder"));
  await fireEvent.input(input, { target: { value: "watch rules" } });
  const row = await screen.findByText(translate("en", "palette.watchRules"));
  await fireEvent.click(row);
}

describe("folder-watch Undo notice — common path stays clean (CPE-1671)", () => {
  it("still shows the plain 'Undid: {rule}' toast, unchanged, when every copy is actually removed", async () => {
    render(App);
    await screen.findAllByText("Local Disk (C:)");

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

    await fireEvent.click(await screen.findByTestId("done-btn"));

    await waitFor(() => expect(listenHandlers.get("folder-watch")?.size ?? 0).toBeGreaterThan(0));
    emitFake("folder-watch", [{ path: "C:\\d\\invoice.pdf", kind: "created" }]);

    await openWatchRulesDialog();
    const undoBtn = await screen.findByTestId("undo-btn", {}, { timeout: 3000 });
    await fireEvent.click(undoBtn);

    // Exactly today's message — no new noise on the fully-successful path.
    const expected = translate("en", "notice.watchUndone", { rule: "Backup" });
    await waitFor(() => expect(screen.getByText(expected)).toBeTruthy());

    // The delete really happened (this isn't green because nothing ran).
    await waitFor(() => expect(deletePermanentCalls).toEqual([["C:\\backup\\invoice.pdf"]]));

    // Neither partial-undo message variant is shown.
    expect(screen.queryByText(translate("en", "notice.watchUndonePartialOne", { rule: "Backup", count: 1 }))).toBeNull();
    expect(screen.queryByText(translate("en", "notice.watchUndonePartialMany", { rule: "Backup", count: 2 }))).toBeNull();
  });
});
