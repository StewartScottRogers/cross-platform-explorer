/**
 * CPE-1840: the status bar's two listing-derived counts — `filteredHidden` ("N entries were hidden…",
 * CPE-1708) and `unreadableCount` ("Couldn't read N entries", CPE-1780) — had a genuinely pinned
 * FIRST-PAINT path (`App.filteredHiddenNote.test.ts`) and THREE staleness paths pinned nowhere. All
 * three mutations below are line-count-preserving and left the whole suite green before this file:
 *
 *   1. delete the cache-served reset (`ExplorerPane.svelte` cache branch: `filteredHidden = 0` /
 *      `unreadableCount = 0`) — a cache hit then keeps the PREVIOUS folder's count on screen;
 *   2. weaken the `<StatusBar>` gate in `App.svelte` from
 *      `isHome || archive || smartFolder || structuredSearch ? 0 : …` to `isHome ? 0 : …` — the count
 *      then survives into a view it does not describe;
 *   3. delete `filteredHidden = fresh.filtered` / `unreadableCount = fresh.unreadable` in
 *      `revalidateDir` — the count then never updates when the background re-list finds a new number.
 *
 * Why it is worth a test file of its own: these counts exist to stop the app making a FALSE STATEMENT
 * about a folder (the whole point of CPE-1708/CPE-1780 — a listing quietly shorter than the folder
 * really is). A count that is right on first paint and stale afterwards makes exactly that false
 * statement, invisibly, because the number still looks plausible.
 *
 * The gate is pinned PER ARM, not as a whole: a single test exercising only `isHome` (which is what
 * `App.filteredHiddenNote.test.ts`'s Ctrl+T cases are) is what left the archive, smart-folder and
 * structured-search arms uncovered in the first place. Four arms, four tests, both fields each.
 *
 * Harness: the same mocked-Tauri App-level harness as `App.filteredHiddenNote.test.ts` /
 * `App.archiveNav.test.ts`, extended with (a) per-path counts for the STREAM (first paint) and for
 * `list_dir` (the background revalidation), which are deliberately allowed to differ, and (b) a
 * `heldListDir` set that makes a chosen path's revalidation never resolve — so a "the note is gone"
 * assertion can only be satisfied by the cache-served RESET, never by a revalidation racing in behind it.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { savedSearches, addSavedSearch } from "./lib/savedSearchStore";
import { smartFolders, saveSmartFolder } from "./lib/smartFolders";
import { setEntryTags } from "./lib/tags";
import type { DirEntry, Place } from "./lib/types";
import type { TreeNode } from "./lib/bindings.gen";

const entry = (name: string, path: string, isDir: boolean, extension = ""): DirEntry => ({
  name,
  path,
  is_dir: isDir,
  size: isDir ? 0 : 1024,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension,
  hidden: false,
  is_symlink: false,
});

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

// Two real folders, so a navigation can go DOWN into a counted folder and BACK into an already-cached
// one — the only route that reaches `loadListing`'s cache branch (`goBack` passes useCache=true).
const listings: Record<string, DirEntry[]> = {
  "C:\\d": [entry("photos", "C:\\d\\photos", true)],
  "C:\\d\\photos": [entry("bundle.zip", "C:\\d\\photos\\bundle.zip", false, "zip")],
};

// What `scan_tree` returns for a structured search rooted at C:\d — a name that appears in NO real
// listing, so seeing it on screen proves we are in the virtual view and not still on a folder.
const scannedTree: TreeNode[] = [{ name: "found.md", isDir: false, size: 10, modified: 1_700_000_000_000 }];

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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

type Counts = { filtered: number; unreadable: number };

/** Counts the STREAM reports per path — the first-paint path (`list_dir_stream`'s terminal result). */
let streamCounts: Record<string, Counts> = {};
/** Counts `list_dir` reports per path — what the BACKGROUND revalidation finds. Deliberately separate
 *  from `streamCounts`: a folder whose contents changed since it was cached is exactly the case
 *  `revalidateDir`'s count refresh exists for, and the only way to tell a refreshed count apart from a
 *  remembered one is to make the two numbers different. */
let freshCounts: Record<string, Counts> = {};
/** Paths whose `list_dir` never resolves, so a cache-served view stays cache-served for the whole test. */
let heldListDir = new Set<string>();

