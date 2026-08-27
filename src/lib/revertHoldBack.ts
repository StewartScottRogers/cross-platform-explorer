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
 *
 * ## CPE-1869 — the list the advice points at
 *
 * The permanent cases tell the user to delete the held-back paths themselves, but only ever showed up to
 * {@link MAX_LISTED} of them. That is fine as a *preview*; it stopped being fine as the user's only way to
 * find the rest. {@link RevertSummary.allHeldBackPaths} carries the untruncated set so a "copy full list"
 * affordance can hand it over without re-running the revert, gated on
 * {@link RevertSummary.advisesManualDelete} so it never appears on the alias/collision hold-back (those
 * paths are the checkpoint's own content under another spelling — a delete affordance there would be the
 * bug, not the fix).
 */
import type { RevertOutcome } from "./bindings.gen";

/**
 * How many held-back paths to name before falling back to "and N more".
 *
 * The cap is real: the unrestorable-key case holds back *everything added since the checkpoint*, which
 * can be thousands of paths. What it costs is recorded honestly rather than pretended away — at scale
 * the advice ("delete these files yourself") and the on-screen list stop agreeing, because the list is
 * truncated. CPE-1845 left the full set visible nowhere else; CPE-1869 closes that with a copy-full-list
 * affordance (see {@link RevertSummary.allHeldBackPaths} / {@link RevertSummary.advisesManualDelete} and
 * `RevertOutcomePanel.svelte`) rather than raising this cap — 8 is still a fine preview once the rest is
 * retrievable.
 */
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
  /**
   * Up to {@link MAX_LISTED} held-back paths, in plan order, each with the detail the backend produced
   * for *that* path (usually `""` — the shared explanation is {@link RevertSummary.reason}). For the
   * alias/collision hold-back the detail names the checkpoint entry the path collides with, which the
   * engine's own comment calls the one thing that genuinely differs per path; dropping it threw away the
   * single most useful per-path fact. Empty for every high-volume case, so it costs nothing at 200 paths.
   */
  listed: { path: string; detail: string }[];
  /**
   * How many held-back paths are not in {@link listed}. Never `1`: replacing one name with the line
   * "and 1 more" is longer than the name it replaced, so the cap stretches by one instead.
   */
  more: number;
  /**
   * **CPE-1869.** Every held-back path, untruncated, in plan order — the answer to "the held-back list
   * tells you to delete files it will not show you". Never rendered as a DOM list ({@link listed} is the
   * on-screen preview, capped at {@link MAX_LISTED}); this is read only by the copy-full-list affordance,
   * so a 200-path revert costs one clipboard write, not 200 extra rows.
   */
  allHeldBackPaths: string[];
  /**
   * **CPE-1869.** Whether the backend's advice is "delete these files yourself" (an empty checkpoint, an
   * unrestorable checkpoint key, or a permanent write refusal) as opposed to "nothing needs doing" (the
   * alias/collision hold-back, where {@link listed}'s paths ARE the checkpoint's own content under
   * another spelling — offering to delete them would destroy it) or "run it again" (the retryable
   * hold-back, where nothing needs deleting yet). Read straight off the backend's
   * `HeldBackSummary::advises_manual_delete` — never inferred from {@link nextStep}'s wording, which is
   * exactly the coupling this module's doc comment says never to do. The copy-full-list affordance is
   * gated on this, not on {@link retryable} alone, so it never appears on the alias/collision hold-back.
   */
  advisesManualDelete: boolean;
  /**
   * Every entry with `outcome: "failed"` — genuine failures AND grouped write refusals together,
   * unchanged in meaning from before CPE-1881 (this is what {@link failed} counts). Each entry carries
   * {@link grouped} so a renderer can tell the two apart **structurally** without touching `error`'s
   * wording: a grouped entry's path is a member of the backend's `write_refusal.paths` (CPE-1881 round
   * 3) — the {@link writeRefusalReason} paragraph explains why every `grouped: true` row was refused,
   * once, instead of a genuine per-file reason.
   */
  failures: { path: string; error: string; grouped: boolean }[];
  /**
   * **CPE-1881.** The shared explanation for a whole group of write refusals with the SAME cause
   * (currently only the hard-link rule) — stated once, the write-side counterpart to {@link reason}.
   * `""` when nothing was grouped. The refused paths themselves are already in {@link failures} (each
   * with its own short per-path fact, e.g. "this file has 3 names (it is hard-linked)" — the backend
   * shortened these once the shared paragraph moved here, so nothing duplicates and nothing is lost);
   * this is purely the paragraph {@link failures}' short per-path text alone cannot state economically.
   */
  writeRefusalReason: string;
  /**
   * How many of {@link failures} the {@link writeRefusalReason} paragraph covers. `0` when `""`.
   *
   * **CPE-1881 round 3 (Visual Critic finding).** This field existed before round 3 and nothing ever
   * rendered it — the only reason a count reached the screen at all was that it happened to be quoted
   * inside {@link writeRefusalReason}'s prose, so a future reword of that sentence could have silently
   * deleted the only visible count. `RevertOutcomePanel.svelte` now renders this explicitly as its own
   * heading, independent of the paragraph's wording.
   */
  writeRefusalCount: number;
  /**
   * **CPE-1881 round 3.** Every refused path, untruncated, in plan order — the write-side counterpart to
   * {@link allHeldBackPaths}, backing an identical "copy all N" affordance. `[]` when nothing was
   * grouped. Read straight off the backend's `write_refusal.paths`, never re-derived from {@link
   * failures} by wording, though the two always agree in practice (same source, filtered two ways).
   */
  allWriteRefusalPaths: string[];
};

