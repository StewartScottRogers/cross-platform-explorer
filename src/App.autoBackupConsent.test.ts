/**
 * CPE-1664 — the drive-connect scheduler's **wiring** to the backup backend.
 *
 * `apply_backup_plan_stream` refuses without `confirmed`, and a mirror plan deletes files under the
 * destination root with no Recycle Bin copy and no undo. For a run nobody is watching, the consent is
 * the per-job "auto-run on connect" box the user ticked.
 *
 * **What this file does NOT do — read this before adding a claim to it.** It does not pin the consent
 * decision. `driveScheduler`'s `jobsForConnect` filters on `j.autoRun`, so the scheduler can only ever
 * deliver `autoRun: true` jobs: the flag's expected value here is *necessarily* `true`, and hard-coding
 * `confirmed: true` in `App.svelte` passes every test below. Two earlier rounds of this PR claimed
 * otherwise in this very docstring; the reviewer and the security auditor each reproduced the
 * hard-coding independently and found the whole suite green. **The decision is pinned in
 * `backup.test.ts`, on `unattendedBackupArgs`, which can be called with an unticked job.**
 *
 * What this file genuinely covers, and why it is still worth having: that the scheduler reaches the
 * backend at all on a drive-connect transition, with a real mirror plan and consent set; and that a job
 * with auto-run off is never started unattended by any drive connect. Both are real regressions
 * otherwise, and nothing else drives the scheduler end to end.
 *
 * **CPE-1925 added two more, and they belong here specifically.** This path — a stored `job.source`, a
 * drive appearing, no dashboard row open, no preview, nobody watching — is where an empty source folder
 * silently failed to arrive, and where a source folder the scan could not read silently took the
 * destination's copies with it. So the plan's `createDirs` entries reaching the backend, and the
 * skipped-folder disclosure reaching the toast, are pinned against the REAL `runBackupJobNow` here
 * rather than only against the pure planner.
 *
 * Harness follows App.dropStackTransfer.test.ts (mounted App, mocked backend, multi-handler event bus).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveBackupJobs } from "./lib/settings";
import { stopDriveScheduler } from "./lib/driveScheduler";
import { unattendedBackupConsent } from "./lib/backup";
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
/** What `scan_tree` returns for the job's SOURCE root. Per-test, because CPE-1925's two unattended
 *  claims are both about shapes the source can have and the destination cannot. */
let sourceTree: unknown[] = [];
/** Per-`OpResult` batches the mocked backend streams back, so a run can report something. */
let streamedResults: { path: string; ok: boolean; error: string }[] = [];
/** When set, `scan_tree` REJECTS for the source root — `scan_tree`'s CPE-1925 answer for a root it
 *  cannot list, which used to be a silent `Ok([])`. */
let scanError = "";
/** How many times a job's plan has been scanned. The signal that a run STARTED, which `backupCalls`
 *  cannot be — a run that fails at the scan never reaches the backend at all. */
let sourceScans = 0;

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  stopDriveScheduler();
  Element.prototype.scrollIntoView = vi.fn();
  backupCalls = [];
  sourceTree = [];
  streamedResults = [];
  scanError = "";
  sourceScans = 0;
  connectedDrives = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return connectedDrives;
      case "home_dir": return "C:\\Users\\t";
      case "list_dir": return { entries: [], filtered: 0 };
      case "list_dir_stream": return 0;
      case "can_restore_from_trash": return true;
      // The two scans the job's plan is built from. Dest holds a file the source doesn't, so the plan
      // carries a mirror DELETE — the destructive shape whose consent is the point of this test.
      case "scan_tree":
        if (args.path === JOB_DEST) return [{ name: "stale.txt", isDir: false, size: 1, modified: 1 }];
        sourceScans += 1;
        if (scanError) throw scanError;
        return sourceTree;
      case "apply_backup_plan_stream":
        backupCalls.push(args);
        (args.onResult as { onmessage: (b: unknown) => void }).onmessage(streamedResults);
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
  // …and the next poll sees the transition. Stepped one second at a time rather than in one 20-second
  // jump: same elapsed time, but the run's completion notice has a 5s auto-clear, and a single long
  // advance fires that clear inside the same call — the notice would be set and gone before any
  // assertion could see it. Stopping as soon as the run has STARTED leaves it on screen. The stop
  // condition is the source scan rather than `backupCalls`, because a run that fails at the scan
  // (CPE-1925 case 7) raises its notice without ever reaching the backend.
  for (let i = 0; i < 20 && sourceScans === 0; i++) await vi.advanceTimersByTimeAsync(1_000);
}

