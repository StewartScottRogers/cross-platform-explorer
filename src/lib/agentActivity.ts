import { writable, type Readable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { normalizeFsActivity, type FsActivity } from "./sidecar";

/**
 * Live filesystem-activity map for the Agent Watch view (CPE-399).
 *
 * The host emits `ai-console://fs-activity` batches while an agent's Project folder is watched
 * (CPE-398); here we fold them into a per-path record the file list annotates and an activity strip
 * summarizes. Entries expire after a short TTL so annotations fade on their own — the list shows
 * what the agent is touching *now*, not a growing history. Strictly additive + idle-by-default: the
 * map is empty and no timer runs until watching starts (AGENT-WATCH.md: "off means off").
 */

/** One file's most-recent activity: its kind and when we saw it (epoch ms). */
export interface AgentActivity {
  kind: FsActivity["kind"];
  at: number;
}

/** How long an annotation lingers before it fades out of the map. */
export const ACTIVITY_TTL_MS = 6000;

const store = writable<Record<string, AgentActivity>>({});

/** Reactive per-path activity map (keyed by absolute path). Empty when not watching. */
export const fsActivity: Readable<Record<string, AgentActivity>> = store;

/** Fold a batch of activities into the previous map (pure; newest kind + timestamp win per path). */
export function foldActivities(
  prev: Record<string, AgentActivity>,
  items: FsActivity[],
  now: number,
): Record<string, AgentActivity> {
  const next = { ...prev };
  for (const it of items) next[it.path] = { kind: it.kind, at: now };
  return next;
}

/** Drop entries older than the TTL (pure). Returns the same reference when nothing expired. */
export function pruneActivities(
  map: Record<string, AgentActivity>,
  now: number,
  ttl = ACTIVITY_TTL_MS,
): Record<string, AgentActivity> {
  const live = Object.entries(map).filter(([, a]) => now - a.at < ttl);
  if (live.length === Object.keys(map).length) return map;
  return Object.fromEntries(live);
}

/** The most recent activities, newest first, for the activity strip (pure). */
export function recentActivities(
  map: Record<string, AgentActivity>,
  limit = 8,
): Array<{ path: string; kind: FsActivity["kind"]; at: number }> {
  return Object.entries(map)
    .map(([path, a]) => ({ path, kind: a.kind, at: a.at }))
    .sort((x, y) => y.at - x.at)
    .slice(0, limit);
}

/** Fold a raw event payload into the store (exposed for headless tests). */
export function ingestActivity(payload: unknown, now = Date.now()): void {
  const items = normalizeFsActivity(payload);
  if (items.length) store.update((prev) => foldActivities(prev, items, now));
}

/** Clear all activity (on stop-watching), so a stale folder never annotates a new one. */
export function clearActivity(): void {
  store.set({});
}

/**
 * Start consuming activity events + expiring stale entries. Returns a teardown that unlistens,
 * stops the pruner, and clears the map. Call when watching begins; call the teardown when it ends.
 */
export async function initAgentActivity(): Promise<() => void> {
  const unlisten = await listen("ai-console://fs-activity", (e) => ingestActivity(e.payload));
  const timer = setInterval(() => store.update((m) => pruneActivities(m, Date.now())), 1000);
  return () => {
    unlisten();
    clearInterval(timer);
    clearActivity();
  };
}
