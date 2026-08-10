/**
 * Navigation Mode's core mode-state reducer (CPE-1552, foundation of epic CPE-1487 "Keyboard
 * Navigation Mode" — an opt-in vim-modal layer over the file list). Pure, DOM-free state model +
 * key-to-intent mapping: given the current `NavState` and an already-normalized
 * `KeyboardEvent.key` value, `reduceNavKey` returns the next state and the `NavIntent` a consumer
 * should act on. This file only knows about mode state and key-to-intent mapping — nothing about
 * what an intent *does* (no imports from `App.svelte`, `selection.ts`, or `commandPalette.ts`).
 *
 * INERT: nothing calls this yet. `App.svelte`'s `handleKeydown` is untouched, so landing this
 * ships with zero behavior change. `src/lib/settings.ts`'s `navigationModeEnabled` (default off)
 * gates the feature once a later ticket (CPE-1556) wires this reducer into real key handling.
 *
 * V1 grammar is unmodified single keys only (no Ctrl/Alt/Shift chords) — matching the vim TUIs
 * surveyed (ranger, vifm, nnn, lf, felix, xplr). Mirrors `keymap.ts`'s "inert plumbing first"
 * shape (CPE-1547).
 */

/** The two modes Navigation Mode V1 models. `"normal"` is the default resting state; `"visual"`
 *  is entered/exited via `v` (or forced back to `"normal"` via `Escape`) and is where a motion
 *  extends a range selection instead of just moving. */
export type NavMode = "normal" | "visual";

/** The full modal state: the current mode, plus the two pending-input buffers a vim-style
 *  grammar needs. `pendingChord` buffers a multi-key sequence's first key (currently only `"g"`,
 *  awaiting a second `g` for `gg`/top). `pendingCount` buffers a typed numeric prefix (`"3"`
 *  before `j` means "3 down"), vim-style. */
export interface NavState {
  mode: NavMode;
  pendingChord: string;
  pendingCount: string;
}

/** A fresh, resting `NavState`: normal mode, no buffered chord or count. */
export function initialNavState(): NavState {
  return { mode: "normal", pendingChord: "", pendingCount: "" };
}

/** What a single reduced key means to a consumer. `reduceNavKey` never throws and never blocks an
 *  unrelated key from being seen as unhandled by a caller — an unrecognized key just yields
 *  `{ kind: "none" }`. */
export type NavIntent =
  | { kind: "none" }
  | { kind: "motion"; dir: "left" | "down" | "up" | "right" | "top" | "bottom"; count: number }
  | { kind: "enterVisual" }
  | { kind: "exitVisual" }
  | { kind: "op"; op: "delete" | "yank" | "paste" }
  | { kind: "startFilter" }
  | { kind: "startCommand" };

/** Clears both pending-input buffers, leaving `mode` untouched. Every reduction that "resolves"
 *  to a real intent (a motion, an op, entering/exiting visual, starting filter/command, or an
 *  unrecognized key) clears the buffers — only a bare `g` (awaiting its pair) or a digit
 *  (awaiting more digits or a motion) leaves a buffer populated. */
function clearBuffers(state: NavState): NavState {
  return { ...state, pendingChord: "", pendingCount: "" };
}

/** Parses `pendingCount` into a vim-style repeat count: empty buffer defaults to `1`; otherwise
 *  the accumulated digits, parsed as a positive integer (never `0` or `NaN` — `"0"` alone can
 *  never be typed since `0` only accumulates *mid*-count). */
function parseCount(pendingCount: string): number {
  if (!pendingCount) return 1;
  const n = parseInt(pendingCount, 10);
  return Number.isFinite(n) && n > 0 ? n : 1;
}

const MOTION_DIRS: Record<string, "left" | "down" | "up" | "right"> = {
  h: "left",
  j: "down",
  k: "up",
  l: "right",
};

/**
 * Reduce a single already-normalized `KeyboardEvent.key` value against the current `NavState`,
 * returning the next state and the intent the key produces. Pure — no DOM, no side effects,
 * safe to call speculatively or replay in a test.
 */
export function reduceNavKey(state: NavState, key: string): { state: NavState; intent: NavIntent } {
  // Escape always forces normal mode and clears both buffers, regardless of what was pending.
  if (key === "Escape") {
    const wasVisual = state.mode === "visual";
    return {
      state: { mode: "normal", pendingChord: "", pendingCount: "" },
      intent: wasVisual ? { kind: "exitVisual" } : { kind: "none" },
    };
  }

  // A buffered `g` awaits its pair. A second `g` emits "motion top" (consuming any pending
  // count); any other key clears the buffer and re-dispatches that key from a clean state, so it
  // is never silently swallowed.
  if (state.pendingChord === "g") {
    if (key === "g") {
      const count = parseCount(state.pendingCount);
      return { state: clearBuffers(state), intent: { kind: "motion", dir: "top", count } };
    }
    return reduceNavKey({ ...state, pendingChord: "" }, key);
  }

  // Digits 1-9 always accumulate into pendingCount; "0" only accumulates once a count is already
  // in progress (a bare "0" alone is not a vim count prefix).
  if (/^[1-9]$/.test(key) || (key === "0" && state.pendingCount !== "")) {
    return { state: { ...state, pendingCount: state.pendingCount + key }, intent: { kind: "none" } };
  }

  const count = parseCount(state.pendingCount);

  const dir = MOTION_DIRS[key];
  if (dir) {
    return { state: clearBuffers(state), intent: { kind: "motion", dir, count } };
  }

  switch (key) {
    case "g":
      // Start buffering, preserving any already-typed count so "3gg" can still jump to line 3.
      return { state: { ...state, pendingChord: "g" }, intent: { kind: "none" } };

    case "G":
      return { state: clearBuffers(state), intent: { kind: "motion", dir: "bottom", count } };

    case "v":
      return state.mode === "visual"
        ? { state: { mode: "normal", pendingChord: "", pendingCount: "" }, intent: { kind: "exitVisual" } }
        : { state: { mode: "visual", pendingChord: "", pendingCount: "" }, intent: { kind: "enterVisual" } };

    case "d":
      return { state: clearBuffers(state), intent: { kind: "op", op: "delete" } };
    case "y":
      return { state: clearBuffers(state), intent: { kind: "op", op: "yank" } };
    case "p":
      return { state: clearBuffers(state), intent: { kind: "op", op: "paste" } };

    case "/":
      return { state: clearBuffers(state), intent: { kind: "startFilter" } };
    case ":":
      return { state: clearBuffers(state), intent: { kind: "startCommand" } };

    default:
      // Unrecognized key: clear pending buffers, emit nothing, never throw.
      return { state: clearBuffers(state), intent: { kind: "none" } };
  }
}
