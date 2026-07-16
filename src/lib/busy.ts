// Global "busy" tracker for perceptible wait conditions (CPE-482). Shows the OS wait/`progress`
// cursor app-wide while any operation is in flight, so the app never looks frozen. Reference-counted
// (nested/concurrent operations are handled) and debounced (~150 ms) so instant operations don't
// flash the cursor. Toggles `document.body.classList` `busy`; app.css maps that to `cursor: progress`.
import { writable } from "svelte/store";

/** How long an operation must run before the busy cursor appears — avoids flicker on fast calls. */
export const SHOW_AFTER_MS = 150;

/** Reactive flag mirroring whether the busy cursor is currently shown (for components that care). */
export const busy = writable(false);

let inFlight = 0;
let shown = false;
let timer: ReturnType<typeof setTimeout> | null = null;

function setShown(value: boolean): void {
  if (shown === value) return;
  shown = value;
  busy.set(value);
  if (typeof document !== "undefined" && document.body) {
    document.body.classList.toggle("busy", value);
  }
}

/**
 * Mark the start of a perceptible operation; returns an idempotent `end` function to call when it
 * finishes (always call it — including on error). The cursor appears only if the operation outlives
 * the debounce, and clears when the LAST in-flight operation ends.
 */
export function beginBusy(): () => void {
  inFlight += 1;
  if (inFlight === 1 && timer === null && !shown) {
    timer = setTimeout(() => {
      timer = null;
      if (inFlight > 0) setShown(true);
    }, SHOW_AFTER_MS);
  }
  let released = false;
  return () => {
    if (released) return;
    released = true;
    inFlight = Math.max(0, inFlight - 1);
    if (inFlight === 0) {
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
      setShown(false);
    }
  };
}

/** Wrap an async (or sync) operation so the busy cursor tracks it, clearing on resolve OR reject. */
export async function withBusy<T>(op: () => Promise<T> | T): Promise<T> {
  const end = beginBusy();
  try {
    return await op();
  } finally {
    end();
  }
}

/** Test-only: reset the tracker between cases. */
export function _resetBusy(): void {
  inFlight = 0;
  shown = false;
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  busy.set(false);
  if (typeof document !== "undefined" && document.body) document.body.classList.remove("busy");
}
