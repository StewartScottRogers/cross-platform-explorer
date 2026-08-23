/**
 * CPE-1845 — turning a `RevertOutcome` into something a person can act on.
 *
 * ## What was wrong
 *
 * Every screen that showed a revert result rendered the same thing: `applied N, skipped M`
 * (`CheckpointDialog.svelte`, `AgentTimeline.svelte`, `CopilotDialog.svelte`). Two problems with that:
 *
 * 1. **`skipped` is not one thing.** Some of those entries are genuine failures (a locked file, a
 *    missing blob). Most, in the measured cases, are deletes the engine *deliberately declined to
 *    perform* because it could not trust the checkpoint — a correct, fail-safe outcome with the user's
 *    files intact. Reported as one number they read as M problems. The backend used to make this
 *    tellable only by string-matching `"not deleted:"`; it now ships `outcome` per entry, so this
 *    module branches on a field.
 * 2. **The reasons were dropped entirely.** `src/docs/16-checkpoints.md` promised the user is told
 *    which cleanups did not happen *and why*; no screen rendered a single reason. This module produces
 *    them, and the doc's "not shown in the dialog yet" hedge goes with it.
 *
 * ## The shape it produces
 *
 * One statement plus a count — never N copies of one paragraph. The backend already collapses the
 * shared explanation into `held_back` (500 held-back deletes used to emit 500 copies of a
 * ~370-character paragraph, ~185 KB); this keeps that collapse all the way to the screen, listing at
 * most {@link MAX_LISTED} path names and counting the rest.
 *
 * `nextStep` comes from the backend rather than being composed here, because it is not the same advice
 * in both cases: a `skipped_by_plan` hold-back really does clear on a re-run, and a
 * `held_back_by_checkpoint` one never will on this machine.
 */
import type { RevertOutcome } from "./bindings.gen";

/** How many held-back paths to name before falling back to "+N more". */
export const MAX_LISTED = 8;

export type RevertSummary = {
  /** Actions the revert performed. */
  applied: number;
  /** Actions attempted that failed — genuine problems. */
  failed: number;
  /** Deletes deliberately not performed. NOT failures. */
  heldBack: number;
  /** The one-line count sentence. */
  headline: string;
  /** The single shared explanation for the hold-back, or `""` when nothing was held back. */
  reason: string;
  /** What to do about it — never "re-run" when {@link retryable} is false. */
  nextStep: string;
  /** Whether running the revert again on this machine can perform the held-back deletes. */
  retryable: boolean;
  /** Up to {@link MAX_LISTED} held-back paths, in plan order. */
  listed: string[];
  /** How many held-back paths are not in {@link listed}. */
  more: number;
  /** The genuine failures, each with its own (distinct) reason. */
  failures: { path: string; error: string }[];
};

const plural = (n: number, one: string, many: string) => (n === 1 ? one : many);

/**
 * Summarise a revert for display. Reads `outcome` discriminants only — never the wording of `error`.
 */
export function summarizeRevert(outcome: RevertOutcome): RevertSummary {
  const failures = outcome.skipped
    .filter((r) => r.outcome === "failed")
    .map((r) => ({ path: r.path, error: r.error }));
  const heldBackPaths = outcome.skipped
    .filter((r) => r.outcome === "skipped_by_plan" || r.outcome === "held_back_by_checkpoint")
    .map((r) => r.path);

  const parts = [`Applied ${outcome.applied} ${plural(outcome.applied, "change", "changes")}`];
  if (failures.length) parts.push(`${failures.length} failed`);
  if (heldBackPaths.length) {
    parts.push(`${heldBackPaths.length} ${plural(heldBackPaths.length, "deletion", "deletions")} held back`);
  }

  const held = outcome.held_back;
  return {
    applied: outcome.applied,
    failed: failures.length,
    heldBack: heldBackPaths.length,
    headline: `${parts.join(", ")}.`,
    reason: held?.reason ?? "",
    nextStep: held?.next_step ?? "",
    // No hold-back means nothing is waiting on a re-run; `false` keeps callers from offering one.
    retryable: held?.retryable ?? false,
    listed: heldBackPaths.slice(0, MAX_LISTED),
    more: Math.max(0, heldBackPaths.length - MAX_LISTED),
    failures,
  };
}
