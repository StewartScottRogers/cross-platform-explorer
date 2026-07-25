// Watched-folder rules driver (CPE-794, epic CPE-734). Subscribes to the backend `folder-watch` FS-event
// stream (sidecar-gated live watcher), and for each landed file runs the CPE-793 planner
// (`planForEntry`) → the CPE-794 executor (`run_watch_actions`). An oscillation guard suppresses the
// events the executor's own moves/renames generate, so a rule can't ping-pong a file forever. Each fire is
// recorded as a reversible `WatchFire` so the activity log can offer **undo** (move the file back / delete
// copies). The rule matching + execution stay platform-agnostic; only the live trigger is sidecar-only.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { unwrap } from "./invoke";
import { commands } from "./bindings.gen"; // typed client (CPE-964)
import { planForEntry, type WatchRule } from "./watchRules";
import type { DirEntry } from "./types";

export interface FolderWatchEvent {
  path: string;
  kind: string;
}

interface OpResult {
  path: string;
  ok: boolean;
  error: string;
}

/** One executed rule, with enough to reverse it: where the file ended (move/rename) + any copies made. */
export interface WatchFire {
  id: string;
  rule: string;
  /** The file's original path when the rule fired. */
  source: string;
  /** Where the file ended after move/rename (equals `source` if the rule only copied). */
  finalPath: string;
  /** Paths of copies the rule produced (deleted on undo). */
  copies: string[];
  /** Display line for the activity log. */
  summary: string;
}

/** The reverse operations for a fire: move the file back (if it moved) + delete any copies. Pure. */
export function undoPlan(fire: WatchFire): { moveBack: { from: string; to: string } | null; deletes: string[] } {
  return {
    moveBack: fire.finalPath !== fire.source ? { from: fire.finalPath, to: fire.source } : null,
    deletes: [...fire.copies],
  };
}

let fireSeq = 0;
const newFireId = () => `wf_${Date.now().toString(36)}_${fireSeq++}`;

/**
 * Suppresses re-processing of paths the executor itself just wrote (source + resolved destinations),
 * for a short window — so a `move`/`rename` rule doesn't re-fire on the file it just produced. Pure and
 * time-injectable so it can be unit-tested.
 */
export class OscillationGuard {
  private until = new Map<string, number>();
  constructor(private windowMs = 3000) {}

  guard(path: string, now: number): void {
    this.until.set(path, now + this.windowMs);
  }

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

const FS_ACTION_KINDS = new Set(["move", "copy", "rename"]);

let unlisten: UnlistenFn | null = null;
const guard = new OscillationGuard();

/**
 * Handle one coalesced `folder-watch` batch: for each created/modified file not currently guarded, stat
 * it, run the rules, execute the fs actions (guarding source + results), and report a reversible
 * `WatchFire`. Exposed for testing with injected deps.
 */
export async function handleFolderBatch(
  batch: FolderWatchEvent[],
  rules: WatchRule[],
  onFire: (fire: WatchFire) => void,
  deps: {
    now: () => number;
    stat: (path: string) => Promise<Pick<DirEntry, "name" | "is_dir" | "size" | "modified">>;
    run: (path: string, actions: { kind: string; resolved: string }[]) => Promise<OpResult[]>;
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
      const fsActions = plan.actions.filter((a) => FS_ACTION_KINDS.has(a.action.kind));
      const actions = fsActions.map((a) => ({ kind: a.action.kind, resolved: a.resolved }));
      if (actions.length === 0) continue;
      // Guard the source + planned dests before executing so their echo events are ignored.
      g.guard(ev.path, now);
      for (const a of actions) g.guard(a.resolved, now);
      const results = await deps.run(ev.path, actions);
      // Fold the results into a reversible record: track where the file ended (move/rename) + copies made.
      let finalPath = ev.path;
      const copies: string[] = [];
      actions.forEach((a, i) => {
        const outPath = results[i]?.path ?? a.resolved;
        g.guard(outPath, now); // the result path's echo event is the executor's own
        if (a.kind === "copy") copies.push(outPath);
        else finalPath = outPath; // move / rename relocates the file
      });
      onFire({
        id: newFireId(),
        rule: plan.rule.name,
        source: ev.path,
        finalPath,
        copies,
        summary: `${plan.rule.name}: ${entry.name} → ${actions.map((a) => a.resolved).join(", ")}`,
      });
    } catch {
      // File gone / stat failed / exec error — skip this one, keep watching.
    }
  }
}

/** Reverse a fire (CPE-794): move the file back to its original path and delete any copies it made. */
export async function undoFire(fire: WatchFire): Promise<void> {
  const plan = undoPlan(fire);
  const now = Date.now();
  if (plan.moveBack) {
    guard.guard(plan.moveBack.from, now);
    guard.guard(plan.moveBack.to, now);
    await commands.moveExact([[plan.moveBack.from, plan.moveBack.to]]);
  }
  for (const p of plan.deletes) {
    guard.guard(p, now);
    await commands.deletePermanent([p]);
  }
}

/** Start (or restart) watching `paths`, running `rulesFn()`'s rules on each landed file. Returns the
    number of folders actually being watched. Sidecar-gated backend — a no-op fails soft in the plain build. */
export async function startFolderWatch(
  paths: string[],
  rulesFn: () => WatchRule[],
  onFire: (fire: WatchFire) => void,
): Promise<number> {
  let count = 0;
  try {
    count = unwrap(await commands.folderWatchStart(paths));
  } catch {
    return 0;
  }
  if (!unlisten) {
    unlisten = await listen<FolderWatchEvent[]>("folder-watch", (e) =>
      handleFolderBatch(e.payload, rulesFn(), onFire, {
        now: () => Date.now(),
        stat: (path) => commands.entryInfo(path).then(unwrap),
        run: (path, actions) => commands.runWatchActions(path, actions),
      }),
    );
  }
  return count;
}

/** Stop watching and drop the event subscription. Idempotent. */
export async function stopFolderWatch(): Promise<void> {
  try {
    await commands.folderWatchStop();
  } catch {
    // ignore
  }
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
}
