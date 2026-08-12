// Pure backup model + incremental planner (CPE-796, epic CPE-736). Compute what a backup run would transfer
// by reusing the CPE-777 folder-tree diff — no DOM/IO, unit-tested — so the copy engine (CPE-797) and
// dashboard dry-run (CPE-798) are thin. A `BackupJob` is just source→dest + options; `planBackup` turns two
// scanned trees into copy/update/delete lists.

import { diffTrees, type CompareNode, type DiffNode } from "./treeDiff";

/** What an incremental run would do, as relative file paths. */
export interface BackupPlan {
  /** In source, absent from dest — copy over. */
  copy: string[];
  /** In both, content differs — overwrite. */
  update: string[];
  /** In dest, absent from source — remove (only in mirror mode). */
  delete: string[];
  /** Count of files already identical. */
  unchanged: number;
}

function walk(nodes: DiffNode[], prefix: string, mirror: boolean, plan: BackupPlan): void {
  for (const n of nodes) {
    const path = prefix ? `${prefix}/${n.name}` : n.name;
    if (n.isDir) {
      walk(n.children ?? [], path, mirror, plan); // dirs are implicit; classify their file leaves
    } else {
      switch (n.status) {
        case "added":
          plan.copy.push(path);
          break;
        case "changed":
          plan.update.push(path);
          break;
        case "removed":
          if (mirror) plan.delete.push(path);
          break;
        case "identical":
          plan.unchanged += 1;
          break;
      }
    }
  }
}

/**
 * Plan an incremental backup of `source` onto `dest`. Diffs dest→source (CPE-777): source-only files are
 * copied, differing files updated, identical skipped, and dest-only files deleted only when `mirror`. Pure.
 */
export function planBackup(source: CompareNode[], dest: CompareNode[], mirror = false): BackupPlan {
  const plan: BackupPlan = { copy: [], update: [], delete: [], unchanged: 0 };
  // diffTrees(left=dest, right=source): right-only → "added" (copy), left-only → "removed" (delete).
  walk(diffTrees(dest, source), "", mirror, plan);
  return plan;
}

// ── job list store (mirrors the other CPE-77x/79x models) ───────────────────────────────────────
export interface BackupJob {
  id: string;
  name: string;
  source: string;
  dest: string;
  mirror?: boolean;
  /** Auto-run this job when its destination drive connects (CPE-797 drive-connect scheduler). */
  autoRun?: boolean;
}

/**
 * Consent for an **unattended** backup run (CPE-1664): the backend refuses a plan unless it is told the
 * user agreed, and for a run nobody is watching the only thing that can honestly say so is the per-job
 * **auto-run on connect** box the user ticked. That opt-in is the consent.
 *
 * `=== true`, not `!!`, deliberately: `parseJobs`' validator never type-checks `autoRun`, so a
 * hand-edited or migrated settings blob carrying `autoRun: "no"` would otherwise be truthy and read as
 * consent.
 *
 * Attended runs do **not** come through here — BackupDashboard's Run/Restore buttons are a live click
 * and pass their own consent.
 */
export function unattendedBackupConsent(job: Pick<BackupJob, "autoRun">): boolean {
  return job.autoRun === true;
}

/**
 * The full argument set for an unattended (`runBackupJobNow`) call of `apply_backup_plan_stream`,
 * including its `confirmed` flag.
 *
 * **This function exists so the consent decision is computed in code a test can reach with an unticked
 * job.** The scheduler cannot: `driveScheduler`'s `jobsForConnect` filters on `j.autoRun`, so it only
 * ever delivers `autoRun: true` jobs, and therefore *no* test driving the real scheduler can tell
 * `unattendedBackupConsent(job)` apart from a hard-coded `true`. Two earlier rounds of comments claimed
 * an App-level test had pinned it; it had not, and hard-coding `true` at the call site left all 3879
 * frontend tests green both times. `unattendedBackupArgs` is directly callable with `autoRun: false`,
 * so `backup.test.ts` pins both directions — which is also the "even if something else calls this"
 * scenario the consent is defence-in-depth against.
 *
 * **What remains unpinned, stated plainly:** that `App.svelte` spreads this result without overriding
 * `confirmed` afterwards. That is one expression in one place, and it is the honest residual — not a
 * protection any current test delivers.
 */
export function unattendedBackupArgs(
  job: Pick<BackupJob, "source" | "dest" | "autoRun">,
  plan: Pick<BackupPlan, "copy" | "update" | "delete">,
): {
  sourceRoot: string;
  destRoot: string;
  copy: string[];
  update: string[];
  deletePaths: string[];
  verify: boolean;
  confirmed: boolean;
} {
  return {
    sourceRoot: job.source,
    destRoot: job.dest,
    copy: plan.copy,
    update: plan.update,
    deletePaths: plan.delete,
    verify: true,
    confirmed: unattendedBackupConsent(job),
  };
}

function newId(): string {
  return `bj_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
}

export function addJob(list: BackupJob[], name: string, source: string, dest: string, mirror = false): BackupJob[] {
  return [...list, { id: newId(), name, source, dest, mirror }];
}
export function removeJob(list: BackupJob[], id: string): BackupJob[] {
  return list.filter((j) => j.id !== id);
}
export function updateJob(list: BackupJob[], id: string, patch: Partial<Omit<BackupJob, "id">>): BackupJob[] {
  return list.map((j) => (j.id === id ? { ...j, ...patch } : j));
}

const isJob = (x: unknown): x is BackupJob => {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  return typeof o.id === "string" && typeof o.name === "string" && typeof o.source === "string" && typeof o.dest === "string";
};

export function parseJobs(json: string | null): BackupJob[] {
  if (!json) return [];
  try {
    const raw = JSON.parse(json);
    return Array.isArray(raw) ? raw.filter(isJob) : [];
  } catch {
    return [];
  }
}
export function serializeJobs(list: BackupJob[]): string {
  return JSON.stringify(list);
}
