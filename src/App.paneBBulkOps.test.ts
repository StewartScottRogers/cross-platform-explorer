/**
 * CPE-1384 — bulk ops invoked from a pane-B context menu still operated on pane A's selection/folder:
 * `duplicate`, `batch-rename`, `batch-media`, `copy-to`/`move-to` either did nothing useful (a pane-B
 * "Batch media…" row was hidden entirely) or silently acted on pane A's live `selectedEntries`/
 * `currentPath` regardless of which pane the menu was opened over. The fix threads `inPaneB` through each
 * (`runAction`'s already-computed local, same routing every other pane-aware case in the switch uses,
 * CPE-1370/1377/1380) and — for the two that open a dialog/native picker while their target could change
 * underneath them (`beginBatchRename`/`beginBatchMedia`/`copyMoveToFolder`) — SNAPSHOTS the target pane +
 * source paths at invocation time, mirroring `askDelete`'s `snapshotConfirmTarget` (CPE-1370 review), so a
 * pane switch while the dialog/picker is open can never retarget the eventual op onto the other pane.
 *
 * `compress`/`extract`/`archive-safety`/`vault-create`/`shred` stayed pane-A-only in THIS ticket (already
 * correctly hidden from a pane-B menu via `ContextMenu`'s existing `!ctxInPaneB`-gated props) — they queue
 * through the global `pendingArchiveOps`/transfer-done machinery, which wasn't pane-routed yet; left as a
 * follow-up. That follow-up is CPE-1386 (see App.paneBArchiveVault.test.ts) — done now.
 *
 * Same mounted-App-with-mocked-backend dual-pane harness as App.clipboardPaneRouting.test.ts /
 * App.deleteSnapshot.test.ts / App.paneBContextMenu.test.ts.
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
  extension: name.split(".").pop() ?? "",
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
const { openDialog } = vi.hoisted(() => ({ openDialog: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => `asset://${p}`, Channel }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialog, save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

let copyEntriesCalls: { paths: string[]; dest: string }[] = [];
let moveEntriesCalls: { paths: string[]; dest: string }[] = [];
let moveExactCalls: { pairs: [string, string][] }[] = [];
let startTransferCalls: { sources: string[]; dest: string; kind: string; policy: string }[] = [];

// Mutable "disk" state, keyed by folder path, so a move/rename is reflected in the NEXT list_dir call —
// lets the tests prove the right pane actually refreshed, not just that the backend call args were right.
let disks: Record<string, DirEntry[]> = {};

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  copyEntriesCalls = [];
  moveEntriesCalls = [];
  moveExactCalls = [];
  startTransferCalls = [];
  openDialog.mockReset();
  disks = {
    [PATH_A]: [file("alpha1.png", PATH_A), file("alpha2.png", PATH_A), folder("destInA", PATH_A)],
    [PATH_B]: [file("bravo1.png", PATH_B), file("bravo2.png", PATH_B), folder("destInB", PATH_B)],
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
      case "list_dir": return listingFor(args.path);
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "copy_entries": {
        const paths = args.paths as string[];
        const dest = args.dest as string;
        copyEntriesCalls.push({ paths, dest });
        for (const p of paths) {
          const name = p.split("\\").pop() as string;
          disks[dest] = [...(disks[dest] ?? []), file(name + ".copy", dest)];
        }
        return paths.map((p) => ({ path: p, ok: true, error: "" }));
      }
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
      case "move_exact": {
        const pairs = args.pairs as [string, string][];
        moveExactCalls.push({ pairs });
        for (const [from, to] of pairs) {
          const dir = from.slice(0, from.lastIndexOf("\\"));
          const newName = to.split("\\").pop() as string;
          disks[dir] = disks[dir].map((e) => (e.path === from ? { ...e, path: to, name: newName } : e));
        }
        return pairs.map(([, to]) => ({ path: to, ok: true, error: "" }));
      }
      case "start_transfer": {
        const sources = args.sources as string[];
        const dest = args.dest as string;
        const kind = args.kind as string;
        const policy = args.policy as string;
        startTransferCalls.push({ sources, dest, kind, policy });
        return startTransferCalls.length; // fake transfer id
      }
      default: return null;
    }
  });
});

/** Boot straight into dual-pane, navigate pane A into its drive, wait for both panes to settle. Returns
 *  each pane's `.pane-col` wrapper — same helper shape as App.clipboardPaneRouting.test.ts. */