const countsFor = (table: Record<string, Counts>, path: string): Counts =>
  table[path] ?? { filtered: 0, unreadable: 0 };

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  savedSearches.set([]);
  smartFolders.set([]);
  streamCounts = {};
  freshCounts = {};
  heldListDir = new Set();
  Element.prototype.scrollIntoView = vi.fn();

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": {
        const path = args.path as string;
        // Never resolves — the caller's `await` simply never returns, which is what "the background
        // revalidation has not come back yet" looks like to the pane.
        if (heldListDir.has(path)) await new Promise<never>(() => {});
        const c = freshCounts[path] ?? countsFor(streamCounts, path);
        return { entries: listings[path] ?? [], filtered: c.filtered, unreadable: c.unreadable };
      }
      case "list_dir_stream": {
        const path = args.path as string;
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const list = listings[path] ?? [];
        if (list.length) ch.onmessage(list);
        const c = countsFor(streamCounts, path);
        return { total: list.length, filtered: c.filtered, unreadable: c.unreadable };
      }
      case "parent_dir": return null;
      case "read_archive_entries": return [{ name: "inside.txt", size: 5, is_dir: false }];
      case "read_file_text": return "";
      case "scan_tree": return scannedTree;
      case "entries_for_paths": {
        const paths = (args.paths as string[] | undefined) ?? [];
        return paths.map((p) => entry(p.split("\\").pop() ?? p, p, false, "txt"));
      }
      case "set_tags": {
        const path = args.path as string;
        return { [path]: { tags: (args.tags as string[] | undefined) ?? [], label: (args.label as string) ?? "" } };
      }
      default: return null;
    }
  });
});

const FILTERED_NOTE = /entries were hidden because their names could not be shown safely/;
const UNREADABLE_NOTE = /Couldn.t read \d+ entr/;

/** Home -> C:\d (fresh listing, cached on completion). */
async function intoDrive(): Promise<void> {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("photos")).toBeTruthy());
}

/** C:\d -> C:\d\photos (fresh listing; this is the folder carrying the non-zero counts). */
async function intoPhotos(): Promise<void> {
  await fireEvent.dblClick(screen.getByText("photos"));
  await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());
}

/** Back to C:\d — `goBack` passes useCache=true, so this is served from the pane's LRU cache. */
async function backToDrive(): Promise<void> {
  await fireEvent.click(screen.getByTitle(/Alt\+Left/));
  await waitFor(() => expect(screen.queryByText("bundle.zip")).toBeNull());
}

/** Both counts non-zero on C:\d\photos, zero on C:\d — the setup every case below shares. */
function countPhotos(filtered: number, unreadable: number): void {
  streamCounts["C:\\d"] = { filtered: 0, unreadable: 0 };
  streamCounts["C:\\d\\photos"] = { filtered, unreadable };
}

describe("a cache-served paint never inherits the previous folder's counts (CPE-1840)", () => {
  // Mutation this reds under: delete `filteredHidden = 0;` from ExplorerPane.svelte's cache branch
  // (~line 379). C:\d's revalidation is HELD, so nothing else can clear the note — if the reset is
  // gone, "2 entries were hidden…" (a fact about C:\d\photos) stays on screen over C:\d.
  it("filteredHidden: a cache hit shows no filtered note at all until its own revalidation returns", async () => {
    countPhotos(2, 0);
    heldListDir.add("C:\\d");

    await intoDrive();
    await intoPhotos();
    await waitFor(() => expect(screen.getByText(FILTERED_NOTE)).toBeTruthy());

    await backToDrive();
    expect(screen.queryByText(FILTERED_NOTE)).toBeNull();

    // Past the 300ms stale-while-revalidate timer: the revalidation has been ISSUED and is hanging, so
    // the note's absence can only be the cache branch's own reset, not a refresh racing in behind it.
    await new Promise((r) => setTimeout(r, 400));
    expect(screen.queryByText(FILTERED_NOTE)).toBeNull();
  });

  // Mutation: delete `unreadableCount = 0;` from the same cache branch (~line 380).
  it("unreadableCount: a cache hit shows no unreadable note at all until its own revalidation returns", async () => {
    countPhotos(0, 3);
    heldListDir.add("C:\\d");

    await intoDrive();
    await intoPhotos();
    await waitFor(() => expect(screen.getByText(UNREADABLE_NOTE)).toBeTruthy());

    await backToDrive();
    expect(screen.queryByText(UNREADABLE_NOTE)).toBeNull();

    await new Promise((r) => setTimeout(r, 400));
    expect(screen.queryByText(UNREADABLE_NOTE)).toBeNull();
  });
});

