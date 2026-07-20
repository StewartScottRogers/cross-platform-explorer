// Drive-connect scheduler (CPE-797, epic CPE-736). Polls the connected-drive set (`list_drives`) and, when
// a drive that hosts an **auto-run** backup job's destination *appears* (absent → present), runs that job.
// Only the connect *transition* fires — a drive that stays plugged in doesn't re-run every tick, and drives
// already connected at startup are seeded so app launch doesn't trigger a backup. The poller is inert unless
// at least one job opts in (`autoRun`), honouring epic CPE-736's "no background cost when no job is
// scheduled". The diff logic is pure + injectable for unit tests; only the live poll touches the backend.

import { invoke } from "./invoke";
import type { BackupJob } from "./backup";
import type { Place } from "./types";

/**
 * The drive root that owns `path`. Windows: `D:\Backups` → `D:\` (drive-letter roots, as `list_drives`
 * reports them). POSIX/UNC or unrecognised: `/` (the single POSIX root is always connected, so POSIX never
 * sees a "new" drive — which is correct). Case-normalised to an upper-case letter so `d:\` == `D:\`.
 */
export function driveRoot(path: string): string {
  const m = /^([A-Za-z]):[\\/]/.exec(path);
  return m ? `${m[1].toUpperCase()}:\\` : "/";
}

const norm = (root: string): string => driveRoot(root);

/** Roots present in `cur` but not in `prev` (case-insensitive) — the drives that just connected. */
export function newlyConnected(prev: string[], cur: string[]): string[] {
  const before = new Set(prev.map(norm));
  const seen = new Set<string>();
  const out: string[] = [];
  for (const r of cur) {
    const n = norm(r);
    if (!before.has(n) && !seen.has(n)) {
      seen.add(n);
      out.push(n);
    }
  }
  return out;
}

/**
 * The auto-run jobs whose destination drive just connected (present in `cur`, absent in `prev`). Pure — the
 * poller feeds it the previous and current connected-root sets and the current job list. A job with
 * `autoRun` false/undefined is never scheduled automatically.
 */
export function jobsForConnect(prev: string[], cur: string[], jobs: BackupJob[]): BackupJob[] {
  const appeared = new Set(newlyConnected(prev, cur));
  if (appeared.size === 0) return [];
  return jobs.filter((j) => j.autoRun && appeared.has(driveRoot(j.dest)));
}

/** True if any job opts into auto-run — the poller stays off otherwise (no background cost). */
export function anyAutoRun(jobs: BackupJob[]): boolean {
  return jobs.some((j) => j.autoRun);
}

const connectedRoots = async (): Promise<string[]> => {
  const drives = await invoke<Place[]>("list_drives");
  return drives.filter((d) => d.kind === "drive").map((d) => d.path);
};

let timer: ReturnType<typeof setInterval> | null = null;
let prevRoots: string[] = [];
let ticking = false;

/**
 * Start (or restart) the poller. Reads `jobsFn()` each tick, and calls `runJob(job)` for each auto-run job
 * whose destination drive just connected. No-op (and stops any running poller) when no job opts in. Seeds
 * the connected set on start so already-plugged drives don't fire at launch.
 */
export async function startDriveScheduler(
  jobsFn: () => BackupJob[],
  runJob: (job: BackupJob) => void | Promise<void>,
  everyMs = 15000,
): Promise<void> {
  stopDriveScheduler();
  if (!anyAutoRun(jobsFn())) return; // opt-in: nothing to watch → no polling
  try {
    prevRoots = await connectedRoots(); // seed: don't fire for drives already present at start
  } catch {
    prevRoots = [];
  }
  timer = setInterval(() => void tick(jobsFn, runJob), everyMs);
}

async function tick(jobsFn: () => BackupJob[], runJob: (job: BackupJob) => void | Promise<void>): Promise<void> {
  if (ticking) return; // don't overlap a slow poll
  ticking = true;
  try {
    const cur = await connectedRoots();
    const due = jobsForConnect(prevRoots, cur, jobsFn());
    prevRoots = cur;
    for (const j of due) {
      try {
        await runJob(j);
      } catch {
        // a failed job never wedges the poller
      }
    }
  } catch {
    // list_drives failed this tick — try again next
  } finally {
    ticking = false;
  }
}

/** Stop polling. Idempotent. */
export function stopDriveScheduler(): void {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
  prevRoots = [];
}
