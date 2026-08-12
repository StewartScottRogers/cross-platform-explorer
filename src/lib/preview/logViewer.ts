/**
 * Pure parsing for log-file preview: per-line severity detection + counts (CPE-1618, epic CPE-1568
 * slice 8). Framework-free (no Svelte import) so the parse/detect/filter logic is unit-testable
 * without mounting a component — mirrors the "pure module behind the preview component" convention
 * `notebook.ts`/`jsonTree.ts`/`csv.ts` already established.
 *
 * A log file is untrusted, attacker-influenced input in plenty of real setups (a service that logs
 * request bodies, a scraped/downloaded log, a log shipped by someone else's misbehaving process, …).
 * {@link parseLog} and {@link detectLevel} never throw.
 *
 * **Caps bound WORK, not just output.** The file's own read size is already bounded by the generic
 * preview text cap (`PREVIEW_MAX_BYTES`, `preview/loaders.ts`) — this module never reads a file itself,
 * it only parses text the caller already capped — but within that capped text, {@link MAX_LINES} still
 * bounds how many of the split lines are ever mapped over (sliced BEFORE mapping, so a file with far
 * more lines than the cap never pays for detecting/stripping the rest), and {@link detectLevel} only
 * ever inspects the first {@link LEVEL_SCAN_CHARS} characters of a line — regardless of how long a
 * single pathological line is — so neither a huge line count nor one huge single line (e.g. a
 * multi-hundred-KB line with no newlines, within the overall read cap) can make level detection do
 * unbounded work. This crew learned the hard way (CPE-1616 font-cache bug, an 8.8s UI freeze) that a
 * cap counting only what's *emitted* isn't a real cap — every cap here bounds what's *examined*.
 *
 * **ANSI escape sequences** are extremely common in real captured logs (colourised by loggers like
 * pino-pretty, winston, colorama-backed Python loggers, …). Left untouched they render as literal
 * `[0;31m` garbage. Reuses `stripAnsi` from `notebook.ts` (CPE-1616) rather than duplicating the same
 * regex a second time in this module.
 *
 * **Level detection is deliberately conservative.** A naive "does ERROR appear anywhere in the line"
 * regex would misclassify an ordinary INFO line that merely *mentions* an error in prose (e.g. "User
 * asked about a checkout error they saw yesterday"). {@link detectLevel} only accepts a level-word match
 * that is genuinely in level *position* — both its lead-in (text before it) and its trailing character
 * (text right after it) have to look like a real log-line prefix/separator, not running prose (CPE-1636):
 *   - **Lead-in** must contain no "isolated" letter word — a run of letters (any case) that isn't
 *     immediately glued onto a digit — no numbered-list marker shape (`"1. ERROR ..."`), and must not end
 *     in a quote character (rules out `"ERROR" is a reserved word...`). A genuine level marker sits at the
 *     very start of the line or right after a timestamp/bracket/pid prefix, and those are built from
 *     digits/punctuation and letters that touch a digit (`2026-08-11`**`T`**`09:14:05`**`Z`** — the ISO
 *     `T`/`Z` always sit directly against a digit), never a standalone word. This one rule replaces two
 *     narrower ones an earlier pass at this fix used (reject-on-any-lowercase, reject-on-2+-uppercase-run)
 *     — both of those still miss a single capitalized word starting a sentence, e.g. `"A warning icon
 *     appears next to any file that couldn't be scanned."` (found during independent real-prose
 *     verification of this ticket: `"A "` contains no lowercase and no uppercase *run*, so it passed both
 *     narrower checks and was misclassified as `warn`) — the isolated-word rule catches this too, since
 *     `"A"` is a letter run touching neither a digit before nor after it.
 *   - **Trailing character** must be whitespace, `:`, `]`, `|`, or end-of-line — a real level marker is
 *     followed by a separator, not glued onto the next word (rules out compounds like `error-like`).
 * A line that doesn't clear both bars renders as plain/unleveled text — never guessed at.
 *
 * **Stack-trace continuations inherit their header's level for filtering (CPE-1638).** A multi-line
 * finding — an exception header plus its stack frames — usually has the level word on the header line
 * only; the frames themselves carry no level word of their own. Filtering to Errors-only would otherwise
 * hide exactly the detail the filter exists to surface. {@link parseLog} runs a second, O(n) pass
 * ({@link groupContinuations}) that gives an unleveled line following a classified one a `filterLevel`
 * inherited from that line when it *looks* like a continuation (`at ...`, `File "..."`, `Caused by: ...`,
 * a trailing `...` — any of those optionally indented — or — only right after an error — a bare
 * `XError:`/`XException:` header). Indentation alone is never enough (F2, PR #842 review): interleaved
 * multi-thread/multi-process output is full of incidentally-indented lines with no relation to the
 * preceding one, so an indented line still has to clear the same shape bar once its own whitespace is
 * stripped. The chain breaks the instant a line doesn't look like a continuation, so unrelated lines are
 * never swept in; see that function's doc comment for the exact shapes recognized.
 */
