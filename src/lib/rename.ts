/**
 * Pure batch-rename engine (CPE-700, epic CPE-699).
 *
 * `applyRecipe(names, recipe)` runs an ordered list of rename operations over a list of filenames and
 * returns the old→new mapping; `validate` derives collision / no-op / invalid-name flags from that
 * mapping. Everything here is **pure** — no filesystem, no UI, no I/O — so it is fully unit-testable and
 * the Batch Rename panel (CPE-702) is a thin shell over it. The backend `rename_many` (CPE-701) applies
 * the exact `{ from, to }` pairs this produces; it does not re-run the transforms.
 *
 * Modelled on `src/lib/search.ts`: a small, dependency-free, well-tested string library.
 */

/** Which part of a filename an operation transforms. `ext` excludes the dot; a dotless name has none. */
export type Scope = "name" | "ext" | "full";

/** A single rename operation. Operations compose left-to-right over each name (see {@link applyRecipe}). */
export type RenameOp =
  | {
      kind: "replace";
      find: string;
      replace: string;
      regex?: boolean;
      all?: boolean;
      caseInsensitive?: boolean;
      scope?: Scope;
    }
  | { kind: "case"; mode: "lower" | "upper" | "title" | "sentence"; scope?: Scope }
  | { kind: "insert"; text: string; position: "prefix" | "suffix" | number; scope?: Scope }
  | { kind: "remove"; from: number; count?: number; scope?: Scope }
  | { kind: "trim"; scope?: Scope }
  | {
      kind: "number";
      start?: number;
      step?: number;
      padding?: number;
      separator?: string;
      position: "prefix" | "suffix" | number;
      scope?: Scope;
    }
  | { kind: "extension"; mode: "set" | "add" | "strip"; ext?: string };

/** An ordered recipe. Applied in array order to every name; numbering counts over the input list order. */
export type RenameRecipe = RenameOp[];

/** One name's transform result. `changed` is false when the recipe left the name untouched. */
export interface RenameResult {
  from: string;
  to: string;
  changed: boolean;
}

/** Per-result validation flags, derived purely from the from→to mapping (+ optional folder siblings). */
export interface Validation {
  /** Two results share a target, or the target collides with an existing sibling. */
  collision: boolean;
  /** The recipe left this name unchanged. */
  noop: boolean;
  /** The target is not a legal filename on the target platform. */
  invalid: boolean;
  /** Human-readable reason for the first problem found, if any. */
  reason?: string;
}

export type Platform = "win" | "posix";

// ── name splitting ────────────────────────────────────────────────────────────────────────────────

/**
 * Split a filename into stem + extension at the LAST dot. A leading dot (`.gitignore`) or no dot yields
 * an empty extension — the whole thing is the stem, matching how a file explorer shows "no extension".
 */
function splitName(filename: string): { stem: string; ext: string; dot: boolean } {
  const idx = filename.lastIndexOf(".");
  if (idx <= 0) return { stem: filename, ext: "", dot: false };
  return { stem: filename.slice(0, idx), ext: filename.slice(idx + 1), dot: true };
}

function joinName(stem: string, ext: string, dot: boolean): string {
  return dot ? `${stem}.${ext}` : stem;
}

/** Apply a string transform to only the scoped part of `filename`, recombining the rest. */
function withScope(filename: string, scope: Scope, fn: (part: string) => string): string {
  if (scope === "full") return fn(filename);
  const { stem, ext, dot } = splitName(filename);
  if (scope === "name") return joinName(fn(stem), ext, dot);
  return joinName(stem, fn(ext), dot); // "ext"
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, n));
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

// ── operations ──────────────────────────────────────────────────────────────────────────────────

function opReplace(op: Extract<RenameOp, { kind: "replace" }>, s: string): string {
  if (op.find === "") return s; // an empty needle is a no-op (avoids match-at-every-position)
  const flags = (op.all ? "g" : "") + (op.caseInsensitive ? "i" : "");
  let re: RegExp;
  try {
    re = new RegExp(op.regex ? op.find : escapeRegExp(op.find), flags);
  } catch {
    return s; // invalid user regex → leave the name untouched rather than throwing
  }
  // In literal mode the replacement is literal too: escape `$` so `$&`/`$1` aren't interpreted.
  const replacement = op.regex ? op.replace : op.replace.replace(/\$/g, "$$$$");
  return s.replace(re, replacement);
}

function opCase(mode: "lower" | "upper" | "title" | "sentence", s: string): string {
  switch (mode) {
    case "lower":
      return s.toLowerCase();
    case "upper":
      return s.toUpperCase();
    case "title":
      return s.replace(/\w\S*/g, (t) => t.charAt(0).toUpperCase() + t.slice(1).toLowerCase());
    case "sentence":
      return s.charAt(0).toUpperCase() + s.slice(1).toLowerCase();
  }
}

function opInsert(op: Extract<RenameOp, { kind: "insert" }>, s: string): string {
  if (op.position === "prefix") return op.text + s;
  if (op.position === "suffix") return s + op.text;
  const p = clamp(op.position, 0, s.length);
  return s.slice(0, p) + op.text + s.slice(p);
}

function opRemove(op: Extract<RenameOp, { kind: "remove" }>, s: string): string {
  const from = clamp(op.from, 0, s.length);
  const count = op.count ?? s.length - from;
  return s.slice(0, from) + s.slice(from + Math.max(0, count));
}

