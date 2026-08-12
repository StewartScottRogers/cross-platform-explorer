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
 *
 * **One coherent detection model (CPE-1655/1656/1657).** The level-word path above and the continuation
 * grouping above it are the "narrow, structural signal only" foundation both a WIDEN and a TIGHTEN pass
 * build on without contradicting it: CPE-1655 widens {@link detectLevel} with a handful of additional
 * *structurally unambiguous, line-start-anchored* shapes for real error/crash output that carries no level
 * word at all (Python's `Traceback (most recent call last):`, Rust's `thread '...' panicked at ...` and
 * `stack backtrace:`, Go's `panic: ...` and `goroutine N [...]:`, DISM's `[pid.tid] [0xHEXCODE]
 * Func:(line):` status shape) plus a markdown-ATX-heading lead-in rejection; CPE-1657 tightens
 * {@link TIMESTAMP_SHAPE_REGEX} from "any digit-separator-digit run" to a genuine ISO-date-or-clock-time
 * shape, closing an adversarial gap in the bracket-corroboration gate without touching anything the widen
 * pass added; CPE-1656 widens {@link groupContinuations}'s recognized frame shapes (Go, Rust, Ruby) and
 * grants Python's own source-excerpt/caret body lines a narrow bounded allowance. Every new "headerless"
 * shape stays inside the SAME safety bar the rest of the module already enforces — anchored, structural,
 * never a bare word match — so the widen and tighten halves of this pass never fight each other: one
 * makes the level-position rule recognize more genuine structural markers, the other makes an existing
 * corroboration check actually correspond to what it claims to check for.
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
 *  swallow a parenthetical prose remark ("[see the docs for more]" has spaces and never matches).
 *
 *  **Only reached once {@link leadHasIsolatedLetterWord}'s timestamp-corroboration gate has already
 *  passed** (round 3, PR #842) — see that function's doc comment for why the bracket alone is no longer
 *  trusted on its own. */
const BRACKET_TOKEN_REGEX = /\[[^[\]\s]+\]/g;

/** A complete `[...]` bracket pair anywhere in a lead-in — unlike {@link BRACKET_TOKEN_REGEX}, this one
 *  DOES allow internal whitespace, because it exists only to answer "is there a bracket here at all",
 *  not to carve out an exemptable token. Used by {@link leadHasIsolatedLetterWord}'s timestamp-gate (round
 *  3, PR #842). Flat and non-backtracking: `[^[\]]*` excludes brackets, so no nested-quantifier blowup,
 *  and it only ever runs against the already-bounded `LEVEL_SCAN_CHARS` lead-in. */
const ANY_BRACKET_PAIR_REGEX = /\[[^[\]]*\]/;

/** A genuine timestamp/date shape — either a full ISO calendar date (`2026-08-11`, all three
 *  four/two/two-digit groups) or a real clock time (`17:04`, `09:14:05`, hour 00-23, minute/second
 *  00-59). Used by {@link leadHasIsolatedLetterWord}'s bracket-corroboration gate (round 3, PR #842) — see
 *  that function's doc comment.
 *
 *  **CPE-1657 tightening.** The original version of this regex (`/\d{1,4}[:-]\d{2}/`) matched *any*
 *  digit-separator-digit run, not a timestamp — a date fragment (`2026-08`, no day component) or an
 *  IP:port octet (`.1:8080`) satisfied it exactly as well as a genuine clock time, letting two adversarial
 *  inputs (`"2026-08 [draft] ERROR budget"`, `"10.0.0.1:8080 [proxy] ERROR rate"`) defeat the bracket gate
 *  and misclassify as `error`. Both are rejected by the tightened shape: `2026-08` has no third
 *  `-\d{2}` day group, so it fails the full-ISO-date alternative; `1:8080`/`.1:80` never has TWO digits
 *  immediately before the `:` (only a single `1`, preceded by `.`, not another digit), and even where a
 *  2-digit group does sit before a colon elsewhere in an IP (there isn't one here), a valid clock minute
 *  requires its first digit to be 0-5 — `80` fails that outright. Every existing positive control
 *  (`[2026-08-11 09:14:02] ...`, `2026-08-11T09:14:05Z ...`, `[2026-08-11] ERR ...`,
 *  `17:04:22.123 [main] ERROR ...`) still matches: the ISO-date alternative catches the bracket-only-date
 *  shape, the clock-time alternative catches everything with a real `HH:MM` pair (including right after an
 *  ISO `T`, since this regex isn't `\b`-anchored — `\b` would fail between `T` and a digit, both word
 *  characters). Single-digit hours (`9:14`, no leading zero) are deliberately NOT matched — no positive
 *  control or real format sampled during this ticket used one, and requiring two hour digits is what
 *  keeps a bare `1:8080` from reading as an hour "1". */
const TIMESTAMP_SHAPE_REGEX = /\d{4}-\d{2}-\d{2}|(?:[01]\d|2[0-3]):[0-5]\d(?::[0-5]\d)?/;

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
  // **Round 3 fix (PR #842 review): timestamp-corroboration gate for ANY bracket in the lead-in.**
  // The F1 bracket exemption above only special-cases letters *inside* a bracket token — but when the
  // bracket is the ONLY letter content in the lead-in (`[TODO]`, `[main]`), exempting it leaves nothing
  // left for the loop below to flag, so the line sails through unchecked. And a bracket whose content has
  // no NON-bracketed letter run to trigger the loop at all (`[1]`, a digit; `[ ]`, whitespace only) was
  // never reached by the letter-run loop in the first place. Every one of these shapes is indistinguishable,
  // on the bracket's content alone, from an ordinary prose opener: a markdown checkbox (`[x]`, `[ ]`), a
  // TODO/FIXME tag, or a citation marker (`[1]`). Real Logback's own
  // documented pattern is `%d [%thread] %level` — the bracket is ALWAYS preceded by a timestamp; nothing
  // in ordinary prose (or this repo's own markdown) ever opens a sentence with a timestamp before a
  // bracket. So a bracket now only earns trust when the lead-in ALSO carries a genuine
  // {@link TIMESTAMP_SHAPE_REGEX} token; a bracket with nothing (or nothing but more brackets/punctuation)
  // before the level word is treated as an isolated word regardless of what's inside it.
  //
  // This deliberately gives up the bare-PID shape (`"[1234] ERROR ..."` with NOTHING else before it) —
  // real logs never emit a PID-bracket with no other context first (RFC3164 syslog's own PID-bracket
  // format always has a timestamp+hostname lead — see the documented gap in the F1 test suite) — in
  // exchange for closing the reopened prose false-positive class. Deliberate, documented trade-off, not
  // an oversight: see CPE-1636's Work Log.
  if (ANY_BRACKET_PAIR_REGEX.test(lead) && !TIMESTAMP_SHAPE_REGEX.test(lead)) return true;

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

/** A lead-in that is *only* a markdown ATX heading marker (`"# "`, `"## "`, up to 6 `#`s) — a documentation
 *  heading shape (`"## Error handling"`), never how a real logger prefixes a level (CPE-1655). Contains no
 *  letters, so {@link leadHasIsolatedLetterWord}'s letter-run loop never sees it and the line would
 *  otherwise sail through unflagged — found in `src/docs/*.md`'s own real prose (1 hit in 3,859 lines) by
 *  the CPE-1655 UAT. Same shape family as {@link LEAD_LIST_MARKER_REGEX}. */
const LEAD_MARKDOWN_HEADING_REGEX = /^#{1,6}\s*$/;

/** Quote characters that, sitting immediately before the match, mark it as a quoted/mentioned word
 *  (`"ERROR" is a reserved word...`) rather than a genuine level marker in position. */
const QUOTE_CHARS = new Set(['"', "'", "`", "\u2018", "\u2019", "\u201C", "\u201D"]);

/** A real level marker is followed by a separator — whitespace, `:`, `]`, `|`, or end-of-line — never
 *  glued straight onto the next character (rules out compounds like "error-like" or "errorish"). */
const TRAILING_SEPARATOR_REGEX = /^[\s:\]|]$/;