import { stripAnsi } from "./notebook";

export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

/** One rendered row: a line's (capped, ANSI-stripped) text plus its detected level, if any. */
export interface LogLine {
  /** 0-based index within the lines actually processed (post-{@link MAX_LINES} cap). */
  index: number;
  /** ANSI-stripped, length-capped line text, ready to render via `{text}` (never `{@html}`). */
  text: string;
  /** This line's OWN detected level — what the badge/border render from. `null` for a continuation line
   *  even though it has a {@link filterLevel} (CPE-1638): a stack frame doesn't get its own "Error" badge
   *  just because it belongs to one, which is exactly what avoids the "wall of red" a 10-frame trace would
   *  otherwise paint. */
  level: LogLevel | null;
  /** True when this line's own text was cut to {@link MAX_LINE_CHARS}. */
  truncated: boolean;
  /** The level a level-filter should key on: `level` when this line has one, otherwise the inherited
   *  level of the classified line it continues (or `null` when it's neither) — see {@link groupContinuations}
   *  and {@link filterLines} (CPE-1638). */
  filterLevel: LogLevel | null;
  /** True when this line has no level of its own but was grouped with a preceding classified line as its
   *  continuation (CPE-1638) — lets the view render it visually subordinate to its header rather than as
   *  its own fully-tinted row. */
  isContinuation: boolean;
}

export interface ParsedLog {
  lines: LogLine[];
  /** The text's real line count, even when {@link linesCapped} cut the list down. */
  totalLines: number;
  linesCapped: boolean;
  /** Per-level counts over the lines actually processed (post-cap) — matches `lines`, not `totalLines`. */
  counts: Record<LogLevel, number>;
}

/** Which levels (plus unleveled lines) a filtered render should show — a small, pure, testable value
 *  the view's level-filter chips drive; see {@link filterLines}. */
export interface LevelFilter {
  levels: Set<LogLevel>;
  showUnleveled: boolean;
}

/** Render/work cap on line count — sliced off the split array BEFORE any line is detected/stripped, so
 *  a log file with far more lines than fit in the generic preview byte cap still stays responsive. */
export const MAX_LINES = 5000;

/** Cap on one line's own rendered length. Purely a render-size bound (detection itself is already
 *  bounded independently by {@link LEVEL_SCAN_CHARS}) — keeps one pathological giant line (e.g. a
 *  minified JSON blob logged on one line) from becoming an enormous unwrapped DOM text node. */
export const MAX_LINE_CHARS = 2000;

/** How many characters from a line's start {@link detectLevel} ever inspects. Every format this
 *  detector targets puts its level marker within the first few characters — bounding the scan window
 *  here means detection work is O(1) per line no matter how long the (possibly pathological) line is. */
const LEVEL_SCAN_CHARS = 48;

const LEVEL_WORD_REGEX = /\b(ERROR|ERR|WARNING|WARN|INFO|DEBUG|DBG|TRACE)\b/i;
const LEVEL_WORD_MAP: Record<string, LogLevel> = {
  ERROR: "error",
  ERR: "error",
  WARNING: "warn",
  WARN: "warn",
  INFO: "info",
  DEBUG: "debug",
  DBG: "debug",
  TRACE: "trace",
};

