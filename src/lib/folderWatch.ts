// Watched-folder rules driver (CPE-794, epic CPE-734). Subscribes to the backend `folder-watch` FS-event
// stream (sidecar-gated live watcher), and for each landed file runs the CPE-793 planner
// (`planForEntry`) → the CPE-794 executor (`run_watch_actions`). An oscillation guard suppresses the
// events the executor's own moves/renames generate, so a rule can't ping-pong a file forever. The rule
// matching + execution stay platform-agnostic; only the live trigger is sidecar-only.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "./invoke";
import { planForEntry, type WatchRule } from "./watchRules";
import type { DirEntry } from "./types";

export interface FolderWatchEvent {
  path: string;
  kind: string;
}

/**
 * Suppresses re-processing of paths the executor itself just wrote (source + resolved destinations),
 * for a short window — so a `move`/`rename` rule doesn't re-fire on the file it just produced. Pure and
 * time-injectable so it can be unit-tested.
 */
export class OscillationGuard {
  private until = new Map<string, number>();
  constructor(private windowMs = 3000) {}

  /** Mark `path` as executor-touched at `now`; events for it are ignored until `now + windowMs`. */
  guard(path: string, now: number): void {
    this.until.set(path, now + this.windowMs);
  }

  /** Whether an event for `path` at `now` should be ignored (recently executor-touched). Expired
      entries are pruned as a side effect so the map can't grow without bound. */
  isGuarded(path: string, now: number): boolean {
    const exp = this.until.get(path);
    if (exp === undefined) return false;
    if (now >= exp) {
      this.until.delete(path);
      return false;
    }
    return true;
  }
}

/** A ready-to-run action for `run_watch_actions` (fs kinds only; `tag` is app metadata, handled elsewhere). */
const FS_ACTION_KINDS = new Set(["move", "copy", "rename"]);

let unlisten: UnlistenFn | null = null;
const guard = new OscillationGuard();

/**
 * Handle one coalesced `folder-watch` batch: for each created/modified file not currently guarded, stat
 * it, run the rules, and execute the fs actions — guarding the source + resolved destinations first.
 * `onFire` reports each executed rule for the activity log. Exposed for testing with injected deps.
 */
export async function handleFolderBatch(
  batch: FolderWatchEvent[],
  rules: WatchRule[],
  onFire: (msg: string) => void,
  deps: {
    now: () => number;
    stat: (path: string) => Promise<Pick<DirEntry, "name" | "is_dir" | "size" | "modified">>;
    run: (path: string, actions: { kind: string; resolved: string }[]) => Promise<unknown>;
    guard?: OscillationGuard;
  },
): Promise<void> {
  const g = deps.guard ?? guard;
  for (const ev of batch) {
    if (ev.kind !== "created" && ev.kind !== "modified") continue;
    const now = deps.now();
    if (g.isGuarded(ev.path, now)) continue;
    try {
      const info = await deps.stat(ev.path);
      if (info.is_dir) continue;
      const entry = { name: info.name, path: ev.path, is_dir: false, size: info.size, modified: info.modified } as DirEntry;
      const plan = planForEntry(entry, rules, now);
      if (!plan) continue;
      const actions = plan.actions
        .filter((a) => FS_ACTION_KINDS.has(a.action.kind))
        .map((a) => ({ kind: a.action.kind, resolved: a.resolved }));
      if (actions.length === 0) continue;
      // Guard the source + destinations before executing so their echo events are ignored.
      g.guard(ev.path, now);
      for (const a of actions) g.guard(a.resolved, now);
      await deps.run(ev.path, actions);
      onFire(`${plan.rule.name}: ${entry.name} → ${actions.map((a) => a.resolved).join(", ")}`);
    } catch {
      // File gone / stat failed / exec error — skip this one, keep watching.
    }
  }
}

/** Start (or restart) watching `paths`, running `rulesFn()`'s rules on each landed file. Returns the
    number of folders actually being watched. Sidecar-gated backend — a no-op fails soft in the plain build. */
export async function startFolderWatch(
  paths: string[],
  rulesFn: () => WatchRule[],
  onFire: (msg: string) => void,
): Promise<number> {
  let count = 0;
  try {
    count = await invoke<number>("folder_watch_start", { paths });
  } catch {
    return 0; // plain build (no watcher) or backend error
  }
  if (!unlisten) {
    unlisten = await listen<FolderWatchEvent[]>("folder-watch", (e) =>
      handleFolderBatch(e.payload, rulesFn(), onFire, {
        now: () => Date.now(),
        stat: (path) => invoke("entry_info", { path }),
        run: (path, actions) => invoke("run_watch_actions", { path, actions }),
      }),
    );
  }
  return count;
}

/** Stop watching and drop the event subscription. Idempotent. */
export async function stopFolderWatch(): Promise<void> {
  try {
    await invoke("folder_watch_stop");
  } catch {
    // ignore
  }
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
}
