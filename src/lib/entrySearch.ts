/**
 * Search power-filters for the instant **folder** search box (CPE-1088, epic CPE-703): `size:`,
 * `date:`/`modified:`, `type:`, `ext:`, `path:`, plain/glob name terms, and a boolean `OR`/`NOT`/`-`/
 * parentheses grammar over all of the above.
 *
 * This is a **client-side TypeScript port** of four Rust modules that ship the same filter engines for
 * the (feature-gated) backend index — `crates/server/src/size_filter.rs`, `date_filter.rs`,
 * `type_class.rs`, and the boolean structure of `query_group.rs` (leaves there are opaque strings; here
 * each leaf compiles directly to a predicate since we know its meaning at parse time). Keep the two in
 * sync: this module is the one that actually ships in the live folder filter today.
 *
 * ## Design
 * - Parse the query string **once** into a boolean predicate tree ({@link makeEntryMatcher}), not per
 *   entry — mirrors `makeMatcher` in `./search.ts`.
 * - A bare name term reuses {@link makeMatcher} from `./search.ts` so glob/`{a,b}` behavior for plain
 *   text is identical to the pre-existing name-only filter (no second glob implementation).
 * - An unrecognised `foo:bar` (a colon-prefix that isn't one of the known keys) is **not** dropped — the
 *   whole token is treated as a literal bare name term instead, same as any other plain text.
 * - A malformed filter token (e.g. `size:abc`, `date:2024-13`, `type:bogus`) compiles to a leaf that
 *   **matches nothing**, never throws. This mirrors the Rust parsers' `None`-on-garbage behavior, lifted
 *   one level: instead of a `None` the caller has to route around, an unparsable filter leaf is simply an
 *   always-false predicate.
 * - The boolean parser bounds its own nesting depth (`MAX_DEPTH`, same value as the Rust
 *   `query_group.rs`) so a pasted `"(".repeat(10_000)` or a long `NOT NOT NOT …` chain can never blow the
 *   call stack — past the cap, further `(`/`NOT` are folded into non-recursive content, exactly like the
 *   Rust module this precedence grammar is ported from.
 */

import { makeMatcher } from "./search";

/** The subset of `DirEntry` the power-filters need. Deliberately narrower than the full bindings type so
 *  this module has no dependency on Tauri bindings and is trivial to unit test with plain fixtures. */
export type EntryLike = {
  name: string;
  path: string;
  extension: string;
  size: number;
  /** Epoch-**milliseconds**, or `null` when the platform/filesystem reports none (never matches a
   *  `date:`/`modified:` filter — see {@link compileDateLeaf}). */
  modified: number | null;
};

/** A compiled predicate over a single entry. */
export type EntryMatcher = (e: EntryLike) => boolean;

// ============================================================================================
// size: — port of crates/server/src/size_filter.rs. 1024-based units, decimal mantissa, ranges.
// ============================================================================================

type SizeOp = "gt" | "lt" | "ge" | "le" | "eq";
type SizeFilter =
  | { kind: "cmp"; op: SizeOp; bytes: number }
  | { kind: "range"; lo: number; hi: number };

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;
const TB = GB * 1024;

/** Suffixes checked longest-first so "kb" isn't cut short at "k" leaving a dangling "b" — same ordering
 *  as the Rust `UNITS` table. */
const SIZE_UNITS: [string, number][] = [
  ["tb", TB],
  ["gb", GB],
  ["mb", MB],
  ["kb", KB],
  ["t", TB],
  ["g", GB],
  ["m", MB],
  ["k", KB],
  ["b", 1],
];

/** Parse an ASCII decimal mantissa: digits with at most one `.`, at least one digit total. `null` for
 *  anything else (letters, signs, multiple dots, empty). */
function parseMantissa(s: string): number | null {
  if (s === "") return null;
  if (![...s].every((ch) => (ch >= "0" && ch <= "9") || ch === ".")) return null;
  if ([...s].filter((ch) => ch === ".").length > 1) return null;
  if (![...s].some((ch) => ch >= "0" && ch <= "9")) return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}

/** Parse a single amount (`1mb`, `500k`, `2.5g`, a bare `500` = bytes) into a byte count. `null` for
 *  anything that isn't a clean amount. Rounds a decimal mantissa to the nearest integer byte count, ties
 *  away from zero — `Math.round` matches Rust's `f64::round` here since every mantissa is non-negative. */