/** Android logcat shape: `E/Tag: message`, `W/Tag(1234): message` — a single level letter immediately
 *  followed by a slash. Anchored at the true start of the line (not just "early in the scan window"),
 *  and deliberately case-sensitive (unlike the word regex): a lowercase `e/` is far more likely to be a
 *  path/URL fragment than a logcat tag, and this detector must never guess. */
const ANDROID_LEVEL_REGEX = /^([EWIDV])\//;
const ANDROID_LEVEL_MAP: Record<string, LogLevel> = {
  E: "error",
  W: "warn",
  I: "info",
  D: "debug",
  V: "trace", // Android "Verbose" — closest existing level to this app's TRACE.
};

// --- CPE-1636: the lead-in/trailing shape checks that keep detectLevel off ordinary prose. Each is a
// flat, non-backtracking regex (no nested quantifiers) tested only against the already-bounded
// LEVEL_SCAN_CHARS prefix, so none of this reintroduces unbounded per-line work. ---

/** Finds every maximal run of ASCII letters in the lead-in, globally — used by
 *  {@link leadHasIsolatedLetterWord} to check each run's preceding character. A single flat character
 *  class with a `g` flag; no backtracking risk. */
const LETTER_RUN_REGEX = /[A-Za-z]+/g;

/** A bracket-wrapped token with no internal whitespace — the shape of a logger/thread-name tag a real
 *  logging framework prefixes a level with (Logback's own documented `%d [%thread] %level` pattern:
 *  `[main]`, `[Thread-3]`, `[pool-2-thread-1]`, `[http-nio-8080-exec-1]`, …). Used by
 *  {@link leadHasIsolatedLetterWord} (F1, CPE-1636 followups) to exempt letters inside one of these from
 *  the isolated-word rule — a single flat character class with a `g` flag, no backtracking risk. Deliberately
 *  narrow: requires the WHOLE token between `[` and `]` to contain no whitespace, so it can't accidentally
 *  swallow a parenthetical prose remark ("[see the docs for more]" has spaces and never matches). */
const BRACKET_TOKEN_REGEX = /\[[^[\]\s]+\]/g;

/** True when `lead` contains a run of letters that is NOT immediately preceded by a digit and NOT wholly
 *  inside a {@link BRACKET_TOKEN_REGEX} logger/thread-tag token — i.e. a standalone alphabetic word, as
 *  opposed to a letter glued directly onto a timestamp's digits (the ISO `T`/`Z` in `09:14:05Z`, which
 *  always touch a digit with no separating space) or sitting inside a bracketed tag with no internal
 *  whitespace (Logback's `[main]`). This is what actually distinguishes "[2026-08-11T09:14:05Z] ERROR"
 *  (never flagged: T and Z both touch a digit) or "17:04:22.123 [main] ERROR" (never flagged, F1: "main"
 *  is wholly inside a bracket token) from "A warning icon appears..." (flagged: "A" touches nothing and
 *  isn't bracketed, i.e. a real word starting a sentence) or "SEE ERROR HANDLING DOCS" (flagged: "SEE"
 *  touches nothing) — a single general rule that subsumes what two narrower ones (reject-any-lowercase,
 *  reject-2+-uppercase-run) used to check separately, and — found during this ticket's independent
 *  real-prose verification pass — catches the single-capitalized-word case those two missed. */
function leadHasIsolatedLetterWord(lead: string): boolean {
  // `matchAll` operates on an internal copy of the regex, so no shared `lastIndex` state to reset here.
  const bracketTokenRanges: Array<[number, number]> = [];
  for (const m of lead.matchAll(BRACKET_TOKEN_REGEX)) {
    bracketTokenRanges.push([m.index, m.index + m[0].length]);
  }
  const isInsideBracketToken = (idx: number) =>
    bracketTokenRanges.some(([start, end]) => idx >= start && idx < end);

  for (const m of lead.matchAll(LETTER_RUN_REGEX)) {
    const idx = m.index;
    if (isInsideBracketToken(idx)) continue;
    const precedingChar = idx > 0 ? lead[idx - 1] : "";
    if (!(precedingChar >= "0" && precedingChar <= "9")) return true;
  }
  return false;
}

