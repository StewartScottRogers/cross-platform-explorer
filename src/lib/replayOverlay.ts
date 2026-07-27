import type { AuditEvent, Baseline, ReplayEntry } from "./bindings.gen";
import { stateAtFrom, childrenAt, type FsState } from "./replayFold";
import type { DirEntry } from "./types";

/**
 * Read-only, ephemeral overlay of the MAIN explorer file pane while scrubbing Replay mode (CPE-1112,
 * epic CPE-728 slice e — the "graduate" of CPE-1111's in-drawer reconstruction). Pure, Svelte-free,
 * invoke-free: given the same `replayFold.ts` fold CPE-1111 already uses, derive a `DirEntry[]` shaped
 * exactly like the live listing so `ExplorerPane`/`FileList` can render it with ZERO component changes
 * beyond swapping which array they're fed. Never reads or writes any live store — every function here
 * takes its inputs as plain arguments and returns a new value.
 *
 * The mode-gating contract (`resolveOverlay`) is what makes "off-means-off" and "restore-on-exit" true
 * by construction rather than by remembering to clean something up: it's a pure function of
 * (isActive, replayData, t, dir) that returns `null` whenever Replay mode is off OR there's no data to
 * show. The caller wires this into a Svelte `$:` block and feeds the result straight into
 * `ExplorerPane`'s `replayOverlay` prop — flipping the toggle off (or the session/tab changing away)
 * makes the next reactive re-evaluation return `null`, and `ExplorerPane` falls back to its live
 * `visible` listing on its own. There is no imperative "restore" step to forget.
 */

/** Split `path` into normalized `/`-segments — mirrors `replayFold.ts`'s internal `segments()` helper
 *  so a path compares identically here regardless of which separator produced it. */
function segments(path: string): string[] {
  return path.replace(/\\/g, "/").split("/").filter((s) => s.length > 0);
}

/**
 * Every normalized path (joined segment string) that has at least one deeper entry in `state` — i.e. a
 * reconstructed directory. Best-effort: `ReplayEntry`/`FsNode` carry no `is_dir` field, so a
 * currently-empty directory (nothing visible inside it at this scrub moment) can't be distinguished
 * from a file this way; this mirrors "does this folder show anything inside it right now" (same
 * semantics as the in-drawer reconstruction's `replayDirSet` in `AgentTimeline.svelte`).
 */
export function dirSetOf(state: FsState): Set<string> {
  const dirs = new Set<string>();
  for (const path of state.keys()) {
    const segs = segments(path);
    for (let i = 1; i < segs.length; i++) dirs.add(segs.slice(0, i).join("/"));
  }
  return dirs;
}

/**
 * Convert reconstructed replay rows (`childrenAt` output) into `DirEntry`-shaped objects — the exact
 * shape the live `FileList` already renders — so the overlay reuses that component with no changes.
 * Pure: allocates a fresh array/objects, never mutates `rows` or `dirs`. Fields with no replay analogue
 * are synthesized conservatively: `size` is always 0 (the reconstruction tracks presence, not stat
 * data), `modified` is the last-touch ts (`null` for a `"baseline"` entry — it was never itself an
 * event), and `hidden` follows the OS-agnostic leading-dot convention used elsewhere in the fold.
 */
export function toDirEntries(rows: ReplayEntry[], dirs: Set<string>): DirEntry[] {
  return rows.map((r) => {
    const is_dir = dirs.has(segments(r.path).join("/"));
    const dot = !is_dir ? r.name.lastIndexOf(".") : -1;
    const extension = dot > 0 ? r.name.slice(dot + 1).toLowerCase() : "";
    return {
      name: r.name,
      path: r.path,
      is_dir,
      size: 0,
      modified: r.kind === "baseline" ? null : r.ts,
      extension,
      hidden: r.name.startsWith("."),
    };
  });
}

/**
 * Full derivation: baseline + events + scrub time + directory → a read-only `DirEntry[]` overlay for
 * the main file pane, via the same `stateAtFrom`/`childrenAt` fold CPE-1111 already uses in the
 * Replay-tab drawer. Never mutates `baseline` or `events` (the fold builds its own state).
 */
export function overlayEntriesAt(
  baseline: Baseline | null | undefined,
  events: AuditEvent[],
  tMs: number,
  dir: string,
): DirEntry[] {
  const state = stateAtFrom(baseline, events, tMs);
  return toDirEntries(childrenAt(state, dir), dirSetOf(state));
}

/** The minimal replay payload `resolveOverlay` needs — a subset of `ReplayData` + the baseline. */
export interface ReplaySource {
  baseline: Baseline | null;
  events: AuditEvent[];
}

/**
 * Mode-gating: the ONE function that decides whether the main file pane shows the reconstruction.
 * Returns `null` (⇒ "show the live listing, unchanged") whenever `active` is false or `source` is
 * absent (no session loaded / load failed / session changed and the reload hasn't landed yet) — never
 * throws, never assumes `source` is populated just because `active` is true. Otherwise returns the
 * `DirEntry[]` overlay for `dir` at `tMs`.
 *
 * This single function IS the "Replay mode toggle" contract: the caller re-derives it on every change
 * to any of its inputs (a Svelte `$:` block), so toggling `active` off — or the tab changing away from
 * Replay, or the watched session changing — restores the live listing on the very next reactive tick,
 * with no separate cleanup path to get wrong.
 */
export function resolveOverlay(
  active: boolean,
  source: ReplaySource | null,
  tMs: number,
  dir: string,
): DirEntry[] | null {
  if (!active || !source) return null;
  return overlayEntriesAt(source.baseline, source.events, tMs, dir);
}
