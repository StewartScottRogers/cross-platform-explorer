import { writable, type Readable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { normalizeFsActivity, type FsActivity } from "./sidecar";
import { normalizePath } from "./agentSessions";

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

/** How many timeline entries to keep — a durable but bounded session history (CPE-400). */
export const TIMELINE_CAP = 300;

/** One durable entry in the session activity timeline (does NOT fade, unlike {@link AgentActivity}). */
export interface TimelineEntry {
  id: number;
  kind: FsActivity["kind"];
  path: string;
  at: number;
}

const store = writable<Record<string, AgentActivity>>({});
const timeline = writable<TimelineEntry[]>([]);
let nextId = 0;

/** Reactive per-path activity map (keyed by absolute path). Empty when not watching. */
export const fsActivity: Readable<Record<string, AgentActivity>> = store;

/** Reactive, newest-first, bounded session activity history for the timeline panel (CPE-400). */
export const agentTimeline: Readable<TimelineEntry[]> = timeline;

/** Prepend a batch to the timeline, newest-first, capped (pure). `baseId` seeds unique keys. */
export function mergeTimeline(
  prev: TimelineEntry[],
  items: FsActivity[],
  now: number,
  baseId: number,
  cap = TIMELINE_CAP,
): TimelineEntry[] {
  const created = items.map((it, i) => ({ id: baseId + i, kind: it.kind, path: it.path, at: now }));
  return [...created.reverse(), ...prev].slice(0, cap);
}

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

/** Fold a normalized batch into the transient map + durable timeline. */
function applyItems(items: FsActivity[], now: number): void {
  store.update((prev) => foldActivities(prev, items, now));
  timeline.update((prev) => mergeTimeline(prev, items, now, nextId));
  nextId += items.length;
}

/** Fold a raw event payload into the transient map + the durable timeline (headless-testable). */
export function ingestActivity(payload: unknown, now = Date.now()): void {
  const items = normalizeFsActivity(payload);
  if (items.length) applyItems(items, now);
}

/**
 * Whether a batch changes which rows belong in `folder` — i.e. a create/remove/rename of a DIRECT
 * child (CPE-401). Drives a live re-list so new files appear and deleted ones vanish. A `modified`
 * doesn't change membership (its row already exists; the annotation is enough), so it's excluded.
 */
export function affectsListing(items: FsActivity[], folder: string): boolean {
  const f = normalizePath(folder);
  if (!f) return false;
  return items.some((it) => {
    if (it.kind === "modified") return false;
    const p = normalizePath(it.path);
    const cut = p.lastIndexOf("/");
    return cut > 0 && p.slice(0, cut) === f;
  });
}

/** Clear all activity + timeline (on stop-watching), so a stale folder never bleeds into a new one. */
export function clearActivity(): void {
  store.set({});
  timeline.set([]);
}

/**
 * Start consuming activity events + expiring stale entries. Returns a teardown that unlistens,
 * stops the pruner, and clears the map. Call when watching begins; call the teardown when it ends.
 */
export async function initAgentActivity(
  onBatch?: (items: FsActivity[]) => void,
): Promise<() => void> {
  const unlisten = await listen("ai-console://fs-activity", (e) => {
    const items = normalizeFsActivity(e.payload);
    if (!items.length) return;
    applyItems(items, Date.now());
    onBatch?.(items); // lets the explorer live-refresh the listing (CPE-401)
  });
  const timer = setInterval(() => store.update((m) => pruneActivities(m, Date.now())), 1000);
  return () => {
    unlisten();
    clearInterval(timer);
    clearActivity();
  };
}