/** A lead-in that is *only* a numbered-list marker (`"1. "`, `"12."`) — a documentation/heading shape
 *  ("1. ERROR handling guide"), never how a real logger prefixes a level. */
const LEAD_LIST_MARKER_REGEX = /^\d+\.\s*$/;

/** Quote characters that, sitting immediately before the match, mark it as a quoted/mentioned word
 *  (`"ERROR" is a reserved word...`) rather than a genuine level marker in position. */
const QUOTE_CHARS = new Set(['"', "'", "`", "\u2018", "\u2019", "\u201C", "\u201D"]);

/** A real level marker is followed by a separator — whitespace, `:`, `]`, `|`, or end-of-line — never
 *  glued straight onto the next character (rules out compounds like "error-like" or "errorish"). */
const TRAILING_SEPARATOR_REGEX = /^[\s:\]|]$/;

/**
 * Detect a line's severity level from common leveled-log shapes (bracketed/plain timestamp prefix,
 * `LEVEL:` prefix, `[LEVEL]` prefix, or Android logcat's `L/Tag:`). Never throws — a plain regex
 * test/slice can't. Returns `null` for anything that doesn't match; see the module doc comment for why
 * this is deliberately conservative rather than a "match the word anywhere" scan.
 */
export function detectLevel(line: string): LogLevel | null {
  const prefix = line.length > LEVEL_SCAN_CHARS ? line.slice(0, LEVEL_SCAN_CHARS) : line;

  const androidMatch = ANDROID_LEVEL_REGEX.exec(prefix);
  if (androidMatch) {
    const level = ANDROID_LEVEL_MAP[androidMatch[1]];
    if (level) return level;
  }

  const wordMatch = LEVEL_WORD_REGEX.exec(prefix);
  if (wordMatch && wordMatch.index !== undefined) {
    const lead = prefix.slice(0, wordMatch.index);
    const matchEnd = wordMatch.index + wordMatch[0].length;
    const trailChar = matchEnd < prefix.length ? prefix[matchEnd] : "";

    const leadLooksLikePrefix =
      !leadHasIsolatedLetterWord(lead) &&
      !LEAD_LIST_MARKER_REGEX.test(lead) &&
      (lead.length === 0 || !QUOTE_CHARS.has(lead[lead.length - 1]));
    const trailLooksLikeSeparator = trailChar === "" || TRAILING_SEPARATOR_REGEX.test(trailChar);

    if (leadLooksLikePrefix && trailLooksLikeSeparator) {
      const level = LEVEL_WORD_MAP[wordMatch[1].toUpperCase()];
      if (level) return level;
    }
  }

  return null;
}

function capText(text: string, max: number): { text: string; truncated: boolean } {
  return text.length > max ? { text: text.slice(0, max), truncated: true } : { text, truncated: false };
}

function emptyCounts(): Record<LogLevel, number> {
  return { error: 0, warn: 0, info: 0, debug: 0, trace: 0 };
}

// --- CPE-1638: stack-trace continuation grouping. ---

/** How many characters of a would-be continuation line {@link looksLikeContinuation} inspects — same
 *  reasoning as {@link LEVEL_SCAN_CHARS}: every shape it recognizes is identifiable from the very start
 *  of the line, so a short bounded window keeps this O(1) per line regardless of line length. */
const CONTINUATION_SCAN_CHARS = 64;

/** Leading spaces/tabs, captured so the corroborating-signal checks below can be re-run against what's
 *  left AFTER the indentation is stripped — see {@link looksLikeContinuation}'s F2 fix. */
const CONTINUATION_LEADING_WS_REGEX = /^[ \t]+/;
const CONTINUATION_ELLIPSIS_REGEX = /^\.\.\./;
const CONTINUATION_CAUSED_BY_REGEX = /^Caused by:/i;
const CONTINUATION_AT_FRAME_REGEX = /^at\s/;
/** A Python traceback frame's own line shape: `File "path", line N, in name`. Only ever checked against
 *  an already-indented line's trimmed remainder (real Python tracebacks always indent this line) — see
 *  {@link looksLikeContinuation}. */
