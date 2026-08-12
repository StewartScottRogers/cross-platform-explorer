/**
 * CPE-1664 — the **unattended** backup consent, pinned at its real call site.
 *
 * `apply_backup_plan_stream` refuses without `confirmed`, and a mirror plan deletes files under the
 * destination root with no Recycle Bin copy and no undo. For a run nobody is watching, the consent is the
 * per-job "auto-run on connect" box the user ticked — read via `unattendedBackupConsent`.
 *
 * This file exists because the PR #855 security audit mutation-tested that decision and nothing caught
 * it: inverting the flag inside `App.svelte`'s `runBackupJobNow` left all 3867 frontend tests green,
 * because the drive-connect scheduler is the only thing that calls it and no test drove the scheduler.
 * So this test drives the scheduler for real — fake timers, a drive appearing between polls — and
 * asserts what actually goes over the wire.
 *
 * Harness follows App.dropStackTransfer.test.ts (mounted App, mocked backend, multi-handler event bus).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveBackupJobs } from "./lib/settings";
import { stopDriveScheduler } from "./lib/driveScheduler";
import type { Place } from "./lib/types";

const PATH_A = "C:\\d";
const DEST_DRIVE = "D:\\"; // the backup destination's drive — absent at launch, appears on a later poll
const JOB_DEST = "D:\\backup";

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

/** Drives reported by `list_drives`; mutated mid-test so a drive "connects" between polls. */
let connectedDrives: Place[] = [];
/** Every `apply_backup_plan_stream` call's args. */
let backupCalls: Record<string, unknown>[] = [];

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  stopDriveScheduler();
  Element.prototype.scrollIntoView = vi.fn();
  backupCalls = [];
  connectedDrives = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return connectedDrives;
      case "home_dir": return "C:\\Users\\t";
      case "list_dir": return [];
      case "list_dir_stream": return 0;
      case "can_restore_from_trash": return true;
      // The two scans the job's plan is built from. Dest holds a file the source doesn't, so the plan
      // carries a mirror DELETE — the destructive shape whose consent is the point of this test.
      case "scan_tree":
        return args.path === JOB_DEST ? [{ name: "stale.txt", isDir: false, size: 1, modified: 1 }] : [];
      case "apply_backup_plan_stream":
        backupCalls.push(args);
        return 0;
      default: return null;
    }
  });
});

afterEach(() => {
  stopDriveScheduler();
  vi.useRealTimers();
});

/** Mount the app with `job` persisted, then make `DEST_DRIVE` appear and let the poller tick once. */
async function connectTheBackupDrive(job: Record<string, unknown>) {
  saveBackupJobs([job as never]);
  vi.useFakeTimers({ shouldAdvanceTime: true });
  render(App);

  // The scheduler seeds its connected set on start (so drives already present don't fire at launch).
  await waitFor(() =>
    expect(invoke.mock.calls.filter((c) => c[0] === "list_drives").length).toBeGreaterThan(0),
  );
  // One poll with only C: present, so the seed is definitely settled before the drive appears.
  await vi.advanceTimersByTimeAsync(20_000);
  // Now the destination drive appears…
  connectedDrives = [...connectedDrives, { name: "Backup (D:)", path: DEST_DRIVE, kind: "drive" }];
  await vi.advanceTimersByTimeAsync(20_000); // …and the next poll sees the transition
}

describe("App — unattended backup consent (CPE-1664)", () => {
  it("a job the user ticked auto-run for runs with confirmed: true when its drive connects", async () => {
    await connectTheBackupDrive({
      id: "j1", name: "Photos", source: "C:\\pics", dest: JOB_DEST, mirror: true, autoRun: true,
    });

    await waitFor(() => expect(backupCalls.length).toBe(1));
    // THE assertion the audit found unpinned: the per-job opt-in is what authorises the run.
    expect(backupCalls[0].confirmed).toBe(true);
    // …and it really is the destructive shape being consented to.
    expect(backupCalls[0].deletePaths).toEqual(["stale.txt"]);
    expect(backupCalls[0].destRoot).toBe(JOB_DEST);
  });

  it("a job with auto-run off is never run unattended at all — no drive connect can start it", async () => {
    await connectTheBackupDrive({
      id: "j2", name: "Photos", source: "C:\\pics", dest: JOB_DEST, mirror: true, autoRun: false,
    });

    // The scheduler doesn't even poll when nothing opts in, so nothing is consented to and nothing runs.
    expect(backupCalls).toHaveLength(0);
  });
});
