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
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import {
  resetSettings,
  saveDualPane,
  savePaneBPath,
  savePins,
  loadColumnWidths,
  loadPins,
  saveMetaColumnsForFolder,
  loadMetaColumnsForFolder,
} from "./lib/settings";
import { __resetMetaColumnCatalogForTests } from "./lib/metaColumnCatalog";
import type { DirEntry, Place } from "./lib/types";
import type { AvailableColumn } from "./lib/bindings.gen";

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

describe("App — pane B's own active metadata columns (CPE-1382)", () => {
  it("pane B shows its own folder's active columns, independent of pane A's", async () => {
    const PATH_B = "C:\\dB";
    const entriesB: DirEntry[] = [file("bravo.txt", PATH_B)];
    // Two distinct simple (string-variant) MetaColumns so neither needs a per-file cell fetch to render
    // its header — only the catalog + the folder's saved `ActiveMetaColumn[]` id are needed for the
    // header label itself (ExplorerPane's `resolvedMetaColumns`).
    const DIMS: AvailableColumn = { id: "dimensions", label: "Dimensions", column: "ImageDimensions", extensions: [] };
    const PAGES: AvailableColumn = { id: "pages", label: "Pages", column: "DocPages", extensions: [] };

    // PATH_A's saved config activates "Dimensions"; PATH_B's activates "Pages" — different sets.
    saveMetaColumnsForFolder(PATH_A, [{ id: "dimensions", width: 110 }]);
    saveMetaColumnsForFolder(PATH_B, [{ id: "pages", width: 90 }]);
    __resetMetaColumnCatalogForTests();

    invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      const listingFor = (path: unknown) => (path === PATH_B ? entriesB : entriesA);
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
        case "metadata_columns_available": return [DIMS, PAGES];
        default: return null;
      }
    });

    saveDualPane(true);
    savePaneBPath(PATH_B);
    render(App);

    // Pane B auto-restores to PATH_B on mount; pane A still starts at Home and needs an explicit
    // navigation (same sequencing as the resize test above).
    await waitFor(() => expect(screen.getByText("bravo.txt")).toBeTruthy());
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

    // Both headers eventually resolve from the shared catalog and paint — one per pane.
    await waitFor(() => expect(screen.getByText("Dimensions")).toBeTruthy());
    await waitFor(() => expect(screen.getByText("Pages")).toBeTruthy());

    // Each header appears exactly ONCE — proving the two panes render DIFFERENT active column sets, not
    // a single shared set duplicated onto both panes (which pre-fix would have shown "Dimensions" twice,
    // since both panes were bound to the same `activeMetaColumns` keyed by pane A's `currentPath`).
    expect(screen.getAllByText("Dimensions").length).toBe(1);
    expect(screen.getAllByText("Pages").length).toBe(1);

    const paneAWrap = screen.getByText("alpha.txt").closest(".pane-col") as HTMLElement;
    const paneBWrap = screen.getByText("bravo.txt").closest(".pane-col") as HTMLElement;
    expect(paneAWrap.textContent).toContain("Dimensions");
    expect(paneAWrap.textContent).not.toContain("Pages");
    expect(paneBWrap.textContent).toContain("Pages");
    expect(paneBWrap.textContent).not.toContain("Dimensions");
  });
});

describe("App — the Sidebar's Home control routes by active pane (CPE-1383)", () => {
  it("navigates pane B to Home when pane B is active, leaving pane A untouched", async () => {
    const pinnedPath = "C:\\Pinned";
    savePins([pinnedPath]);
    saveDualPane(true);
    savePaneBPath(PATH_A); // pane B starts on a real folder (not Home) so the Home control has somewhere to go
    render(App);

    // Pane B auto-restores to PATH_A on mount; pane A still starts at Home and needs an explicit
    // navigation — both panes end up mirrored onto the same real folder (same setup as the column-resize
    // test above), so BEFORE clicking Home there are two "alpha.txt" rows, one per pane.
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getAllByText("alpha.txt").length).toBe(2));

    // DOM order is deterministic: pane A's `.pane-col` is always rendered before pane B's (App.svelte
    // renders the pane-A block first, the pane-B block second inside `{#if dualPane}`) — same convention
    // the CPE-1378 column-resize test above relies on for `[paneASeparators, paneBSeparators]`.
    const [paneAWrap, paneBWrap] = screen.getAllByText("alpha.txt").map(
      (el) => el.closest(".pane-col") as HTMLElement,
    );

    // Make pane B the active pane (clicking its wrapper sets `activePane = 1`, mirroring a real user
    // click before reaching for the Sidebar's Home button).
    await fireEvent.click(paneBWrap);

    // "Home" text also matches the (pane-A-only) NavToolbar breadcrumb's root crumb, so scope to the
    // Sidebar's nav-item specifically.
    const homeButton = screen.getAllByText("Home")
      .map((el) => el.closest(".nav-item"))
      .find((el): el is HTMLElement => el !== null) as HTMLElement;
    await fireEvent.click(homeButton);

    // Pane B now renders the Home landing view (its pinned folder shows as a Quick-access card) — proof
    // the Sidebar's Home control routed to `navigateB(HOME)`, not pane A's `navigate(HOME)`.
    await waitFor(() => expect(paneBWrap.querySelector(".home")).toBeTruthy());
    expect(paneBWrap.textContent).toContain("Pinned");

    // Pane A is completely untouched: still a real folder showing its listing, no Home view.
    expect(paneAWrap.querySelector(".home")).toBeNull();
    expect(paneAWrap.textContent).toContain("alpha.txt");
  });

  it("navigates pane A to Home when pane A is active (unchanged default behaviour)", async () => {
    const pinnedPath = "C:\\Pinned";
    savePins([pinnedPath]);
    saveDualPane(true);
    savePaneBPath(PATH_A);
    render(App);

    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getAllByText("alpha.txt").length).toBe(2));

    const [paneAWrap, paneBWrap] = screen.getAllByText("alpha.txt").map(
      (el) => el.closest(".pane-col") as HTMLElement,
    );

    // Pane A is active by default (never clicked pane B here) — clicking Home must still land on pane A.
    // "Home" text also matches the (pane-A-only) NavToolbar breadcrumb's root crumb, so scope to the
    // Sidebar's nav-item specifically (same disambiguation as the test above).
    const homeButton = screen.getAllByText("Home")
      .map((el) => el.closest(".nav-item"))
      .find((el): el is HTMLElement => el !== null) as HTMLElement;
    await fireEvent.click(homeButton);

    await waitFor(() => expect(paneAWrap.querySelector(".home")).toBeTruthy());
    expect(paneAWrap.textContent).toContain("Pinned");
    // Pane B is untouched.
    expect(paneBWrap.querySelector(".home")).toBeNull();
    expect(paneBWrap.textContent).toContain("alpha.txt");
  });
});

