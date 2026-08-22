/**
 * CPE-1854: the status bar's two `currentPath`-DERIVED readouts — the git branch chip (CPE-462) and the
 * free/total disk figures (CPE-403) — pinned per view per readout, the same per-arm discipline CPE-1840
 * applied to the two listing-derived counts one field over.
 *
 * What was actually wrong (both halves are covered below):
 *
 *   1. REACTIVITY. `$: refreshGitStatus(currentPath)` read `isHome`/`archive` inside the FUNCTION BODY.
 *      Svelte tracks only the identifiers that appear IN a reactive statement, so neither was a
 *      dependency and the guard could only ever fire on the NEXT path change — never on entering a
 *      view. Entering an archive/smart folder/structured search does not change `currentPath`, so the
 *      chip simply stayed. `$: updateDiskSpace(currentPath, isHome, !!archive)` listed its identifiers,
 *      which is exactly why disk cleared on entering an archive and git did not: one character of
 *      discipline apart, one observable behaviour apart.
 *   2. MISSING ARMS. Neither `smartFolder` nor `structuredSearch` appeared in either guard, so both
 *      readouts survived into both virtual views regardless of reactivity.
 *   3. LATE RESULTS (found while fixing 1+2, not in the ticket). Both fetches are async and neither
 *      re-checked its assumptions at RESOLVE time. `updateDiskSpace` re-checked `currentPath === path`,
 *      which is not enough: opening a smart folder or a structured search leaves `currentPath` alone, so
 *      an in-flight response landed and repainted a readout the guard had already blanked.
 *   4. And `refreshGitStatus` had NO stale-response check AT ALL — which breaks the ORDINARY
 *      FOLDER-TO-FOLDER case, no virtual view required, and is the most user-visible defect fixed here.
 *      Navigate away from a slow repository (a large one, or one on a network share) before its
 *      `forge_repo_status` returns and the FIRST folder's branch painted over the second folder's, live
 *      Pull and Push buttons included. Found by the independent UAT, which measured it on a throwaway
 *      probe rather than reasoning about it. Four resolve-time tests at the bottom cover 3 and 4; the
 *      last of them is a regression guard for the check `updateDiskSpace` already had and nothing pinned.
 *
 * Why this matters more than a stale number: the git chip carries live **Pull / Push** buttons, so a
 * stale branch chip is a false statement sitting next to two actions. Every git case below therefore
 * asserts the BUTTONS are gone too, not just the branch name — that is the acceptance criterion about
 * "not actionable against a branch the chip is no longer describing", and the answer is structural: the
 * whole `{#if git && git.is_repo}` block (branch, counts, Pull, Push, Sync…) renders or does not.
 *
 * Every test is a BEFORE/AFTER pair inside one render: it first asserts the readout IS on screen in a
 * real folder (so the absence assertion can never pass vacuously — a broken harness that renders no
 * status bar at all fails at the first assertion), then enters the view and asserts it is gone. The two
 * folder-to-folder cases are stronger than absence: `expectGitChipPresent(branch)` asserts the OTHER
 * folder's branch is absent as well, so "the right chip" and "a chip" cannot be confused, and the two
 * folders carry deliberately different branch names and different free-space figures.
 *
 * Mutation-tested, one mutation at a time against the fixed code:
 *
 *   M1  restore the pre-fix git shape verbatim — `refreshGitStatus(path)` guarding on `!path || isHome
 *       || archive` read from the body, with `$: refreshGitStatus(currentPath);`  →  all three git-chip
 *       tests red. This is the ticket's named mutation: an identifier removed from the reactive
 *       statement with the guard body left intact, the shape that fails silently.
 *   M2  same shape on the disk side — `updateDiskSpace(path)` reading `pathReadoutsSuppressed` from the
 *       body, with `$: updateDiskSpace(currentPath);`  →  all three disk tests red.
 *   M3  drop the two virtual-view arms: `$: pathReadoutsSuppressed = isHome || !!archive;`  →  the four
 *       smart-folder/structured-search tests red, the two archive tests stay green.
 *   M4  drop the archive arm: `$: pathReadoutsSuppressed = isHome || !!smartFolder ||
 *       !!structuredSearch;`  →  exactly the two archive tests red.
 *   M5  delete `|| pathReadoutsSuppressed` from `refreshGitStatus`'s resolve-time re-check  →  the git
 *       smart-folder late-result test red.
 *   M6  delete `&& !pathReadoutsSuppressed` from `updateDiskSpace`'s resolve-time re-check  →  the disk
 *       smart-folder late-result test red.
 *   M7  delete `currentPath !== path ||` from `refreshGitStatus`'s resolve-time re-check — i.e. the
 *       pre-ticket state of that line, which had no path check at all  →  the git folder-to-folder test
 *       red, with C:\d's branch on screen while the user is in C:\d\photos.
 *   M8  delete `currentPath === path &&` from `updateDiskSpace`'s resolve-time re-check  →  the disk
 *       folder-to-folder test red.
 *
 * LIMIT, stated rather than worked around: jsdom does not apply component CSS under this project's
 * vitest config, so `getComputedStyle` reports nothing useful here and NOTHING in this file can check
 * layout, ordering, truncation or where the readouts sit in the bar. Every assertion is the presence or
 * absence of TEXT. The status bar's narrow-width behaviour is a separate concern (CPE-1836).
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

const listings: Record<string, DirEntry[]> = {
  "C:\\d": [entry("photos", "C:\\d\\photos", true)],
  "C:\\d\\photos": [entry("bundle.zip", "C:\\d\\photos\\bundle.zip", false, "zip")],
};

/** What `scan_tree` returns for the saved structured search — a name in NO real listing, so seeing it
 *  proves the search view is genuinely open rather than a folder still being shown. */
