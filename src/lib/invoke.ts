// CPE-548: shared busy-tracking `invoke` wrapper — the global boundary for the busy cursor (CPE-547).
//
// Importing `invoke` from HERE (instead of `@tauri-apps/api/core`) makes any Tauri command call flip
// the app-wide wait cursor when it runs long, for free — the busy tracker (CPE-482, `./busy`) handles
// the ~150 ms debounce + ref-counting, so fast calls never flicker and overlapping calls clear only on
// the last one. This is the single mechanism the epic's "everywhere" goal needs: call sites opt IN just
// by choosing this import.
//
// Streaming / self-progress call sites (agent sessions, updater downloads, sidecar streaming) that
// already show their own progress import `rawInvoke` instead, so they don't double-signal (see CPE-550).
import { invoke as coreInvoke } from "@tauri-apps/api/core";
import { withBusy } from "./busy";

/**
 * The untracked Tauri invoke — identical to `@tauri-apps/api/core`'s `invoke`, with NO busy-cursor
 * tracking. Use this only for streaming / self-progress operations that opt out of the global wait
 * cursor (CPE-550). Everywhere else, prefer the default {@link invoke} export.
 */
export const rawInvoke = coreInvoke;

/**
 * Tauri `invoke`, wrapped so a long-running call raises the app-wide busy cursor (CPE-547). A drop-in
 * replacement for `@tauri-apps/api/core`'s `invoke`: same arguments, same return value, and errors
 * propagate unchanged. The busy guard is released on BOTH resolve and reject, so a failing command can
 * never leave the cursor stuck.
 */
export function invoke<T = unknown>(...args: Parameters<typeof coreInvoke>): Promise<T> {
  return withBusy(() => coreInvoke<T>(...args));
}