describe("App — the Column Picker dialog is pane-aware (CPE-1388)", () => {
  it("opening the picker from pane B loads/saves pane B's OWN column set, leaving pane A's untouched", async () => {
    const PATH_B = "C:\\dB";
    const entriesB: DirEntry[] = [file("bravo.txt", PATH_B)];
    const DIMS: AvailableColumn = { id: "dimensions", label: "Dimensions", column: "ImageDimensions", extensions: [] };
    const PAGES: AvailableColumn = { id: "pages", label: "Pages", column: "DocPages", extensions: [] };
    // Neither folder has any active columns saved yet — both dialogs would start from an empty set,
    // so the assertions below can only be explained by WRITE routing, not by different starting data.
    __resetMetaColumnCatalogForTests();

    invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      const listingFor = (path: unknown) => (path === PATH_B ? entriesB : entriesA);
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
        case "metadata_columns_available": return [DIMS, PAGES];
        default: return null;
      }
    });

    saveDualPane(true);
    savePaneBPath(PATH_B);
    render(App);

    await waitFor(() => expect(screen.getByText("bravo.txt")).toBeTruthy());
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

    const paneAWrap = screen.getByText("alpha.txt").closest(".pane-col") as HTMLElement;
    const paneBWrap = screen.getByText("bravo.txt").closest(".pane-col") as HTMLElement;

    // Open the column picker from PANE B's own header "Columns…" button (never pane A's).
    await fireEvent.click(within(paneBWrap).getByTestId("open-column-picker"));
    const dialog = await screen.findByRole("dialog");
    // Pre-fix, the dialog was hard-wired to `activeMetaColumns` (pane A's) regardless of which pane's
    // button was clicked — this assertion alone doesn't distinguish the two (both start empty), but the
    // save-target check below does.
    expect(within(dialog).queryByTestId("active-pages")).toBeNull();

    await fireEvent.click(within(dialog).getByTestId("add-pages"));

    // Persisted under pane B's OWN folder — never pane A's `currentPath` (CPE-1388's actual fix).
    await waitFor(() => expect(loadMetaColumnsForFolder(PATH_B).some((c) => c.id === "pages")).toBe(true));
    expect(loadMetaColumnsForFolder(PATH_A).some((c) => c.id === "pages")).toBe(false);

    await fireEvent.click(within(dialog).getByTestId("done-btn"));

    // Pane B's header now renders the new column; pane A's is completely untouched.
    await waitFor(() => expect(within(paneBWrap).queryByText("Pages")).toBeTruthy());
    expect(within(paneAWrap).queryByText("Pages")).toBeNull();
  });

  it("opening the picker from pane A still loads/saves pane A's OWN column set (unchanged default routing)", async () => {
    const PATH_B = "C:\\dB";
    const entriesB: DirEntry[] = [file("bravo.txt", PATH_B)];
    const DIMS: AvailableColumn = { id: "dimensions", label: "Dimensions", column: "ImageDimensions", extensions: [] };
    __resetMetaColumnCatalogForTests();

    invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      const listingFor = (path: unknown) => (path === PATH_B ? entriesB : entriesA);
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
        case "metadata_columns_available": return [DIMS];
        default: return null;
      }
    });

    saveDualPane(true);
    savePaneBPath(PATH_B);
    render(App);

    await waitFor(() => expect(screen.getByText("bravo.txt")).toBeTruthy());
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

    const paneAWrap = screen.getByText("alpha.txt").closest(".pane-col") as HTMLElement;
    const paneBWrap = screen.getByText("bravo.txt").closest(".pane-col") as HTMLElement;

    await fireEvent.click(within(paneAWrap).getByTestId("open-column-picker"));
    const dialog = await screen.findByRole("dialog");
    await fireEvent.click(within(dialog).getByTestId("add-dimensions"));

    await waitFor(() => expect(loadMetaColumnsForFolder(PATH_A).some((c) => c.id === "dimensions")).toBe(true));
    expect(loadMetaColumnsForFolder(PATH_B).some((c) => c.id === "dimensions")).toBe(false);

    await fireEvent.click(within(dialog).getByTestId("done-btn"));
    await waitFor(() => expect(within(paneAWrap).queryByText("Dimensions")).toBeTruthy());
    expect(within(paneBWrap).queryByText("Dimensions")).toBeNull();
  });
});