async function bootDualPane() {
  saveDualPane(true);
  savePaneBPath(PATH_B);
  render(App);

  await waitFor(() => expect(screen.getByText("bravo1.png")).toBeTruthy());
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha1.png")).toBeTruthy());

  const paneAWrap = screen.getByText("alpha1.png").closest(".pane-col") as HTMLElement;
  const paneBWrap = screen.getByText("bravo1.png").closest(".pane-col") as HTMLElement;
  return { paneAWrap, paneBWrap };
}

describe("App — Duplicate is pane-aware (CPE-1384)", () => {
  it("Duplicate from a pane-B context menu duplicates pane B's file into pane B's folder, not pane A's", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png")); // pane A has its own selection

    const bravoRow = within(paneBWrap).getByText("bravo1.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Duplicate"));

    await waitFor(() => expect(copyEntriesCalls.length).toBe(1));
    expect(copyEntriesCalls[0]).toEqual({ paths: [`${PATH_B}\\bravo1.png`], dest: PATH_B });
    // Pane B refreshed and shows the duplicate; pane A is untouched.
    await waitFor(() => expect(within(paneBWrap).queryByText("bravo1.png.copy")).toBeTruthy());
    expect(within(paneAWrap).queryByText("alpha1.png.copy")).toBeNull();
  });

  it("single-pane mode: Ctrl+D still duplicates pane A's own selection", async () => {
    saveDualPane(false);
    savePaneBPath(PATH_B); // leftover state from a previous dual-pane session
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha1.png")).toBeTruthy());

    await fireEvent.click(screen.getByText("alpha1.png"));
    await fireEvent.keyDown(window, { key: "d", ctrlKey: true });

    await waitFor(() => expect(copyEntriesCalls.length).toBe(1));
    expect(copyEntriesCalls[0]).toEqual({ paths: [`${PATH_A}\\alpha1.png`], dest: PATH_A });
  });
});

