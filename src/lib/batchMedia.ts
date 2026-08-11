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
 * CPE-1623: true when a Rename (or Convert `to_ext`) template could move the computed output into a
 * different directory than its input, or off the volume its input's own directory names entirely — a
 * path separator (`/` or `\`) anywhere, a `:` anywhere, or a **whole-segment** `..` traversal. Mirrors
 * `crates/server/src/batch_media.rs`'s `template_escapes_directory()`/`validate()` rejection exactly
 * (kept in lockstep — see that fn's doc for the reasoning behind each character), so the dialog can
 * disable "+ Add" and explain why *before* the user ever reaches the backend — the engine
 * (`validate()`/`plan()`) remains the actual enforcement point either way; this is purely a friendlier,
 * earlier echo of the same rule. The template only ever substitutes into the file's STEM (or, for
 * Convert, the extension) — see `plan()`'s `Rename`/`Convert` arms — so it has no legitimate reason to
 * name a directory or a drive at all.
 *
 * **`..` is only a traversal risk as a whole path segment (UAT follow-up), not any occurrence:** an
 * earlier cut of this check flagged `..` as a bare substring, which wrongly rejected ordinary filenames
 * like `"shot..final"` or a version stamp `"v1..2"` — see the ticket's acceptance criterion "ordinary
 * rename templates (no separators) are unaffected". Once any separator/`:` has already returned `true`
 * above, the template is guaranteed to contain none of them, so "is `..` a whole segment" reduces to "is
 * the (trimmed) template exactly `..`".
 */
export function templateEscapesDirectory(template: string): boolean {
  if (template.includes("/") || template.includes("\\") || template.includes(":")) return true;
  return template.trim() === "..";
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
 * The live `navigator.platform`/`navigator.userAgent` sniff, mirroring `terminalClient.ts`'s
 * `shellChoicesFor` pattern (a pure function taking `platform: string`, defaulted here from the DOM so
 * production call sites need no argument while tests can pass an explicit platform string). Empty string
 * when `navigator` isn't available (e.g. a non-DOM test runner) — {@link isCaseInsensitivePlatform}
 * treats that as "assume case-sensitive", the conservative direction (never wrongly folds a real
 * distinction away).
 */
function defaultPlatform(): string {
  if (typeof navigator === "undefined") return "";
  return navigator.platform || navigator.userAgent || "";
}

/**
 * True for the platforms whose DEFAULT filesystem is case-insensitive (Windows, macOS) — never for
 * Linux/other Unix, where folding case would wrongly treat two distinct, real files as one. Mirrors
 * `crates/server/src/batch_media.rs`'s `fold_case` `#[cfg(...)]` gate (CPE-1613): this is a platform
 * default assumption, not a live filesystem probe.
 */
function isCaseInsensitivePlatform(platform: string): boolean {
  return /win/i.test(platform) || /mac/i.test(platform);
}

/**
 * Normalize a path string into `/`-joined components, resolving `.` and lexical `..` segments — purely
 * textual, mirrors `crates/server/src/batch_media.rs`'s `lexical_normalize`. Treats both `/` and `\` as
 * separators so a Windows-style path normalizes sanely regardless of which OS the browser/webview thinks
 * it's on. Preserves a leading root marker (POSIX `/`, or an uppercased `C:/`-style drive prefix) so an
 * absolute and a relative path never lexically collide.
 */
function lexicalNormalize(path: string): string {
  let root = "";
  if (path.startsWith("/") || path.startsWith("\\")) {
    root = "/";
  } else if (/^[a-zA-Z]:/.test(path)) {
    root = `${path[0].toUpperCase()}:/`;
  }

  const parts: string[] = [];
  for (const raw of path.split(/[\\/]/)) {
    if (raw === "" || raw === ".") continue;
    if (raw === "..") {
      if (parts.length > 0 && parts[parts.length - 1] !== "..") {
        parts.pop();
      } else if (!root) {
        parts.push("..");
      }
      continue; // ".." above an absolute root is lexically discarded — can't go any higher
    }
    parts.push(raw);
  }
  return root + parts.join("/");
}

/**
 * CPE-1613: decide whether two path STRINGS the backend planner produced refer to the same underlying
 * file — mirroring `crates/server/src/batch_media.rs`'s `same_file` definition so the frontend guard
 * ({@link overwritesInPlace}) and the engine's `confirmed_overwrite` refusal check agree. Fixing one and
 * not the other just moves the hole (see the ticket). Unlike the Rust side, the frontend has no
 * synchronous filesystem access to canonicalize a real path or resolve symlinks/junctions, so this is a
 * **lexical-only** approximation: normalize separators and resolve `.`/`..` segments
 * ({@link lexicalNormalize}), then fold case ONLY on the platforms that default to a case-insensitive
 * filesystem ({@link isCaseInsensitivePlatform}). `platform` defaults to the live navigator sniff; pass
 * it explicitly in tests.
 */
export function sameFile(a: string, b: string, platform: string = defaultPlatform()): boolean {
  if (a === b) return true;
  const fold = isCaseInsensitivePlatform(platform) ? (s: string) => s.toLowerCase() : (s: string) => s;
  return fold(lexicalNormalize(a)) === fold(lexicalNormalize(b));
}

/**
 * CPE-1590: the planned items that would **overwrite their own input file in place** — i.e. the backend
 * planner (`batch_media::plan`) resolved an output {@link sameFile} as its input. This only happens with
 * "Write to new files" unchecked (`non_destructive: false`) *and* an op combination with no dedicated
 * output-renaming suffix (a lone Compress, Strip metadata, or Watermark — see `batch_media.rs`'s
 * `plan()`), OR — the CPE-1613 fix — a `Convert` whose target extension differs from the input's only by
 * case on a case-insensitive filesystem. Comparing the concrete planned paths via {@link sameFile}
 * (rather than re-deriving "which op combos are suffix-less" on the frontend) is robust to any future op
 * the backend adds — if the planner ever resolves a same-file output for ANY reason, this still catches
 * it. Pure; used to gate the destructive-overwrite confirm step before Apply is allowed to run.
 * `platform` defaults to the live navigator sniff; pass it explicitly in tests.
 */
export function overwritesInPlace(items: PlannedItem[], platform: string = defaultPlatform()): PlannedItem[] {
  return items.filter((it) => sameFile(it.input, it.output, platform));
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
