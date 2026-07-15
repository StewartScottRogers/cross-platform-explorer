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
