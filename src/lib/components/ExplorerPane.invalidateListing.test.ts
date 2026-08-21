/**
 * CPE-1780 (F1): `revalidateDir`'s stale-while-revalidate background refresh (CPE-756) sets `entries` for
 * whichever navigation `gen` scheduled it — but `App.svelte`'s `loadPath` short-circuits HOME (and
 * `enterArchive`/`openSmartFolder`/`openStructuredSearch` never call `loadListing` at all) WITHOUT
 * touching this pane's `loadGen`. So a `revalidateDir` scheduled 300ms earlier (from a cache-hit load of
 * the PREVIOUS folder) could still fire after the caller moved on, pass its stale `gen === loadGen`
 * check, and silently reassign `entries` for a view that isn't showing that folder as a plain listing any
 * more. `invalidateListing()` (exported alongside `loadListing`) is the fix: App calls it at every one of
 * those transition points to bump `loadGen` without starting a new load.
 *
 * This tests the generation-gate at the level the bug actually lives — ExplorerPane's own
 * `loadGen`/`revalidateDir` — rather than through the full App.svelte integration.
 * `App.filteredHiddenNote.test.ts`'s "Ctrl+T" case already covers the FILTERED-COUNT symptom of this same
 * bug class at the App level; this covers `entries` itself, the deeper thing CPE-1708 only worked around
 * at its own point of consumption.
 *
 * Reviewer round 2 proved `invalidateListing()`'s FIRST implementation (a bare `loadGen++`) caused two
 * distinct regressions, both covered below:
 *   - Blocker 1: bumping `loadGen` while a `loadListing` was still awaiting `list_dir_stream` left
 *     `loading` stuck `true` forever — `loadListing`'s own `finally` guards on `gen === loadGen`, which
 *     the bump had just invalidated, and none of App's `exitSmartFolder`/`exitStructuredSearch`/
 *     `exitArchive` reload the plain listing to clear it either.
 *   - Blocker 2: `loadListing`'s CPE-665 cancel-the-previous-stream logic derived the id to cancel as
 *     `loadGen - 1`. A bare `loadGen++` burns a generation no stream ever used (a "phantom" generation),
 *     so the NEXT real load's cancel would target an id that was never used, leaving the REAL in-flight
 *     backend walk running to completion — defeating CPE-665.
 *
 * Mocking follows this repo's established component-test pattern (`InstantSearch.test.ts` /
 * `ExplorerPane.metaColumns.test.ts`): mock `@tauri-apps/api/core`'s `invoke`/`Channel` (both the typed
 * `commands.*` client and the raw `rawInvoke`/`createChannel` seam flow through it), and use fake timers
 * for the 300ms revalidate delay — `vi.advanceTimersByTimeAsync` still lets the mocked `list_dir`
 * promise's microtasks resolve, per that file's `settle()` comment.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import ExplorerPane from "./ExplorerPane.svelte";
import { emptySelection } from "../selection";
import type { DirEntry } from "../types";

const entryA: DirEntry = {
  name: "a.txt",
  path: "/a/a.txt",
  is_dir: false,
  size: 1,
  modified: 0,
  extension: "txt",
  hidden: false,
  is_symlink: false,
};

// The background revalidate's response — deliberately a DIFFERENT entry than what's cached, so any leak
// through a stale `gen` is unmistakable in the rendered rows.
const entryStale: DirEntry = {
  name: "stale-from-old-folder.txt",
  path: "/a/stale-from-old-folder.txt",
  is_dir: false,
  size: 2,
  modified: 0,
  extension: "txt",
  hidden: false,
  is_symlink: false,
};

/** The default `invoke` behaviour every test starts from (re-installed in `beforeEach` so a test that
 *  overrides it with `invoke.mockImplementation(...)` can never leak into a LATER test — `mockClear()`
 *  alone only clears call history, not an overridden implementation). */
function defaultInvokeImpl(cmd: string, args?: any): Promise<any> {
  if (cmd === "list_dir_stream") {
    args.onEntry.onmessage([entryA]);
    return Promise.resolve({ total: 1, filtered: 0, unreadable: 0 });
  }
  if (cmd === "list_dir") {
    return Promise.resolve({ entries: [entryStale], filtered: 0, unreadable: 0 });
  }
  if (cmd === "cancel_dir_stream") return Promise.resolve();
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
}

const invoke = vi.fn(defaultInvokeImpl);

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockReset();
  invoke.mockImplementation(defaultInvokeImpl);
});
afterEach(() => {
  vi.useRealTimers();
});