const scannedTree: TreeNode[] = [{ name: "found.md", isDir: false, size: 10, modified: 1_700_000_000_000 }];

const GB = 1024 * 1024 * 1024;
/** Per-path disk figures. Distinct per folder so an assertion names a SPECIFIC drive readout rather
 *  than "some free-space text", which is what makes the late-result test below meaningful. */
const DISK: Record<string, { free: number; total: number }> = {
  "C:\\d": { free: 7 * GB, total: 100 * GB }, //  "7.0 GB free of 100.0 GB"
  "C:\\d\\photos": { free: 3 * GB, total: 50 * GB }, //  "3.0 GB free of 50.0 GB"
};
const DRIVE_DISK = "7.0 GB free of 100.0 GB";
const PHOTOS_DISK = "3.0 GB free of 50.0 GB";

/** Per-folder branch names, both unique in the UI so `queryByText` for either is unambiguous — and
 *  DIFFERENT from each other, so a folder-to-folder test can say WHICH repo the chip is describing
 *  rather than only that some chip is present. */
const BRANCH_DRIVE = "hotfix/drive-root-7"; //  C:\d
const BRANCH_PHOTOS = "release/ledger-9"; //  C:\d\photos
const BRANCH_BY_PATH: Record<string, string> = {
  "C:\\d": BRANCH_DRIVE,
  "C:\\d\\photos": BRANCH_PHOTOS,
};

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

/** PER-PATH gates that make one folder's `forge_repo_status` / `disk_space` hang until released — the
 *  only way to stage a response that is STILL IN FLIGHT when the user has moved on. Per-path rather than
 *  global specifically so the folder-to-folder tests at the bottom can hold folder A's fetch while
 *  folder B's resolves normally, which is the ordering the real bug needs and a single shared gate
 *  cannot express. */
let gitHeld = new Set<string>();
let diskHeld = new Set<string>();
const gitReleases = new Map<string, () => void>();
const diskReleases = new Map<string, () => void>();