function parseSizeAmount(s: string): number | null {
  if (s === "") return null;
  const lower = s.toLowerCase();
  let mantissaStr = s;
  let unit = 1;
  for (const [suffix, mult] of SIZE_UNITS) {
    if (lower.endsWith(suffix)) {
      mantissaStr = s.slice(0, s.length - suffix.length);
      unit = mult;
      break;
    }
  }
  const mantissa = parseMantissa(mantissaStr);
  if (mantissa === null || mantissa < 0) return null;
  const bytes = Math.round(mantissa * unit);
  if (!Number.isFinite(bytes) || bytes < 0) return null;
  return bytes;
}

/** Parse a `size:` token body (`>1mb`, `<=500k`, `=0`, `1mb..1gb`, `2.5g`) into a {@link SizeFilter}.
 *  `null` for garbage, empty input, or a range with `hi < lo` — never throws. */
function parseSizeFilter(token: string): SizeFilter | null {
  const t = token.trim();
  if (t === "") return null;

  const rangeIdx = t.indexOf("..");
  if (rangeIdx !== -1) {
    const lo = parseSizeAmount(t.slice(0, rangeIdx).trim());
    const hi = parseSizeAmount(t.slice(rangeIdx + 2).trim());
    if (lo === null || hi === null || hi < lo) return null;
    return { kind: "range", lo, hi };
  }

  let op: SizeOp = "eq";
  let rest = t;
  if (t.startsWith(">=")) { op = "ge"; rest = t.slice(2); }
  else if (t.startsWith("<=")) { op = "le"; rest = t.slice(2); }
  else if (t.startsWith(">")) { op = "gt"; rest = t.slice(1); }
  else if (t.startsWith("<")) { op = "lt"; rest = t.slice(1); }
  else if (t.startsWith("=")) { op = "eq"; rest = t.slice(1); }

  const bytes = parseSizeAmount(rest.trim());
  if (bytes === null) return null;
  return { kind: "cmp", op, bytes };
}

function sizeMatches(f: SizeFilter, bytes: number): boolean {
  if (f.kind === "range") return bytes >= f.lo && bytes <= f.hi;
  switch (f.op) {
    case "gt": return bytes > f.bytes;
    case "lt": return bytes < f.bytes;
    case "ge": return bytes >= f.bytes;
    case "le": return bytes <= f.bytes;
    case "eq": return bytes === f.bytes;
  }
}

/** Compile a `size:` leaf. A malformed token (NaN-producing garbage included) matches nothing — never
 *  throws, per CPE-1088's numeric-safety note. */
function compileSizeLeaf(rest: string): EntryMatcher {
  const f = parseSizeFilter(rest);
  if (f === null) return () => false;
  return (e) => sizeMatches(f, e.size);
}

// ============================================================================================
// date: / modified: — port of crates/server/src/date_filter.rs. Relative + absolute, `now` injectable.
// ============================================================================================

const SECS_PER_DAY = 86_400;
const SECS_PER_WEEK = SECS_PER_DAY * 7;
/** Fixed 30-day approximation of "a month" — documented, not calendar-accurate (matches the Rust module). */
const SECS_PER_MONTH_APPROX = SECS_PER_DAY * 30;
/** Fixed 365-day approximation of "a year" — documented, not calendar-accurate (matches the Rust module). */
const SECS_PER_YEAR_APPROX = SECS_PER_DAY * 365;
/** Inclusive bound on a parsed absolute-date year — mirrors `MAX_ABS_YEAR` in `date_filter.rs`, which
 *  exists so a huge digit string as a "year" is rejected before it can overflow downstream arithmetic. */
const MAX_ABS_YEAR = 99_999;

type DateFilter =
  | { kind: "before"; t: number }
  | { kind: "after"; t: number }
  | { kind: "between"; lo: number; hi: number };

function dateMatches(f: DateFilter, mtimeS: number): boolean {
  switch (f.kind) {
    case "before": return mtimeS < f.t;
    case "after": return mtimeS >= f.t;
    case "between": return mtimeS >= f.lo && mtimeS <= f.hi;
  }
}

/** Truncating integer division, matching Rust's `/` on integers (used only where the operands can be
 *  negative — `daysFromCivil`'s era math for a year just below 0). */
