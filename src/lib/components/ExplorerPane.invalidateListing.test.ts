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
 * Mocking follows this repo's established component-test pattern (`InstantSearch.test.ts` /
 * `ExplorerPane.metaColumns.test.ts`): mock `@tauri-apps/api/core`'s `invoke`/`Channel` (both the typed
 * `commands.*` client and the raw `rawInvoke`/`createChannel` seam flow through it), and use fake timers
 * for the 300ms revalidate delay — `vi.advanceTimersByTimeAsync` still lets the mocked `list_dir`
 * promise's microtasks resolve, per that file's `settle()` comment.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
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

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "list_dir_stream") {
    args.onEntry.onmessage([entryA]);
    return Promise.resolve({ total: 1, filtered: 0, unreadable: 0 });
  }
  if (cmd === "list_dir") {
    return Promise.resolve({ entries: [entryStale], filtered: 0, unreadable: 0 });
  }
  if (cmd === "cancel_dir_stream") return Promise.resolve();
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockClear();
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
