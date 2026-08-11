/**
 * Pure parsing for Jupyter `.ipynb` notebooks (CPE-1616, epic CPE-1568 slice 6). Framework-free (no
 * Svelte import) so the parse/capping logic is unit-testable without mounting a component — mirrors the
 * "pure module behind the preview component" convention `jsonTree.ts`/`binaryInspector.ts` already
 * established.
 *
 * A `.ipynb` is untrusted input (CLAUDE.md robustness discipline): {@link parseNotebook} NEVER throws —
 * every failure mode (invalid JSON, a non-object root, a missing/non-array `cells`, a cell that isn't an
 * object, a `source`/output field of the wrong shape, …) resolves to a `{ok:false}` result the caller
 * renders as a clear reason, same discipline as `parseJson`/`formatJson` in `jsonTree.ts`.
 *
 * Every cap below bounds the WORK done, not just what's shown: cells beyond {@link MAX_CELLS} and outputs
 * beyond {@link MAX_OUTPUTS_PER_CELL} are sliced off the raw array BEFORE it is mapped/processed (never
 * processed-then-discarded), and a cell's source / an output's text is truncated with `.slice()` before it
 * is ever handed to markdown rendering or syntax highlighting — the same "cap must bound items examined,
 * not just items emitted" discipline this crew learned the hard way from a font-cache bug that froze the
 * UI for 8.8s despite an output cap, because the cap only trimmed what was shown, not what was computed.
 *
 * **ANSI escape codes** (CPE-1616 Visual Critic finding): a real Jupyter traceback/stream is routinely
 * colourised by the kernel (IPython's exception formatter, `colorama`, `tqdm`, …) with raw ANSI SGR/CSI
 * escape sequences (e.g. `ESC[0;31m`). Left untouched, those render as literal garbage — `[0;31m`
 * fragments interleaved with the real message — since the view puts this text straight into a `<pre>`
 * (auto-escaped, not `{@html}`). {@link stripAnsi} removes them so the text is readable; rendering them as
 * real colour was considered and rejected for v1 — it would need `{@html}` plus very careful sanitisation
 * of attacker-controlled SGR parameters to stay safe, for a "nice to have" over the readability floor this
 * ticket actually needs. Applied to stream text, `text/plain` results, and error tracebacks — anywhere raw
 * captured-terminal text reaches the view — same discipline as the size caps: stripped, never trusted as
 * already-clean.
 */

export type NotebookCellType = "markdown" | "code" | "raw" | "unknown";

/** `stream` output: raw stdout/stderr text written during execution. */
export interface NotebookOutputStream {
  kind: "stream";
  name: string; // usually "stdout" | "stderr"; whatever the notebook says, un-validated
  text: string;
  truncated: boolean;
}

/** `execute_result` / `display_data` output: a MIME bundle. Only `text/plain` and `image/png` are
 *  rendered (the ticket's explicit v1 scope) — every other MIME type present is reported by name in
 *  `otherMimeTypes` so the view can say honestly "N more output type(s) not shown" instead of silently
 *  dropping them. */
export interface NotebookOutputResult {
  kind: "result";
  text: string | null;
  imageDataUrl: string | null;
  /** True when an `image/png` payload was present but exceeded {@link MAX_OUTPUT_IMAGE_CHARS} and was
   *  skipped rather than inflating the DOM with a multi-megabyte data: URL. */
  imageOmitted: boolean;
  otherMimeTypes: string[];
  truncated: boolean;
}

/** `error` output: an uncaught exception from cell execution. Kept as its own kind so the view can style
 *  a failed cell distinctly (per the ticket's Scope). */
export interface NotebookOutputError {
  kind: "error";
  ename: string;
  evalue: string;
  traceback: string;
  truncated: boolean;
}

export type NotebookOutput = NotebookOutputStream | NotebookOutputResult | NotebookOutputError;

export interface NotebookCell {
  index: number;
  type: NotebookCellType;
  source: string;
  sourceTruncated: boolean;
  executionCount: number | null;
  outputs: NotebookOutput[];
  /** The cell's real output count, even when {@link outputsCapped} cut the list down. */
  outputsTotal: number;
  outputsCapped: boolean;
}

export interface ParsedNotebook {
  cells: NotebookCell[];
  /** The notebook's real cell count, even when {@link cellsCapped} cut the list down. */
  totalCells: number;
  cellsCapped: boolean;
  /** Kernel/source language, lowercased, for syntax highlighting — from
   *  `metadata.kernelspec.language`/`metadata.language_info.name`, falling back to `"python"` when
   *  neither is present (the ticket's documented "Python by default" behaviour). */
  language: string;
  nbformat: number | null;
}

export type NotebookParseResult = { ok: true; notebook: ParsedNotebook } | { ok: false; error: string };