function idiv(a: number, b: number): number {
  return Math.trunc(a / b);
}

function daySpan(day: number): DateFilter {
  const lo = day * SECS_PER_DAY;
  return { kind: "between", lo, hi: lo + SECS_PER_DAY - 1 };
}

function isLeapYear(y: number): boolean {
  return (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0;
}

function daysInMonth(y: number, m: number): number {
  switch (m) {
    case 1: case 3: case 5: case 7: case 8: case 10: case 12: return 31;
    case 4: case 6: case 9: case 11: return 30;
    case 2: return isLeapYear(y) ? 29 : 28;
    default: return 0;
  }
}

/** Civil date (year, month 1-12, day 1-31) → days since the Unix epoch. Howard Hinnant's
 *  `days_from_civil` algorithm, ported verbatim from `date_filter.rs`. Does not validate ranges — callers
 *  do that first via {@link parseBounded}/{@link daysInMonth}. */
function daysFromCivil(y: number, m: number, d: number): number {
  const yy = m <= 2 ? y - 1 : y;
  const era = idiv(yy >= 0 ? yy : yy - 399, 400);
  const yoe = yy - era * 400;
  const mp = m > 2 ? m - 3 : m + 9;
  const doy = idiv(153 * mp + 2, 5) + d - 1;
  const doe = yoe * 365 + idiv(yoe, 4) - idiv(yoe, 100) + doy;
  return era * 146097 + doe - 719_468;
}

function parseDigits(s: string): number | null {
  if (s === "" || ![...s].every((ch) => ch >= "0" && ch <= "9")) return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}

function parseBounded(s: string, min: number, max: number): number | null {
  const v = parseDigits(s);
  if (v === null || v < min || v > max) return null;
  return v;
}

/** Parse the year component of an absolute-date token, bounded to `0..=MAX_ABS_YEAR` — rejects a
 *  syntactically-valid-but-huge digit string up front, before it can overflow `daysFromCivil`'s math
 *  (the same reviewer-caught bug class the Rust module documents). */
function parseYear(s: string): number | null {
  if (s === "" || s.length > 5 || ![...s].every((ch) => ch >= "0" && ch <= "9")) return null;
  const v = Number(s);
  if (!Number.isFinite(v) || v > MAX_ABS_YEAR) return null;
  return v;
}

/** Parse a `<N<unit>` / `>N<unit>` relative-age token, e.g. `<7d`, `>1w`, `<30d`. */
function parseRelative(t: string, nowS: number): DateFilter | null {
  const op = t[0];
  const rest = t.slice(1).trim();
  if (rest === "") return null;
  const numPart = rest.slice(0, rest.length - 1);
  const unitPart = rest.slice(rest.length - 1);
  if (numPart === "" || ![...numPart].every((ch) => ch >= "0" && ch <= "9")) return null;
  const n = Number(numPart);
  if (!Number.isFinite(n)) return null;
  let unitSecs: number;
  switch (unitPart.toLowerCase()) {
    case "d": unitSecs = SECS_PER_DAY; break;
    case "w": unitSecs = SECS_PER_WEEK; break;
    case "m": unitSecs = SECS_PER_MONTH_APPROX; break;
    case "y": unitSecs = SECS_PER_YEAR_APPROX; break;
    default: return null;
  }
  const delta = n * unitSecs;
  const threshold = nowS - delta;
  if (op === "<") return { kind: "after", t: threshold };
  if (op === ">") return { kind: "before", t: threshold };
  return null;
}

/** Parse an absolute `YYYY`, `YYYY-MM`, or `YYYY-MM-DD` token into its whole-span {@link DateFilter}. */
function parseAbsolute(t: string): DateFilter | null {
  const parts = t.split("-");
  if (parts.length === 1) {
    const year = parseYear(parts[0]);
    if (year === null) return null;
    const lo = daysFromCivil(year, 1, 1) * SECS_PER_DAY;
    const hi = daysFromCivil(year + 1, 1, 1) * SECS_PER_DAY - 1;
    return { kind: "between", lo, hi };
  }
  if (parts.length === 2) {
    const year = parseYear(parts[0]);
    const month = parseBounded(parts[1], 1, 12);
    if (year === null || month === null) return null;
    const lo = daysFromCivil(year, month, 1) * SECS_PER_DAY;
    const ny = month === 12 ? year + 1 : year;
    const nm = month === 12 ? 1 : month + 1;
    const hi = daysFromCivil(ny, nm, 1) * SECS_PER_DAY - 1;
    return { kind: "between", lo, hi };
  }
  if (parts.length === 3) {
    const year = parseYear(parts[0]);
    const month = parseBounded(parts[1], 1, 12);
    const day = parseBounded(parts[2], 1, 31);
    if (year === null || month === null || day === null) return null;
    if (day > daysInMonth(year, month)) return null; // e.g. day 32, or Feb 30, or Apr 31
    const lo = daysFromCivil(year, month, day) * SECS_PER_DAY;
    return { kind: "between", lo, hi: lo + SECS_PER_DAY - 1 };
  }
  return null;
}

/** Parse a `date:`/`modified:` token body against `nowS` (UTC epoch **seconds**). Recognises `today`,
 *  `yesterday`, `<N<unit>`/`>N<unit>` (unit one of `d`/`w`/`m`/`y`), and absolute `YYYY`/`YYYY-MM`/
 *  `YYYY-MM-DD`. `null` for malformed input — never throws. */
function parseDateFilter(token: string, nowS: number): DateFilter | null {
  const t = token.trim();
  if (t === "") return null;
  const lower = t.toLowerCase();
  if (lower === "today") return daySpan(Math.floor(nowS / SECS_PER_DAY));
  if (lower === "yesterday") return daySpan(Math.floor(nowS / SECS_PER_DAY) - 1);
  const c0 = t[0];
  if (c0 === "<" || c0 === ">") return parseRelative(t, nowS);
  return parseAbsolute(t);
}

/** Compile a `date:`/`modified:` leaf against the matcher's fixed `now` (ms, converted to whole seconds
 *  once here so the resolved thresholds are computed a single time, not per entry). A `null` `modified`
 *  on the entry never matches — there is no timestamp to test. A malformed filter token matches nothing. */
function compileDateLeaf(rest: string, nowMs: number): EntryMatcher {
  const nowS = Math.floor(nowMs / 1000);
  const f = parseDateFilter(rest, nowS);
  if (f === null) return () => false;
  return (e) => {
    if (e.modified === null) return false;
    return dateMatches(f, Math.floor(e.modified / 1000));
  };
}

// ============================================================================================
// type: — port of crates/server/src/type_class.rs. Ext -> FileClass tables must agree with the backend.
// ============================================================================================

type FileClass = "image" | "video" | "audio" | "document" | "archive" | "code" | "executable" | "other";

const IMAGE_EXTS = new Set([
  "png", "jpg", "jpeg", "jpe", "gif", "webp", "bmp", "svg", "tif", "tiff", "heic", "heif", "ico", "avif",
]);
const VIDEO_EXTS = new Set([
  "mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "3gp",
]);
const AUDIO_EXTS = new Set([
  "mp3", "flac", "wav", "ogg", "oga", "m4a", "aac", "wma", "opus", "aiff",
]);
const DOCUMENT_EXTS = new Set([
  "pdf", "doc", "docx", "txt", "md", "rtf", "odt", "xls", "xlsx", "ods", "ppt", "pptx", "odp", "csv",
  "epub",
]);
const ARCHIVE_EXTS = new Set([
  "zip", "tar", "gz", "tgz", "7z", "rar", "xz", "zst", "bz2", "iso", "cab",
]);
const CODE_EXTS = new Set([
  "rs", "ts", "tsx", "js", "jsx", "py", "go", "c", "h", "cpp", "cc", "hpp", "java", "rb", "sh", "ps1",
  "php", "cs", "swift", "kt", "json", "toml", "yaml", "yml", "html", "css",
]);
const EXECUTABLE_EXTS = new Set(["exe", "dll", "so", "dylib", "app", "msi", "bat", "cmd", "com", "bin"]);

/** Classify a file extension (no leading dot, any case) into its semantic class. An unrecognised or
 *  empty extension yields `"other"`. */
function classOf(ext: string): FileClass {
  const e = ext.toLowerCase();
  if (IMAGE_EXTS.has(e)) return "image";
  if (VIDEO_EXTS.has(e)) return "video";
  if (AUDIO_EXTS.has(e)) return "audio";
  if (DOCUMENT_EXTS.has(e)) return "document";
  if (ARCHIVE_EXTS.has(e)) return "archive";
  if (CODE_EXTS.has(e)) return "code";
  if (EXECUTABLE_EXTS.has(e)) return "executable";
  return "other";
}

/** Parse a class name (case-insensitive, singular/plural aliases) into a {@link FileClass}. `"other"` is
 *  not a selectable filter target — a file only lands there by exclusion, same as the Rust module. */
function classFromName(name: string): FileClass | null {
  switch (name.toLowerCase()) {
    case "image": case "images": return "image";
    case "video": case "videos": return "video";
    case "audio": return "audio";
    case "document": case "documents": case "doc": case "docs": return "document";
    case "archive": case "archives": return "archive";
    case "code": return "code";
    case "executable": case "executables": case "exe": return "executable";
    default: return null;
  }
}

/** Parse a `type:` token body (`image` or `image,video`) into a list of classes (an any-of / OR filter).
 *  `null` for empty input, an empty entry in the comma list, or any unrecognised class name. */
function parseTypeFilter(token: string): FileClass[] | null {
  const t = token.trim();
  if (t === "") return null;
  const classes: FileClass[] = [];
  for (const partRaw of t.split(",")) {
    const part = partRaw.trim();
    if (part === "") return null;
    const c = classFromName(part);
    if (c === null) return null;
    classes.push(c);
  }
  return classes.length > 0 ? classes : null;
}

/** Compile a `type:` leaf. A malformed token (empty, or any unrecognised class name) matches nothing. */
function compileTypeLeaf(rest: string): EntryMatcher {
  const classes = parseTypeFilter(rest);
  if (classes === null) return () => false;
  return (e) => classes.includes(classOf(e.extension));
}

// ============================================================================================
// ext: / path: — port of crates/server/src/index_query.rs's structured-filter semantics.
// ============================================================================================

/** Compile an `ext:` leaf: a comma list of extensions (a leading dot tolerated on each), any-of. An empty
 *  or all-empty list matches nothing (same as a lone `ext:` token in `index_query.rs`, which contributes
 *  no constraint and so leaves the overall query empty). */
function compileExtLeaf(rest: string): EntryMatcher {
  const exts = rest
    .split(",")
    .map((s) => s.trim().replace(/^\.+/, "").toLowerCase())
    .filter((s) => s !== "");
  if (exts.length === 0) return () => false;
  return (e) => exts.includes(e.extension.toLowerCase());
}

/** Compile a `path:` leaf: a case-insensitive substring match against the entry's full path. An empty
 *  term matches nothing (mirrors a lone `path:` token contributing no constraint in `index_query.rs`). */
function compilePathLeaf(rest: string): EntryMatcher {
  const term = rest.trim().toLowerCase();
  if (term === "") return () => false;
  return (e) => e.path.toLowerCase().includes(term);
}

// ============================================================================================
// Boolean structure — port of crates/server/src/query_group.rs's grammar (OR < AND < NOT < parens),
// with each leaf compiled directly to a predicate (no intermediate opaque-string leaf + separate eval
// pass, since we know each leaf's meaning at parse time).
// ============================================================================================

/** Maximum nesting depth the parser will recurse into for parenthesised groups and stacked `NOT`/`-`
 *  prefixes alike — matches `MAX_DEPTH` in `query_group.rs`. Bounds the call stack against adversarial or
 *  pasted input (e.g. `"(".repeat(10_000)`) so it can never blow the stack; past the cap, further nesting
 *  folds into ordinary, non-recursive content instead of recursing further. */
const MAX_DEPTH = 128;

type Node =
  | { kind: "leaf"; test: EntryMatcher }
  | { kind: "not"; child: Node }
  | { kind: "and"; parts: Node[] }
  | { kind: "or"; parts: Node[] };

type Tok =
  | { t: "(" }
  | { t: ")" }
  | { t: "OR" }
  | { t: "AND" }
  | { t: "NOT" }
  | { t: "WORD"; v: string };

/** Split `query` into tokens. `(`/`)` are hard separators (a leaf word never contains one). A word
 *  starting with `-` and having at least one more character splits into a `NOT` followed by the rest as a
 *  `WORD` (so `-token` tokenizes identically to `NOT token`); `OR`/`AND`/`NOT` are recognised
 *  case-insensitively, everything else is an opaque leaf word (original case preserved so name-term
 *  matching and filter values keep their original text). */
function lex(query: string): Tok[] {
  const toks: Tok[] = [];
  let i = 0;
  const n = query.length;
  while (i < n) {
    const c = query[i];
    if (/\s/.test(c)) { i += 1; continue; }
    if (c === "(") { toks.push({ t: "(" }); i += 1; continue; }
    if (c === ")") { toks.push({ t: ")" }); i += 1; continue; }
    const start = i;
    while (i < n && !/\s/.test(query[i]) && query[i] !== "(" && query[i] !== ")") {
      i += 1;
    }
    const word = query.slice(start, i);
    if (word === "") continue; // defensive; shouldn't happen given the checks above

    if (word.startsWith("-") && word.length > 1) {
      toks.push({ t: "NOT" });
      toks.push({ t: "WORD", v: word.slice(1) });
      continue;
    }
    const upper = word.toUpperCase();
    if (upper === "OR") toks.push({ t: "OR" });
    else if (upper === "AND") toks.push({ t: "AND" });
    else if (upper === "NOT") toks.push({ t: "NOT" });
    else toks.push({ t: "WORD", v: word });
  }
  return toks;
}

/** Whether `tok` can begin an atom (a leaf, a `NOT`, or a parenthesised group) — used to decide whether
 *  the implicit-AND loop should keep consuming another operand. */
function startsAtom(tok: Tok | undefined): boolean {
  return !!tok && (tok.t === "WORD" || tok.t === "NOT" || tok.t === "(");
}

/** Parser cursor state, threaded through the mutually-recursive precedence levels below. */
interface ParseState {
  tokens: Tok[];
  pos: number;
  /** The matcher's fixed `now` (ms), passed down to `date:`/`modified:` leaf compilation. */
  now: number;
}

/** Parse a query string into a predicate tree over leaf tokens, compiling each leaf's filter semantics as
 *  it goes. See the module docs for the grammar, precedence, unbalanced-parens / empty-query rules, and
 *  the {@link MAX_DEPTH} nesting bound. Never throws and never overflows the stack, however deeply nested
 *  or however long the input is. */
function parseQuery(tokens: Tok[], now: number): Node {
  const st: ParseState = { tokens, pos: 0, now };
  const parts: Node[] = [];
  for (;;) {
    // A stray, unmatched ")" at top level is a no-op: skip past it and keep parsing.
    while (st.tokens[st.pos]?.t === ")") st.pos += 1;
    if (st.pos >= st.tokens.length) break;
    parts.push(parseOr(st, 0));
  }
  if (parts.length === 1) return parts[0];
  return { kind: "and", parts }; // empty input -> and([]) matches everything; multiple -> ANDed together
}

/** `OR`-level: one or more AND-groups separated by `OR`. */
function parseOr(st: ParseState, depth: number): Node {
  const parts = [parseAnd(st, depth)];
  while (st.tokens[st.pos]?.t === "OR") {
    st.pos += 1;
    parts.push(parseAnd(st, depth));
  }
  return parts.length === 1 ? parts[0] : { kind: "or", parts };
}

/** `AND`-level: one or more atoms joined by juxtaposition or the explicit word `AND`. */
function parseAnd(st: ParseState, depth: number): Node {
  const parts: Node[] = [];
  for (;;) {
    if (st.tokens[st.pos]?.t === "AND") { st.pos += 1; continue; } // explicit AND is a no-op separator
    if (startsAtom(st.tokens[st.pos])) {
      parts.push(parseNot(st, depth));
    } else {
      break;
    }
  }
  return parts.length === 1 ? parts[0] : { kind: "and", parts };
}

/** `NOT`-level: zero or more `NOT`/`-` prefixes (stacking) around a single atom. Each prefix recurses and
 *  so counts against {@link MAX_DEPTH}; once the cap is hit, further `NOT`/`-` tokens are consumed
 *  without adding another wrapper or another stack frame. */
function parseNot(st: ParseState, depth: number): Node {
  if (st.tokens[st.pos]?.t === "NOT") {
    if (depth < MAX_DEPTH) {
      st.pos += 1;
      return { kind: "not", child: parseNot(st, depth + 1) };
    }
    while (st.tokens[st.pos]?.t === "NOT") st.pos += 1; // cap reached: swallow iteratively, no more recursion
  }
  return parseAtom(st, depth);
}

/** Innermost level: a leaf token (compiled immediately), or a parenthesised sub-expression. Tolerates a
 *  missing closing paren by simply not consuming one. Opening a group recurses and so counts against
 *  {@link MAX_DEPTH}; once the cap is hit, a further `(` is treated as an ordinary leaf character instead
 *  of opening another nested group. */
function parseAtom(st: ParseState, depth: number): Node {
  const tok = st.tokens[st.pos];
  if (tok?.t === "(" && depth < MAX_DEPTH) {
    st.pos += 1;
    const inner = parseOr(st, depth + 1);
    if (st.tokens[st.pos]?.t === ")") st.pos += 1;
    // else: unbalanced "(" with no matching ")" before EOF — auto-close, don't throw.
    return inner;
  }
  if (tok?.t === "(") {
    // MAX_DEPTH reached: don't recurse into another parseOr (the stack-overflow vector) — fold this "("
    // into a literal leaf instead.
    st.pos += 1;
    return { kind: "leaf", test: compileLeaf("(", st.now) };
  }
  if (tok?.t === "WORD") {
    st.pos += 1;
    return { kind: "leaf", test: compileLeaf(tok.v, st.now) };
  }
  // Called only when `startsAtom` said yes, or at the top of an empty/degenerate group; fall back to the
  // empty match-everything node rather than throwing on anything unexpected.
  return { kind: "and", parts: [] };
}

/** The recognised `key:` prefixes, checked longest-match order isn't needed here since none of them share
 *  a common prefix with another. */
const LEAF_PREFIXES: [string, (rest: string, now: number) => EntryMatcher][] = [
  ["size:", (rest) => compileSizeLeaf(rest)],
  ["date:", (rest, now) => compileDateLeaf(rest, now)],
  ["modified:", (rest, now) => compileDateLeaf(rest, now)],
  ["type:", (rest) => compileTypeLeaf(rest)],
  ["ext:", (rest) => compileExtLeaf(rest)],
  ["path:", (rest) => compilePathLeaf(rest)],
];

/** Compile a single leaf word into a predicate. A recognised `key:` prefix dispatches to its filter
 *  compiler; anything else — including an unrecognised `foo:bar` — is a bare name term, matched via the
 *  shared {@link makeMatcher} so glob/`{a,b}` behavior is identical to the pre-existing name-only filter. */
function compileLeaf(word: string, now: number): EntryMatcher {
  const lower = word.toLowerCase();
  for (const [prefix, compile] of LEAF_PREFIXES) {
    if (lower.startsWith(prefix)) {
      return compile(word.slice(prefix.length), now);
    }
  }
  const nameMatch = makeMatcher(word);
  return (e) => nameMatch(e.name);
}

function evalNode(node: Node, e: EntryLike, depth: number): boolean {
  if (depth >= MAX_DEPTH) return true; // pathological tree past the cap: permissive fallback, never a crash
  switch (node.kind) {
    case "leaf": return node.test(e);
    case "not": return !evalNode(node.child, e, depth + 1);
    case "and": return node.parts.every((p) => evalNode(p, e, depth + 1));
    case "or": return node.parts.some((p) => evalNode(p, e, depth + 1));
  }
}

/**
 * Compile `query` **once** into a reusable predicate over {@link EntryLike} entries. Filtering a folder
 * calls the matcher per entry, so the query parse + every filter token's compilation must happen here,
 * not per entry.
 *
 * Supports `size:`, `date:`/`modified:`, `type:`, `ext:`, `path:`, plain/glob name terms (via
 * {@link makeMatcher}), and the boolean grammar `OR` < implicit `AND` < `NOT`/`-` < `( … )` — see the
 * module docs. An empty/whitespace query matches everything. `now` (epoch-ms) is injectable so relative
 * `date:`/`modified:` windows are deterministic in tests; defaults to `Date.now()`.
 */
export function makeEntryMatcher(query: string, now: number = Date.now()): EntryMatcher {
  const tokens = lex(query);
  const tree = parseQuery(tokens, now);
  return (e: EntryLike) => evalNode(tree, e, 0);
}
