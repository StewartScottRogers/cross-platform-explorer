/**
 * Integration test: an OPEN smart folder (structured search + tag-only) recomputes on a FILESYSTEM
 * change, not just a `$tags`/query change (CPE-1230, epic CPE-978). `App.savedSearch.test.ts` already
 * proves the CPE-1229 open-evaluator runs on open; this proves the other half of the DoD — a landed
 * `folder-watch` FS-event (CPE-794's existing signal, reused rather than a second `notify` watcher)
 * triggers a real recompute while the folder stays open, is ignored when irrelevant, and stops firing
 * once the folder is closed (no stale recompute after exit).
 *
 * `@tauri-apps/api/event.listen` is mocked to a fake that records every registered handler so the test
 * can simulate the backend's `folder-watch` batch directly, instead of standing up a real Tauri IPC
 * bridge (which doesn't exist in jsdom).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { savedSearches, addSavedSearch } from "./lib/savedSearchStore";
import { smartFolders, saveSmartFolder } from "./lib/smartFolders";
import { setEntryTags } from "./lib/tags";
import type { Place } from "./lib/types";
import type { TreeNode } from "./lib/bindings.gen";

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

const scannedTreeV1: TreeNode[] = [{ name: "keep.md", isDir: false, size: 10, modified: 1_700_000_000_000 }];
// The "after a file landed on disk" version of the same scan — one more matching file present.
const scannedTreeV2: TreeNode[] = [
  ...scannedTreeV1,
  { name: "new.md", isDir: false, size: 20, modified: 1_700_000_001_000 },
];

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));

// Records every `listen(event, handler)` registration so a test can fire a synthetic backend event by
// calling the captured handler directly — the fake stands in for the Tauri IPC bridge jsdom lacks.
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

let scanTreeCallCount = 0;

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  savedSearches.set([]);
  smartFolders.set([]);
  listenHandlers.clear();
  scanTreeCallCount = 0;
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
      case "scan_tree": {
        scanTreeCallCount += 1;
        return scanTreeCallCount === 1 ? scannedTreeV1 : scannedTreeV2;
      }
      case "entries_for_paths": {
        const paths = (args?.paths as string[] | undefined) ?? [];
        return paths.map((p) => ({
          name: p.split("\\").pop() ?? p,
          path: p,
          is_dir: false,
          size: 1,
          modified: 1_700_000_000_000,
          extension: "",
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
      default: return null;
    }
  });
});

describe("structured search live-refresh on filesystem change (CPE-1230)", () => {
  it("recomputes the open structured search when a relevant folder-watch event lands, debounced", async () => {
    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    const row = await screen.findByText("Markdown docs");
    await fireEvent.click(row);
    await waitFor(() => expect(screen.getByText("keep.md")).toBeTruthy());
    expect(scanTreeCallCount).toBe(1);

    // Simulate the EXISTING `folder-watch` backend signal (CPE-794) firing for a file that landed
    // under the search's captured root — no manual re-run triggered it.
    expect(listenHandlers.has("folder-watch")).toBe(true);
    emitFake("folder-watch", [{ path: "C:\\d\\new.md", kind: "created" }]);

    // Debounced (300ms) — the new match shows up without any user action.
    await waitFor(() => expect(screen.getByText("new.md")).toBeTruthy(), { timeout: 2000 });
    expect(scanTreeCallCount).toBe(2);
  });

  it("does not recompute for a folder-watch event outside the search's scope", async () => {
    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    render(App);
    await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(await screen.findByText("Markdown docs"));
    await waitFor(() => expect(screen.getByText("keep.md")).toBeTruthy());
    expect(scanTreeCallCount).toBe(1);

    emitFake("folder-watch", [{ path: "C:\\other\\unrelated.md", kind: "created" }]);
    // Give the debounce window a chance to fire if it were (wrongly) going to.
    await new Promise((r) => setTimeout(r, 400));
    expect(scanTreeCallCount).toBe(1);
  });

  it("stops recomputing once the structured search is closed", async () => {
    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    render(App);
    await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(await screen.findByText("Markdown docs"));
    await waitFor(() => expect(screen.getByText("keep.md")).toBeTruthy());

    // Leave the structured search (back to Home).
    await fireEvent.click(await screen.findByText("Local Disk (C:)"));
    await waitFor(() => expect(screen.queryByText("keep.md")).toBeNull());

    const countAfterClose = scanTreeCallCount;
    emitFake("folder-watch", [{ path: "C:\\d\\new.md", kind: "created" }]);
    await new Promise((r) => setTimeout(r, 400));
    expect(scanTreeCallCount).toBe(countAfterClose); // no recompute — the listener was torn down on exit
  });
});

describe("tag smart folder live-refresh on filesystem change (CPE-1230)", () => {
  it("re-stats an open tag smart folder's matched paths when one of them changes on disk", async () => {
    render(App);
    await screen.findAllByText("Local Disk (C:)");
    // Tag AFTER mount: `initTags()` runs once on mount and would otherwise stomp a pre-mount tag back
    // to the (mocked, empty) backend store.
    await setEntryTags("C:\\d\\a.txt", ["invoice"], "");
    saveSmartFolder("Invoices", "invoice");

    const row = await screen.findByText("Invoices");
    await fireEvent.click(row);
    await waitFor(() => expect(screen.getByText("a.txt")).toBeTruthy());

    const callsBefore = invoke.mock.calls.filter(([cmd]) => cmd === "entries_for_paths").length;
    expect(listenHandlers.has("folder-watch")).toBe(true);
    // The tracked file itself changed on disk (e.g. it was recreated after a delete) — in scope.
    emitFake("folder-watch", [{ path: "C:\\d\\a.txt", kind: "modified" }]);

    await waitFor(() => {
      const callsAfter = invoke.mock.calls.filter(([cmd]) => cmd === "entries_for_paths").length;
      expect(callsAfter).toBeGreaterThan(callsBefore);
    }, { timeout: 2000 });
  });
});

describe("onDestroy releases the live-refresh debounce/listener even while still open (CPE-1633)", () => {
  it("cancels the pending debounce timer and unlistens folder-watch when App is destroyed mid-open", async () => {
    // Spy on the REAL global timer functions BEFORE mounting — `smartRefreshDebounce = new
    // TrailingDebounce(300)` captures `setTimeout`/`clearTimeout` as constructor-default-parameter
    // references at component-instance creation time, so a spy installed after `render(App)` would miss
    // it. No fake timers / no waiting out the 300ms window either — that's the slow, flaky pattern this
    // ticket exists to remove. We only need to observe whether the scheduled debounce timer gets
    // cleared, not let it actually fire.
    const setTimeoutSpy = vi.spyOn(window, "setTimeout");
    const clearTimeoutSpy = vi.spyOn(window, "clearTimeout");

    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    // Baseline BEFORE opening the search — `folder-watch` also has an independent consumer
    // (`folderWatch.ts`'s watch-rule execution, per CPE-1230's design note), so the fix must be checked
    // by diffing which handler the search's OWN listener added, not by asserting the whole event's
    // handler set goes to zero.
    const baselineHandlers = new Set(listenHandlers.get("folder-watch") ?? []);

    await fireEvent.click(await screen.findByText("Markdown docs"));
    await waitFor(() => expect(screen.getByText("keep.md")).toBeTruthy());

    // The live-refresh listener is armed while the structured search is open.
    const armedHandlers = listenHandlers.get("folder-watch")!;
    const smartRefreshHandler = [...armedHandlers].find((h) => !baselineHandlers.has(h));
    expect(smartRefreshHandler).toBeDefined(); // sanity: the search really armed its own listener

    setTimeoutSpy.mockClear();

    // Land a relevant folder-watch batch — this SCHEDULES the 300ms trailing debounce
    // (`smartRefreshDebounce.schedule(...)`) without firing it yet.
    emitFake("folder-watch", [{ path: "C:\\d\\new.md", kind: "created" }]);

    const debounceCallIndex = setTimeoutSpy.mock.calls.findIndex(([, delay]) => delay === 300);
    expect(debounceCallIndex).toBeGreaterThanOrEqual(0); // sanity: the debounce really was armed
    const debounceHandle = setTimeoutSpy.mock.results[debounceCallIndex]!.value;

    // Destroy the component WITHOUT closing the search first — mirrors a window close mid-navigation in
    // the real app, or @testing-library/svelte's own afterEach(cleanup()) in tests.
    cleanup();

    // The pending debounce timer must be cancelled on destroy...
    expect(clearTimeoutSpy).toHaveBeenCalledWith(debounceHandle);
    // ...and the search's OWN folder-watch listener must be released too (the unrelated baseline
    // consumer, if any, is out of scope for this ticket and is left alone).
    expect(armedHandlers.has(smartRefreshHandler!)).toBe(false);

    setTimeoutSpy.mockRestore();
    clearTimeoutSpy.mockRestore();
  });
});