/** Render/work cap on cell count (CPE-1616 acceptance: "a notebook with hundreds of cells stays
 *  responsive"). Cells beyond this are never even mapped over — see the module doc comment. */
export const MAX_CELLS = 300;

/** Cap on one cell's source text handed to markdown rendering / syntax highlighting — bounds the work a
 *  single pathological cell (e.g. one giant minified line) can force, independent of the cell-count cap. */
export const MAX_CELL_SOURCE_CHARS = 100_000;

/** Cap on how many of one cell's outputs are processed. */
export const MAX_OUTPUTS_PER_CELL = 20;

/** Cap on one output's rendered text (stream text, `text/plain`, or a traceback). */
export const MAX_OUTPUT_TEXT_CHARS = 20_000;

/** Cap on an `image/png` output's base64 payload length (~3 MB decoded) — bigger than this is skipped
 *  ({@link NotebookOutputResult.imageOmitted}) rather than built into a giant `data:` URL. */
export const MAX_OUTPUT_IMAGE_CHARS = 4_000_000;

/** Cap on how many bytes of the `.ipynb` file itself are read (CPE-1616). Bigger than the generic
 *  text-preview cap ({@link PREVIEW_MAX_BYTES} in `loaders.ts`, 256 KiB) because notebooks routinely embed
 *  base64 `image/png` outputs inline; still bounded (double `CompareDialog`'s 4 MiB precedent) so a huge
 *  file errors cleanly (matching `read_file_text`'s "error rather than truncate" behaviour) instead of
 *  being silently cut mid-JSON. */
export const NOTEBOOK_READ_MAX_BYTES = 8 * 1024 * 1024;

/** A `source`/`text` field per the nbformat spec is either a single string or an array of line strings to
 *  concatenate. Tolerant of anything else (a malformed notebook could put a number, `null`, or a nested
 *  array there) — non-string array entries are dropped rather than throwing. */
function joinSource(value: unknown): string {
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value.filter((s): s is string => typeof s === "string").join("");
  return "";
}

function capText(text: string, max: number): { text: string; truncated: boolean } {
  return text.length > max ? { text: text.slice(0, max), truncated: true } : { text, truncated: false };
}

/** Matches an ANSI escape sequence: CSI forms (the common `ESC [ ... <letter>` SGR colour-code shape a
 *  Jupyter kernel emits, e.g. `ESC[0;31m`) and OSC forms terminated by BEL. Same shape as the well-known
 *  `strip-ansi` npm package's regex (reimplemented inline — a one-line regex doesn't earn a dependency,
 *  per this repo's lean-core rule), so it covers more than the bare colour-code subset the fixtures
 *  exercise — cursor moves, erase-line, etc. — anything a real captured terminal stream might contain.
 *  Built via `new RegExp(string)` with `\u` escapes (not literal control bytes) so this source file stays
 *  plain ASCII on disk. */
const ANSI_ESCAPE_PATTERN =
  "[\\u001B\\u009B][[\\]()#;?]*(?:(?:(?:(?:;[-a-zA-Z\\d/#&.:=?%@~_]+)*|" +
  "[a-zA-Z\\d]+(?:;[-a-zA-Z\\d/#&.:=?%@~_]*)*)?\\u0007)|" +
  "(?:(?:\\d{1,4}(?:;\\d{0,4})*)?[\\dA-PR-TZcf-ntqry=><~]))";
const ANSI_ESCAPE_REGEX = new RegExp(ANSI_ESCAPE_PATTERN, "g");

/** Strips ANSI escape sequences from text. `.ipynb` traceback/stream text is untrusted, captured-terminal
 *  input — see the module doc comment. Never throws (a plain regex replace on a string can't). */
