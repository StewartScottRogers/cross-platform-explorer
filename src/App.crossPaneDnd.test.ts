/**
 * CPE-1371 — cross-pane drag-and-drop. Before this fix pane B was mounted with `canDrag={false}`, no
 * `on:drop`, and its own un-shared `draggedPaths`, so a drag from pane B never started and a drop onto
 * pane B did nothing (the hallmark commander gesture "copy/move A<->B by dragging" didn't exist — see
 * the ticket). The fix: pane B now shares the SAME `draggedPaths` binding pane A and Sidebar already use
 * (so a drag started in either pane can be dropped in the other), enables `canDrag`, and wires `on:drop`
 * to the same `dropInto` App.svelte already used for pane A/Sidebar drops — reusing its move/copy
 * decision (`resolveEffect`) and self-descendant guard (`isValidDrop`, gating `FileList`'s own
 * `validTarget`) rather than forking new logic.
 *
 * `dropInto`'s post-move refresh used to hard-code pane A (`loadPath(currentPath)`) — harmless while
 * pane B could never be a drop source/target, but wrong the moment it can: a B->A move would leave pane
 * B showing a file that no longer exists there. `refreshDropSourcePane` (App.svelte) fixes this by
 * refreshing whichever pane's folder the dragged paths' PARENT directory actually matches, so these
 * tests also assert the correct pane goes stale-free after each direction.
 *
 * Same mounted-App-with-mocked-backend dual-pane harness as App.deleteSnapshot.test.ts (CPE-1370 review).
 * jsdom has no native `DragEvent`, so drag events are built by hand (same technique as
 * FileList.dragout.test.ts / FileList.hoverSameVolume.test.ts's fireDragStart/fireDragOver).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
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
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];

// Mutable "disk" state so a successful move is reflected in the NEXT list_dir/list_dir_stream call —
// lets the tests prove a pane actually refreshes (the moved-away item disappears) rather than only
// asserting the move_entries call args.
let diskA: DirEntry[] = [];
let diskB: DirEntry[] = [];

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
let listDirCalls: string[] = [];

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  moveEntriesCalls = [];
  listDirCalls = [];
  diskA = [file("alpha.txt", PATH_A), folder("destInA", PATH_A)];
  diskB = [file("bravo.txt", PATH_B), folder("destInB", PATH_B)];
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => (path === PATH_B ? diskB : diskA);
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir":
        listDirCalls.push(args.path as string);
        return listingFor(args.path);
      case "list_dir_stream": {
        listDirCalls.push(args.path as string);
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "move_entries": {
        const paths = args.paths as string[];
        const dest = args.dest as string;
        moveEntriesCalls.push({ paths, dest });
        // Simulate the OS move: remove from source disk, add to dest disk (by name only — the tests
        // only ever assert on the SOURCE pane losing the item).
        diskA = diskA.filter((e) => !paths.includes(e.path));
        diskB = diskB.filter((e) => !paths.includes(e.path));
        return paths.map((p) => ({ path: p, ok: true, error: "" }));
      }
      default: return null;
    }
  });
});

/** A minimal DataTransfer stub — jsdom doesn't provide one. */
function fakeDataTransfer() {
  return { setData: vi.fn(), getData: () => "", setDragImage: vi.fn(), effectAllowed: "", dropEffect: "" };
}

function fireDragStart(row: HTMLElement, dt: ReturnType<typeof fakeDataTransfer>) {
  const ev = new Event("dragstart", { bubbles: true, cancelable: true });
  Object.assign(ev, { altKey: false, shiftKey: false, ctrlKey: false, dataTransfer: dt });
  return fireEvent(row, ev);
}

function fireDragOver(row: HTMLElement, dt: ReturnType<typeof fakeDataTransfer>, shiftKey = true) {
  const ev = new Event("dragover", { bubbles: true, cancelable: true });
  Object.assign(ev, { ctrlKey: false, shiftKey, dataTransfer: dt });
  return fireEvent(row, ev);
}

function fireDrop(row: HTMLElement, dt: ReturnType<typeof fakeDataTransfer>, shiftKey = true) {
  const ev = new Event("drop", { bubbles: true, cancelable: true });
  Object.assign(ev, { ctrlKey: false, shiftKey, dataTransfer: dt });
  return fireEvent(row, ev);
}

/** Boot straight into dual-pane, navigate pane A into its drive, wait for both panes to settle. */
async function bootDualPane() {
  saveDualPane(true);
  savePaneBPath(PATH_B);
  render(App);

  await waitFor(() => expect(screen.getByText("bravo.txt")).toBeTruthy());
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
}

