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

/** One file the agent has READ this session (CPE-741) — the durable "consulted" set. Unlike the
 *  transient activity map (which fades) and the interleaved timeline, this is a deduped, newest-first
 *  list of just the reads, so you can see the context the agent gathered. */
export interface ConsultedEntry {
  path: string;
  /** When it was most recently read (epoch ms). */
  at: number;
  /** How many times it's been read this session. */
  count: number;
}

/** How many consulted files to retain (newest-first), bounded like the timeline. */
export const CONSULTED_CAP = 500;

const store = writable<Record<string, AgentActivity>>({});
const timeline = writable<TimelineEntry[]>([]);
const consulted = writable<ConsultedEntry[]>([]);
let nextId = 0;

/** Reactive per-path activity map (keyed by absolute path). Empty when not watching. */
export const fsActivity: Readable<Record<string, AgentActivity>> = store;

/** Reactive, newest-first, bounded session activity history for the timeline panel (CPE-400). */
export const agentTimeline: Readable<TimelineEntry[]> = timeline;

/** Reactive, newest-first, deduped set of files the agent has READ this session (CPE-741). Empty when
 *  not watching / when the agent reports no reads. */
export const agentConsulted: Readable<ConsultedEntry[]> = consulted;

/** Fold a batch into the consulted set (pure): each `read` dedupes by path, bumps its count + recency,
 *  and moves to the front; non-reads are ignored. Bounded to `cap`. Returns `prev` when unchanged. */
export function foldConsulted(
  prev: ConsultedEntry[],
  items: FsActivity[],
  now: number,
  cap = CONSULTED_CAP,
): ConsultedEntry[] {
  let next = prev;
  let changed = false;
  for (const it of items) {
    if (it.kind !== "read") continue;
    if (!changed) {
      next = prev.slice();
      changed = true;
    }
    const i = next.findIndex((e) => e.path === it.path);
    const count = i >= 0 ? next[i].count + 1 : 1;
    if (i >= 0) next.splice(i, 1);
    next.unshift({ path: it.path, at: now, count });
  }
  if (changed && next.length > cap) next = next.slice(0, cap);
  return changed ? next : prev;
}

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

/** Fold a batch of activities into the previous map (pure; newest kind + timestamp win per path).
 *  A `read` (CPE-405) is the weakest signal: it never overwrites a stronger mutation kind already
 *  recorded for that path, so a file the agent edited *and* read still annotates as edited. */
export function foldActivities(
  prev: Record<string, AgentActivity>,
  items: FsActivity[],
  now: number,
): Record<string, AgentActivity> {
  const next = { ...prev };
  for (const it of items) {
    const existing = next[it.path];
    if (it.kind === "read" && existing && existing.kind !== "read") continue;
    next[it.path] = { kind: it.kind, at: now };
  }
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
  consulted.update((prev) => foldConsulted(prev, items, now));
  nextId += items.length;
}

/** Fold a raw event payload into the transient map + the durable timeline (headless-testable). */
export function ingestActivity(payload: unknown, now = Date.now()): void {
  const items = normalizeFsActivity(payload);
  if (items.length) applyItems(items, now);
}

/**
 * Whether any of `paths` is a descendant of `dir` — i.e. the agent is changing files somewhere
 * inside this folder (CPE-402). Used to light up folder rows so you can follow the agent down the
 * tree. Excludes `dir` itself. Cross-platform (case/separator-normalized).
 */
export function folderHasActivity(paths: string[], dir: string): boolean {
  return folderHasActivityNorm(normalizeActivityPaths(paths), dir);
}

/** Normalize a list of activity paths once, so the per-row folder check (below) doesn't re-normalize
    the whole list for every folder row (CPE-698). Empty results are dropped — they never match. */
export function normalizeActivityPaths(paths: string[]): string[] {
  const out: string[] = [];
  for (const p of paths) {
    const n = normalizePath(p);
    if (n) out.push(n);
  }
  return out;
}

/** Like {@link folderHasActivity} but takes **already-normalized** paths (from
    {@link normalizeActivityPaths}), so a whole file list normalizes the activity set once instead of
    once per folder row (CPE-698). */
export function folderHasActivityNorm(normPaths: string[], dir: string): boolean {
  const d = normalizePath(dir);
  if (!d) return false;
  const prefix = d + "/";
  return normPaths.some((p) => p.startsWith(prefix));
}

/**
 * Whether a batch changes which rows belong in `folder` — i.e. a create/remove/rename of a DIRECT
 * child (CPE-401). Drives a live re-list so new files appear and deleted ones vanish. A `modified`
 * or `read` doesn't change membership (its row already exists; the annotation is enough), so those
 * are excluded — a read never re-lists.
 */
export function affectsListing(items: FsActivity[], folder: string): boolean {
  const f = normalizePath(folder);
  if (!f) return false;
  return items.some((it) => {
    if (it.kind === "modified" || it.kind === "read") return false;
    const p = normalizePath(it.path);
    const cut = p.lastIndexOf("/");
    return cut > 0 && p.slice(0, cut) === f;
  });
}

/** Clear all activity + timeline (on stop-watching), so a stale folder never bleeds into a new one. */
export function clearActivity(): void {
  store.set({});
  timeline.set([]);
  consulted.set([]);
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
