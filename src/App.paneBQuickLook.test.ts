/**
 * CPE-1432 — Space quick-look (image + media) act on the correct pane. Before this fix `openQuickLook`
 * (CPE-645) and `openMediaQuickLook` (CPE-1430) unconditionally read pane A's live `selectedEntries`/
 * `visible`, regardless of `activePane`: pressing Space while pane B was active still opened the
 * quick-look for pane A's selection and stepped through pane A's folder. The fix threads the keyboard
 * path's already-computed `inPaneB` (`dualPane && activePane === 1`, `handleKeydown`'s existing local)
 * through `paneStateFor`, the same routing every other pane-aware key already uses (CPE-1370/1377/1384/
 * 1424) — pane A / single-pane behavior is untouched (`inPaneB` defaults to `false`).
 *
 * Both `QuickLook.svelte`'s `.name` span and `MediaQuickLook.svelte`'s `.mq-name` span carry
 * `title={current file name}`, so `screen.getByTitle(name)` is a single selector that proves which
 * pane's file the overlay actually opened on, for both the image and the media quick-look.
 *
 * Same mounted-App-with-mocked-backend dual-pane harness as App.paneBBulkOps.test.ts /
 * App.clipboardPaneRouting.test.ts / App.paneBContextMenu.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveDualPane, savePaneBPath } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

// jsdom doesn't implement HTMLMediaElement.play/pause; MediaPlayer guards those calls, but stub them so
// the media quick-look's autoplay path (CPE-1430) doesn't log "Not implemented" noise (mirrors
// MediaQuickLook.test.ts).
Object.defineProperty(HTMLMediaElement.prototype, "play", {
  configurable: true,
  value: vi.fn(() => Promise.resolve()),
});
Object.defineProperty(HTMLMediaElement.prototype, "pause", {
  configurable: true,
  value: vi.fn(),
});

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

let disks: Record<string, DirEntry[]> = {};

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  disks = {
    [PATH_A]: [
      file("alpha1.png", PATH_A),
      file("alpha2.png", PATH_A),
      file("alphaVid.mp4", PATH_A),
      folder("destInA", PATH_A),
    ],
    [PATH_B]: [
      file("bravo1.png", PATH_B),
      file("bravo2.png", PATH_B),
      file("bravoVid.mp4", PATH_B),
      folder("destInB", PATH_B),
    ],
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
      default: return null;
    }
  });
});

/** Boot straight into dual-pane, navigate pane A into its drive, wait for both panes to settle. Returns
 *  each pane's `.pane-col` wrapper — clicking it (not a row inside it, which stops propagation) is how
 *  the app itself sets `activePane` (same helper shape as App.clipboardPaneRouting.test.ts /
 *  App.deleteSnapshot.test.ts). */
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

describe("App — Space image quick-look is pane-aware (CPE-1432)", () => {
  it("Space with pane B active opens the image quick-look for pane B's selection and steps through pane B's folder", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    // Give pane A a DIFFERENT selection first — proves a mutation that forces pane A wouldn't pass.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    await fireEvent.click(paneBWrap); // make pane B the active pane
    await fireEvent.click(within(paneBWrap).getByText("bravo1.png"));
    await fireEvent.keyDown(window, { key: " " });

    await waitFor(() => expect(screen.getByTitle("bravo1.png")).toBeTruthy());
    expect(screen.queryByTitle("alpha1.png")).toBeNull(); // NOT pane A's file

    // Stepping (→) cycles within pane B's own images (bravo1 → bravo2), not pane A's.
    await fireEvent.keyDown(window, { key: "ArrowRight" });
    await waitFor(() => expect(screen.getByTitle("bravo2.png")).toBeTruthy());
    expect(screen.queryByTitle("alpha2.png")).toBeNull();
  });

  it("Space with pane A active still opens pane A's own image quick-look (unchanged)", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneBWrap);
    await fireEvent.click(within(paneBWrap).getByText("bravo1.png"));

    await fireEvent.click(paneAWrap); // switch active pane back to A
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));
    await fireEvent.keyDown(window, { key: " " });

    await waitFor(() => expect(screen.getByTitle("alpha1.png")).toBeTruthy());
    expect(screen.queryByTitle("bravo1.png")).toBeNull();
  });

  it("single-pane mode: Space still opens quick-look for pane A's own selection (unchanged)", async () => {
    saveDualPane(false);
    savePaneBPath(PATH_B); // leftover state from a previous dual-pane session
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha1.png")).toBeTruthy());

    await fireEvent.click(screen.getByText("alpha1.png"));
    await fireEvent.keyDown(window, { key: " " });

    await waitFor(() => expect(screen.getByTitle("alpha1.png")).toBeTruthy());
  });
});

describe("App — Space media quick-look is pane-aware (CPE-1432)", () => {
  it("Space with pane B active opens the media quick-look for pane B's selection and steps through pane B's folder", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alphaVid.mp4")); // pane A has its own selection

    await fireEvent.click(paneBWrap); // make pane B the active pane
    await fireEvent.click(within(paneBWrap).getByText("bravoVid.mp4"));
    await fireEvent.keyDown(window, { key: " " });

    await waitFor(() => expect(screen.getByTitle("bravoVid.mp4")).toBeTruthy());
    expect(screen.queryByTitle("alphaVid.mp4")).toBeNull(); // NOT pane A's file

    // Only one media file per pane here, so stepping (→) with repeat off wraps back to the same track —
    // still pane B's, proving the playlist was built from pane B's listing, not pane A's.
    await fireEvent.keyDown(window, { key: "ArrowRight" });
    await waitFor(() => expect(screen.getByTitle("bravoVid.mp4")).toBeTruthy());
  });

  it("Space with pane A active still opens pane A's own media quick-look (unchanged)", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneBWrap);
    await fireEvent.click(within(paneBWrap).getByText("bravoVid.mp4"));

    await fireEvent.click(paneAWrap); // switch active pane back to A
    await fireEvent.click(within(paneAWrap).getByText("alphaVid.mp4"));
    await fireEvent.keyDown(window, { key: " " });

    await waitFor(() => expect(screen.getByTitle("alphaVid.mp4")).toBeTruthy());
    expect(screen.queryByTitle("bravoVid.mp4")).toBeNull();
  });

  it("single-pane mode: Space still opens the media quick-look for pane A's own selection (unchanged)", async () => {
    saveDualPane(false);
    savePaneBPath(PATH_B);
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alphaVid.mp4")).toBeTruthy());

    await fireEvent.click(screen.getByText("alphaVid.mp4"));
    await fireEvent.keyDown(window, { key: " " });

    await waitFor(() => expect(screen.getByTitle("alphaVid.mp4")).toBeTruthy());
  });
});
