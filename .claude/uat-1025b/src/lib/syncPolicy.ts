// Per-repo two-way-mirror sync policy (CPE-495). The on-diverge strategy is a per-repository choice,
// so it's persisted keyed by the repo's path (a small localStorage map). Safe-by-default: `merge`
// (the engine never force-pushes regardless). Kept tiny + pure so it's unit-testable.

export type OnDiverge = "merge" | "rebase" | "manual";

const KEY = "cpe.syncPolicy";
const DEFAULT: OnDiverge = "merge";

function readMap(): Record<string, OnDiverge> {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as Record<string, OnDiverge>) : {};
  } catch {
    return {};
  }
}

/** The saved on-diverge policy for `path`, or the safe `merge` default. */
export function loadSyncPolicy(path: string): OnDiverge {
  const v = readMap()[path];
  return v === "rebase" || v === "manual" || v === "merge" ? v : DEFAULT;
}

/** Persist the on-diverge policy for `path`. */
export function saveSyncPolicy(path: string, policy: OnDiverge): void {
  try {
    const m = readMap();
    m[path] = policy;
    localStorage.setItem(KEY, JSON.stringify(m));
  } catch {
    /* storage unavailable — the choice just won't persist */
  }
}

/** Human label for a planned `forge_sync` action name (from the backend `SyncPlan`). */
export function syncActionLabel(action: string): string {
  switch (action) {
    case "pull-ff":
      return "Fast-forward pull";
    case "pull-merge":
      return "Pull (merge)";
    case "pull-rebase":
      return "Pull (rebase)";
    case "push":
      return "Push";
    default:
      return action;
  }
}