export function stripAnsi(text: string): string {
  return text.replace(ANSI_ESCAPE_REGEX, "");
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

function detectLanguage(metadata: unknown): string {
  if (isPlainObject(metadata)) {
    const kernelspec = metadata.kernelspec;
    if (isPlainObject(kernelspec) && typeof kernelspec.language === "string" && kernelspec.language.trim()) {
      return kernelspec.language.trim().toLowerCase();
    }
    const languageInfo = metadata.language_info;
    if (isPlainObject(languageInfo) && typeof languageInfo.name === "string" && languageInfo.name.trim()) {
      return languageInfo.name.trim().toLowerCase();
    }
  }
  return "python";
}

function cellType(v: unknown): NotebookCellType {
  return v === "markdown" || v === "code" || v === "raw" ? v : "unknown";
}

function parseOutput(raw: unknown): NotebookOutput | null {
  if (!isPlainObject(raw)) return null;
  const outputType = raw.output_type;

  if (outputType === "stream") {
    const name = typeof raw.name === "string" ? raw.name : "stdout";
    const { text, truncated } = capText(stripAnsi(joinSource(raw.text)), MAX_OUTPUT_TEXT_CHARS);
    return { kind: "stream", name, text, truncated };
  }

  if (outputType === "error") {
    const ename = typeof raw.ename === "string" ? raw.ename : "Error";
    const evalue = typeof raw.evalue === "string" ? raw.evalue : "";
    const tb = Array.isArray(raw.traceback)
      ? raw.traceback.filter((s): s is string => typeof s === "string").join("\n")
      : "";
    const { text: traceback, truncated } = capText(stripAnsi(tb), MAX_OUTPUT_TEXT_CHARS);
    return { kind: "error", ename, evalue, traceback, truncated };
  }

  if (outputType === "execute_result" || outputType === "display_data") {
    const data = raw.data;
    if (!isPlainObject(data)) {
      return { kind: "result", text: null, imageDataUrl: null, imageOmitted: false, otherMimeTypes: [], truncated: false };
    }
    let text: string | null = null;
    let truncated = false;
    if ("text/plain" in data) {
      const cap = capText(stripAnsi(joinSource(data["text/plain"])), MAX_OUTPUT_TEXT_CHARS);
      text = cap.text;
      truncated = cap.truncated;
    }
    let imageDataUrl: string | null = null;
    let imageOmitted = false;
    if ("image/png" in data) {
      const b64 = joinSource(data["image/png"]).replace(/\s+/g, "");
      if (b64.length > 0) {
        if (b64.length <= MAX_OUTPUT_IMAGE_CHARS) imageDataUrl = `data:image/png;base64,${b64}`;
        else imageOmitted = true;
      }
    }
    const otherMimeTypes = Object.keys(data).filter((k) => k !== "text/plain" && k !== "image/png");
    return { kind: "result", text, imageDataUrl, imageOmitted, otherMimeTypes, truncated };
  }

  // Unknown/missing output_type: degrade to "nothing rendered" rather than guessing at an unknown shape.
  return null;
}

function parseCell(raw: unknown, index: number): NotebookCell {
  if (!isPlainObject(raw)) {
    return { index, type: "unknown", source: "", sourceTruncated: false, executionCount: null, outputs: [], outputsTotal: 0, outputsCapped: false };
  }
  const type = cellType(raw.cell_type);
  const { text: source, truncated: sourceTruncated } = capText(joinSource(raw.source), MAX_CELL_SOURCE_CHARS);
  const executionCount = typeof raw.execution_count === "number" ? raw.execution_count : null;

  let outputs: NotebookOutput[] = [];
  let outputsTotal = 0;
  let outputsCapped = false;
  if (Array.isArray(raw.outputs)) {
    outputsTotal = raw.outputs.length;
    outputsCapped = outputsTotal > MAX_OUTPUTS_PER_CELL;
    // Slice BEFORE mapping — the cap bounds outputs actually examined, not just ones rendered.
    const toProcess = outputsCapped ? raw.outputs.slice(0, MAX_OUTPUTS_PER_CELL) : raw.outputs;
    outputs = toProcess.map(parseOutput).filter((o): o is NotebookOutput => o !== null);
  }

  return { index, type, source, sourceTruncated, executionCount, outputs, outputsTotal, outputsCapped };
}

/**
 * Parse raw `.ipynb` JSON text into a render-ready shape. Never throws — see the module doc comment for
 * the full robustness discipline.
 */
export function parseNotebook(raw: string): NotebookParseResult {
  let root: unknown;
  try {
    root = JSON.parse(raw);
  } catch (e) {
    return { ok: false, error: `Not valid JSON (${e instanceof Error ? e.message : String(e)}).` };
  }
  if (!isPlainObject(root)) {
    return { ok: false, error: "Not a notebook: the top-level JSON value isn't an object." };
  }
  if (!Array.isArray(root.cells)) {
    return { ok: false, error: 'Not a notebook: missing a top-level "cells" array.' };
  }

  const nbformat = typeof root.nbformat === "number" ? root.nbformat : null;
  const language = detectLanguage(root.metadata);

  const totalCells = root.cells.length;
  const cellsCapped = totalCells > MAX_CELLS;
  // Slice BEFORE mapping — the cap bounds cells actually examined (parsed/highlighted), not just ones
  // rendered, so a notebook with e.g. 50,000 cells never pays for the other 49,700.
  const toProcess = cellsCapped ? root.cells.slice(0, MAX_CELLS) : root.cells;
  const cells = toProcess.map((c: unknown, i: number) => parseCell(c, i));

  return { ok: true, notebook: { cells, totalCells, cellsCapped, language, nbformat } };
}