describe("App — Batch rename is pane-aware (CPE-1384)", () => {
  it("Batch-rename from a pane-B context menu renames pane B's selection within pane B's folder", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    await fireEvent.click(within(paneBWrap).getByText("bravo1.png"));
    await fireEvent.click(within(paneBWrap).getByText("bravo2.png"), { ctrlKey: true });

    const bravoRow = within(paneBWrap).getByText("bravo2.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Rename…"));

    const dialog = await screen.findByRole("dialog");
    // "Add text" mode with a prefix guarantees every previewed name actually changes.
    await fireEvent.click(within(dialog).getByText("Add text"));
    await fireEvent.input(within(dialog).getByLabelText("Prefix"), { target: { value: "x-" } });
    await fireEvent.click(within(dialog).getByText("Rename"));

    await waitFor(() => expect(moveExactCalls.length).toBe(1));
    const pairs = moveExactCalls[0].pairs;
    expect(pairs.every(([from]) => from.startsWith(PATH_B))).toBe(true); // pane B's folder, not pane A's
    expect(pairs.some(([from]) => from === `${PATH_B}\\bravo1.png`)).toBe(true);
    expect(pairs.some(([from]) => from === `${PATH_B}\\bravo2.png`)).toBe(true);
    // Pane A's file list was never touched by this op.
    expect(within(paneAWrap).getByText("alpha1.png")).toBeTruthy();
  });

  it("snapshot-safe: switching the active pane while the batch-rename dialog is open doesn't retarget the rename", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();

    await fireEvent.click(within(paneBWrap).getByText("bravo1.png"));
    await fireEvent.click(within(paneBWrap).getByText("bravo2.png"), { ctrlKey: true });
    const bravoRow = within(paneBWrap).getByText("bravo2.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Rename…"));
    const dialog = await screen.findByRole("dialog");

    // The active pane changes WHILE the dialog is still open (e.g. the user clicks into pane A).
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    await fireEvent.click(within(dialog).getByText("Add text"));
    await fireEvent.input(within(dialog).getByLabelText("Prefix"), { target: { value: "x-" } });
    await fireEvent.click(within(dialog).getByText("Rename"));

    await waitFor(() => expect(moveExactCalls.length).toBe(1));
    // Still pane B's files — the pane switch after the dialog opened must not retarget it onto pane A's
    // alpha1.png/alpha2.png (CPE-1370's snapshot-at-open-time reasoning, applied here to CPE-1384).
    const pairs = moveExactCalls[0].pairs;
    expect(pairs.every(([from]) => from.startsWith(PATH_B))).toBe(true);
  });

  it("single-pane mode: batch-rename still targets pane A's own folder", async () => {
    saveDualPane(false);
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha1.png")).toBeTruthy());

    await fireEvent.click(screen.getByText("alpha1.png"));
    await fireEvent.click(screen.getByText("alpha2.png"), { ctrlKey: true });
    const row = screen.getByText("alpha2.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(row);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Rename…"));
    const dialog = await screen.findByRole("dialog");
    await fireEvent.click(within(dialog).getByText("Add text"));
    await fireEvent.input(within(dialog).getByLabelText("Prefix"), { target: { value: "x-" } });
    await fireEvent.click(within(dialog).getByText("Rename"));

    await waitFor(() => expect(moveExactCalls.length).toBe(1));
    expect(moveExactCalls[0].pairs.every(([from]) => from.startsWith(PATH_A))).toBe(true);
  });

  it("post-apply refresh targets the SNAPSHOT folder the rename actually ran in, not pane B's live path if it was renavigated while the dialog was open (CPE-1387)", async () => {
    // Mirror both panes onto the SAME folder (PATH_A) — a common commander pattern — so the bug is
    // externally observable: pre-fix, the refresh only ever reloaded whichever LIVE path the target pane
    // happened to be on (`paneBPath`/`currentPath`), never the snapshotted `dir`. Opening the dialog from
    // pane B, then renavigating pane B elsewhere, meant the refresh reloaded pane B's NEW (unrelated,
    // un-renamed) folder while pane A — still showing the actually-renamed PATH_A — never refreshed at
    // all and kept displaying the stale, pre-rename names.
    saveDualPane(true);
    savePaneBPath(PATH_A);
    render(App);
    await waitFor(() => expect(screen.getAllByText("alpha1.png").length).toBeGreaterThanOrEqual(1));
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getAllByText("alpha1.png").length).toBe(2));

    const [paneAWrap, paneBWrap] = screen.getAllByText("alpha1.png").map(
      (el) => el.closest(".pane-col") as HTMLElement,
    );

    // Open the batch-rename dialog FROM pane B, targeting the mirrored folder (PATH_A) — `beginBatchRename`
    // snapshots `dir: paneBPath` (= PATH_A) right now.
    await fireEvent.click(within(paneBWrap).getByText("alpha1.png"));
    await fireEvent.click(within(paneBWrap).getByText("alpha2.png"), { ctrlKey: true });
    const bravoRow = within(paneBWrap).getByText("alpha2.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Rename…"));
    const dialog = await screen.findByRole("dialog");

    // WHILE the dialog is still open, renavigate pane B away into its own empty subfolder — live
    // `paneBPath` now names a folder the rename never touched.
    await fireEvent.dblClick(within(paneBWrap).getByText("destInA"));
    await waitFor(() => expect(within(paneBWrap).queryByText("alpha1.png")).toBeNull());

    await fireEvent.click(within(dialog).getByText("Add text"));
    await fireEvent.input(within(dialog).getByLabelText("Prefix"), { target: { value: "x-" } });
    await fireEvent.click(within(dialog).getByText("Rename"));

    await waitFor(() => expect(moveExactCalls.length).toBe(1));
    expect(moveExactCalls[0].pairs.every(([from]) => from.startsWith(PATH_A))).toBe(true);

    // Pane A — which still shows the actually-renamed folder (PATH_A) — refreshes and shows the new
    // names. Pre-fix this never happened (the refresh only ever touched whatever pane B's LIVE path was).
    await waitFor(() => expect(within(paneAWrap).queryByText("x-alpha1.png")).toBeTruthy());
    // Pane B stays exactly where the user renavigated it (destInA, still empty) — its own navigation is
    // respected, not silently reloaded with the (unrelated) rename result.
    expect(within(paneBWrap).queryByText("x-alpha1.png")).toBeNull();
  });
});