function opNumber(op: Extract<RenameOp, { kind: "number" }>, s: string, index: number): string {
  const start = op.start ?? 1;
  const step = op.step ?? 1;
  const n = start + index * step;
  const digits = String(Math.abs(n));
  const padded = op.padding ? digits.padStart(op.padding, "0") : digits;
  const num = (n < 0 ? "-" : "") + padded;
  const sep = op.separator ?? "";
  if (op.position === "prefix") return num + sep + s;
  if (op.position === "suffix") return s + sep + num;
  const p = clamp(op.position, 0, s.length);
  return s.slice(0, p) + num + s.slice(p);
}

function opExtension(op: Extract<RenameOp, { kind: "extension" }>, filename: string): string {
  const parts = splitName(filename);
  if (op.mode === "strip") return parts.stem;
  const ext = (op.ext ?? "").replace(/^\.+/, ""); // tolerate a leading dot in the supplied extension
  if (op.mode === "add") {
    // Add an extension only when there isn't one already.
    return parts.dot || ext === "" ? filename : `${filename}.${ext}`;
  }
  // "set": replace (or add) the extension.
  return ext === "" ? parts.stem : `${parts.stem}.${ext}`;
}

/** Apply one operation to one filename (with its list index, for numbering). */
function applyOp(op: RenameOp, filename: string, index: number): string {
  switch (op.kind) {
    case "replace":
      return withScope(filename, op.scope ?? "name", (part) => opReplace(op, part));
    case "case":
      return withScope(filename, op.scope ?? "name", (part) => opCase(op.mode, part));
    case "insert":
      return withScope(filename, op.scope ?? "name", (part) => opInsert(op, part));
    case "remove":
      return withScope(filename, op.scope ?? "name", (part) => opRemove(op, part));
    case "trim":
      return withScope(filename, op.scope ?? "name", (part) => part.trim().replace(/\s+/g, " "));
    case "number":
      return withScope(filename, op.scope ?? "name", (part) => opNumber(op, part, index));
    case "extension":
      return opExtension(op, filename);
  }
}

/**
 * Run `recipe` over each name in `names`, composing operations in order. Numbering counts over the input
 * list order (index 0, 1, 2…), so callers pass the selection in the order the user sees it.
 */
export function applyRecipe(names: string[], recipe: RenameRecipe): RenameResult[] {
  return names.map((from, i) => {
    let to = from;
    for (const op of recipe) to = applyOp(op, to, i);
    return { from, to, changed: to !== from };
  });
}

// ── validation ──────────────────────────────────────────────────────────────────────────────────

const WIN_RESERVED = new Set([
  "CON",
  "PRN",
  "AUX",
  "NUL",
  ...Array.from({ length: 9 }, (_, i) => `COM${i + 1}`),
  ...Array.from({ length: 9 }, (_, i) => `LPT${i + 1}`),
]);

function checkValidName(name: string, platform: Platform): { ok: boolean; reason?: string } {
  if (name.trim() === "") return { ok: false, reason: "empty name" };
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f]/.test(name)) return { ok: false, reason: "control character" };
  if (platform === "win") {
    if (/[<>:"/\\|?*]/.test(name)) return { ok: false, reason: "illegal character" };
    if (/[. ]$/.test(name)) return { ok: false, reason: "trailing dot or space" };
    const base = (name.split(".")[0] ?? "").toUpperCase();
    if (WIN_RESERVED.has(base)) return { ok: false, reason: "reserved name" };
  } else if (name.includes("/")) {
    return { ok: false, reason: "illegal character" };
  }
  return { ok: true };
}

/**
 * Derive per-result validation from the from→to mapping. Collision = two results target the same name
 * (case-insensitively on Windows), or a target equals a sibling in `existing` that isn't just this file
 * renamed to itself. `existing` are other names in the folder NOT part of the rename set (optional).
 */
export function validate(
  results: RenameResult[],
  opts?: { platform?: Platform; existing?: string[] },
): Validation[] {
  const platform = opts?.platform ?? "win";
  const norm = platform === "win" ? (x: string) => x.toLowerCase() : (x: string) => x;

  const targetCounts = new Map<string, number>();
  for (const r of results) {
    const k = norm(r.to);
    targetCounts.set(k, (targetCounts.get(k) ?? 0) + 1);
  }
  const existingSet = new Set((opts?.existing ?? []).map(norm));

  return results.map((r) => {
    const noop = r.to === r.from;
    let reason: string | undefined;
    let invalid = false;
    let collision = false;

    const nameCheck = checkValidName(r.to, platform);
    if (!nameCheck.ok) {
      invalid = true;
      reason = nameCheck.reason;
    }
    if ((targetCounts.get(norm(r.to)) ?? 0) > 1) {
      collision = true;
      reason ??= "duplicate target";
    } else if (!noop && existingSet.has(norm(r.to))) {
      collision = true;
      reason ??= "name already exists";
    }
    return { collision, noop, invalid, reason };
  });
}

/** Convenience: {@link applyRecipe} zipped with {@link validate}, for a live preview table. */
export function previewRename(
  names: string[],
  recipe: RenameRecipe,
  opts?: { platform?: Platform; existing?: string[] },
): Array<RenameResult & Validation> {
  const results = applyRecipe(names, recipe);
  const flags = validate(results, opts);
  return results.map((r, i) => ({ ...r, ...flags[i] }));
}