describe("the background revalidation refreshes both counts (CPE-1840)", () => {
  // Mutation: delete `filteredHidden = fresh.filtered;` from `revalidateDir` (~line 341). The cached
  // view then sits on 0 forever, silently under-reporting a folder that now hides 4 names.
  it("filteredHidden: a cache-served folder picks up the count its re-list actually finds", async () => {
    countPhotos(0, 0);
    freshCounts["C:\\d"] = { filtered: 4, unreadable: 0 }; // what the re-list finds, unlike the cached paint

    await intoDrive();
    await intoPhotos();
    await backToDrive();
    expect(screen.queryByText(FILTERED_NOTE)).toBeNull(); // cache-served: 0 (unknown) for now

    await waitFor(
      () => expect(screen.getByText("4 entries were hidden because their names could not be shown safely")).toBeTruthy(),
      { timeout: 3000 },
    );
  });

  // Mutation: delete `unreadableCount = fresh.unreadable;` from `revalidateDir` (~line 342).
  it("unreadableCount: a cache-served folder picks up the count its re-list actually finds", async () => {
    countPhotos(0, 0);
    freshCounts["C:\\d"] = { filtered: 0, unreadable: 6 };

    await intoDrive();
    await intoPhotos();
    await backToDrive();
    expect(screen.queryByText(UNREADABLE_NOTE)).toBeNull();

    await waitFor(() => expect(screen.getByText("Couldn't read 6 entries")).toBeTruthy(), { timeout: 3000 });
  });
});

/**
 * The `<StatusBar>` gate, one test per arm. Every arm is entered from a folder whose counts are BOTH
 * non-zero and which none of these views reloads — `enterArchive`, `openSmartFolder` and
 * `openStructuredSearch` never call `loadPath`/`loadListing`, so `filteredHidden`/`unreadableCount` are
 * still 2/3 in App's state throughout. The gate at the point of consumption is therefore the ONLY thing
 * that can clear the notes, which is what makes each of these tests red under mutation 2 rather than
 * passing on a reload that would have zeroed the counts anyway.
 */
describe("the <StatusBar> gate, pinned per arm (CPE-1840)", () => {
  async function intoCountedFolder(): Promise<void> {
    countPhotos(2, 3);
    await intoDrive();
    await intoPhotos();
    await waitFor(() => expect(screen.getByText(FILTERED_NOTE)).toBeTruthy());
    expect(screen.getByText(UNREADABLE_NOTE)).toBeTruthy();
  }

  // isHome arm. Mirrors App.filteredHiddenNote.test.ts's Ctrl+T cases; kept here so all four arms of the
  // one expression are pinned in one place and a future reader can see the set is complete.
  it("Home: neither count survives into a view with no listing at all", async () => {
    await intoCountedFolder();

    await fireEvent.keyDown(window, { key: "t", ctrlKey: true });
    await waitFor(() => expect(screen.queryByText("bundle.zip")).toBeNull());

    expect(screen.queryByText(FILTERED_NOTE)).toBeNull();
    expect(screen.queryByText(UNREADABLE_NOTE)).toBeNull();
  });

  // archive arm.
  it("archive: neither count survives into an in-place archive browse-view", async () => {
    await intoCountedFolder();

    await fireEvent.dblClick(screen.getByText("bundle.zip"));
    await waitFor(() => expect(screen.getByText("inside.txt")).toBeTruthy()); // really inside the zip

    expect(screen.queryByText(FILTERED_NOTE)).toBeNull();
    expect(screen.queryByText(UNREADABLE_NOTE)).toBeNull();
  });

  // smart-folder arm (tag-only virtual folder).
  it("smart folder: neither count survives into a tag smart folder", async () => {
    await intoCountedFolder();

    // Tag AFTER mount — `initTags()` runs once on mount and would otherwise stomp a pre-mount tag.
    // The tagged path is in NO listing, so seeing its row proves we're in the virtual view.
    await setEntryTags("C:\\d\\tagged.txt", ["invoice"], "");
    saveSmartFolder("Invoices", "invoice");

    await fireEvent.click(await screen.findByText("Invoices"));
    await waitFor(() => expect(screen.getByText("tagged.txt")).toBeTruthy());

    expect(screen.queryByText(FILTERED_NOTE)).toBeNull();
    expect(screen.queryByText(UNREADABLE_NOTE)).toBeNull();
  });

  // structured-search arm (saved `Condition[]` query opened as a virtual listing).
  it("structured search: neither count survives into an open saved search", async () => {
    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    await intoCountedFolder();

    await fireEvent.click(await screen.findByText("Markdown docs"));
    await waitFor(() => expect(screen.getByText("found.md")).toBeTruthy()); // really in the search view

    expect(screen.queryByText(FILTERED_NOTE)).toBeNull();
    expect(screen.queryByText(UNREADABLE_NOTE)).toBeNull();
  });
});
