/**
 * CPE-1380 — clipboard copy/cut/paste act on the correct pane. Before this fix `doCopy`/`doCut`/
 * `doPaste` unconditionally read pane A's `selectedEntries`/`currentPath`: with pane B active,
 * Ctrl+C/Ctrl+X staged pane A's selection instead of pane B's, and Ctrl+V (and a context-menu Paste
 * opened over pane B) always pasted into pane A's folder. The fix threads an `inPaneB` parameter through
 * all three, resolved the same way every other pane-routed action already is (CPE-1370/1377): the
 * keyboard path passes `dualPane && activePane === 1`, a context-menu invocation passes `ctx.inPaneB`
 * (menu-open time, independent of which pane is focused) via `runAction`'s `inPaneB` local.
 *
 * A cut+paste MOVE also needed its own post-move refresh (`refreshPasteAffectedPanes`): unlike
 * `dropInto`'s drag-drop (whose destination is always a child ROW inside the receiving pane, so only the
 * SOURCE pane ever needs reloading — `refreshDropSourcePane`, CPE-1371), a paste's destination IS the
 * target pane's own current-folder listing, so both the pane(s) that show the source's parent AND the
 * destination pane must reload, or one of them is left showing a stale/ghost view.
 *
 * Same mounted-App-with-mocked-backend dual-pane harness as App.deleteSnapshot.test.ts /
 * App.crossPaneDnd.test.ts / App.paneBContextMenu.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveDualPane, savePaneBPath } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const file = (name: string, dir: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: false,
  size: 10,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "txt",
  hidden: false,
  is_symlink: false,
});

const folder = (name: string, dir: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: true,
  size: 0,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "",
  hidden: false,
  is_symlink: false,
});

const PATH_A = "C:\\d";
const PATH_B = "C:\\dB";
const DEST_IN_A = `${PATH_A}\\destInA`;
const DEST_IN_B = `${PATH_B}\\destInB`;
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];

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

let moveEntriesCalls: { paths: string[]; dest: string }[] = [];
let startTransferCalls: { sources: string[]; dest: string; kind: string; policy: string; confirmed?: boolean }[] = [];

// Mutable "disk" state, keyed by folder path, so a move is reflected in the NEXT list_dir/list_dir_stream
// call — lets the tests prove a pane actually refreshed rather than only asserting the backend call args.
let disks: Record<string, DirEntry[]> = {};

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  moveEntriesCalls = [];
  startTransferCalls = [];
  disks = {
    [PATH_A]: [file("alpha.txt", PATH_A), folder("destInA", PATH_A)],
    [PATH_B]: [file("bravo.txt", PATH_B), folder("destInB", PATH_B)],
    [DEST_IN_A]: [],
    [DEST_IN_B]: [],
  };
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => disks[path as string] ?? [];
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: listingFor(args.path), filtered: 0 };
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      // Selecting a .txt row triggers the Preview pane's syntax-highlight fetch — default it to empty so
      // that unrelated async path doesn't throw (App.archivePassword.test.ts's same reasoning).
      case "read_file_text": return "";
      case "move_entries": {
        const paths = args.paths as string[];
        const dest = args.dest as string;
        moveEntriesCalls.push({ paths, dest });
        for (const p of paths) {
          const name = p.split("\\").pop() as string;
          for (const key of Object.keys(disks)) disks[key] = disks[key].filter((e) => e.path !== p);
          disks[dest] = [...(disks[dest] ?? []), file(name, dest)];
        }
        return paths.map((p) => ({ path: p, ok: true, error: "" }));
      }
      case "start_transfer": {
        const sources = args.sources as string[];
        const dest = args.dest as string;
        const kind = args.kind as string;
        const policy = args.policy as string;
        startTransferCalls.push({ sources, dest, kind, policy, confirmed: args.confirmed as boolean });
        return startTransferCalls.length; // fake transfer id
      }
      default: return null;
    }
  });
});

/** Boot straight into dual-pane, navigate pane A into its drive, wait for both panes to settle. Returns
 *  each pane's `.pane-col` wrapper — clicking it (not a row inside it, which stops propagation) is how
 *  the app itself sets `activePane` (same helper shape as App.deleteSnapshot.test.ts). */
