// Scheduled / background auto-mirror (CPE-497). Off by default, per-repo. The scheduling + safety
// decisions live here as pure functions so they're unit-testable without a clock or the backend:
// - when a repo is *due* for an auto-sync (last-sync + interval), and
// - which planned actions are *safe to run unattended* (only fast-forward pull + push — a divergence
//   or possible conflict pauses and surfaces rather than reconciling blindly, and it never force-pushes
//   because `forge_sync` has no force action at all).

export interface AutoMirror {
  enabled: boolean;
  /** Minutes between background syncs. */
  intervalMinutes: number;
}

export const DEFAULT_INTERVAL_MIN = 15;
/** Interval choices offered in the UI. */
export const INTERVAL_CHOICES = [5, 15, 30, 60, 120] as const;

const KEY = "cpe.autoMirror";
const DEFAULT: AutoMirror = { enabled: false, intervalMinutes: DEFAULT_INTERVAL_MIN };

function readMap(): Record<string, AutoMirror> {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as Record<string, AutoMirror>) : {};
  } catch {
    return {};
  }
}

/** The saved auto-mirror config for `path` — **off by default** (AC: off unless opted in). */
export function loadAutoMirror(path: string): AutoMirror {
  const v = readMap()[path];
  if (!v || typeof v.enabled !== "boolean") return { ...DEFAULT };
  const n = Number(v.intervalMinutes);
  return {
    enabled: v.enabled,
    intervalMinutes: Number.isFinite(n) && n > 0 ? n : DEFAULT_INTERVAL_MIN,
  };
}

export function saveAutoMirror(path: string, cfg: AutoMirror): void {
  try {
    const m = readMap();
    m[path] = { enabled: cfg.enabled, intervalMinutes: cfg.intervalMinutes };
    localStorage.setItem(KEY, JSON.stringify(m));
  } catch {
    /* storage unavailable — the toggle just won't persist */
  }
}

/** Is a repo due for an auto-sync? Never before its interval has elapsed since the last run; a repo
    that has never synced this session (`lastSyncMs == null`) is due immediately. */
export function isDue(lastSyncMs: number | null, intervalMinutes: number, nowMs: number): boolean {
  if (lastSyncMs == null) return true;
  return nowMs - lastSyncMs >= intervalMinutes * 60_000;
}

/** A minimal view of the backend `RepoSyncStatus` the safety filter needs. */
export interface AutoPlan {
  actions?: string[];
  conflicts_possible?: boolean;
  blocked?: string | null;
  dirty?: boolean;
}

/** The subset of a plan's actions that are safe to run **unattended**. Only `pull-ff` and `push`
    ever qualify: a `pull-merge` / `pull-rebase` (i.e. a divergence), a possible conflict, or a blocked
    plan yields `[]` so the scheduler pauses and surfaces instead of reconciling in the background.
    A `pull-ff` into a dirty tree is also withheld (it can fail / touch un-committed work) — a push of
    already-committed work is still fine. */
export function autoSyncActions(plan: AutoPlan): string[] {
  if (plan.conflicts_possible || plan.blocked) return [];
  const acts = plan.actions ?? [];
  // If the planner wanted a merge/rebase pull, the histories diverged — never auto-reconcile.
  if (acts.some((a) => a === "pull-merge" || a === "pull-rebase")) return [];
  return acts.filter((a) => {
    if (a === "push") return true;
    if (a === "pull-ff") return !plan.dirty;
    return false;
  });
}

/** Why an otherwise-due repo was NOT auto-synced, for surfacing a paused note (or null if it ran). */
export function pausedReason(plan: AutoPlan): string | null {
  if (plan.blocked) return plan.blocked;
  if (plan.conflicts_possible || (plan.actions ?? []).some((a) => a === "pull-merge" || a === "pull-rebase"))
    return "Histories diverged — auto-sync paused; open Sync… to reconcile.";
  return null;
}