const holdGit = (path: string): void => void gitHeld.add(path);
const holdDisk = (path: string): void => void diskHeld.add(path);
function releaseGit(path: string): void {
  gitHeld.delete(path);
  gitReleases.get(path)?.();
  gitReleases.delete(path);
}
function releaseDisk(path: string): void {
  diskHeld.delete(path);
  diskReleases.get(path)?.();
  diskReleases.delete(path);
}

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  savedSearches.set([]);
  smartFolders.set([]);
  gitHeld = new Set();
  diskHeld = new Set();
  gitReleases.clear();
  diskReleases.clear();
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
        return { entries: listings[path] ?? [], filtered: 0, unreadable: 0 };
      }
      case "list_dir_stream": {
        const path = args.path as string;
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const list = listings[path] ?? [];
        if (list.length) ch.onmessage(list);
        return { total: list.length, filtered: 0, unreadable: 0 };
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
      // Every real folder in this harness is a repo, BOTH ahead and behind, so the chip renders its
      // branch name AND both Pull and Push buttons — the pair the acceptance criterion is about.
      case "forge_repo_status": {
        const path = args.path as string;
        if (gitHeld.has(path)) await new Promise<void>((r) => gitReleases.set(path, r));
        const branch = BRANCH_BY_PATH[path] ?? BRANCH_PHOTOS;
        return {
          is_repo: true, branch, upstream: `origin/${branch}`, ahead: 2, behind: 1,
          dirty: false, actions: ["pull-ff", "push"], up_to_date: false, conflicts_possible: false,
          blocked: null, warnings: [], conflicted: false,
        };
      }
      case "disk_space": {
        const path = args.path as string;
        if (diskHeld.has(path)) await new Promise<void>((r) => diskReleases.set(path, r));
        return DISK[path] ?? { free: 0, total: 0 };
      }
      default: return null;
    }
  });
});

/** The whole `{#if git && git.is_repo}` block, naming WHICH repo it is describing: the branch and the
 *  two live actions next to it, plus the other folder's branch explicitly absent — so "the right chip"
 *  and "a chip" can never be confused for one another. */
function expectGitChipPresent(branch: string): void {
  expect(screen.getByText(new RegExp(branch))).toBeTruthy();
  const other = branch === BRANCH_DRIVE ? BRANCH_PHOTOS : BRANCH_DRIVE;
  expect(screen.queryByText(new RegExp(other))).toBeNull();
  expect(screen.getByText("Pull")).toBeTruthy();
  expect(screen.getByText("Push")).toBeTruthy();
}
function expectGitChipAbsent(): void {
  expect(screen.queryByText(new RegExp(BRANCH_DRIVE))).toBeNull();
  expect(screen.queryByText(new RegExp(BRANCH_PHOTOS))).toBeNull();
  // AC: the actions must go with the statement they act on — a Pull/Push button surviving a suppressed
  // chip would operate on `currentPath`'s repo with nothing on screen naming it.
  expect(screen.queryByText("Pull")).toBeNull();
  expect(screen.queryByText("Push")).toBeNull();
  expect(screen.queryByText("Sync…")).toBeNull();
}

/** Home -> C:\d, then C:\d -> C:\d\photos: a real folder, two levels in, with `bundle.zip` on screen
 *  ready to be opened as an archive. Both readouts describe C:\d\photos on arrival. */
async function intoPhotos(): Promise<void> {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("photos")).toBeTruthy());
  await fireEvent.dblClick(screen.getByText("photos"));
  await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());
}

/** Open `bundle.zip` in place. `currentPath` stays C:\d\photos — which is precisely why the broken
 *  reactive statement never re-ran here. */
async function enterArchive(): Promise<void> {
  await fireEvent.dblClick(screen.getByText("bundle.zip"));
  await waitFor(() => expect(screen.getByText("inside.txt")).toBeTruthy());
}

/** Open a tag smart folder. Tagged AFTER mount because `initTags()` runs once on mount and would
 *  otherwise stomp a pre-mount tag; the tagged path is in no listing, so its row proves the view. */
async function enterSmartFolder(): Promise<void> {
  await setEntryTags("C:\\d\\tagged.txt", ["invoice"], "");
  saveSmartFolder("Invoices", "invoice");
  await fireEvent.click(await screen.findByText("Invoices"));
  await waitFor(() => expect(screen.getByText("tagged.txt")).toBeTruthy());
}