async function bootDualPane() {
  saveDualPane(true);
  savePaneBPath(PATH_B);
  render(App);

  await waitFor(() => expect(screen.getByText("bravo.txt")).toBeTruthy());
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

  const paneAWrap = screen.getByText("alpha.txt").closest(".pane-col") as HTMLElement;
  const paneBWrap = screen.getByText("bravo.txt").closest(".pane-col") as HTMLElement;
  return { paneAWrap, paneBWrap };
}

describe("App — clipboard copy/cut/paste is pane-aware (CPE-1380)", () => {
  it("Ctrl+C then Ctrl+V while pane B is active copies pane B's selection into pane B's folder, not pane A's", async () => {
    const { paneBWrap } = await bootDualPane();

    await fireEvent.click(paneBWrap); // make pane B the active pane
    await fireEvent.click(screen.getByText("bravo.txt")); // select pane B's own file
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });

    // Navigate pane B into its own empty subfolder so the paste destination differs from bravo.txt's own
    // folder (pasting a COPY into the exact same folder it's already in is a real collision, not what
    // this test is proving — the routing is the point, not conflict handling).
    await fireEvent.dblClick(within(paneBWrap).getByText("destInB"));
    await waitFor(() => expect(within(paneBWrap).queryByText("bravo.txt")).toBeNull());

    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    await waitFor(() => expect(startTransferCalls.length).toBe(1));
    expect(startTransferCalls[0]).toEqual({
      sources: [`${PATH_B}\\bravo.txt`], // pane B's selection — NOT pane A's alpha.txt
      dest: DEST_IN_B, // pane B's CURRENT folder at paste time — NOT pane A's currentPath
      kind: "copy",
      policy: "keepboth",
      confirmed: false, // no dialog was shown and nothing is replaced — no consent claimed (CPE-1662)
    });
  });

  it("Ctrl+X then Ctrl+V while pane B is active MOVES the correct source into pane B's folder and refreshes both panes (no ghost)", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();

    // Cut FROM pane A (the default active pane — never clicked into here) while pane B is what will
    // become active for the paste, proving the CUT source and the PASTE destination are independently
    // pane-routed, not both forced onto whichever pane happens to be active at paste time.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha.txt"));
    await fireEvent.keyDown(window, { key: "x", ctrlKey: true });

    await fireEvent.click(paneBWrap);
    await fireEvent.dblClick(within(paneBWrap).getByText("destInB")); // navigate pane B into an empty subfolder
    await waitFor(() => expect(within(paneBWrap).queryByText("bravo.txt")).toBeNull());

    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    expect(moveEntriesCalls[0]).toEqual({ paths: [`${PATH_A}\\alpha.txt`], dest: DEST_IN_B });

    // Pane A (the SOURCE) refreshes — alpha.txt disappears from its folder view.
    await waitFor(() => expect(within(paneAWrap).queryByText("alpha.txt")).toBeNull());
    // Pane B (the DESTINATION, now showing destInB) refreshes and shows the moved-in file — no ghost,
    // no silent loss.
    await waitFor(() => expect(within(paneBWrap).queryByText("alpha.txt")).toBeTruthy());
  });

  it("both panes mirroring the cut source's folder refresh after the move — the source-side ghost is cleared even when the destination is a different pane", async () => {
    // Both panes start on the SAME folder (a common commander pattern: compare/sort one dir two ways).
    saveDualPane(true);
    savePaneBPath(PATH_A);
    render(App);
    await waitFor(() => expect(screen.getAllByText("alpha.txt").length).toBeGreaterThanOrEqual(1));
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getAllByText("alpha.txt").length).toBe(2));

    const paneAWrap = screen.getAllByText("alpha.txt")[0].closest(".pane-col") as HTMLElement;
    const paneBWrap = screen.getAllByText("alpha.txt")[1].closest(".pane-col") as HTMLElement;

    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha.txt"));
    await fireEvent.keyDown(window, { key: "x", ctrlKey: true });

    // Pane B navigates into destInA (still inside the mirrored folder) so the paste destination differs
    // from the source's parent — pasting cut items back into their own folder is a no-op the app refuses.
    await fireEvent.click(paneBWrap);
    await fireEvent.dblClick(within(paneBWrap).getByText("destInA"));
    await waitFor(() => expect(within(paneBWrap).queryByText("destInA")).toBeNull());

    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    expect(moveEntriesCalls[0]).toEqual({ paths: [`${PATH_A}\\alpha.txt`], dest: DEST_IN_A });

    // Pane A (still showing the mirrored parent folder) must lose alpha.txt — proving the source-side
    // refresh fires even though pane A never navigated anywhere itself.
    await waitFor(() => expect(within(paneAWrap).queryByText("alpha.txt")).toBeNull());
    // Pane B (now showing destInA) must gain it.
    await waitFor(() => expect(within(paneBWrap).queryByText("alpha.txt")).toBeTruthy());
  });

  it("single-pane mode: Ctrl+C/Ctrl+V still target pane A's own folder, ignoring a stray pane-B path from a previous session", async () => {
    saveDualPane(false); // dual-pane OFF
    savePaneBPath(PATH_B); // leftover state from a previous dual-pane session
    render(App);

    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

    await fireEvent.click(screen.getByText("alpha.txt"));
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });

    await fireEvent.dblClick(screen.getByText("destInA"));
    await waitFor(() => expect(screen.queryByText("alpha.txt")).toBeNull());

    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    await waitFor(() => expect(startTransferCalls.length).toBe(1));
    // The destination is pane A's OWN current folder (destInA, navigated above) — never PATH_B, even
    // though `paneBPath` still holds a leftover value, because `inPaneB` is gated on `dualPane` too.
    expect(startTransferCalls[0].dest).toBe(DEST_IN_A);
    expect(startTransferCalls[0].sources).toEqual([`${PATH_A}\\alpha.txt`]);
  });

  it("context-menu Copy on a pane-B row, then context-menu Paste on pane B's empty area, routes via ctx.inPaneB — not the live active pane", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    // Pane A stays the "active" pane throughout (never clicked into pane B) — proving the context-menu
    // path reads `ctx.inPaneB` (menu-open time), not `activePane` (focus time), same reasoning as
    // App.paneBContextMenu.test.ts's delete/rename/new-folder cases.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha.txt"));

    const bravoRow = within(paneBWrap).getByText("bravo.txt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    // Scoped to the popup itself — `CommandBar`'s toolbar has its own, always-rendered "Copy (Ctrl+C)"
    // icon button with the same title (App.paneBContextMenu.test.ts's "Rename" precedent).
    const copyMenu = within(await screen.findByRole("menu"));
    await fireEvent.click(copyMenu.getByTitle("Copy (Ctrl+C)"));

    // Navigate pane B (still not the "active" pane) into its own empty subfolder so the paste destination
    // differs from bravo.txt's own folder.
    await fireEvent.dblClick(within(paneBWrap).getByText("destInB"));
    await waitFor(() => expect(within(paneBWrap).queryByText("bravo.txt")).toBeNull());

    // destInB is empty, so FileList renders its `.empty-state` placeholder (not `.rows`) — both wire the
    // same `contextEmpty` handler (FileList.svelte).
    const paneBEmptyArea = paneBWrap.querySelector(".rows, .empty-state") as HTMLElement;
    await fireEvent.contextMenu(paneBEmptyArea);
    const pasteMenu = within(await screen.findByRole("menu"));
    await fireEvent.click(pasteMenu.getByText("Paste"));

    await waitFor(() => expect(startTransferCalls.length).toBe(1));
    expect(startTransferCalls[0]).toEqual({
      sources: [`${PATH_B}\\bravo.txt`], // pane B's row — NOT pane A's alpha.txt selection
      dest: DEST_IN_B, // pane B's current folder — NOT pane A's currentPath
      kind: "copy",
      policy: "keepboth",
      confirmed: false, // no dialog was shown and nothing is replaced — no consent claimed (CPE-1662)
    });
    // Pane A's selection (still alpha.txt) was never disturbed by any of this.
    expect(within(paneAWrap).getByText("alpha.txt").closest(".row")?.className).toContain("selected");
  });
});