// --- CPE-1655: "headerless" error shapes — real error/crash output that carries NO level word anywhere
// (a bare crash dump, a native status-code line) is otherwise entirely invisible to the Errors filter, per
// the CPE-1655 UAT against real Python/Rust/Node crash output and a real `C:\Windows\Logs\DISM\dism.log`.
// Each shape below is a narrow, structural, line-start-anchored ("^") literal or near-literal marker that
// never appears in ordinary prose — anchoring at the true start of the line is itself part of the safety
// margin here (unlike the level-word path's lead-in machinery, nothing can precede these to make them look
// like a mid-sentence mention). Checked against a wider window than the level-word path
// ({@link HEADERLESS_SCAN_CHARS} vs {@link LEVEL_SCAN_CHARS}) because DISM's own shape runs a bit longer
// before its own structural marker (the closing `)`) appears. Verified against real captured output — see
// each regex's doc comment — and against the CPE-1636 zero-false-positive `src/docs/*.md` prose corpus. ---

/** How many characters from a line's start the headerless-error-shape regexes below ever inspect — wider
 *  than {@link LEVEL_SCAN_CHARS} because DISM's real shape (`[pid.tid] [0xHEXCODE] FunctionName:(line):`)
 *  runs to ~55 characters before its own closing `)` appears. Still a small fixed bound, so this stays
 *  O(1) per line regardless of how long the rest of the line is. */
