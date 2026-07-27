/**
 * Pure UI-side helpers for the Batch-Media dialog (CPE-1093, epic CPE-723): turning an ordered
 * `MediaOp[]` into pill labels + a `BatchJob`, filtering the multi-selection down to the files the
 * backend transform engine can actually decode, and NaN-safe progress math for the streamed-apply
 * bar. Kept pure/testable so the dialog component itself stays dumb, matching `batchRename.ts`'s
 * split between pure planning logic and the component that renders it.
 */

import type { BatchJob, MediaOp } from "./bindings.gen";

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
