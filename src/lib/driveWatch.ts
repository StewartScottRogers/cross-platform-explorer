// Live removable-drive detection for the sidebar (CPE-1280, epic CPE-716). Polls `list_drives` on a
// short interval and fires a callback whenever the connected drive SET changes — a USB drive plugged in
// appears, an unplugged one drops out — so the sidebar Drives section stays live without relaunching.
// Cheap + always-on: `list_drives` only probes drive-letter roots, and the callback fires ONLY on a real
// add/remove, so an idle app does no re-render work (honours the fast/small/predictable tiebreaker). The
// diff is pure + injectable for unit tests; only the live poll touches the backend.
//
// Distinct from `driveScheduler.ts` (CPE-797), which is opt-in and runs backup jobs on a connect
// transition — this watcher is always on and only refreshes the drive list in the UI.

import { commands } from "./bindings.gen"; // typed client (CPE-964)
import type { Place } from "./types";

/**
 * Stable signature of a drive set: sorted, case-normalised drive-root paths joined by a space (drive
 * roots — `C:\`, `/` — never contain spaces). Two lists with the same members (any order) share a
 * signature, so the watcher only fires on a genuine drive add/remove — not on a reordering.
 */
export function drivesSignature(drives: Place[]): string {
  return drives
    .map((d) => d.path.toUpperCase())
    .sort()
    .join(" ");
}

/** True when `cur` has a different member set than `prev` (order-independent). */
export function drivesChanged(prev: Place[], cur: Place[]): boolean {
  return drivesSignature(prev) !== drivesSignature(cur);
}

let timer: ReturnType<typeof setInterval> | null = null;
let lastSig = "";
let ticking = false;
let onChange: ((drives: Place[]) => void) | null = null;

/**
 * Start (or restart) the watcher. Seeds the current drive set so an already-present drive doesn't fire at
 * launch, then polls every `everyMs`, calling `cb(drives)` whenever the set changes. `cb` receives the
 * fresh list so the caller can reassign it and refresh usage/eject state.
 */
export async function startDriveWatch(cb: (drives: Place[]) => void, everyMs = 4000): Promise<void> {
  stopDriveWatch();
  onChange = cb;
  try {
    lastSig = drivesSignature(await commands.listDrives()); // seed: don't fire for drives present at start
  } catch {
    lastSig = "";
  }
  timer = setInterval(() => void driveWatchTick(), everyMs);
}

/** One poll: re-enumerate drives and fire the callback if the set changed. Never overlaps a slow poll. */
export async function driveWatchTick(): Promise<void> {
  if (ticking || !onChange) return; // don't overlap; inert until started
  ticking = true;
  try {
    const cur = await commands.listDrives();
    const sig = drivesSignature(cur);
    if (sig !== lastSig) {
      lastSig = sig;
      onChange(cur);
    }
  } catch {
    // list_drives failed this tick — try again next
  } finally {
    ticking = false;
  }
}

/** Force a check right now (e.g. on window focus) for instant feedback after alt-tabbing back. */
export async function pokeDriveWatch(): Promise<void> {
  await driveWatchTick();
}

/** Stop polling. Idempotent. */
export function stopDriveWatch(): void {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
  lastSig = "";
  onChange = null;
}