const HEADERLESS_SCAN_CHARS = 96;

/** DISM/CBS's own native status-line shape, with no level word at all: `[pid.tid] [0xHEXCODE]
 *  FunctionName:(lineNumber)` — confirmed against a real 64,499-line `C:\Windows\Logs\DISM\dism.log` on
 *  this machine (CPE-1655): every one of the 816 lines matching this shape carried one of exactly three
 *  hex codes (`0x8007007b`, `0xc142011c`, `0x80070002`), and every one of those has its top hex nibble in
 *  `8`-`f` — the Win32 `FAILED(hr)` convention (the HRESULT severity bit set) — never a `0x0...` success
 *  code. Requiring that top nibble is both a genuine structural signal (this hex block is a status code,
 *  not an arbitrary counter) and, per that same real-file measurement, never over-fires on the file's
 *  other 63,683 lines. */
const DISM_STATUS_LINE_REGEX = /^\[\d+\.\d+\] \[0x[89a-fA-F][0-9a-fA-F]{7}\] [A-Za-z_]\w*:\(\d+\)/;

/** Python's own exact traceback-block opener, with no level word anywhere in the dump — confirmed against
 *  a real `python` `KeyError` crash captured for this ticket (CPE-1655): `Traceback (most recent call
 *  last):` is a fixed literal string CPython's own runtime emits, only ever for an actual uncaught
 *  exception. Classifying this line as the finding's header lets {@link groupContinuations}'s existing
 *  Python-frame handling sweep the `File "..."` frames (and their own CPE-1656-added source-excerpt/caret
 *  body lines) and the terminal `XError: message`/`XException: message` summary line all into the same
 *  error group. */
const PYTHON_TRACEBACK_HEADER_REGEX = /^Traceback \(most recent call last\):\s*$/;

/** Rust's own panic-header shape, with no level word anywhere: `thread '<name>' panicked at <location>:` —
 *  optionally with a `(<pid>)` between the thread name and `panicked at`, the shape a real
 *  `RUST_BACKTRACE=1` run emits (confirmed against a real captured Rust panic for this ticket, CPE-1655/
 *  1656: `thread 'main' (27836) panicked at C:\...\main.rs:3:6:`). Rust-specific and structurally
 *  unambiguous — no prose sentence starts with a quoted thread name followed by this exact phrase. */
const RUST_PANIC_HEADER_REGEX = /^thread '[^']*'\s*(?:\(\d+\)\s*)?panicked at\b/;

/** Rust's own `stack backtrace:` section label. Re-anchors the error chain right where the actual frame
 *  list begins: real `RUST_BACKTRACE=1` output has a free-text message line (and sometimes a `note:` line)
 *  between the panic header and the frame list that don't themselves look like any recognized continuation
 *  shape, breaking the forward chain from {@link RUST_PANIC_HEADER_REGEX} before it ever reaches the
 *  frames (confirmed on a real captured panic, CPE-1656) — this line restarts it so
 *  {@link CONTINUATION_RUST_FRAME_INDEX_REGEX} and the existing `at ...` continuation shape can sweep the
 *  frames that follow. */
const RUST_STACK_BACKTRACE_HEADER_REGEX = /^stack backtrace:\s*$/;

/** Go's own panic-header shape: `panic: <message>`, with no level word. Go's canonical/documented panic
 *  output format (this repo has no local Go toolchain to capture a live run — CPE-1655/1656 note this gap
 *  explicitly; the shape itself is standard and stable across Go versions). */
const GO_PANIC_HEADER_REGEX = /^panic: /;