const plural = (n: number, one: string, many: string) => (n === 1 ? one : many);

/**
 * Summarise a revert for display. Reads `outcome` discriminants only — never the wording of `error`.
 */
export function summarizeRevert(outcome: RevertOutcome): RevertSummary {
  // CPE-1881 round 3: which failed paths are members of the grouped write refusal, read straight off
  // the backend's own `write_refusal.paths` — never by matching `error`'s wording (this module's
  // standing rule). A `Set` so per-entry membership is O(1) rather than an O(N) `includes` per row.
  const groupedPaths = new Set(outcome.write_refusal?.paths ?? []);
  const failures = outcome.skipped
    .filter((r) => r.outcome === "failed")
    .map((r) => ({ path: r.path, error: r.error, grouped: groupedPaths.has(r.path) }));
  const heldBackPaths = outcome.skipped
    .filter((r) => r.outcome === "skipped_by_plan" || r.outcome === "held_back_by_checkpoint")
    .map((r) => ({ path: r.path, detail: r.error }));

  const writeRefusalCount = outcome.write_refusal?.count ?? 0;
  // CPE-1881 round 3 (D2): a grouped refusal is a deliberate, correct stand-down — the same reason
  // `heldBackPaths` gets its own headline clause instead of being folded into "failed". Before this,
  // the headline's first line ("200 failed") flatly contradicted the very next line's paragraph
  // explaining these were refused on purpose. Split exactly like held-back already is: genuine
  // failures keep the word "failed"; the grouped count gets its own "refused" clause. `genuineFailed`
  // can't go negative — `writeRefusalCount` is always `<= failures.length` by construction (every
  // grouped path is one of the `outcome: "failed"` entries `write_refusal` was built from).
  const genuineFailed = failures.length - writeRefusalCount;
  const parts = [`Applied ${outcome.applied} ${plural(outcome.applied, "change", "changes")}`];
  if (genuineFailed > 0) parts.push(`${genuineFailed} failed`);
  if (writeRefusalCount > 0) parts.push(`${writeRefusalCount} refused`);
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
    // "and 1 more" is a longer line than the name it replaces, so at exactly one over the cap the list
    // stretches by one rather than truncating.
    listed: heldBackPaths.slice(0, heldBackPaths.length === MAX_LISTED + 1 ? MAX_LISTED + 1 : MAX_LISTED),
    more: heldBackPaths.length > MAX_LISTED + 1 ? heldBackPaths.length - MAX_LISTED : 0,
    allHeldBackPaths: heldBackPaths.map((p) => p.path),
    advisesManualDelete: held?.advises_manual_delete ?? false,
    failures,
    writeRefusalReason: outcome.write_refusal?.reason ?? "",
    writeRefusalCount,
    allWriteRefusalPaths: outcome.write_refusal?.paths ?? [],
  };
}