const CONTINUATION_PYTHON_FRAME_REGEX = /^File "/;
/** A bare exception-type header with no level word of its own (`"AbortError: Request aborted"`,
 *  `"NullPointerException: ..."`) — recognized ONLY as a continuation of an immediately preceding ERROR
 *  line (never a warn/info/etc.), and only when it ends in the conventional "Error"/"Exception" suffix,
 *  so an unrelated capitalized sentence ("Note: see docs") is never swept in. */
const BARE_EXCEPTION_HEADER_REGEX = /^[A-Za-z][A-Za-z0-9]*(?:Error|Exception):\s/;

/** True when `text` looks like it continues a line already classified as `parentLevel` — see the module
 *  doc comment's CPE-1638 section for the shapes recognized and why each is included. Deliberately
 *  conservative: prefers under-grouping (missing a real continuation) to over-grouping (sweeping in an
 *  unrelated line), per the ticket's explicit steer.
 *
 *  **F2 fix (PR #842 review):** bare leading whitespace used to be accepted as a continuation signal all
 *  by itself. That's far too weak — interleaved multi-thread/multi-process log output is full of
 *  incidentally-indented lines (sub-status messages, nested JSON, anything a logger chose to indent for
 *  readability) that have nothing to do with the preceding line. Indentation is now necessary but never
 *  sufficient: an indented line only counts once its OWN leading whitespace is stripped away and what
 *  remains still matches one of the real corroborating shapes below (a stack frame, a Python traceback
 *  frame line, a "Caused by:" continuation, or a trailing elision) — the same bar an unindented line has
 *  to clear. */
function looksLikeContinuation(text: string, parentLevel: LogLevel): boolean {
  const head = text.length > CONTINUATION_SCAN_CHARS ? text.slice(0, CONTINUATION_SCAN_CHARS) : text;
  if (CONTINUATION_ELLIPSIS_REGEX.test(head)) return true; // "... 9 more"
  if (CONTINUATION_CAUSED_BY_REGEX.test(head)) return true; // "Caused by: ..."
  if (CONTINUATION_AT_FRAME_REGEX.test(head)) return true; // an unindented "at ..." frame
  if (parentLevel === "error" && BARE_EXCEPTION_HEADER_REGEX.test(head)) return true;

  const wsMatch = CONTINUATION_LEADING_WS_REGEX.exec(head);
  if (wsMatch) {
    const trimmed = head.slice(wsMatch[0].length);
    if (
      CONTINUATION_AT_FRAME_REGEX.test(trimmed) || // an indented "at ..." frame
      CONTINUATION_ELLIPSIS_REGEX.test(trimmed) || // an indented "... N more"
      CONTINUATION_CAUSED_BY_REGEX.test(trimmed) || // an indented "Caused by: ..."
      CONTINUATION_PYTHON_FRAME_REGEX.test(trimmed) // a Python `File "...", line N, in ...` frame
    ) {
      return true;
    }
  }
  return false;
}

/**
 * Second, O(n) pass over already-detected lines: an unleveled line immediately following a classified
 * (or already-grouped) line, that {@link looksLikeContinuation} of it, inherits that line's level into
 * `filterLevel` and is flagged `isContinuation` — so an errors-only filter keeps the whole finding
 * (header + trace) together instead of just the bare header. The chain breaks — `filterLevel` resets to
 * `null` — the instant a line doesn't look like a continuation, so unrelated lines that merely follow an
 * error are never swept in. Mutates `lines` in place (each entry's `filterLevel`/`isContinuation`
 * were already initialized in {@link parseLog}'s map); never rescans line text beyond the bounded
 * {@link CONTINUATION_SCAN_CHARS} window per line, so this stays flat over lines already sliced to
 * {@link MAX_LINES}.
 */