/** Open the saved structured search registered by the caller before render. */
async function enterStructuredSearch(): Promise<void> {
  await fireEvent.click(await screen.findByText("Markdown docs"));
  await waitFor(() => expect(screen.getByText("found.md")).toBeTruthy());
}

describe("the git branch chip never describes a folder that is not on screen (CPE-1854)", () => {
  it("archive: the chip and its Pull/Push actions go on entering the archive, not on the next navigation", async () => {
    await intoPhotos();
    await waitFor(() => expectGitChipPresent(BRANCH_PHOTOS));

    await enterArchive();

    // Under M1 this is where it fails: `currentPath` never changed, so the guard never re-ran and the
    // containing folder's branch (plus both buttons) is still sitting there over the zip's contents.
    await waitFor(() => expectGitChipAbsent());
  });

  it("smart folder: the chip and its Pull/Push actions go on opening the smart folder", async () => {
    await intoPhotos();
    await waitFor(() => expectGitChipPresent(BRANCH_PHOTOS));

    await enterSmartFolder();

    await waitFor(() => expectGitChipAbsent());
  });

  it("structured search: the chip and its Pull/Push actions go on opening the saved search", async () => {
    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    await intoPhotos();
    await waitFor(() => expectGitChipPresent(BRANCH_PHOTOS));

    await enterStructuredSearch();

    await waitFor(() => expectGitChipAbsent());
  });
});

describe("the free-space readout never describes a drive that is not on screen (CPE-1854)", () => {
  it("archive: the disk figures go on entering the archive", async () => {
    await intoPhotos();
    await waitFor(() => expect(screen.getByText(PHOTOS_DISK)).toBeTruthy());

    await enterArchive();

    await waitFor(() => expect(screen.queryByText(PHOTOS_DISK)).toBeNull());
    expect(screen.queryByText(DRIVE_DISK)).toBeNull(); // nor the previous folder's, which is the stale case
  });

  it("smart folder: the disk figures go on opening the smart folder", async () => {
    await intoPhotos();
    await waitFor(() => expect(screen.getByText(PHOTOS_DISK)).toBeTruthy());

    await enterSmartFolder();

    await waitFor(() => expect(screen.queryByText(PHOTOS_DISK)).toBeNull());
    expect(screen.queryByText(DRIVE_DISK)).toBeNull();
  });

  it("structured search: the disk figures go on opening the saved search", async () => {
    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    await intoPhotos();
    await waitFor(() => expect(screen.getByText(PHOTOS_DISK)).toBeTruthy());

    await enterStructuredSearch();

    await waitFor(() => expect(screen.queryByText(PHOTOS_DISK)).toBeNull());
    expect(screen.queryByText(DRIVE_DISK)).toBeNull();
  });
});

const DRIVE = "C:\\d";
const PHOTOS = "C:\\d\\photos";

/** Home -> C:\d only, without descending — the starting point for the resolve-time cases below. */
async function intoDrive(): Promise<void> {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("photos")).toBeTruthy());
}

/** C:\d -> C:\d\photos. Separate from `intoPhotos` because these tests must assert things BETWEEN the
 *  two navigations. */
async function descendToPhotos(): Promise<void> {
  await fireEvent.dblClick(screen.getByText("photos"));
  await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());
}

/**
 * The resolve-time half. Both fetches are async, and neither re-checked its assumptions when the
 * response finally came back.
 *
 * The FIRST TWO cases are the virtual-view ones: opening a smart folder or a structured search does NOT
 * change `currentPath`, so the `currentPath === path` re-check `updateDiskSpace` already had could not
 * tell "this response is for the folder I am still in" from "this response is for the folder I am still
 * in but am no longer LOOKING at".
 *
 * The LAST TWO are the ordinary-folder ones, and they are the most user-visible thing in this change —
 * they need no virtual view at all. `refreshGitStatus` had NO stale-response check whatsoever before
 * this ticket: navigate from a slow repository (a big one, or one on a network share) to another folder
 * before its `forge_repo_status` returns, and the FIRST folder's branch painted over the second one's,
 * with live Pull and Push buttons beside it. Plain folder-to-folder navigation, no archive, no smart
 * folder. Its disk twin is a REGRESSION GUARD rather than a fix: `updateDiskSpace`'s `currentPath ===
 * path` check already handled that case correctly, but nothing tested it, so nothing stopped it being
 * deleted — and the git side is the proof of what happens when that check is missing.
 *
 * All four are staged by holding one specific path's backend response (`gitHeld`/`diskHeld`) across the
 * navigation, then releasing it and letting the promise land.
 */