/** Go's own goroutine-trace section header: `goroutine <N> [<status>]:`. Real Go panic output has a blank
 *  line between the `panic: ...` header and this line, which already breaks the continuation chain (an
 *  intentional, tested behavior — see {@link groupContinuations}'s "does not inherit a level across a
 *  blank-line break" coverage) — so this line is classified as its OWN header rather than relying on the
 *  chain to bridge the gap, re-anchoring right where `main.main()` and its indented source-location line
 *  can group under it via {@link CONTINUATION_GO_FRAME_REGEX} / {@link CONTINUATION_SOURCE_LOCATION_REGEX}. */
const GO_GOROUTINE_HEADER_REGEX = /^goroutine \d+ \[[^\]]*\]:\s*$/;

/**
 * Detect a line's severity level from common leveled-log shapes (bracketed/plain timestamp prefix,
 * `LEVEL:` prefix, `[LEVEL]` prefix, or Android logcat's `L/Tag:`) — plus, since CPE-1655, a handful of
 * structurally-unambiguous "headerless" error shapes that carry no level word at all (see the block
 * above). Never throws — a plain regex test/slice can't. Returns `null` for anything that doesn't match;
 * see the module doc comment for why this is deliberately conservative rather than a "match the word
 * anywhere" scan.
 */
