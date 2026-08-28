/**
 * CPE-1366: navigating away from an in-place archive browse-view must EXIT the archive. `loadPath` (real-
 * filesystem navigation) resets the other virtual view-modes (`smartFolder`, `structuredSearch`) but used
 * to leave `archive` set — so Back/Forward and tab-switch (which call `loadPath` directly, without the
 * manual `exitArchive()` guard the sidebar/crumb paths use) stranded the archive view: the zip's inner
 * entries bled onto a real folder / an unrelated tab. The fix clears `archive` inside `loadPath` (the
 * single chokepoint).
 *
 * The regression must Back into a real FOLDER (not Home — Home swaps to the places view and unmounts the
 * FileList, masking the strand). So we descend Home -> C:\d -> C:\d\photos, enter the zip there, then Back
 * to C:\d: with the strand present the zip's inner entry renders over C:\d; with the fix C:\d's real
 * listing returns. Same mocked-Tauri harness as App.archivePassword.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

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

// Real-filesystem listings by path: C:\d holds subfolder "photos"; C:\d\photos holds the zip we browse.
const listings: Record<string, DirEntry[]> = {
  "C:\\d": [entry("photos", "C:\\d\\photos", true)],
  "C:\\d\\photos": [entry("bundle.zip", "C:\\d\\photos\\bundle.zip", false, "zip")],
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

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: listings[args.path as string] ?? [], filtered: 0 };
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const list = listings[args.path as string] ?? [];
        if (list.length) ch.onmessage(list);
        return list.length;
      }
      case "parent_dir": return null;
      case "read_archive_entries": return [{ name: "inside.txt", size: 5, is_dir: false }];
      case "read_file_text": return "";
      default: return null;
    }
  });
});

describe("App — leaving an archive browse-view on Back (CPE-1366)", () => {
  it("Back into a real folder exits the archive instead of stranding its inner entries", async () => {
    render(App);
    // Descend Home -> C:\d, so the drive's subfolder shows and history has real-folder depth.
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("photos")).toBeTruthy());

    // -> C:\d\photos (history is now [Home, C:\d, C:\d\photos]).
    await fireEvent.dblClick(screen.getByText("photos"));
    await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());

    // Enter the archive in place — its inner entry renders in the file list.
    await fireEvent.dblClick(screen.getByText("bundle.zip"));
    await waitFor(() => expect(screen.getByText("inside.txt")).toBeTruthy());

    // Back -> C:\d (a REAL folder). Found by the NavToolbar button's stable "Alt+Left" title.
    await fireEvent.click(screen.getByTitle(/Alt\+Left/));

    // The archive must be gone: the zip's inner entry is no longer stranded over the real folder. Before
    // the fix, "inside.txt" rendered here via the archive override while currentPath was C:\d (the perf
    // marks confirm Back lands on C:\d, a real folder whose FileList IS mounted — so this isn't the Home
    // masking case). `bundle.zip` (only in the subfolder) is likewise gone — we're on C:\d's real view.
    await waitFor(() => expect(screen.queryByText("inside.txt")).toBeNull());
    expect(screen.queryByText("bundle.zip")).toBeNull();
  });
});

/**
 * CPE-1979 — the address bar must also get you out, and re-entering the SAME path is the case that was
 * broken. `enterArchive` never moves `currentPath` (it sets `archive` and leaves history alone), so the
 * address bar goes on displaying the CONTAINING folder while the listing shows the archive's inner
 * entries. Opening the bar and pressing Enter therefore submits a value equal to `currentPath`, and
 * `NavToolbar#commit`'s "nothing would change" short-circuit (`value === currentPath`) swallowed it:
 * no `navigate` dispatch, so `onCrumbNavigate`'s `exitArchive()` and `loadPath`'s `archive = null` were
 * both unreachable and the user stayed inside the archive with no feedback at all.
 *
 * Measured cost of that in CI, which is why this has a ticket of its own: `gui-smoke`'s between-spec
 * `resetAppState` navigates back to the run's tmp dir through this exact primitive, and
 * `archive-browse.smoke.ts` leaves the app inside a `.tar.gz`. 77 of 77 completed shard-2 job logs over
 * 2026-08-28T00:21Z-17:11Z failed that reset with `expected the breadcrumb to show
 * "cpe-gui-smoke-XXXXXX"`, then recovered by restarting the whole WebDriver session — which in 11 of
 * those 77 also killed the driver and spent CPE-1955's respawn budget.
 *
 * Red-proof (run before commit): with `pathOverlaidByView` removed from `commit`'s condition this test
 * fails on the first `waitFor` — `inside.txt` is still rendered — while the CPE-1366 test above stays
 * green, i.e. the two cover different exits and neither shadows the other.
 */
describe("App — leaving an archive browse-view via the address bar (CPE-1979)", () => {
  it("re-entering the archive's own containing folder in the address bar exits the archive", async () => {
    const { container } = render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("photos")).toBeTruthy());

    await fireEvent.dblClick(screen.getByText("photos"));
    await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());

    // Inside the archive: the inner entry renders, and `currentPath` is still C:\d\photos.
    await fireEvent.dblClick(screen.getByText("bundle.zip"));
    await waitFor(() => expect(screen.getByText("inside.txt")).toBeTruthy());

    // Open the address bar the way clicking its empty area does (NavToolbar's `.address` click handler
    // fires only when the event target IS the nav itself, which is what dispatching on it produces).
    // `startEdit` then seeds the input with `currentPath` — so pressing Enter with nothing typed submits
    // exactly the value the old short-circuit rejected. Nothing is typed here ON PURPOSE: retyping the
    // path by hand would test the same code path less honestly.
    const address = container.querySelector("nav.address");
    expect(address).not.toBeNull();
    await fireEvent.click(address!);
    const input = (await screen.findByLabelText("Address")) as HTMLInputElement;
    expect(input.value).toBe("C:\\d\\photos");
    await fireEvent.keyDown(input, { key: "Enter" });

    // The archive is gone and the real folder's listing is back.
    await waitFor(() => expect(screen.queryByText("inside.txt")).toBeNull());
    await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());
  });
});
