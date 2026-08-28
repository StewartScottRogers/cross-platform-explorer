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
  /**
   * Directories to create in the destination for their own sake (CPE-1925) — the ones no `copy`/
   * `update` entry would create as a side effect of writing a file into them.
   *
   * **Why this exists.** Before it, the plan carried only files, so a directory reached the
   * destination only implicitly. A source directory with no files anywhere under it — a scaffolded
   * `logs/`, an output folder, a mount point, anything whose contents are gitignored — had no entry of
   * any kind, was never created, and the run still reported a clean `ok` for every file it did carry.
   * The user got a tree whose *shape* had quietly changed, with nothing in the plan, the progress
   * count, or the result saying so.
   *
   * **It is the minimal set.** Only the deepest directory of a chain appears: `a/b/c` creates `a` and
   * `a/b` on the way, so listing all three would triple the entries for one outcome. And a directory
   * that already holds a planned file copy is left out entirely, because the copy creates it. A first
   * full backup of a large tree therefore gains a handful of entries, not one per folder.
   */
  createDirs: string[];
  /**
   * Source directories this plan **deliberately does not carry**, each with the reason (CPE-1925).
   *
   * The scan reports a directory as childless for three different reasons and only one of them means
   * "empty": `read_dir` failed on it, or the scan's depth cap stopped at it, or there is genuinely
   * nothing inside. Creating an empty directory in the destination for either of the first two would
   * be **asserting a fact the scan never established** — a directory whose contents could not be read
   * would arrive at the destination looking deliberately empty. So those are excluded from
   * `createDirs` and named here instead, for the preview and the run summary to show. Silence is the
   * one answer this ticket does not allow; an empty `skippedDirs` is the ordinary case.
   */
  skippedDirs: SkippedDir[];
  /** Count of files already identical. */
  unchanged: number;
}

/** One entry of {@link BackupPlan.skippedDirs}: a source directory the plan will not reproduce, and
 *  which of the two "the scan could not look inside" reasons applies. */
export interface SkippedDir {
  path: string;
  reason: "unreadable" | "depth-limit";
}

/**
 * Classify one level of the diff, appending to `plan`.
 *
 * Returns whether this level leaves the **enclosing** directory materialised in the destination —
 * that is, whether the enclosing directory will exist there once the plan has run, either because it
 * is already in the destination or because some entry below it creates it. That single boolean is
 * what keeps `createDirs` minimal: a directory only earns an entry when nothing underneath it would
 * have produced one.
 */
function walk(nodes: DiffNode[], prefix: string, mirror: boolean, plan: BackupPlan): boolean {
  let materialised = false;
  for (const n of nodes) {
    const path = prefix ? `${prefix}/${n.name}` : n.name;
    if (n.isDir) {
      // A dest-only directory: its file leaves are mirror-delete candidates and nothing about it needs
      // creating. Its return value is deliberately ignored — the destination side says nothing about
      // whether the *source* shape is reproduced.
      if (n.status === "removed") {
        walk(n.children ?? [], path, mirror, plan);
        continue;
      }

      // Does the destination already hold a *directory* at this name? `added` means it holds nothing,
      // and a file→directory type change — which `diffTrees` emits as `changed` with no `children`
      // array at all, where every real directory node carries one (possibly empty) — means it holds a
      // file. Both need the directory created in its own right; the type change will be refused by
      // the engine with a file standing in the way, and reported per entry, which is still an
      // improvement on today, where that whole source subtree is dropped without a word.
      const typeChange = n.children === undefined;
      const inDest = n.status !== "added" && !typeChange;

      // CPE-1925. The scan could not see inside this source directory (`read_dir` refused, or the
      // depth cap stopped there), so its emptiness is unknown. Two consequences, both about refusing
      // to act on an inference:
      //
      //  1. It gets no `createDirs` entry. The directory itself is real, but a directory placed in the
      //     destination *looking deliberately empty* asserts something this scan never established,
      //     and a restored tree carrying that lie is worse than one visibly missing the folder.
      //  2. Nothing under it may be mirror-deleted. Its children came back as an empty list, so every
      //     file the DESTINATION holds under that path diffs as "removed", and a mirror run would
      //     delete the very copies it exists to protect because one directory could not be read.
      //     Passing `mirror = false` down makes that impossible; the deletes it suppresses are exactly
      //     the ones derived from an unknown, and a later run that CAN read the directory will still
      //     remove anything genuinely extraneous.
      //
      // Either way the directory is named in `skippedDirs` rather than passed over in silence.
      const unknown = n.unreadable ? "unreadable" : n.truncated ? "depth-limit" : null;
      if (unknown) {
        plan.skippedDirs.push({ path, reason: unknown });
        walk(n.children ?? [], path, false, plan);
        if (inDest) materialised = true;
        continue;
      }

      if (walk(n.children ?? [], path, mirror, plan)) {
        materialised = true; // something below creates this directory on its way in
        continue;
      }
      // Nothing below will create it, so if it is not already in the destination it needs an entry of
      // its own — this is the empty directory the whole ticket is about.
      if (!inDest) plan.createDirs.push(path);
      materialised = true;
    } else {
      switch (n.status) {
        case "added":
          plan.copy.push(path);
          materialised = true;
          break;
        case "changed":
          plan.update.push(path);
          materialised = true;
          break;
        case "removed":
          if (mirror) plan.delete.push(path);
          break;
        case "identical":
          plan.unchanged += 1;
          materialised = true; // it is already there, so its directory is too
          break;
      }
    }
  }
  return materialised;
}

/**
 * Plan an incremental backup of `source` onto `dest`. Diffs dest→source (CPE-777): source-only files are
 * copied, differing files updated, identical skipped, and dest-only files deleted only when `mirror`.
 * Source directories that no file copy would create are carried in `createDirs`, and the ones whose
 * contents the scan could not see are named in `skippedDirs` rather than guessed at (CPE-1925). Pure.
 */
export function planBackup(source: CompareNode[], dest: CompareNode[], mirror = false): BackupPlan {
  const plan: BackupPlan = { copy: [], update: [], delete: [], createDirs: [], skippedDirs: [], unchanged: 0 };
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
  plan: Pick<BackupPlan, "copy" | "update" | "delete" | "createDirs">,
): {
  sourceRoot: string;
  destRoot: string;
  copy: string[];
  update: string[];
  deletePaths: string[];
  createDirs: string[];
  verify: boolean;
  confirmed: boolean;
} {
  return {
    sourceRoot: job.source,
    destRoot: job.dest,
    copy: plan.copy,
    update: plan.update,
    deletePaths: plan.delete,
    // CPE-1925: carried here as well as from the dashboard, because an unattended run is exactly the
    // one nobody is watching — a scheduled job that silently reshaped the tree would go unnoticed for
    // as long as the backup went unread.
    createDirs: plan.createDirs,
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