describe("App — Batch media is pane-aware (CPE-1384)", () => {
  it("'Batch media…' is now offered from a pane-B context menu (previously hidden) and opens with pane B's own files", async () => {
    const { paneBWrap } = await bootDualPane();

    await fireEvent.click(within(paneBWrap).getByText("bravo1.png"));
    await fireEvent.click(within(paneBWrap).getByText("bravo2.png"), { ctrlKey: true });
    const bravoRow = within(paneBWrap).getByText("bravo2.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    // Pre-fix, `mediaEligible` was forced off for a pane-B-opened menu, so this row never appeared.
    expect(menu.getByText("Batch media…")).toBeTruthy();
    await fireEvent.click(menu.getByText("Batch media…"));

    const dialog = await screen.findByRole("dialog");
    // The dialog only ever received the COUNT, not which pane — proving it's fed pane B's own eligible
    // selection (2 files) rather than an empty/stale pane-A one is the only externally-observable signal
    // `beginBatchMedia`'s snapshot gives us here (the full paths never render in the dialog's own DOM).
    expect(within(dialog).getByText("Batch media — 2 files")).toBeTruthy();
  });
});

describe("App — Copy to folder… / Move to folder… are pane-aware (CPE-1384)", () => {
  it("'Copy to folder…' from a pane-B context menu copies pane B's selection, not pane A's", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    openDialog.mockResolvedValue(DEST_IN_A); // pick an arbitrary destination outside either pane's view
    const bravoRow = within(paneBWrap).getByText("bravo1.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    // Pre-fix, `canTerminal` (and thus this row) was forced off for a pane-B-opened menu.
    expect(menu.getByText("Copy to folder…")).toBeTruthy();
    await fireEvent.click(menu.getByText("Copy to folder…"));

    await waitFor(() => expect(startTransferCalls.length).toBe(1));
    expect(startTransferCalls[0]).toEqual({
      sources: [`${PATH_B}\\bravo1.png`], // pane B's row — NOT pane A's alpha1.png selection
      dest: DEST_IN_A,
      kind: "copy",
      policy: "keepboth",
    });
  });

  it("'Move to folder…' from a pane-B context menu moves pane B's selection and refreshes pane B (not pane A)", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    openDialog.mockResolvedValue(DEST_IN_A);
    const bravoRow = within(paneBWrap).getByText("bravo1.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Move to folder…"));

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    expect(moveEntriesCalls[0]).toEqual({ paths: [`${PATH_B}\\bravo1.png`], dest: DEST_IN_A });
    // Pane B (the source) refreshes — bravo1.png disappears from its own listing.
    await waitFor(() => expect(within(paneBWrap).queryByText("bravo1.png")).toBeNull());
    // Pane A's own listing/selection is untouched.
    expect(within(paneAWrap).getByText("alpha1.png")).toBeTruthy();
  });

  it("snapshot-safe: switching the active pane while the native picker is open doesn't retarget a Move to folder…", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();

    // Control exactly when the (real-world-modal) native picker resolves, so we can flip the active pane
    // WHILE it's still open — mirroring App.deleteSnapshot.test.ts's confirm-dialog-stays-open window.
    let resolvePicker!: (dest: string) => void;
    openDialog.mockReturnValue(new Promise<string>((res) => { resolvePicker = res; }));

    const bravoRow = within(paneBWrap).getByText("bravo1.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Move to folder…"));

    // The active pane flips to A while the picker promise is still unresolved.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    resolvePicker(DEST_IN_A);

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    // Still pane B's bravo1.png — the source pane/selection was captured at invocation time, before the
    // picker opened, so the later pane switch can't retarget this onto pane A's alpha1.png.
    expect(moveEntriesCalls[0].paths).toEqual([`${PATH_B}\\bravo1.png`]);
  });

  it("single-pane mode: Move to folder… still targets pane A's own selection", async () => {
    saveDualPane(false);
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha1.png")).toBeTruthy());

    openDialog.mockResolvedValue(DEST_IN_A);
    await fireEvent.click(screen.getByText("alpha2.png"));
    const row = screen.getByText("alpha2.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(row);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Move to folder…"));

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    expect(moveEntriesCalls[0]).toEqual({ paths: [`${PATH_A}\\alpha2.png`], dest: DEST_IN_A });
  });
});
