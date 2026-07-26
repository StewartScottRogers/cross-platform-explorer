/**
 * Pure UI-side helpers for the Batch-Media dialog (CPE-1093, epic CPE-723): turning an ordered
 * `MediaOp[]` into pill labels + a `BatchJob`, filtering the multi-selection down to the files the
 * backend transform engine can actually decode, and NaN-safe progress math for the streamed-apply
 * bar. Kept pure/testable so the dialog component itself stays dumb, matching `batchRename.ts`'s
 * split between pure planning logic and the component that renders it.
 */

import { isImage } from "./filetypes";
import type { BatchJob, MediaOp } from "./bindings.gen";

/**
 * A short one-line pill label for a single op, in the order it will run, e.g. `"Resize 1024px"`,
 * `"Convert → webp"`, `"Rotate 90°"`. Mirrors the wording `batch_media::plan`'s summary uses on the
 * backend, so the pill and the live-preview summary read consistently.
 */
export function mediaOpLabel(op: MediaOp): string {
  switch (op.op) {
    case "resize":
      return `Resize ${op.max_px}px`;
    case "convert":
      return `Convert → ${op.to_ext}`;
    case "rotate":
      return `Rotate ${op.degrees}°`;
    case "flip":
      return op.horizontal ? "Flip horizontal" : "Flip vertical";
    case "rename":
      return `Rename "${op.template}"`;
    case "strip_metadata":
      return "Strip metadata";
    default: {
      // Exhaustiveness guard: a new MediaOp variant must be given a label above before it compiles.
      const exhaustive: never = op;
      return exhaustive;
    }
  }
}

/** Build the `BatchJob` the backend expects from the ordered op list + the non-destructive toggle. */
export function opsToJob(ops: MediaOp[], nonDestructive: boolean): BatchJob {
  return { ops, non_destructive: nonDestructive };
}

/** The result of {@link partitionEligible}: which selected entries the batch engine can operate on,
 *  and how many were dropped. */
export interface EligibilitySplit<T> {
  eligible: T[];
  skipped: number;
}

/**
 * Split a multi-selection into files the batch-media engine can operate on (images, not folders) vs.
 * the rest — the dialog pre-filters these out with a "N of M files aren't images and will be skipped"
 * notice rather than sending them to the backend and having every op fail per-file. Reuses the same
 * `isImage` extension check the Icons-view thumbnailer and Quick-look already use, so "eligible for
 * batch media" always agrees with "shows a real thumbnail" elsewhere in the app. Pure; generic over
 * any entry shape carrying at least `name`/`is_dir` so it works directly on `DirEntry[]`.
 */
export function partitionEligible<T extends { name: string; is_dir: boolean }>(
  entries: T[],
): EligibilitySplit<T> {
  const eligible = entries.filter((e) => !e.is_dir && isImage(e.name));
  return { eligible, skipped: entries.length - eligible.length };
}

/**
 * Percent complete for the streamed-apply progress bar, clamped to `[0, 100]` and rounded to a whole
 * number. NaN-safe: an unknown/zero total (before the plan has resolved, or a degenerate empty plan)
 * reads as `0`, never `NaN` — which would otherwise collapse the bar's `width` CSS to nothing.
 */
export function progressPercent(done: number, total: number): number {
  if (!Number.isFinite(total) || total <= 0) return 0;
  if (!Number.isFinite(done) || done <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((done / total) * 100)));
}