export function detectLevel(line: string): LogLevel | null {
  const headerlessPrefix = line.length > HEADERLESS_SCAN_CHARS ? line.slice(0, HEADERLESS_SCAN_CHARS) : line;
  if (
    DISM_STATUS_LINE_REGEX.test(headerlessPrefix) ||
    PYTHON_TRACEBACK_HEADER_REGEX.test(headerlessPrefix) ||
    RUST_PANIC_HEADER_REGEX.test(headerlessPrefix) ||
    RUST_STACK_BACKTRACE_HEADER_REGEX.test(headerlessPrefix) ||
    GO_PANIC_HEADER_REGEX.test(headerlessPrefix) ||
    GO_GOROUTINE_HEADER_REGEX.test(headerlessPrefix)
  ) {
    return "error";
  }

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
      !LEAD_MARKDOWN_HEADING_REGEX.test(lead) &&
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
 *  so an unrelated capitalized sentence ("Note: see docs") is never swept in. Also used, from CPE-1655
 *  on, as a *root* header for a bare exception dump with nothing classified above it at all (a real Node
 *  crash: `TypeError: Cannot read properties of undefined (reading 'foo')` with no level word anywhere in
 *  the file) — see {@link groupContinuations}'s dedicated handling for why that's a distinct case from
 *  this continuation-only one and doesn't reopen CPE-1638's "wall of red" concern. */
const BARE_EXCEPTION_HEADER_REGEX = /^[A-Za-z][A-Za-z0-9]*(?:Error|Exception):\s/;

/** Rust backtrace frame-number line: `0: symbol`, `12: symbol` — the shape cargo's own panic-handler
 *  backtrace prints for each frame, always indented for column alignment (confirmed against a real
 *  captured `RUST_BACKTRACE=1` panic, CPE-1656: `   0: std::panicking::panic_handler`). Recognized so a
 *  frame-index line no longer breaks the continuation chain before the immediately-following indented
 *  `at ...` location line is even reached — that line already matches {@link CONTINUATION_AT_FRAME_REGEX}
 *  once its own turn comes; the fix is making sure the chain survives the line before it. Only ever
 *  checked against an indented line's trimmed remainder. */
const CONTINUATION_RUST_FRAME_INDEX_REGEX = /^\d+:\s/;

/** A Go stack-trace frame's function-call line: a package-qualified call ending in `)` — `main.main()`,
 *  `runtime.gopanic(...)` — Go's own documented panic-output shape, always unindented, immediately
 *  followed by an indented file:line location line. Checked against the line unindented, same as
 *  {@link CONTINUATION_AT_FRAME_REGEX}'s unindented case, since real Go frame lines carry no leading
 *  whitespace of their own. */
const CONTINUATION_GO_FRAME_REGEX = /^[\w./*]+\([^)]*\)\s*$/;

/** A Go or Rust bare source-location continuation: `path/to/file.ext:line` optionally followed by more
 *  text (Go's own `+0xNN` offset suffix, or nothing) — Go's `/app/main.go:10 +0x1b` frame-location line,
 *  which (unlike Java/Node/Rust) carries no `at ` prefix of its own. Only ever checked against an indented
 *  line's trimmed remainder (Go/Rust always indent this line with a tab). */
const CONTINUATION_SOURCE_LOCATION_REGEX = /^\S*\.\w+:\d+(?:[\s+:].*)?$/;

/** A Ruby backtrace frame: `from /path/to/file.rb:10:in \`method'` — Ruby's own documented backtrace
 *  format (this repo has no local Ruby toolchain to capture a live run — CPE-1656 notes this gap
 *  explicitly; the shape itself is Ruby's stable, well-known convention). Only ever checked against an
 *  indented line's trimmed remainder (Ruby always indents this line with a tab). */
const CONTINUATION_RUBY_FROM_REGEX = /^from\s/;

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
  if (CONTINUATION_GO_FRAME_REGEX.test(head)) return true; // Go's unindented "pkg.Func()" frame (CPE-1656)
  if (parentLevel === "error" && BARE_EXCEPTION_HEADER_REGEX.test(head)) return true;

  const wsMatch = CONTINUATION_LEADING_WS_REGEX.exec(head);
  if (wsMatch) {
    const trimmed = head.slice(wsMatch[0].length);
    if (
      CONTINUATION_AT_FRAME_REGEX.test(trimmed) || // an indented "at ..." frame
      CONTINUATION_ELLIPSIS_REGEX.test(trimmed) || // an indented "... N more"
      CONTINUATION_CAUSED_BY_REGEX.test(trimmed) || // an indented "Caused by: ..."
      CONTINUATION_PYTHON_FRAME_REGEX.test(trimmed) || // a Python `File "...", line N, in ...` frame
      CONTINUATION_RUST_FRAME_INDEX_REGEX.test(trimmed) || // Rust's indented "0: symbol" frame (CPE-1656)
      CONTINUATION_SOURCE_LOCATION_REGEX.test(trimmed) || // Go/Rust's indented bare "path:line" (CPE-1656)
      CONTINUATION_RUBY_FROM_REGEX.test(trimmed) // Ruby's indented "from path:line:in `method'" (CPE-1656)
    ) {
      return true;
    }
  }
  return false;
}

/** How many "body" lines (CPE-1656) a real Python 3.11+ traceback frame can carry directly under its own
 *  `File "...", line N, in ...` line: the source-excerpt line CPython prints (`    deep1()`), and
 *  sometimes a further indented caret/tilde annotation line under it (`           ^^^^^^^` or
 *  `           ~^^^^^^^^^^^^^^^`) pinpointing the exact expression that failed — confirmed against a real
 *  captured `python -c`-style `KeyError` traceback for this ticket. Both are arbitrary source text that
 *  can't be shape-matched like a real stack frame, so {@link groupContinuations} grants a narrow, bounded
 *  allowance of up to this many indented lines (content unchecked) immediately after a real
 *  {@link CONTINUATION_PYTHON_FRAME_REGEX} match, closing the moment a non-indented line is seen or the
 *  next real `File "..."` frame resets the allowance back to full. */
const PYTHON_FRAME_BODY_LINES = 2;

/**
 * Second, O(n) pass over already-detected lines: an unleveled line immediately following a classified
 * (or already-grouped) line, that {@link looksLikeContinuation} of it, inherits that line's level into
 * `filterLevel` and is flagged `isContinuation` — so an errors-only filter keeps the whole finding
 * (header + trace) together instead of just the bare header. The chain breaks — `filterLevel` resets to
 * `null` — the instant a line doesn't look like a continuation, so unrelated lines that merely follow an
 * error are never swept in. Mutates `lines` in place, including — in the two cases documented below —
 * `level` itself (every OTHER line's `level` was already finalized by {@link detectLevel} in
 * {@link parseLog}'s map and is never touched here); never rescans line text beyond the bounded
 * {@link CONTINUATION_SCAN_CHARS} window per line, so this stays flat over lines already sliced to
 * {@link MAX_LINES}.
 *
 * **CPE-1656: Python frame body lines.** A real Python traceback interleaves each `File "..."` frame with
 * 1-2 lines of arbitrary source text (see {@link PYTHON_FRAME_BODY_LINES}) that can't be shape-matched.
 * Immediately after a real Python-frame match, up to that many indented lines are swept in unconditionally
 * (content unchecked, indentation required) so the chain survives to the NEXT `File "..."` frame — and
 * ultimately to the traceback's terminal `XError: message` summary line — instead of breaking on the very
 * first source-excerpt line.
 *
 * **CPE-1655: a root bare-exception header.** {@link BARE_EXCEPTION_HEADER_REGEX} (`TypeError: ...`,
 * `KeyError: ...`) is normally trusted ONLY as a continuation of an already-classified ERROR line — never
 * given its own `level` — specifically so an exception header immediately following a real leveled error
 * (CPE-1638's original AbortError/BUGSNAG case) doesn't paint a second redundant badge (the "wall of red"
 * CPE-1638 exists to avoid). But a real Node crash dump (`TypeError: Cannot read properties of undefined
 * (reading 'foo')` with no level word ANYWHERE in the file, confirmed against a real captured `node`
 * crash) has NOTHING classified above it — there is no preceding badge to be redundant with, so this is
 * the finding's own true root, not a continuation. Only in that specific circumstance (`carryLevel` is
 * currently `null`) does this pass promote the line to its own real `level`/`filterLevel` and start a
 * fresh chain from it, so the `at ...` frames beneath it group too.
 */
function groupContinuations(lines: LogLine[]): void {
  let carryLevel: LogLevel | null = null;
  let pythonFrameBodyRemaining = 0;
  for (const line of lines) {
    if (line.level) {
      carryLevel = line.level;
      line.filterLevel = line.level;
      line.isContinuation = false;
      pythonFrameBodyRemaining = 0;
      continue;
    }

    if (carryLevel) {
      const head =
        line.text.length > CONTINUATION_SCAN_CHARS ? line.text.slice(0, CONTINUATION_SCAN_CHARS) : line.text;
      const wsMatch = CONTINUATION_LEADING_WS_REGEX.exec(head);
      const trimmed = wsMatch ? head.slice(wsMatch[0].length) : head;
      const isPythonFrame = wsMatch !== null && CONTINUATION_PYTHON_FRAME_REGEX.test(trimmed);

      if (isPythonFrame) {
        line.filterLevel = carryLevel;
        line.isContinuation = true;
        pythonFrameBodyRemaining = PYTHON_FRAME_BODY_LINES;
      } else if (pythonFrameBodyRemaining > 0 && wsMatch) {
        line.filterLevel = carryLevel;
        line.isContinuation = true;
        pythonFrameBodyRemaining -= 1;
      } else if (looksLikeContinuation(line.text, carryLevel)) {
        line.filterLevel = carryLevel;
        line.isContinuation = true;
        pythonFrameBodyRemaining = 0;
      } else {
        carryLevel = null;
        line.filterLevel = null;
        line.isContinuation = false;
        pythonFrameBodyRemaining = 0;
      }
      continue;
    }

    // carryLevel is null: nothing classified precedes this line. A root bare-exception header (CPE-1655)
    // gets a real level of its own here — see this function's doc comment for why that's safe from
    // CPE-1638's "wall of red" concern in this specific circumstance.
    const head =
      line.text.length > CONTINUATION_SCAN_CHARS ? line.text.slice(0, CONTINUATION_SCAN_CHARS) : line.text;
    if (BARE_EXCEPTION_HEADER_REGEX.test(head)) {
      line.level = "error";
      line.filterLevel = "error";
      line.isContinuation = false;
      carryLevel = "error";
      pythonFrameBodyRemaining = 0;
    } else {
      line.filterLevel = null;
      line.isContinuation = false;
      pythonFrameBodyRemaining = 0;
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

  const lines: LogLine[] = toProcess.map((raw, index) => {
    const clean = stripAnsi(raw);
    const level = detectLevel(clean);
    const { text, truncated } = capText(clean, MAX_LINE_CHARS);
    return { index, text, level, truncated, filterLevel: level, isContinuation: false };
  });

  // CPE-1638: group stack-trace continuation lines with their header so an errors-only filter keeps the
  // whole finding, not just the bare header. Overwrites the placeholder filterLevel/isContinuation set
  // above for any line that turns out to be a continuation of the preceding one — and, in the CPE-1655
  // root-bare-exception-header case, `level` itself (see groupContinuations' doc comment), which is why
  // `counts` is tallied AFTER this call runs, not inline in the map above.
  groupContinuations(lines);

  const counts = emptyCounts();
  for (const line of lines) {
    if (line.level) counts[line.level]++;
  }

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