describe("App — the drive-connect scheduler's backup wiring (CPE-1664)", () => {
  it("a job the user ticked auto-run for runs, consented, when its drive connects", async () => {
    const job = {
      id: "j1", name: "Photos", source: "C:\\pics", dest: JOB_DEST, mirror: true, autoRun: true,
    };
    await connectTheBackupDrive(job);

    await waitFor(() => expect(backupCalls.length).toBe(1));
    // Written against the helper rather than a literal so the intent is legible — but note this CANNOT
    // fail for a hard-coded `true`, since the scheduler only delivers `autoRun: true` jobs. The
    // decision itself is pinned in backup.test.ts; see this file's header.
    expect(backupCalls[0].confirmed).toBe(unattendedBackupConsent(job));
    // …and it really is the destructive shape reaching the backend.
    expect(backupCalls[0].deletePaths).toEqual(["stale.txt"]);
    expect(backupCalls[0].destRoot).toBe(JOB_DEST);
  });

  // ---- CPE-1925 round 2: the unattended path is the one nobody is watching, and it is exactly where
  // the destructive shapes reach. Round 1 argued both of these mattered and tested neither here.

  it("carries the plan's directory entries to the backend on the unattended path (CPE-1925)", async () => {
    // A source folder with no files under it. Before CPE-1925 the plan model had no entry kind for it,
    // so the run reported a clean ok and the folder simply never arrived in the destination — and on
    // this path there is no dashboard row and nobody watching to notice the shape had changed.
    sourceTree = [{ name: "logs", isDir: true, children: [] }];
    await connectTheBackupDrive({
      id: "j3", name: "Photos", source: "C:\\pics", dest: JOB_DEST, mirror: true, autoRun: true,
    });

    await waitFor(() => expect(backupCalls.length).toBe(1));
    expect(backupCalls[0].createDirs).toEqual(["logs"]);
  });

  it("discloses the folders it could not see inside, on the run nobody is watching (CPE-1925)", async () => {
    // The scan reached this folder and could not read it, so its emptiness is unknown: it gets no
    // `createDirs` entry, nothing under it is mirror-deleted, and — the part this test exists for — the
    // toast SAYS SO. On the dashboard the plan preview shows the same count; on this path there is no
    // preview, so silence here would be total, and silence is the one answer this ticket does not allow.
    sourceTree = [{ name: "locked", isDir: true, children: [], unreadable: true }];
    streamedResults = [{ path: "stale.txt", ok: true, error: "" }];
    await connectTheBackupDrive({
      id: "j4", name: "Photos", source: "C:\\pics", dest: JOB_DEST, mirror: true, autoRun: true,
    });

    await waitFor(() => expect(backupCalls.length).toBe(1));
    expect(backupCalls[0].createDirs).toEqual([]); // its emptiness was never established

    await waitFor(() => expect(document.body.textContent).toContain("Folders not carried: 1"));
    expect(document.body.textContent).toContain("locked");
  });

  it("a source root the scan cannot list stops the run and says so, rather than mirroring an empty tree (CPE-1925)", async () => {
    // Case 7. `scan_tree` used to answer `Ok([])` for a root `read_dir` had refused — byte-identical to
    // an empty folder — and `planBackup` in mirror mode turns an empty source into `delete everything
    // in the destination`. Measured on the round-1 branch, on ext4, with a `0o000` source root:
    // `delete: ["a.txt","b.txt"]`, engine `ok=2 fail=0`, destination empty afterwards, source untouched.
    // The scan is now an Err, and this is the assertion that the Err reaches the user and stops the run
    // rather than being swallowed — the whole reason it was made fail-closed.
    scanError = "C:\\pics: could not be listed completely, so this scan cannot say what is in it";
    await connectTheBackupDrive({
      id: "j5", name: "Photos", source: "C:\\pics", dest: JOB_DEST, mirror: true, autoRun: true,
    });

    expect(backupCalls).toHaveLength(0); // nothing was applied, so nothing was deleted
    await waitFor(() => expect(document.body.textContent).toContain("could not be listed completely"));
  });

  it("a job with auto-run off is never run unattended at all — no drive connect can start it", async () => {
    await connectTheBackupDrive({
      id: "j2", name: "Photos", source: "C:\\pics", dest: JOB_DEST, mirror: true, autoRun: false,
    });

    // The scheduler doesn't even poll when nothing opts in, so nothing is consented to and nothing runs.
    expect(backupCalls).toHaveLength(0);
  });
});