describe("ExplorerPane.invalidateListing (CPE-1780 F1)", () => {
  it("a revalidateDir scheduled before the caller leaves the listing cannot mutate entries once invalidateListing() runs", async () => {
    const { component } = render(ExplorerPane, { selection: emptySelection(), entries: [] });

    // First load: a real fetch, populating the LRU cache with entryA (loadListing's non-cache branch).
    await (component as any).loadListing("/a", false);
    expect(await screen.findByText("a.txt")).toBeTruthy();

    // Second load of the SAME folder from cache: paints entryA instantly, then schedules a
    // stale-while-revalidate `revalidateDir` 300ms out (CPE-756).
    await (component as any).loadListing("/a", true);
    expect(await screen.findByText("a.txt")).toBeTruthy();

    // The caller (App) now leaves this listing WITHOUT calling loadListing again — e.g. navigating to
    // Home — so it calls invalidateListing() instead (the fix under test).
    (component as any).invalidateListing();

    // Let the scheduled revalidateDir actually fire and its mocked "list_dir" promise resolve.
    await vi.advanceTimersByTimeAsync(400);

    // The stale revalidate's DIFFERENT entry must never land — entries stayed exactly what they were
    // when the caller left, not what a superseded background refresh (for a folder no longer being shown
    // as a plain listing) tried to set them to.
    expect(screen.queryByText("stale-from-old-folder.txt")).toBeNull();
    expect(screen.getByText("a.txt")).toBeTruthy();
  });

  // Sanity check proving the race is real (not a mock artifact that would pass either way): WITHOUT
  // invalidateListing(), the exact same scheduled revalidateDir DOES refresh entries.
  it("sanity: without invalidateListing(), the same scheduled revalidateDir DOES refresh entries", async () => {
    const { component } = render(ExplorerPane, { selection: emptySelection(), entries: [] });

    await (component as any).loadListing("/a", false);
    expect(await screen.findByText("a.txt")).toBeTruthy();
    await (component as any).loadListing("/a", true);
    expect(await screen.findByText("a.txt")).toBeTruthy();

    // No invalidateListing() call this time — same gen the revalidate was scheduled under.
    await vi.advanceTimersByTimeAsync(400);

    expect(await screen.findByText("stale-from-old-folder.txt")).toBeTruthy();
  });
});

describe("ExplorerPane.invalidateListing settles the pane (CPE-1780 Reviewer round 2, blocker 1)", () => {
  it("clears `loading` for a load that was still in flight when invalidateListing() bumped the generation", async () => {
    invoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === "list_dir_stream" && args.path === "/slow") {
        // The backend walk never resolves inside this test — "still in flight", exactly the moment a
        // real navigation (e.g. opening a smart folder) could interrupt it.
        return new Promise<never>(() => {});
      }
      return Promise.reject(new Error(`unexpected command in this test: ${cmd}`));
    });

    const { component } = render(ExplorerPane, { selection: emptySelection(), entries: [] });
    void (component as any).loadListing("/slow", false); // fire-and-forget: intentionally never resolves

    await tick();
    expect(screen.getByText("Loading…")).toBeTruthy(); // sanity: it really is showing "Loading…" right now

    // The user opens a smart folder (or archive/structured search) while this load is still in flight —
    // App calls invalidateListing() at that transition (CPE-1780 F1). Before this fix, `loadListing`'s
    // `finally { if (gen === loadGen) loading = false; }` would never run (`gen` no longer matches
    // `loadGen` once this bump happens), stranding the pane at "Loading…" forever — none of App's
    // `exitSmartFolder`/`exitStructuredSearch`/`exitArchive` reload the plain listing to clear it either.
    (component as any).invalidateListing();
    await tick();

    expect(screen.queryByText("Loading…")).toBeNull();
  });
});

describe("ExplorerPane.invalidateListing cancels the REAL in-flight stream (CPE-1780 Reviewer round 2, blocker 2)", () => {
  it("cancels the actual last-started stream id, not a loadGen-derived phantom one", async () => {
    let bArgs: { streamId: number; path: string } | null = null;
    invoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === "list_dir_stream") {
        if (args.path === "/a") {
          args.onEntry.onmessage([entryA]);
          return Promise.resolve({ total: 1, filtered: 0, unreadable: 0 });
        }
        if (args.path === "/b") {
          bArgs = args;
          return new Promise<never>(() => {}); // still walking when invalidateListing() fires below
        }
      }
      if (cmd === "cancel_dir_stream") return Promise.resolve();
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });

    const { component } = render(ExplorerPane, { selection: emptySelection(), entries: [] });

    // /a completes normally first — its own stream finishes on its own, so nothing is left owed a cancel.
    await (component as any).loadListing("/a", false);
    invoke.mockClear();

    // /b starts and its list_dir_stream call is STILL AWAITING (never resolves in this test) when the
    // user navigates away.
    void (component as any).loadListing("/b", false);
    await tick();
    expect(bArgs).toBeTruthy(); // sanity: /b's real stream really was started, and we captured its id

    (component as any).invalidateListing();

    const cancelCalls = invoke.mock.calls.filter(([cmd]) => cmd === "cancel_dir_stream");
    expect(cancelCalls).toHaveLength(1);
    expect(cancelCalls[0][1]).toEqual({ streamId: (bArgs as unknown as { streamId: number }).streamId });
  });
});
