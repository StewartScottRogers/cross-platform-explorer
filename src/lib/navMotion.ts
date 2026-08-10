/**
 * Navigation Mode's motion -> selection-engine bridge (CPE-1553, epic CPE-1487 "Keyboard
 * Navigation Mode"). `applyNavIntent` translates a `NavIntent` produced by `navMode.ts`'s pure
 * reducer into a new `Selection`, entirely by delegating to CPE-711's existing selection engine
 * (`selection.ts`'s `click`/`moveLead`) and CPE-769's existing grid-delta math
 * (`gridnav.ts`'s `arrowDelta`) — this file adds no new move/clamp logic of its own.
 *
 * INERT: nothing calls this yet. `App.svelte`'s key handling is untouched, so landing this ships
 * with zero behavior change; CPE-1556 is the ticket that wires `reduceNavKey` + `applyNavIntent`
 * into real keyboard handling, gated behind `navigationModeEnabled` (default off).
 */
import type { NavIntent, NavMode } from "./navMode";
import { type Selection, click, moveLead } from "./selection";
import { arrowDelta } from "./gridnav";

/** Which pane layout is active — mirrors `arrowDelta`'s "list = 1 column, grid = N columns"
 *  split, so `h`/`l` are inert in `"list"` (details/list view has no horizontal axis) but move
 *  one column left/right in `"grid"` (icon/gallery view). */
export type NavLayout = "list" | "grid";

const ARROW_KEY_FOR_DIR: Record<"left" | "down" | "up" | "right", "ArrowLeft" | "ArrowDown" | "ArrowUp" | "ArrowRight"> = {
  left: "ArrowLeft",
  down: "ArrowDown",
  up: "ArrowUp",
  right: "ArrowRight",
};

/**
 * Apply a `NavIntent` to a `Selection`, given the pane's current item count, layout, and (for
 * grid layouts) column count.
 *
 * - Non-`"motion"` intents (`none`, `enterVisual`/`exitVisual`, `op`, `startFilter`,
 *   `startCommand`) are out of scope for this bridge — CPE-1556 dispatches those directly to the
 *   existing `doCopy`/`doCut`/`doPaste`/etc. handlers — so they return `sel` unchanged.
 * - `itemCount === 0` is a no-op (nothing to move onto).
 * - `dir: "left" | "down" | "up" | "right"` resolves to a single-step index delta via the
 *   existing `arrowDelta` helper (list layout always passes 1 column, so `h`/`l` yield a 0 delta
 *   exactly like ArrowLeft/ArrowRight already do in list/details view), multiplied by
 *   `intent.count`, then handed to `moveLead` — which already does the "clamp to
 *   `[0, itemCount - 1]`, extend from the anchor when `shift`" work the mouse and today's
 *   arrow-key handling both rely on.
 * - `dir: "top" | "bottom"` jumps directly to index `0`/`itemCount - 1` (vim `gg`/`G` semantics
 *   are an absolute jump, not a lead-relative delta), applied via `click` so the shift/no-shift
 *   split matches `moveLead`'s.
 * - `mode === "normal"`: the new index becomes the sole selection (plain click / lead move).
 * - `mode === "visual"`: the new index extends the range from the existing anchor (shift-click /
 *   shift-move), so `v` + repeated motions grows/shrinks a contiguous range exactly like today's
 *   Shift+Arrow does.
 */
export function applyNavIntent(
  intent: NavIntent,
  sel: Selection,
  itemCount: number,
  mode: NavMode,
  layout: NavLayout,
  columns?: number,
): Selection {
  if (intent.kind !== "motion") return sel;
  if (itemCount === 0) return sel;

  const shift = mode === "visual";

  if (intent.dir === "top" || intent.dir === "bottom") {
    const newIndex = intent.dir === "top" ? 0 : itemCount - 1;
    return click(sel, newIndex, { ctrl: false, shift });
  }

  const cols = layout === "grid" ? Math.max(1, Math.floor(columns ?? 1) || 1) : 1;
  const step = arrowDelta(ARROW_KEY_FOR_DIR[intent.dir], cols);
  return moveLead(sel, step * intent.count, itemCount, shift);
}
