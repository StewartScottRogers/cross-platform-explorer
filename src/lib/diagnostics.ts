import { writable, type Readable } from "svelte/store";

/**
 * Diagnostics mode (CPE-758): time **every** backend/OS call and surface it on screen.
 *
 * All OS resource access goes through the shared `invoke` / `rawInvoke` wrapper (`./invoke`), so
 * instrumenting that one chokepoint covers everything — directory listings, disk space, git status,
 * dir sizes, checksums, previews, search, archive reads, thumbnails — and any *future* backend call is
 * covered for free, as long as it uses the wrapper (which the busy-cursor convention already requires).
 *
 * Off by default and gated by a persisted setting the user toggles from the Application menu; when off,
 * {@link recordCall} is a no-op so there is zero overhead on the normal path.
 */

/** One recorded backend call. */
export interface DiagCall {
  cmd: string;
  ms: number;
  at: number;
  ok: boolean;
}

/** How many recent calls to keep. */
export const DIAG_CAP = 80;

let enabled = false;
const store = writable<DiagCall[]>([]);

/** Recent backend calls, newest-first. Empty while Diagnostics is off. */
export const diagCalls: Readable<DiagCall[]> = store;

/** Enable/disable recording (driven by the Application → Diagnostics toggle). Clears history when off. */
export function setDiagnosticsEnabled(on: boolean): void {
  enabled = on;
  if (!on) store.set([]);
}

/** Whether recording is active — lets the invoke wrapper skip all work when Diagnostics is off. */
export function diagnosticsEnabled(): boolean {
  return enabled;
}

/** Record one backend call's duration. No-op when Diagnostics is off (zero overhead on the normal path). */
export function recordCall(cmd: string, ms: number, ok = true): void {
  if (!enabled) return;
  store.update((prev) => [{ cmd, ms: Math.round(ms), at: Date.now(), ok }, ...prev].slice(0, DIAG_CAP));
}
