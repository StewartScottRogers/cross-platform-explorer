/**
 * CPE-1378 — pane B's `<ExplorerPane>` was missing `bind:columnWidths`/`activeMetaColumns`/
 * `on:resizeMetaColumns`/`on:openColumnPicker` (column resize/picker inert in pane B) and `inHome` +
 * every Home-action event (`homeSelect`/`unpin`/`unfavorite`/`removeRecent`/`removeRecentFolder`/
 * `clearRecents`/`loadShared`/`addNetworkLocation`/`removeNetworkLocation`) — so pane B could never show
 * the Home landing view or act on it at all.
 *
 * Investigation while fixing (see the PR description): `columnWidths` (the built-in Name/Date/Type/Size
 * widths) is a single GLOBAL setting — `settings.loadColumnWidths()` takes no folder/pane key and
 * `applySettings()` reloads it as one app-wide value alongside `pins`/`recents`/`favorites` — so pane B
 * was wired to the SAME `columnWidths` binding as pane A (not a per-pane `columnWidthsB`), matching the
 * existing design rather than forking it. `activeMetaColumns` (per-folder metadata columns, keyed by
 * pane A's `currentPath`) was passed through unchanged (shared) per the same reasoning — pane B's own
 * folder doesn't yet get its own metadata-column config; flagged as a follow-up in the PR description.
 *
 * Home's underlying stores (`pins`/`favorites`/`recents`/`recentFolders`) are already shared across both
 * panes' `<ExplorerPane>`s (reactive props), so wiring pane B's Home-action events to the SAME top-level
 * handlers pane A uses (CPE-1378's own risk assessment: "routing is low-risk") needs no new logic.
 *
 * Same mounted-App-with-mocked-backend dual-pane harness as App.deleteSnapshot.test.ts /
 * App.paneBDisplayProps.test.ts. Pane B is booted directly at the Home landing by persisting its saved
 * path as App.svelte's private `HOME` sentinel value (` home` — not exported, so reproduced literally
 * here; matches the technique `App.deleteSnapshot.test.ts` uses of persisting settings before `render`
 * so the app boots straight into the state under test) — there's currently no in-app control that routes
 * pane B to Home itself (the shared Sidebar's Home button always targets pane A, unchanged, out of this
 * ticket's scope), so this is the only way to reach a pane-B Home view at all today.
 *
 * Side-fix found while writing this test: `navigateB` had no `path === HOME` guard (unlike pane A's
 * `loadPath`, which short-circuits before ever fetching a listing for HOME), so booting pane B straight
 * at Home still fired a real `loadListing(" home", …)` — a bogus `list_dir` call for a path that isn't a
 * real folder, whose result `<HomeView>` never reads anyway (it renders from `places`/`pins`/`recents`/…,
 * not `entries`/`visible`). Harmless-looking (masked by the `{#if inHome}` branch), but wasteful, and it
 * reproducibly collided with pane A's own listing-load in this exact test (two independent
 * `<ExplorerPane>` instances' dev-only perf-mark generation counters both starting at 1, racing on the
 * same mark name) — an unhandled rejection that failed the run even though every assertion passed.
 * `navigateB` now mirrors `loadPath`'s HOME short-circuit.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveDualPane, savePaneBPath, savePins, loadColumnWidths, loadPins } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

/** App.svelte's private Home-landing sentinel (`const HOME = " home"`) — not exported, so reproduced by
 *  value; see the file-header comment. */
const HOME_SENTINEL = " home";

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

const PATH_A = "C:\\d";
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];
const entriesA: DirEntry[] = [file("alpha.txt", PATH_A)];

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
      case "list_dir": return entriesA;
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage(entriesA);
        return entriesA.length;
      }
      case "parent_dir": return null;
      default: return null;
    }
  });
});

describe("App — pane B's column resize (CPE-1378)", () => {
  it("resizing a column from pane B's header persists to the SAME shared width state pane A uses (columnWidths is a single global setting, not per-pane)", async () => {
    saveDualPane(true);
    savePaneBPath(PATH_A); // pane B shows a plain, real folder too — same folder is fine for this check
    render(App);

    // Pane B auto-restores to its persisted folder on mount; pane A still starts at Home by default and
    // needs an explicit navigation. Wait for pane B to fully settle FIRST, then navigate pane A — same
    // sequencing App.deleteSnapshot.test.ts's `bootDualPane` uses to avoid a pre-existing test-environment
    // quirk (both panes' `<ExplorerPane>` instances number their own dev-only perf-mark generation from 1,
    // so two truly concurrent loads could collide on the same mark name — unrelated to this fix).
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);

    // Both panes now settle showing "alpha.txt" (mirrored into the same folder).
    await waitFor(() => expect(screen.getAllByText("alpha.txt").length).toBe(2));

    const paneWraps = screen.getAllByText("alpha.txt").map(
      (el) => el.closest(".pane-col") as HTMLElement,
    );
    expect(paneWraps.length).toBe(2);
    const [paneASeparators, paneBSeparators] = paneWraps.map((w) =>
      [...w.querySelectorAll('[role="separator"]')] as HTMLElement[],
    );
    expect(paneASeparators.length).toBeGreaterThan(0);
    expect(paneBSeparators.length).toBeGreaterThan(0);

    const before = Number(paneASeparators[0].getAttribute("aria-valuenow"));

    // Resize the Name column from PANE B's own header (keyboard resize — ArrowRight grows it by a fixed
    // 8px step; no pointer-drag geometry needed in jsdom).
    await fireEvent.keyDown(paneBSeparators[0], { key: "ArrowRight" });

    await waitFor(() => {
      // Pane A's SAME column boundary grew by the same amount — proving the resize landed in the one
      // shared `columnWidths` array both panes are bound to, not a pane-B-only copy that pane A never
      // sees (which is what a naive per-pane `columnWidthsB` would have produced).
      expect(Number(paneASeparators[0].getAttribute("aria-valuenow"))).toBe(before + 8);
    });
    // And it's the persisted global setting pane A itself reads on every `applySettings()` — proving
    // it's a real settings write, not just a transient in-memory prop.
    expect(loadColumnWidths()[0]).toBe(before + 8);
  });
});

describe("App — pane B's Home landing + actions (CPE-1378)", () => {
  it("shows the Home landing in pane B when its saved path is Home, and a pane-B Home action (Unpin) updates the shared pins store", async () => {
    const pinnedPath = "C:\\Pinned";
    savePins([pinnedPath]);
    saveDualPane(true);
    savePaneBPath(HOME_SENTINEL);
    render(App);

    // Pane A settles on its own real folder (never navigated to Home in this test) while pane B, booted
    // straight at Home, shows the pinned folder as a Quick-access card — proof `inHome` reached pane B's
    // `<ExplorerPane>` at all (pre-fix, pane B could never render `<HomeView>`, regardless of its path).
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
    await waitFor(() => expect(screen.getByText("Pinned")).toBeTruthy()); // leaf name of pinnedPath

    const unpinBadge = screen.getByTitle("Unpin from Quick access");
    await fireEvent.click(unpinBadge);

    // The pinned card is derived purely from the `pins` store, so it disappears once unpinned — proving
    // pane B's `on:unpin` reached the SAME shared `pins`/`settings.savePins` pane A's Home view uses.
    await waitFor(() => expect(screen.queryByText("Pinned")).toBeNull());
    expect(loadPins()).not.toContain(pinnedPath);
    // Pane A's own listing is untouched by a pane-B Home action.
    expect(screen.getByText("alpha.txt")).toBeTruthy();
  });
});
