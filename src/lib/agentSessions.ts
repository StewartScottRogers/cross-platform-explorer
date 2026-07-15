import { writable, type Readable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import {
  applySessionAnnouncement,
  parseSessionAnnouncement,
  type AgentSession,
} from "./sidecar";

/**
 * Live registry of coding-agent sessions launched from the AI Console (Agent Watch, CPE-396).
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

/** Test/introspection helper: the current session list synchronously. */
export function currentSessions(): AgentSession[] {
  let snapshot: AgentSession[] = [];
  store.subscribe((v) => (snapshot = v))();
  return snapshot;
}

/** Apply one raw `session:<json>` payload to the store (exposed for headless tests). */
export function ingestSessionState(state: string): void {
  const ann = parseSessionAnnouncement(state);
  if (ann) store.update((list) => applySessionAnnouncement(list, ann));
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
