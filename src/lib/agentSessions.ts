import { writable, type Readable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import {
  applySessionAnnouncement,
  parseSessionAnnouncement,
  type AgentSession,
} from "./sidecar";
import { ingestSessionAnnouncement, flushSession } from "./agentSessionMetrics";

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
 * The set of running-agent sessions to actually watch right now (CPE-1606, revised by CPE-1626): every
 * session whose project folder is the CURRENT deepest match for `current` — i.e. exactly the sessions
 * `watchTargetFor` would point at, all of them if several share that cwd (CPE-1625: fleet/parallel agents
 * plausibly point two sessions at the identical folder, so this is a `filter`, not a single lookup). Not
 * "every currently-running session" (the CPE-1099 behavior this replaces, which kept a watcher armed for
 * a project the explorer never opened) and not a sticky "visited this run" set either (the CPE-1606
 * retention this replaces — see `AGENT-WATCH.md`'s Boundaries section for why retention is no longer
 * needed now that `flushSession` tells a pause from a real end on its own, CPE-1626). A session whose
 * folder isn't the current target — because it was never opened, OR because the explorer has since
 * navigated away — is simply not in this list, so `reconcileAgentWatch` disarms it: "off means off" holds
 * both for a project you never open AND for one you've since left. Pure and identity-shaped so it stays
 * trivially testable: no sessions, or nothing at `current`, ⇒ empty ⇒ nothing is armed.
 */
export function watchTargets(sessions: AgentSession[], current: string): AgentSession[] {
  const target = watchTargetFor(sessions, current);
  return target ? sessions.filter((s) => normalizePath(s.cwd) === normalizePath(target)) : [];
}

/** Test/introspection helper: the current session list synchronously. */
export function currentSessions(): AgentSession[] {
  let snapshot: AgentSession[] = [];
  store.subscribe((v) => (snapshot = v))();
  return snapshot;
}

/** Apply one raw `session:<json>` payload to the store (exposed for headless tests). Also folds the
 *  same announcement into `agentSessionMetrics` (CPE-1107) — the started/ended stamp is a second fold
 *  hung off this one ingest path, not a new listener.
 *
 *  CPE-1626: a real `ended` announcement flushes that session's metrics row IMMEDIATELY here, regardless
 *  of whether the session happens to be currently armed/watched. Before this, the only flush trigger was
 *  `reconcileAgentWatch`'s (App.svelte) armed-set diff — which never even LOOKS at a session that was
 *  already unarmed (paused, because the explorer had navigated away from its folder): a session ending
 *  while paused, with a sibling still armed, would sit unflushed until some later reconcile happened to
 *  drain the armed set to zero. Flushing right here, at the actual lifecycle event, makes that latency
 *  gap disappear — the row is never "deferred", let alone at risk of the deck closing before it's caught.
 *  `flushSession` is itself gated on `endedAt` (CPE-1626) so this is always safe to fire-and-forget. */
export function ingestSessionState(state: string): void {
  const ann = parseSessionAnnouncement(state);
  if (ann) {
    store.update((list) => applySessionAnnouncement(list, ann));
    ingestSessionAnnouncement(ann);
    if (ann.event === "ended") void flushSession(ann.session.sessionId);
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