/**
 * CPE-1662's consent, on the MAIN paste path. The PR #855 audit mutation-tested it and found that
 * inverting `resolveCopyConflict`'s `confirmed: true` left every frontend test green — only the
 * Drop-Stack twin of this handler was covered. The backend refuses `policy: "overwrite"` without the
 * flag, so an unpinned `true` here is the difference between the Replace button working and every
 * overwrite paste failing (or, inverted the other way, between asking and not asking).
 */
describe("App — the copy-conflict dialog is what supplies overwrite consent (CPE-1662)", () => {
  it("Replace sends policy overwrite WITH confirmed: true, and nothing is sent before the click", async () => {
    const { paneBWrap } = await bootDualPane();

    await fireEvent.click(paneBWrap);
    await fireEvent.click(screen.getByText("bravo.txt"));
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    // Paste into the SAME folder the file is already in — a genuine name collision, so the CPE-624
    // conflict dialog opens instead of the transfer starting.
    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    expect(await screen.findByText("Some items already exist")).toBeTruthy();
    // The user has been asked but has not answered: nothing may have reached the backend yet.
    expect(startTransferCalls.length).toBe(0);

    await fireEvent.click(screen.getByText("Replace"));

    await waitFor(() => expect(startTransferCalls.length).toBe(1));
    expect(startTransferCalls[0].policy).toBe("overwrite");
    expect(startTransferCalls[0].confirmed).toBe(true);
  });

  it("Skip answers the same dialog but claims no overwrite — consent travels, intent does not", async () => {
    const { paneBWrap } = await bootDualPane();

    await fireEvent.click(paneBWrap);
    await fireEvent.click(screen.getByText("bravo.txt"));
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    expect(await screen.findByText("Some items already exist")).toBeTruthy();
    await fireEvent.click(screen.getByText("Skip"));

    await waitFor(() => expect(startTransferCalls.length).toBe(1));
    // Consent is a SEPARATE argument from the policy (the CPE-1646 lesson): answering the dialog is
    // what sets it, and a non-destructive answer still destroys nothing regardless.
    expect(startTransferCalls[0].policy).toBe("skip");
    expect(startTransferCalls[0].confirmed).toBe(true);
  });

  it("cancelling the dialog sends nothing at all", async () => {
    const { paneBWrap } = await bootDualPane();

    await fireEvent.click(paneBWrap);
    await fireEvent.click(screen.getByText("bravo.txt"));
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    expect(await screen.findByText("Some items already exist")).toBeTruthy();
    await fireEvent.click(screen.getByText("Cancel"));

    await waitFor(() => expect(screen.queryByText("Some items already exist")).toBeNull());
    expect(startTransferCalls.length).toBe(0);
  });
});

