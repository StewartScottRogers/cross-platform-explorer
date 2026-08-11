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
 * asked about a checkout error they saw yesterday"). {@link detectLevel} only accepts a level-word
 * match whose lead-in (the text before it, within the scan window) contains no lowercase letter — a
 * genuine level marker sits at the very start of the line or right after a timestamp/bracket/pid
 * prefix, and those are built from digits/punctuation/uppercase (`[2026-08-11 12:00:00]`,
 * `2026-08-11T12:00:00Z`, `[WARN]`, …), never lowercase prose. A line that doesn't clear this bar
 * renders as plain/unleveled text — never guessed at.
 */
import { stripAnsi } from "./notebook";

export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

/** One rendered row: a line's (capped, ANSI-stripped) text plus its detected level, if any. */
export interface LogLine {
  /** 0-based index within the lines actually processed (post-{@link MAX_LINES} cap). */
  index: number;
  /** ANSI-stripped, length-capped line text, ready to render via `{text}` (never `{@html}`). */
  text: string;
  level: LogLevel | null;
  /** True when this line's own text was cut to {@link MAX_LINE_CHARS}. */
  truncated: boolean;
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
    // A genuine level marker's lead-in is a timestamp/bracket/pid shape — digits, punctuation, and
    // uppercase letters (ISO "T"/"Z", …) only. Any lowercase letter means the "level word" is really
    // just part of an ordinary sentence, and this line should render as unleveled plain text instead.
    if (!/[a-z]/.test(lead)) {
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
    return { index, text, level, truncated };
  });

  return { lines, totalLines, linesCapped, counts };
}

/** Lines matching the current {@link LevelFilter} — pure client-side filter, no re-parse/re-fetch. An
 *  unleveled line (`level === null`) is shown iff `showUnleveled`; a leveled line is shown iff its
 *  level is in `filter.levels`. */
export function filterLines(lines: LogLine[], filter: LevelFilter): LogLine[] {
  return lines.filter((line) => (line.level ? filter.levels.has(line.level) : filter.showUnleveled));
}

/** All five levels, in severity order (most to least severe) — the canonical order the filter chips and
 *  any level-keyed UI should iterate in. */
export const ALL_LEVELS: LogLevel[] = ["error", "warn", "info", "debug", "trace"];