function groupContinuations(lines: LogLine[]): void {
  let carryLevel: LogLevel | null = null;
  for (const line of lines) {
    if (line.level) {
      carryLevel = line.level;
      line.filterLevel = line.level;
      line.isContinuation = false;
    } else if (carryLevel && looksLikeContinuation(line.text, carryLevel)) {
      line.filterLevel = carryLevel;
      line.isContinuation = true;
    } else {
      carryLevel = null;
      line.filterLevel = null;
      line.isContinuation = false;
    }
  }
}

/**
 * Parse raw log text into per-line render-ready rows with detected levels. Never throws.
 *
 * Splits on both `\n` and `\r\n` line endings; a single trailing newline doesn't produce a spurious
 * trailing empty "line" (matching how every line-oriented tool treats a final newline). The number of
 * lines actually processed is capped to {@link MAX_LINES} — see the module doc comment for why the
 * slice happens before any per-line work, not after.
 */
export function parseLog(raw: string): ParsedLog {
  // An empty file has zero lines, not one empty line.
  const rawLines = raw.length === 0 ? [] : raw.split(/\r\n|\n/);
  if (rawLines.length > 0 && rawLines[rawLines.length - 1] === "") rawLines.pop();

  const totalLines = rawLines.length;
  const linesCapped = totalLines > MAX_LINES;
  const toProcess = linesCapped ? rawLines.slice(0, MAX_LINES) : rawLines;

  const counts = emptyCounts();
  const lines: LogLine[] = toProcess.map((raw, index) => {
    const clean = stripAnsi(raw);
    const level = detectLevel(clean);
    if (level) counts[level]++;
    const { text, truncated } = capText(clean, MAX_LINE_CHARS);
    return { index, text, level, truncated, filterLevel: level, isContinuation: false };
  });

  // CPE-1638: group stack-trace continuation lines with their header so an errors-only filter keeps the
  // whole finding, not just the bare header. Overwrites the placeholder filterLevel/isContinuation set
  // above for any line that turns out to be a continuation of the preceding one.
  groupContinuations(lines);

  return { lines, totalLines, linesCapped, counts };
}

/** Lines matching the current {@link LevelFilter} — pure client-side filter, no re-parse/re-fetch. Keys
 *  on {@link LogLine.filterLevel} rather than `level` (CPE-1638) so a continuation line travels with its
 *  header: shown iff `filterLevel` is in `filter.levels`, or iff `showUnleveled` when `filterLevel` is
 *  `null` (a truly unrelated line, never grouped with anything). */
export function filterLines(lines: LogLine[], filter: LevelFilter): LogLine[] {
  return lines.filter((line) => (line.filterLevel ? filter.levels.has(line.filterLevel) : filter.showUnleveled));
}

/** All five levels, in severity order (most to least severe) — the canonical order the filter chips and
 *  any level-keyed UI should iterate in. */
export const ALL_LEVELS: LogLevel[] = ["error", "warn", "info", "debug", "trace"];

/** Cap on how many fetched log-window pages `LogPreview.svelte` keeps cached while paging backward
 *  (CPE-1644 B′) — exhaustively paging through a huge file must not re-accumulate the whole file
 *  in memory, a slower, opt-in version of the exact problem CPE-1637's bounded windowed reads exist to
 *  fix. Each cached page is at most `PREVIEW_MAX_BYTES` (`preview/loaders.ts`), so this bounds the cache
 *  to a fixed multiple of that regardless of how many times the user clicks "Load earlier". */
export const MAX_CACHED_LOG_PAGES = 20;

/** Append a newly-fetched page to the cache, evicting from the shallow/oldest end once the cache exceeds
 *  {@link MAX_CACHED_LOG_PAGES}. Pure so it's unit-testable without mounting the component; `LogPreview`
 *  navigation is strictly append-only while paging backward (only a full reset via "Back to latest" ever
 *  moves the other way — see that component), so evicting the oldest/shallowest entries never discards a
 *  page the current pointer could still reach. */
export function pushLogPage<T>(pages: T[], page: T): T[] {
  const next = [...pages, page];
  return next.length > MAX_CACHED_LOG_PAGES ? next.slice(next.length - MAX_CACHED_LOG_PAGES) : next;
}