describe("an in-flight response cannot repaint a readout the guard already blanked (CPE-1854)", () => {
  it("git: a response that lands after the smart folder opens is dropped", async () => {
    await intoDrive();
    await waitFor(() => expectGitChipPresent(BRANCH_DRIVE)); // C:\d resolved normally: really there

    holdGit(PHOTOS); // C:\d\photos' status request will now hang
    await descendToPhotos();

    await enterSmartFolder();
    await waitFor(() => expectGitChipAbsent());

    releaseGit(PHOTOS); // the held request for C:\d\photos comes back NOW, inside the smart folder
    await new Promise((r) => setTimeout(r, 50));
    expectGitChipAbsent(); // red under M5: the chip reappears over the smart folder
  });

  it("disk: a response that lands after the smart folder opens is dropped", async () => {
    await intoDrive();
    await waitFor(() => expect(screen.getByText(DRIVE_DISK)).toBeTruthy());

    holdDisk(PHOTOS);
    await descendToPhotos();

    await enterSmartFolder();
    await waitFor(() => expect(screen.queryByText(DRIVE_DISK)).toBeNull());

    releaseDisk(PHOTOS);
    await new Promise((r) => setTimeout(r, 50));
    // Red under M6: C:\d\photos' figures land and paint over the smart folder, which describes no drive.
    expect(screen.queryByText(PHOTOS_DISK)).toBeNull();
    expect(screen.queryByText(DRIVE_DISK)).toBeNull();
  });

  // No virtual view anywhere in this one. C:\d is a slow repo; the user does not wait for it and moves
  // into C:\d\photos, whose own status returns immediately. When C:\d's response finally lands it must be
  // DROPPED — the chip is describing C:\d\photos now. Before this ticket `refreshGitStatus` had no such
  // check at all and `hotfix/drive-root-7` repainted over `release/ledger-9`, Pull and Push included.
  it("git: a slow folder's branch never repaints over the folder the user moved to", async () => {
    holdGit(DRIVE); // C:\d is the slow repository — hold it from the very first fetch
    await intoDrive();
    expectGitChipAbsent(); // nothing has resolved yet, so there is no chip to be right or wrong about

    await descendToPhotos();
    await waitFor(() => expectGitChipPresent(BRANCH_PHOTOS)); // the folder we are actually in

    releaseGit(DRIVE); // C:\d's status comes back now — for a folder we have left
    await new Promise((r) => setTimeout(r, 50));

    // `expectGitChipPresent` asserts the OTHER branch is absent, so this is the whole claim in one call:
    // the chip still names C:\d\photos and has not been overwritten by C:\d's late answer.
    expectGitChipPresent(BRANCH_PHOTOS);
  });

  // The disk twin of the case above. This one is a regression guard, not a fix: `updateDiskSpace` has
  // always re-checked `currentPath === path`. Nothing pinned it until now, which is exactly how the git
  // side ended up shipping without the equivalent line.
  it("disk: a slow folder's free-space figures never repaint over the folder the user moved to", async () => {
    holdDisk(DRIVE);
    await intoDrive();
    expect(screen.queryByText(DRIVE_DISK)).toBeNull();

    await descendToPhotos();
    await waitFor(() => expect(screen.getByText(PHOTOS_DISK)).toBeTruthy());

    releaseDisk(DRIVE);
    await new Promise((r) => setTimeout(r, 50));

    expect(screen.getByText(PHOTOS_DISK)).toBeTruthy();
    expect(screen.queryByText(DRIVE_DISK)).toBeNull();
  });
});
