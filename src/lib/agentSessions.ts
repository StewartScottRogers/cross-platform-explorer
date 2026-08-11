import { writable, type Readable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import {
  applySessionAnnouncement,
  parseSessionAnnouncement,
  type AgentSession,
} from "./sidecar";
import { ingestSessionAnnouncement } from "./agentSessionMetrics";

/**
 * Live registry of coding-agent sessions launched from the Agent Deck (Agent Watch, CPE-396).
 *
 * The host forwards each `session:<json>` Status the console emits as an `ai-console://session`
 * Tauri event; here we decode it and fold it into a reactive list the left pane (CPE-397) renders
 * and the watcher (CPE-398) anchors to. Strictly additive + idle-by-default: nothing is allocated
 * and no watching happens until a session actually announces itself, so the plain explorer with no
 * agent running is completely unaffected (AGENT-WATCH.md: "off means off").
 */

const store = writable<AgentSession[]>([]);

/** Reactive list of currently-active agent sessions (empty when none are running). */
export const agentSessions: Readable<AgentSession[]> = store;

/** Normalize a path for cross-platform comparison: forward slashes, no trailing slash, lowercased
 *  (Windows is case-insensitive; over-matching two truly-distinct case-only paths on Linux is a
 *  benign edge for this "which project am I in" check). */
export function normalizePath(p: string): string {
  return p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

/**
 * The deepest running-agent Project folder that contains — or equals — `current`, or "" if the
 * explorer isn't inside any agent's project (CPE-399). Drives when Agent Watch turns on/off:
 * navigating into a watched agent's tree watches it; leaving stops it (off means off).
 */
export function watchTargetFor(sessions: AgentSession[], current: string): string {
  const c = normalizePath(current);
  let best = "";
  for (const s of sessions) {
    const cw = normalizePath(s.cwd);
    if ((c === cw || c.startsWith(cw + "/")) && cw.length > normalizePath(best).length) best = s.cwd;
  }
  return best;
}

/**
 * Grows the "visited this run" set that gates which sessions actually get an armed filesystem watcher
 * (CPE-1606). If `current` falls inside a running session's project, that session's id is added — once
 * a project is visited, it stays watched for the rest of the session's life, even after the explorer
 * navigates away. That retention is deliberate: without it, hopping between two sibling agent projects
 * (`/work/api` then `/work/web`, both running agents) would tear down and re-arm a `notify` watcher on
 * every single navigation, and it would silently truncate the Radar/Cost/History tabs (CPE-1099/1107) to
 * whatever the explorer happens to be looking at *right now* instead of everything you've actually
 * looked at this run. A session you never open, on the other hand, is never added here, so it never gets
 * a watcher — restoring the "off means off" boundary `AGENT-WATCH.md` promises: watching costs nothing
 * for a project you haven't opened.
 *
 * Also prunes ids for sessions that are no longer running, so the set can't grow without bound across a
 * long-lived app session. Pure, and returns the same `visited` instance when nothing changes, so callers
 * (and tests) can rely on reference equality to skip redundant reconcile work.
 */
export function markVisited(
  sessions: AgentSession[],
  current: string,
  visited: ReadonlySet<string>,
): Set<string> {
  const running = new Set(sessions.map((s) => s.sessionId));
  const target = watchTargetFor(sessions, current);
  const hitId = target
    ? sessions.find((s) => normalizePath(s.cwd) === normalizePath(target))?.sessionId
    : undefined;

  let changed = hitId !== undefined && !visited.has(hitId);
  if (!changed) for (const id of visited) if (!running.has(id)) { changed = true; break; }
  if (!changed) return visited as Set<string>;

  const next = new Set<string>();
  for (const id of visited) if (running.has(id)) next.add(id);
  if (hitId) next.add(hitId);
  return next;
}

/**
 * The set of running-agent sessions to actually watch (CPE-1606): only sessions whose project folder
 * the explorer has visited at least once this run — i.e. `visited` (grown by `markVisited` as
 * navigation happens) — not "every currently-running session" (the CPE-1099 behavior this replaces,
 * which kept a watcher armed for a project the explorer never opened; see AGENT-WATCH.md's "off means
 * off" boundary). Kept pure and identity-shaped so it stays trivially testable: no sessions, or nothing
 * visited yet, ⇒ empty ⇒ nothing is ever armed.
 */
export function watchTargets(sessions: AgentSession[], visited: ReadonlySet<string>): AgentSession[] {
  return sessions.filter((s) => visited.has(s.sessionId));
}

/** Test/introspection helper: the current session list synchronously. */
export function currentSessions(): AgentSession[] {
  let snapshot: AgentSession[] = [];
  store.subscribe((v) => (snapshot = v))();
  return snapshot;
}

/** Apply one raw `session:<json>` payload to the store (exposed for headless tests). Also folds the
 *  same announcement into `agentSessionMetrics` (CPE-1107) — the started/ended stamp is a second fold
 *  hung off this one ingest path, not a new listener. */
export function ingestSessionState(state: string): void {
  const ann = parseSessionAnnouncement(state);
  if (ann) {
    store.update((list) => applySessionAnnouncement(list, ann));
    ingestSessionAnnouncement(ann);
  }
}

/** Clear every active-session leaf at once — used when the whole Agent Deck is stopped from the
 *  explorer (its process is reaped, so no per-session `ended` announcements arrive). CPE-457. */
export function clearAgentSessions(): void {
  store.set([]);
}

/**
 * Start listening for session announcements. Returns an unlisten function. Safe to call when the
 * sidecar platform is off — the event simply never fires. Call once at app start.
 */
export async function initAgentSessions(): Promise<() => void> {
  const unlisten = await listen<string>("ai-console://session", (e) => {
    if (typeof e.payload === "string") ingestSessionState(e.payload);
  });
  return unlisten;
}
