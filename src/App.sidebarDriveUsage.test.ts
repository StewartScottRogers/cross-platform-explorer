/**
 * CPE-1859 — the sidebar's per-drive usage bars (`Sidebar.svelte`, CPE-406) were filled on mount and on
 * a change to the drive SET only, so the "N free" figure under every drive could be hours stale: copy
 * 50 GB, empty the trash, extract an archive, and the bar still showed the number read at launch.
 *
 * Why that stopped being cosmetic: CPE-1854 removed the status bar's free-space figure inside Home,
 * archives, smart folders and structured searches, and its UAT justified the removal partly on the
 * grounds that free space is "already on screen permanently" in the sidebar. That argument is sound,
 * and it PROMOTES these bars to the primary free-space readout — which makes a launch-time snapshot a
 * false statement rather than a slightly old one.
 *
 * Unlike `StatusBar.diskAnchor.test.ts` next door, these are real behavioural assertions, not source
 * text: each drives the app and reads the FIGURE THE SIDEBAR IS SHOWING before and after. They are
 * possible in jsdom precisely because staleness is a text question, not a layout one.
 *
 * The drive lives at `D:\x`, deliberately outside every folder this harness can navigate to, so every
 * `disk_space` call naming it comes from `loadDriveUsage` and never from the status bar's own
 * per-navigation probe (CPE-403) — which is what makes the call COUNTS below mean what they say.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { savedSearches } from "./lib/savedSearchStore";
import { smartFolders } from "./lib/smartFolders";
import type { Place } from "./lib/types";

const GB = 1024 * 1024 * 1024;
const DRIVE_PATH = "D:\\x";
const drives: Place[] = [{ name: "Data (D:)", path: DRIVE_PATH, kind: "drive" }];

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

/** What the next `disk_space` on the drive answers. Reassigned mid-test to stage "the user just freed
 *  40 GB" — the whole point being that nothing else about the app changes. */
let driveFree = 7 * GB;
/** How many times the drive has been probed, and an optional gate that holds the next probe open so the
 *  in-flight guard can be tested against a genuinely slow drive rather than a fast one. */
let driveProbes = 0;
let heldRelease: (() => void) | null = null;
let holdDriveProbe = false;

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  savedSearches.set([]);
  smartFolders.set([]);
  driveFree = 7 * GB;
  driveProbes = 0;
  heldRelease = null;
  holdDriveProbe = false;
  Element.prototype.scrollIntoView = vi.fn();

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: [], filtered: 0, unreadable: 0 };
      case "list_dir_stream": return { total: 0, filtered: 0, unreadable: 0 };
      case "parent_dir": return null;
      case "disk_space": {
        if (args.path !== DRIVE_PATH) return { free: 0, total: 0 };
        driveProbes += 1;
        if (holdDriveProbe) await new Promise<void>((r) => (heldRelease = r));
        return { free: driveFree, total: 100 * GB };
      }
      default: return null;
    }
  });
});

afterEach(() => {
  vi.useRealTimers();
});

/** The sidebar's own label is exactly `${formatSize(free)} free` — a whole text node distinct from the
 *  status bar's `"… free of …"`, so an exact-string query here can only match the sidebar row. */
const freeLabel = (gb: string) => `${gb} GB free`;

describe("CPE-1859 — the sidebar drive bars refresh instead of holding a launch-time snapshot", () => {
  it("shows the figure probed at mount", async () => {
    render(App);
    await waitFor(() => expect(screen.getByText(freeLabel("7.0"))).toBeTruthy());
  });

  it("re-probes on window focus, so returning to the app shows the CURRENT free space", async () => {
    render(App);
    // Before/after inside one render: the old figure must genuinely be on screen first, so the
    // assertion below cannot pass against a sidebar that failed to render at all.
    await waitFor(() => expect(screen.getByText(freeLabel("7.0"))).toBeTruthy());

    driveFree = 47 * GB; // the user emptied the trash while the app was in the background
    window.dispatchEvent(new Event("focus"));

    await waitFor(() => expect(screen.getByText(freeLabel("47.0"))).toBeTruthy());
    expect(screen.queryByText(freeLabel("7.0"))).toBeNull();
  });

  it("re-probes on the 60s tick, so it also refreshes while the app stays in the foreground", async () => {
    // Fake timers must be installed BEFORE render, or `setInterval` in onMount schedules against the
    // real clock and no amount of `advanceTimersByTime` afterwards can reach it. `shouldAdvanceTime`
    // keeps the fake clock creeping forward on its own so App's real async startup still settles.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(App);
    await waitFor(() => expect(screen.getByText(freeLabel("7.0"))).toBeTruthy());
    const atMount = driveProbes;

    driveFree = 12 * GB;
    await vi.advanceTimersByTimeAsync(60_000);

    expect(driveProbes).toBeGreaterThan(atMount);
    await waitFor(() => expect(screen.getByText(freeLabel("12.0"))).toBeTruthy());
  });

  it("does not stack overlapping probes while one is still in flight", async () => {
    // The cost this guard exists for: `disk_space` against a disconnected mapped network drive can
    // block for the OS's own timeout, which is longer than the 60s tick. Without the in-flight guard a
    // dead share would accumulate one probe per tick forever.
    render(App);
    await waitFor(() => expect(screen.getByText(freeLabel("7.0"))).toBeTruthy());

    holdDriveProbe = true;
    window.dispatchEvent(new Event("focus")); // starts a probe and blocks in it
    await waitFor(() => expect(driveProbes).toBe(2));

    window.dispatchEvent(new Event("focus"));
    window.dispatchEvent(new Event("focus"));
    await Promise.resolve();
    expect(driveProbes).toBe(2); // both suppressed — not merely coalesced later

    holdDriveProbe = false;
    heldRelease?.();
    await waitFor(() => expect(screen.getByText(freeLabel("7.0"))).toBeTruthy());
  });
});