describe("App — cross-pane drag-and-drop (CPE-1371)", () => {
  it("drags a row FROM pane A and drops it onto a folder in pane B — routes through dropInto and refreshes pane A", async () => {
    await bootDualPane();

    const alphaRow = screen.getByText("alpha.txt").closest(".row") as HTMLElement;
    const destInBRow = screen.getByText("destInB").closest(".row") as HTMLElement;

    const dt = fakeDataTransfer();
    await fireDragStart(alphaRow, dt);
    await fireDragOver(destInBRow, dt, true); // Shift forces "move" deterministically (no sameVolume round-trip)
    await fireDrop(destInBRow, dt, true);

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    expect(moveEntriesCalls[0]).toEqual({ paths: [`${PATH_A}\\alpha.txt`], dest: `${PATH_B}\\destInB` });

    // Pane A (the SOURCE) must refresh so the moved-away file disappears — proving the drop actually did
    // something, not just that move_entries was invoked with plausible-looking args.
    await waitFor(() => expect(screen.queryByText("alpha.txt")).toBeNull());
    // Pane B (bravo.txt, destInB) is untouched by the move and must still be visible.
    expect(screen.getByText("bravo.txt")).toBeTruthy();
    expect(screen.getByText("destInB")).toBeTruthy();
  });

  it("drags a row FROM pane B and drops it onto a folder in pane A — the reverse direction also works, proving draggedPaths is shared", async () => {
    await bootDualPane();

    const bravoRow = screen.getByText("bravo.txt").closest(".row") as HTMLElement;
    const destInARow = screen.getByText("destInA").closest(".row") as HTMLElement;

    const dt = fakeDataTransfer();
    await fireDragStart(bravoRow, dt);
    await fireDragOver(destInARow, dt, true);
    await fireDrop(destInARow, dt, true);

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    expect(moveEntriesCalls[0]).toEqual({ paths: [`${PATH_B}\\bravo.txt`], dest: `${PATH_A}\\destInA` });

    // Pane B (the SOURCE this time) must refresh — the fix generalizing `dropInto`'s hard-coded
    // pane-A-only refresh is what makes this assertion pass instead of leaving a stale bravo.txt row.
    await waitFor(() => expect(screen.queryByText("bravo.txt")).toBeNull());
    expect(screen.getByText("alpha.txt")).toBeTruthy();
    expect(screen.getByText("destInA")).toBeTruthy();
  });

  it("the self-descendant guard still applies in pane B (newly enabled — was never reachable while canDrag was false)", async () => {
    await bootDualPane();

    // Drag pane B's own folder and attempt to drop it on itself — isValidDrop must refuse this, exactly
    // as it already does for pane A, so move_entries is never called.
    const destInBRow = screen.getByText("destInB").closest(".row") as HTMLElement;
    const dt = fakeDataTransfer();
    await fireDragStart(destInBRow, dt);
    await fireDragOver(destInBRow, dt, true);
    await fireDrop(destInBRow, dt, true);

    // Give any (incorrect) async path a tick to run before asserting the negative.
    await new Promise((r) => setTimeout(r, 0));
    expect(moveEntriesCalls.length).toBe(0);
    expect(screen.getByText("destInB")).toBeTruthy();
  });

  it("both panes mirroring the SAME folder both refresh after a move — no ghost row left behind (Reviewer/UAT catch)", async () => {
    // A common commander pattern: both panes navigated to the SAME folder (compare/sort one dir two
    // ways). `refreshDropSourcePane`'s old mutually-exclusive if/else refreshed EITHER pane B OR pane A,
    // never both — whichever pane it skipped kept rendering a GHOST row for a file the move had already
    // removed, because `list_dir`/`list_dir_stream` was never re-issued for it.
    saveDualPane(true);
    savePaneBPath(PATH_A); // pane B mirrors pane A's folder, not its own PATH_B
    render(App);

    // Pane B auto-restores to PATH_A on mount; then navigate pane A into the same drive/folder.
    await waitFor(() => expect(screen.getAllByText("alpha.txt").length).toBeGreaterThanOrEqual(1));
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getAllByText("alpha.txt").length).toBe(2));
    expect(screen.getAllByText("destInA").length).toBe(2);

    const alphaRows = screen.getAllByText("alpha.txt").map((el) => el.closest(".row") as HTMLElement);
    const destRows = screen.getAllByText("destInA").map((el) => el.closest(".row") as HTMLElement);

    const dt = fakeDataTransfer();
    await fireDragStart(alphaRows[0], dt); // drag from pane A's row
    await fireDragOver(destRows[1], dt, true); // drop onto pane B's row for the same subfolder (cross-pane)
    await fireDrop(destRows[1], dt, true);

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    expect(moveEntriesCalls[0]).toEqual({ paths: [`${PATH_A}\\alpha.txt`], dest: `${PATH_A}\\destInA` });

    // BOTH panes show the source folder, so BOTH must refresh — asserting a full 0, not just "pane A" or
    // "pane B" individually, is what catches a fix that only refreshes one of the two.
    await waitFor(() => expect(screen.queryAllByText("alpha.txt").length).toBe(0));
  });
});
