import type { AuditEvent, Baseline, FsNode, ReplayEntry } from "./bindings.gen";

/**
 * Pure, dependency-free port of the Rust replay reconstruction fold (CPE-1110, epic CPE-728 slice c).
 *
 * The backend (`crates/server/src/replay.rs` + `replay_view.rs`) reconstructs "what did the watched
 * folder look like at moment T" by folding the durable audit-event stream over a baseline snapshot.
 * `replay_load` ships that raw data (events + baseline) to the frontend once, on Replay-tab open; these
 * functions then re-derive the folder listing per scrub tick **entirely client-side**, so scrubbing
 * costs no IPC round-trip per tick.
 *
 * This is a FAITHFUL 1:1 port of the Rust semantics — it must agree with `replay::state_at_from` and
 * `replay_view::children_at` on every scenario (`replayFold.test.ts` uses the Rust tests as the oracle):
 * - `created`/`modified` → insert/overwrite the path's node (`ts`/`kind`).
 * - `removed` → delete the path.
 * - `renamed` → move the entry from `path` to the target parsed out of `detail` (`"-> <target>"`);
 *   an unparseable/missing `detail` is a no-op (the entry stays put).
 * - `read` (and any unrecognised kind) → no state change.
 * - Events are folded in ascending `ts` order (stable), stopping at the first event with `ts > t`.
 *
 * Kept deliberately Svelte-free and invoke-free (pure data in, pure data out) so it's trivially
 * unit-testable and reusable — mirrors the style of `agentReplay.ts`.
 */

/**
 * A reconstructed live file-set: path → its last-touch node. Mirrors the Rust `replay::FsState`
 * (`BTreeMap<String, FsNode>`). A `Map` is used here to preserve keys efficiently; `childrenAt` sorts
 * its output so ordering matches the Rust `BTreeMap`'s ascending-path iteration regardless.
 */
export type FsState = Map<string, FsNode>;

/** Marker prefix a `renamed` event's `detail` carries ahead of the target path (see `replay.rs`). */
const RENAME_TARGET_MARKER = "-> ";

/**
 * Parse a `renamed` event's target path out of `detail`, per the `"-> <target>"` convention. `null`
 * if `detail` is absent (`undefined`/`null`) or doesn't carry the marker (or the target is empty).
 */
function renameTarget(e: AuditEvent): string | null {
  const detail = e.detail;
  if (detail == null) return null;
  if (!detail.startsWith(RENAME_TARGET_MARKER)) return null;
  const target = detail.slice(RENAME_TARGET_MARKER.length);
  return target.length === 0 ? null : target;
}

/** Apply one event to `state` in place, per the fold rules above (mirrors `replay::fold`). */
function fold(state: FsState, e: AuditEvent): void {
  switch (e.kind) {
    case "created":
    case "modified":
      state.set(e.path, { ts: e.ts, kind: e.kind });
      break;
    case "removed":
      state.delete(e.path);
      break;
    case "renamed": {
      const target = renameTarget(e);
      if (target !== null) {
        state.delete(e.path);
        state.set(target, { ts: e.ts, kind: e.kind });
      }
      // unparseable target: treat as a no-op, path stays put
      break;
    }
    default:
      // "read" is an access, not a mutation; an unrecognised kind is treated the same way (no-op),
      // so the fold stays forward-compatible with new event kinds.
      break;
  }
}

/**
 * Seed an `FsState` from a baseline snapshot: each pre-existing path becomes a node with `ts = 0` and
 * kind `"baseline"` (mirrors `replay_baseline::Baseline::to_state`). A missing baseline (`null`) or one
 * with no paths yields an empty seed, making `stateAtFrom` identical to an events-only fold.
 */
export function baselineToState(baseline: Baseline | null | undefined): FsState {
  const state: FsState = new Map();
  if (!baseline) return state;
  for (const p of baseline.paths) {
    state.set(p, { ts: 0, kind: "baseline" });
  }
  return state;
}

/**
 * Reconstruct the live file-set at time `tMs`, seeded from `baseline`: seed the baseline nodes, then
 * fold every event with `ts <= tMs` in ascending `ts` order. Mirrors `replay::state_at_from` exactly
 * (an empty/`null` baseline makes it equivalent to the plain `state_at`). The input `events` array is
 * not mutated (a sorted copy is folded). JS `Array.sort` is stable (ES2019+), matching Rust's
 * `sort_by_key`, so equal-`ts` events fold in their original journal order.
 */
export function stateAtFrom(
  baseline: Baseline | null | undefined,
  events: AuditEvent[],
  tMs: number,
): FsState {
  const sorted = [...events].sort((a, b) => a.ts - b.ts);
  const state = baselineToState(baseline);
  for (const e of sorted) {
    if (e.ts > tMs) break;
    fold(state, e);
  }
  return state;
}

/**
 * Split `path` into its non-empty `/`-separated segments, normalizing `\` to `/` first (mirrors
 * `replay_view::segments`) so the split behaves identically regardless of which separator produced the
 * string — purely string-based, never `std::path` platform semantics.
 */
function segments(path: string): string[] {
  return path
    .replace(/\\/g, "/")
    .split("/")
    .filter((s) => s.length > 0);
}

/**
 * The direct children (exactly one level below `dir`) present in `state` — mirrors
 * `replay_view::children_at`. A nested grandchild (`a/b/c` under `a`) is excluded. `dir` may be the
 * root (empty string, `/`, or any string with zero non-empty segments). Output is deterministic:
 * sorted ascending by `path`, matching the Rust `BTreeMap`'s iteration order.
 */
export function childrenAt(state: FsState, dir: string): ReplayEntry[] {
  const dirSegs = segments(dir);
  const out: ReplayEntry[] = [];
  for (const [path, node] of state) {
    const segs = segments(path);
    if (segs.length !== dirSegs.length + 1) continue;
    let match = true;
    for (let i = 0; i < dirSegs.length; i++) {
      if (segs[i] !== dirSegs[i]) {
        match = false;
        break;
      }
    }
    if (!match) continue;
    out.push({ name: segs[dirSegs.length], path, ts: node.ts, kind: node.kind });
  }
  out.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  return out;
}
