/**
 * Pure UI-side helpers for the Batch-Media dialog (CPE-1093, epic CPE-723): turning an ordered
 * `MediaOp[]` into pill labels + a `BatchJob`, filtering the multi-selection down to the files the
 * backend transform engine can actually decode, and NaN-safe progress math for the streamed-apply
 * bar. Kept pure/testable so the dialog component itself stays dumb, matching `batchRename.ts`'s
 * split between pure planning logic and the component that renders it.
 */

import type { BatchJob, MediaOp, PlannedItem } from "./bindings.gen";
import { parentDir } from "./contentSearch";

/**
 * Extensions the batch-media **encoder** can actually write, mirroring `batch_transform.rs`'s
 * `ext_to_format` set exactly (png/jpg/jpeg/gif/webp/bmp/tif/tiff). This is deliberately **narrower**
 * than `isImage` (the thumbnail/decode set): e.g. `avif` decodes for a thumbnail but the encoder can't
 * emit it, so an all-avif batch would be 0-written/all-skipped. Eligibility keys off *encode* support,
 * not *decode*, so the dialog only offers files a batch can actually transform.
 */
const BATCH_MEDIA_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"]);

/** True when the batch-media engine can re-encode this filename's format (see {@link BATCH_MEDIA_EXTS}). */
export function canBatchTransform(name: string): boolean {
  const dot = name.lastIndexOf(".");
  if (dot < 0) return false;
  return BATCH_MEDIA_EXTS.has(name.slice(dot + 1).toLowerCase());
}

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
    case "compress":
      return `Compress q${op.quality}`;
    case "watermark": {
      if (!op.image) return "Watermark (none)";
      const name = op.image.split(/[\\/]/).pop() ?? op.image;
      return `Watermark ${name} ${op.position} ${op.opacity}%`;
    }
    default: {
      // Exhaustiveness guard: a new MediaOp variant must be given a label above before it compiles.
      const exhaustive: never = op;
      return exhaustive;
    }
  }
}

/**
 * Build the `BatchJob` the backend expects from the ordered op list + the non-destructive toggle.
 * `confirmed_overwrite` always starts `false` here — this is the job used for the live plan preview
 * and for the initial (pre-confirmation) build of every apply. See {@link confirmOverwriteJob} for the
 * ONLY place in the codebase allowed to flip it to `true`.
 */
export function opsToJob(ops: MediaOp[], nonDestructive: boolean): BatchJob {
  return { ops, non_destructive: nonDestructive, confirmed_overwrite: false };
}

/**
 * CPE-1599: the engine now refuses to run any plan containing an in-place overwrite (planned
 * `output === input`) unless `BatchJob.confirmed_overwrite` is `true` — a backend-side backstop for the
 * `showOverwriteConfirm` panel this file's {@link overwritesInPlace} already gates. This function is the
 * **single, named seam** that is allowed to set that flag: `BatchMediaDialog.svelte`'s `apply()` calls it
 * ONLY once the user has actually clicked "Overwrite N files" on the confirm panel (i.e. only when
 * {@link overwritesInPlace} found at least one in-place item AND the panel's own button was the thing
 * that invoked `apply()`). No other call site in the app should ever construct a job with
 * `confirmed_overwrite: true` by hand — route through here so a reviewer only has to audit this one
 * function's callers, not every place a `BatchJob` literal could be written.
 */
export function confirmOverwriteJob(job: BatchJob): BatchJob {
  return { ...job, confirmed_overwrite: true };
}

/**
 * One folder whose pre-overwrite `checkpointCreate` SUCCEEDED but left `skippedCount` file(s) uncaptured
 * (oversize/budget) — distinct from an outright checkpoint failure (CPE-1599 UAT follow-up: `App.svelte`'s
 * `noticeCheckpointFailures` and `BatchMediaDialog.svelte`'s results panel both need this shape, so it
 * lives here rather than being declared inside the `.svelte` component, which can't export a plain type
 * from its instance script).
 */
export interface CheckpointPartial {
  dir: string;
  skippedCount: number;
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
 * notice rather than sending them to the backend and having every op fail per-file. Keys off
 * {@link canBatchTransform} (the encoder-writable set), which is narrower than the thumbnail `isImage`
 * check — a decode-only format like `avif` shows a thumbnail but can't be batch-transformed, so it must
 * not be offered here. Pure; generic over any entry shape carrying at least `name`/`is_dir` so it works
 * directly on `DirEntry[]`.
 */
export function partitionEligible<T extends { name: string; is_dir: boolean }>(
  entries: T[],
): EligibilitySplit<T> {
  const eligible = entries.filter((e) => !e.is_dir && canBatchTransform(e.name));
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

/**
 * Map a batch report's `skipped` entries — `[inputPath, reason]` pairs from `BatchReport.skipped` — to
 * display rows (basename + reason) for the dialog's skip panel (CPE-1115). Batch-media is skip-on-error: a
 * file the engine can't process (e.g. a placeholder/corrupt image that won't decode) is left untouched and
 * recorded here rather than aborting the batch — so the UI must surface these clearly instead of silently
 * dropping them. Pure; basename via the same `/\`-split used elsewhere in the app.
 */
export function skipRows(report: { skipped: [string, string][] }): { name: string; reason: string }[] {
  return report.skipped.map(([path, reason]) => ({
    name: path.split(/[\\/]/).pop() || path,
    reason,
  }));
}

/**
 * CPE-1590: the planned items that would **overwrite their own input file in place** — i.e. the backend
 * planner (`batch_media::plan`) resolved `output === input`. This only happens with "Write to new files"
 * unchecked (`non_destructive: false`) *and* an op combination with no dedicated output-renaming suffix
 * (a lone Compress, Strip metadata, or Watermark — see `batch_media.rs`'s `plan()`). Comparing the
 * concrete planned paths directly (rather than re-deriving "which op combos are suffix-less" on the
 * frontend) is robust to any future op the backend adds — if the planner ever resolves a same-path
 * output for ANY reason, this still catches it. Pure; used to gate the destructive-overwrite confirm
 * step before Apply is allowed to run.
 */
export function overwritesInPlace(items: PlannedItem[]): PlannedItem[] {
  return items.filter((it) => it.input === it.output);
}

/**
 * The distinct parent directories of `paths`, in first-seen order — used to take a best-effort
 * pre-overwrite checkpoint of every folder about to lose original files (CPE-1590), mirroring the
 * single-checkpoint-before-the-batch pattern `MetadataStudioDialog`/`DeclutterDialog`/
 * `SimilarImagesDialog` already use via `commands.checkpointCreate`. Pure.
 */
export function uniqueParentDirs(paths: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const p of paths) {
    const dir = parentDir(p);
    if (dir && !seen.has(dir)) {
      seen.add(dir);
      out.push(dir);
    }
  }
  return out;
}