describe("App — doPaste is re-entrancy-safe against a double-fire cut/move (CPE-1385)", () => {
  it("two rapid Ctrl+V after a Cut call moveEntries EXACTLY ONCE, not twice", async () => {
    saveDualPane(false);
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

    await fireEvent.click(screen.getByText("alpha.txt"));
    await fireEvent.keyDown(window, { key: "x", ctrlKey: true }); // cut

    // Navigate into an empty subfolder so the paste destination differs from the cut item's own folder.
    await fireEvent.dblClick(screen.getByText("destInA"));
    await waitFor(() => expect(screen.queryByText("alpha.txt")).toBeNull());

    // Swap in a move_entries handler backed by a promise WE control, so the assertion below can prove
    // the guard fires before the first paste even resolves — not just "eventually settles at 1".
    let resolveMove!: (v: { path: string; ok: boolean; error: string }[]) => void;
    const movePromise = new Promise<{ path: string; ok: boolean; error: string }[]>((res) => { resolveMove = res; });
    invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      const listingFor = (path: unknown) => disks[path as string] ?? [];
      switch (cmd) {
        case "special_folders": return [];
        case "list_drives": return drives;
        case "home_dir": return "C:\\Users\\t";
        case "can_restore_from_trash": return true;
        case "list_dir": return { entries: listingFor(args.path), filtered: 0 };
        case "list_dir_stream": {
          const ch = args.onEntry as { onmessage: (b: unknown) => void };
          const data = listingFor(args.path);
          ch.onmessage(data);
          return data.length;
        }
        case "parent_dir": return null;
        case "read_file_text": return "";
        case "move_entries": {
          const paths = args.paths as string[];
          const dest = args.dest as string;
          moveEntriesCalls.push({ paths, dest });
          // Mutate the "disk" the same way the default beforeEach handler does, so the eventual refresh
          // (once `movePromise` resolves below) shows the moved file — but return the CONTROLLED promise
          // itself, so the test can assert the call count before the frontend ever sees a result.
          for (const p of paths) {
            const name = p.split("\\").pop() as string;
            for (const key of Object.keys(disks)) disks[key] = disks[key].filter((e) => e.path !== p);
            disks[dest] = [...(disks[dest] ?? []), file(name, dest)];
          }
          return movePromise; // stays pending until we resolve it below
        }
        default: return null;
      }
    });

    // Two rapid Ctrl+V: `doPaste` is invoked fire-and-forget from the keydown handler (never awaited by
    // its caller), and `fireEvent.keyDown` dispatches synchronously — so NOT awaiting between these two
    // calls reproduces the exact CPE-1385 window: both `doPaste` invocations run their synchronous prefix
    // back-to-back, before either's `await commands.moveEntries(...)` has a chance to settle.
    fireEvent.keyDown(window, { key: "v", ctrlKey: true });
    fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    // Let any already-queued microtasks flush WITHOUT resolving `movePromise` — proves the second
    // `doPaste` already no-op'd via `clipEmpty(clipboard)` rather than merely "not yet reached" moveEntries.
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(moveEntriesCalls.length).toBe(1); // NOT 2 — the sync clear-before-await guard did its job

    resolveMove([{ path: `${DEST_IN_A}\\alpha.txt`, ok: true, error: "" }]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy()); // the single move completed + refreshed
  });

  it("paste-COPY is unaffected: the clipboard is NOT cleared after a copy, so a second Ctrl+V can repeat it", async () => {
    saveDualPane(false);
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

    await fireEvent.click(screen.getByText("alpha.txt"));
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true }); // COPY, not cut

    await fireEvent.dblClick(screen.getByText("destInA"));
    await waitFor(() => expect(screen.queryByText("alpha.txt")).toBeNull());

    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });
    await waitFor(() => expect(startTransferCalls.length).toBe(1));

    // A SECOND, separately-awaited Ctrl+V still starts another copy — proving CPE-1385's fix (which only
    // guards the cut/move branch) left copy-repeat intentionally intact.
    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });
    await waitFor(() => expect(startTransferCalls.length).toBe(2));
    expect(startTransferCalls[1]).toEqual({
      sources: [`${PATH_A}\\alpha.txt`],
      dest: DEST_IN_A,
      kind: "copy",
      policy: "keepboth",
      confirmed: false, // no dialog was shown and nothing is replaced — no consent claimed (CPE-1662)
    });
  });

  it("a FAILED move restores the cut clipboard (not just clears it) so a retry Ctrl+V re-attempts the same move, and shows the error notice", async () => {
    saveDualPane(false);
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

    await fireEvent.click(screen.getByText("alpha.txt"));
    await fireEvent.keyDown(window, { key: "x", ctrlKey: true }); // cut

    await fireEvent.dblClick(screen.getByText("destInA"));
    await waitFor(() => expect(screen.queryByText("alpha.txt")).toBeNull());

    // move_entries REJECTS every call in this test (e.g. permission denied / locked file) — the whole-
    // call-rejection case, where `moveEntries` never resolves with per-item results at all.
    invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      const listingFor = (path: unknown) => disks[path as string] ?? [];
      switch (cmd) {
        case "special_folders": return [];
        case "list_drives": return drives;
        case "home_dir": return "C:\\Users\\t";
        case "can_restore_from_trash": return true;
        case "list_dir": return { entries: listingFor(args.path), filtered: 0 };
        case "list_dir_stream": {
          const ch = args.onEntry as { onmessage: (b: unknown) => void };
          const data = listingFor(args.path);
          ch.onmessage(data);
          return data.length;
        }
        case "parent_dir": return null;
        case "read_file_text": return "";
        case "move_entries": {
          const paths = args.paths as string[];
          const dest = args.dest as string;
          moveEntriesCalls.push({ paths, dest });
          throw new Error("permission denied");
        }
        default: return null;
      }
    });

    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    // (a) the error notice shows.
    await waitFor(() => expect(screen.getByText((c) => c.includes("permission denied"))).toBeTruthy());
    expect(moveEntriesCalls.length).toBe(1);

    // (b) the clipboard is INTACT afterward — a subsequent Ctrl+V retries the SAME move rather than
    // silently no-op'ing. Pre-fix (clear-only, no restore), the clipboard stayed empty after the failure,
    // so this second paste's `clipEmpty(clipboard)` guard would have short-circuited it and
    // `moveEntriesCalls.length` would have stayed at 1.
    await fireEvent.keyDown(window, { key: "v", ctrlKey: true });
    await waitFor(() => expect(moveEntriesCalls.length).toBe(2));
    expect(moveEntriesCalls[1]).toEqual({ paths: [`${PATH_A}\\alpha.txt`], dest: DEST_IN_A });
  });
});
