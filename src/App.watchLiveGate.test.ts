/**
 * Regression test (CPE-1230 review defect — "off means off"): `reconcileWatch` computed
 * `rulePaths = watchLive ? watchedFolders : []` (correctly gating the RULE folders) but still passed
 * `() => watchRules` — every enabled rule, unconditionally — into `startFolderWatch` whenever the
 * combined path set was nonempty. Since a smart folder's own scope paths feed that same combined set,
 * simply OPENING a smart folder whose scope happens to overlap a configured (but paused, `watchLive`
 * false) rule's folder would reactivate the rule: a landed `folder-watch` event would run its
 * move/copy/rename actions even though "Live watching" is off. The fix gates the rules function itself
 * on `watchLive` too (`() => (watchLive ? watchRules : [])`), so a smart-folder-only watch never
 * executes a paused rule set — while the smart folder's own recompute (independent of `watchLive`,
 * CPE-1230's main feature) must still fire.
 *
 * Isolated in its OWN file (rather than added to `App.smartFolderLiveRefresh.test.ts`) because
 * `folderWatch.ts`'s rules listener is a MODULE-level singleton (`unlisten` only ever attaches once
 * per module instance) — a second `it()` sharing a module instance with an earlier test that already
 * triggered `startFolderWatch` would silently keep the FIRST test's stale listener/closure instead of
 * getting a fresh one bound to this test's own component instance and rules. One test per file sidesteps
 * that entirely (vitest gives each test file a fresh module graph by default).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveWatchRules } from "./lib/settings";
import { addRule } from "./lib/watchRules";
import { setEntryTags } from "./lib/tags";
import { saveSmartFolder, smartFolders } from "./lib/smartFolders";
import { savedSearches } from "./lib/savedSearchStore";
import type { Place } from "./lib/types";

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));

// Records every `listen(event, handler)` registration so the test can fire a synthetic backend event
// by calling every captured handler directly (both the smart-folder recompute listener AND the
// watch-rules executor listener are plain consumers of the same `folder-watch` event).
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
  savedSearches.set([]);
  smartFolders.set([]);
  listenHandlers.clear();
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
      case "entries_for_paths": {
        const paths = (args?.paths as string[] | undefined) ?? [];
        return paths.map((p) => ({
          name: p.split("\\").pop() ?? p,
          path: p,
          is_dir: false,
          size: 1,
          modified: 1_700_000_000_000,
          extension: (p.split(".").pop() ?? ""),
          hidden: false,
          is_symlink: false,
        }));
      }
      case "set_tags": {
        const path = args?.path as string;
        const tagList = (args?.tags as string[] | undefined) ?? [];
        const label = (args?.label as string | undefined) ?? "";
        return { [path]: { tags: tagList, label } };
      }
      // Backs `handleFolderBatch`'s per-file stat before it plans/runs a matching rule. Raw payload —
      // `commands.entryInfo` (bindings.gen.ts) wraps this in the `Result` envelope itself.
      case "entry_info": {
        const path = args?.path as string;
        const name = path.split("\\").pop() ?? path;
        return { name, is_dir: false, size: 1, modified: 1_700_000_000_000 };
      }
      // The rule's FS action executor — must NEVER be called while `watchLive` is off.
      case "run_watch_actions": {
        const path = args?.path as string;
        return [{ path, ok: true, error: "" }];
      }
      default: return null;
    }
  });
});

describe("watched-folder rules stay paused while `watchLive` is off, even with a smart folder open over the same folder (CPE-1230 review defect)", () => {
  it("a landed folder-watch event does not execute the configured rule, but the open smart folder still recomputes", async () => {
    // An enabled rule that would rename any .txt file it sees — armed in storage BEFORE mount, since
    // App.svelte reads `settings.loadWatchRules()` once at component init.
    saveWatchRules(
      addRule([], "txt-rename", { kind: "ext", exts: ["txt"] }, [{ kind: "rename", template: "moved-{name}" }]),
    );

    render(App);
    await screen.findAllByText("Local Disk (C:)");

    // Tag AFTER mount: `initTags()` runs once on mount and would otherwise stomp a pre-mount tag back
    // to the (mocked, empty) backend store.
    await setEntryTags("C:\\d\\a.txt", ["invoice"], "");
    saveSmartFolder("Invoices", "invoice");

    const row = await screen.findByText("Invoices");
    await fireEvent.click(row);
    await waitFor(() => expect(screen.getByText("a.txt")).toBeTruthy());

    // `watchLive` ("Live watching") was never toggled on — it defaults to false and nothing in this
    // test flips it — yet the smart folder's own scope (over "C:\d", the tagged file's folder) is what
    // now arms the shared backend watcher, per CPE-1230.
    const recomputeCallsBefore = invoke.mock.calls.filter(([cmd]) => cmd === "entries_for_paths").length;

    // A change lands on the tagged file itself: in-scope for the smart folder AND matches the paused
    // rule's condition (a .txt file).
    emitFake("folder-watch", [{ path: "C:\\d\\a.txt", kind: "modified" }]);

    // The smart folder still live-refreshes — CPE-1230's actual feature is unaffected by this gate.
    await waitFor(() => {
      const recomputeCallsAfter = invoke.mock.calls.filter(([cmd]) => cmd === "entries_for_paths").length;
      expect(recomputeCallsAfter).toBeGreaterThan(recomputeCallsBefore);
    }, { timeout: 2000 });

    // But the paused rule's action must never have run — "off means off".
    expect(invoke.mock.calls.some(([cmd]) => cmd === "run_watch_actions")).toBe(false);
  });
});
