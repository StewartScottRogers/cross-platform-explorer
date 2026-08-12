import { describe, it, expect } from "vitest";
import {
  detectLevel,
  parseLog,
  filterLines,
  pushLogPage,
  ALL_LEVELS,
  MAX_LINES,
  MAX_LINE_CHARS,
  MAX_CACHED_LOG_PAGES,
  type LogLevel,
  type LogLine,
} from "./logViewer";

// CPE-1618 (epic CPE-1568 slice 8): unit coverage for the pure log-line level detector/parser/filter. A
// log file is untrusted, attacker-influenced input, so this exercises malformed/hostile/adversarial
// shapes (huge single line, huge line count, mixed ANSI garbage) as well as the detector's accuracy —
// see the ticket's explicit acceptance criterion that a line merely mentioning "error" in prose must
// NOT be misclassified.

describe("detectLevel — recognized shapes", () => {
  it("detects a bracketed-timestamp-prefixed level", () => {
    expect(detectLevel("[2026-08-11 09:14:02] INFO  Starting service")).toBe("info");
    expect(detectLevel("[2026-08-11 09:14:03] WARN  Config missing, using default")).toBe("warn");
    expect(detectLevel("[2026-08-11 09:14:05] ERROR Failed to connect")).toBe("error");
  });

  it("detects an ISO-timestamp-prefixed level", () => {
    expect(detectLevel("2026-08-11T09:14:05Z ERROR Payment gateway timeout")).toBe("error");
  });

  it("detects a colon-suffixed level at the line start", () => {
    expect(detectLevel("ERROR: Unhandled exception in request handler")).toBe("error");
    expect(detectLevel("WARN: disk usage above 90%")).toBe("warn");
  });

  it("detects a bracket-wrapped level", () => {
    expect(detectLevel("[WARN] disk space low")).toBe("warn");
    expect(detectLevel("[ERROR] crash in worker thread")).toBe("error");
  });

  it("is case-insensitive for the word forms", () => {
    expect(detectLevel("error: lowercase level")).toBe("error");
    expect(detectLevel("Warn: mixed case level")).toBe("warn");
  });

  it("recognizes the common abbreviations", () => {
    expect(detectLevel("[2026-08-11] ERR disk write failed")).toBe("error");
    expect(detectLevel("[2026-08-11] DBG cache miss for key foo")).toBe("debug");
  });

  it("recognizes WARNING as well as WARN", () => {
    expect(detectLevel("WARNING: certificate expires soon")).toBe("warn");
  });

  it("detects DEBUG and TRACE", () => {
    expect(detectLevel("[2026-08-11 09:14:05] DEBUG Retrying connection (attempt 1/3)")).toBe("debug");
    expect(detectLevel("[2026-08-11 09:14:05] TRACE Socket state: CLOSED -> CONNECTING")).toBe("trace");
  });

  it("detects Android logcat single-letter shapes", () => {
    expect(detectLevel("E/NetworkClient: Failed to reach api.example.com")).toBe("error");
    expect(detectLevel("W/NetworkClient: Falling back to cached response")).toBe("warn");
    expect(detectLevel("I/ActivityManager: Displaying com.example.app")).toBe("info");
    expect(detectLevel("D/Cache: hit for key 42")).toBe("debug");
    expect(detectLevel("V/Layout: measure pass 3")).toBe("trace");
  });

  it("detects a level after a pid-bracket prefix", () => {
    expect(detectLevel("[1234] ERROR worker crashed")).toBe("error");
  });
});

describe("detectLevel — must NOT misclassify", () => {
  it("does not flag a line that merely mentions 'error' in ordinary prose", () => {
    expect(detectLevel("User asked about a checkout error they saw yesterday.")).toBeNull();
  });

  it("does not flag a line mentioning 'warning' well into a sentence", () => {
    expect(detectLevel("The support agent gave the customer a warning about late fees.")).toBeNull();
  });

  it("does not flag a lowercase android-style prefix (not real logcat)", () => {
    expect(detectLevel("e/notactuallyalevel: just a path-looking string")).toBeNull();
  });

  it("does not flag ERRORLEVEL (a whole different word, no word boundary)", () => {
    expect(detectLevel("ERRORLEVEL=1")).toBeNull();
  });

  it("returns null for a line with no level marker at all", () => {
    expect(detectLevel("Request payload: userId=42 action=checkout")).toBeNull();
  });

  it("returns null for an empty line", () => {
    expect(detectLevel("")).toBeNull();
  });

  it("never throws on pathological input", () => {
    expect(() => detectLevel("x".repeat(1_000_000))).not.toThrow();
    expect(() => detectLevel(" ERROR-weird-bytes-".repeat(1000))).not.toThrow();
  });
});

// CPE-1636: the lead-in "no lowercase before the word" heuristic alone let a level word preceded by a
// quote mark, a digit-dot list marker, or all-caps prose pass through as a real level — reproduced by the
// independent Reviewer of CPE-1618 (PR #829) against four real prose lines. Each of these is a negative
// control that FAILS against the pre-fix code (the old check only looked for lowercase in the lead-in,
// and none of these four lead-ins contain a lowercase letter).
describe("detectLevel — CPE-1636 prose false positives (reviewer-reproduced)", () => {
  it("does not flag a quoted mention of a level word", () => {
    expect(detectLevel('"ERROR" is a reserved word in this DSL, see docs.')).toBeNull();
  });

  it("does not flag a quoted level-shaped phrase even though what follows the quote looks like a real separator", () => {
    expect(
      detectLevel('"ERROR: connection refused" appears in the logs when the DB is down.'),
    ).toBeNull();
  });

  it("does not flag a level word after a numbered-list marker", () => {
    expect(detectLevel("1. ERROR handling guide")).toBeNull();
  });

  it("does not flag a level word inside an ALL-CAPS prose sentence", () => {
    expect(detectLevel("SEE ERROR HANDLING DOCS FOR MORE INFO")).toBeNull();
  });
});

// CPE-1636 — a fifth false positive, found independently during this ticket's real-prose verification
// pass (not from the original reviewer's four): a lead-in of just a single capitalized word (most often
// the English indefinite article "A", starting a sentence) contains no lowercase letter AND no run of 2+
// uppercase letters, so it slipped past both of the original fix's lead-in checks. Genuinely real prose —
// "A warning icon appears next to any file that couldn't be scanned." is the kind of sentence that shows
// up in this app's own UI copy and docs. Fixed by generalizing the lead-in check to "no isolated letter
// word" (a letter run not glued onto a digit), which subsumes both of the narrower original checks.
describe("detectLevel — CPE-1636 fifth false positive: a lone capitalized lead-in word", () => {
  it("does not flag a level word after a single capitalized word starting a sentence", () => {
    expect(
      detectLevel("A warning icon appears next to any file that couldn't be scanned."),
    ).toBeNull();
  });

  it("does not flag a level word after other single-letter sentence starters", () => {
    expect(detectLevel("I saw an ERROR dialog pop up during the demo.")).toBeNull();
  });

  it("still detects a genuine ISO-timestamp lead-in, where the letters DO touch a digit", () => {
    // Regression guard for the fix's own mechanism: T and Z here are letters in the lead-in, but each is
    // glued directly onto a digit (no separating space) — that's what must keep this line detected.
    expect(detectLevel("2026-08-11T09:14:05Z ERROR Payment gateway timeout")).toBe("error");
  });
});

// CPE-1636 acceptance criterion: "Every correctly-unclassified case listed under 'what is already
// correct' still behaves the same" — these were already null before the fix (via the pre-existing
// lowercase-in-lead-in rule) and must stay null after it; not regression reproductions of the bug, just
// non-regression coverage for formats the fix must not disturb.
describe("detectLevel — CPE-1636 already-correct shapes must not regress", () => {
  it("does not flag JSON-per-line's level field", () => {
    expect(detectLevel('{"level":"error","msg":"payment failed"}')).toBeNull();
  });

  it("does not flag logfmt's level field", () => {
    expect(detectLevel("time=2026-08-11T09:14:05Z level=error msg=timeout")).toBeNull();
  });

  it("does not flag a syslog line with a lowercase hostname before the level word", () => {
    expect(detectLevel("Aug 11 09:14:05 web-server-01 app[1234]: error occurred during checkout")).toBeNull();
  });

  it("does not flag a URL path fragment mentioning error, embedded in an ordinary sentence", () => {
    expect(detectLevel("Routing GET /error-report to the reporting handler")).toBeNull();
  });

  it("does not flag ERRORLEVEL=1 (no word boundary after ERROR)", () => {
    expect(detectLevel("ERRORLEVEL=1")).toBeNull();
  });

  it("does not flag a lowercase android-style prefix", () => {
    expect(detectLevel("e/notactuallyalevel: just a path-looking string")).toBeNull();
  });

  it("does not flag mid-sentence prose mentioning a level word", () => {
    expect(detectLevel("The checkout flow logs an error when the card is declined.")).toBeNull();
  });

  it("does not flag a stack-trace continuation line (no level word of its own)", () => {
    expect(detectLevel("    at Object.<anonymous> (/app/index.js:42:11)")).toBeNull();
  });
});

// CPE-1636 acceptance criterion: genuine level lines across supported formats must still be detected —
// tightening detection is exactly how a fix like this loses real errors, so this is the other half of the
// negative control (positive cases must still pass).
describe("detectLevel — CPE-1636 genuine levels must still be detected after tightening", () => {
  it("still detects every previously-recognized shape", () => {
    expect(detectLevel("[2026-08-11 09:14:05] ERROR Failed to connect")).toBe("error");
    expect(detectLevel("2026-08-11T09:14:05Z ERROR Payment gateway timeout")).toBe("error");
    expect(detectLevel("ERROR: Unhandled exception in request handler")).toBe("error");
    expect(detectLevel("[ERROR] crash in worker thread")).toBe("error");
    expect(detectLevel("[1234] ERROR worker crashed")).toBe("error");
    expect(detectLevel("[2026-08-11] ERR disk write failed")).toBe("error");
    expect(detectLevel("WARNING: certificate expires soon")).toBe("warn");
    expect(detectLevel("E/NetworkClient: Failed to reach api.example.com")).toBe("error");
  });

  it("still detects a level immediately followed by a pipe separator", () => {
    expect(detectLevel("ERROR| worker crashed")).toBe("error");
  });
});

describe("parseLog — ANSI-coloured input", () => {
  it("strips ANSI colour codes before rendering and still detects the level underneath", () => {
    // A real SGR colour sequence (ESC [ 31 m ... ESC [ 0 m) built from the actual escape byte — the
    // shape a colourised logger (pino-pretty, winston, colorama, …) really emits, not literal "[31m"
    // text. parseLog runs stripAnsi (reused from notebook.ts) before handing text to detectLevel/render.
    const esc = String.fromCharCode(27);
    const raw = esc + "[31mERROR" + esc + "[0m Payment gateway timeout";
    const result = parseLog(raw);
    expect(result.lines).toHaveLength(1);
    // The rendered text must be clean — no literal escape-code garbage left for `{text}` to display.
    expect(result.lines[0].text).toBe("ERROR Payment gateway timeout");
    expect(result.lines[0].level).toBe("error");
  });

  it("strips ANSI codes even on a line with no recognizable level", () => {
    const esc = String.fromCharCode(27);
    const raw = esc + "[36mplain informational text" + esc + "[0m";
    const result = parseLog(raw);
    expect(result.lines[0].text).toBe("plain informational text");
    expect(result.lines[0].level).toBeNull();
  });
});

describe("parseLog — line splitting", () => {
  it("splits on \\n and reports one row per line", () => {
    const result = parseLog("INFO one\nWARN two\nERROR three");
    expect(result.lines).toHaveLength(3);
    expect(result.lines.map((l) => l.level)).toEqual(["info", "warn", "error"]);
    expect(result.totalLines).toBe(3);
    expect(result.linesCapped).toBe(false);
  });

  it("splits on \\r\\n without leaving a trailing \\r in the text", () => {
    const result = parseLog("INFO one\r\nWARN two\r\n");
    expect(result.lines.map((l) => l.text)).toEqual(["INFO one", "WARN two"]);
  });

  it("doesn't produce a spurious trailing empty line for a final newline", () => {
    const result = parseLog("INFO one\n");
    expect(result.lines).toHaveLength(1);
  });

  it("handles an empty string as zero lines, never throwing", () => {
    expect(() => parseLog("")).not.toThrow();
    const result = parseLog("");
    expect(result.lines).toHaveLength(0);
    expect(result.totalLines).toBe(0);
  });

  it("treats a single line with no trailing newline as one line", () => {
    const result = parseLog("just one line, no newline");
    expect(result.lines).toHaveLength(1);
  });
});

describe("parseLog — counts", () => {
  it("counts each level across the processed lines", () => {
    const result = parseLog(
      ["INFO a", "INFO b", "WARN c", "ERROR d", "ERROR e", "ERROR f", "plain unleveled line"].join("\n"),
    );
    expect(result.counts).toEqual({ info: 2, warn: 1, error: 3, debug: 0, trace: 0 });
  });
});

describe("parseLog — caps bound WORK, not just output", () => {
  it("caps the number of lines processed to MAX_LINES, sliced before any per-line work", () => {
    const totalLines = MAX_LINES + 500;
    const text = Array.from({ length: totalLines }, (_, i) => `INFO line ${i}`).join("\n");
    const result = parseLog(text);
    expect(result.lines).toHaveLength(MAX_LINES);
    expect(result.totalLines).toBe(totalLines);
    expect(result.linesCapped).toBe(true);
    // Counts only reflect the processed slice, not the full file.
    expect(result.counts.info).toBe(MAX_LINES);
  });

  it("does not mark linesCapped when the file is at or under the cap", () => {
    const text = Array.from({ length: MAX_LINES }, (_, i) => `INFO line ${i}`).join("\n");
    const result = parseLog(text);
    expect(result.linesCapped).toBe(false);
    expect(result.lines).toHaveLength(MAX_LINES);
  });

  it("caps a single pathologically long line's rendered text to MAX_LINE_CHARS and flags it truncated", () => {
    const hugeLine = "ERROR " + "x".repeat(50_000);
    const result = parseLog(hugeLine);
    expect(result.lines).toHaveLength(1);
    expect(result.lines[0].text.length).toBe(MAX_LINE_CHARS);
    expect(result.lines[0].truncated).toBe(true);
    expect(result.lines[0].level).toBe("error"); // detection still works — it only scans the head.
  });

  it("does not flag a short line as truncated", () => {
    const result = parseLog("INFO short line");
    expect(result.lines[0].truncated).toBe(false);
  });

  it("stays responsive (completes quickly) on a large adversarial file: many lines, huge lines, ANSI noise", () => {
    const esc = String.fromCharCode(27);
    const lines: string[] = [];
    for (let i = 0; i < MAX_LINES + 1000; i++) {
      if (i % 500 === 0) lines.push("ERROR " + (esc + "[31m").repeat(200) + "x".repeat(20_000));
      else lines.push(`[2026-08-11 00:00:00] INFO line ${i}`);
    }
    const text = lines.join("\n");
    const start = Date.now();
    const result = parseLog(text);
    const elapsedMs = Date.now() - start;
    expect(result.lines).toHaveLength(MAX_LINES);
    // Generous bound (CI machines vary) — the point is "doesn't hang", not micro-benchmarking.
    expect(elapsedMs).toBeLessThan(2000);
  });
});

// CPE-1638: filtering to Errors used to hide a finding's stack trace, because the trace's continuation
// lines carry no level word of their own (`filterLines` keyed on `level`, which is null for every
// continuation line). Reproduced on a real log (`electron-2026-07-24.log`) by the independent UAT of
// CPE-1618: filtering to Errors left 1 of 22 lines visible — the bare header, with no trace at all.
describe("parseLog — CPE-1638 stack-trace continuations inherit their header's level", () => {
  // A trimmed-down reproduction of the real excerpt from the ticket: an error header, an unindented bare
  // exception-type line (no level word), several indented "at ..." frames, a trailing "... N more" elision
  // line, and then one clearly UNRELATED line that must NOT be swept into the group.
  const REAL_EXCERPT_LINES = [
    "2026-07-24T13:03:50.615Z error [BUGSNAG] Uncaught exception in main process", // 0: header, level=error
    "AbortError: Request aborted", // 1: bare exception line, no level word
    "    at RequestAborter.abort (electron/lib/net.js:120:11)", // 2: indented frame
    "    at ClientRequest.emit (node:events:513:28)", // 3: indented frame
    "    at TLSSocket.socketErrorListener (node:_http_client:495:9)", // 4: indented frame
    "    ... 9 more lines omitted", // 5: elision line
    "Server accepted a new connection from 10.0.0.5", // 6: UNRELATED — must not be swept in
  ];
  const REAL_EXCERPT = REAL_EXCERPT_LINES.join("\n");

  it("gives the header its own level, and none of the continuation lines a level of their own (no false 'wall of red')", () => {
    const result = parseLog(REAL_EXCERPT);
    expect(result.lines[0].level).toBe("error");
    for (const i of [1, 2, 3, 4, 5]) expect(result.lines[i].level).toBeNull();
  });

  it("inherits the header's level into filterLevel for every real continuation line, flagged isContinuation", () => {
    const result = parseLog(REAL_EXCERPT);
    for (const i of [1, 2, 3, 4, 5]) {
      expect(result.lines[i].filterLevel, `line ${i}`).toBe("error");
      expect(result.lines[i].isContinuation, `line ${i}`).toBe(true);
    }
  });

  it("does NOT sweep the unrelated trailing line into the group — the boundary the ticket explicitly asks to test", () => {
    const result = parseLog(REAL_EXCERPT);
    expect(result.lines[6].filterLevel).toBeNull();
    expect(result.lines[6].isContinuation).toBe(false);
  });

  it("filtering to Errors-only keeps the header AND its whole trace, not just the bare header — fails against the pre-fix filterLines (which keyed on `level`, null for every continuation line)", () => {
    const result = parseLog(REAL_EXCERPT);
    const shown = filterLines(result.lines, { levels: new Set<LogLevel>(["error"]), showUnleveled: false });
    expect(shown.map((l) => l.index)).toEqual([0, 1, 2, 3, 4, 5]);
    expect(shown.map((l) => l.text)).toEqual(REAL_EXCERPT_LINES.slice(0, 6));
    // The unrelated line must still be excluded — grouping isn't a license to keep everything after an error.
    expect(shown.some((l) => l.text.includes("Server accepted"))).toBe(false);
  });

  it("filter counts ('Showing N of M') stay accurate with grouping applied", () => {
    const result = parseLog(REAL_EXCERPT);
    const shown = filterLines(result.lines, { levels: new Set<LogLevel>(["error"]), showUnleveled: false });
    expect(shown.length).toBe(6);
    expect(result.lines.length).toBe(7);
  });

  it("does not inherit a level across a blank-line break in the chain", () => {
    const text = ["ERROR something broke", "", "Unrelated line after a blank"].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBeNull(); // blank line: no leading whitespace char to match
    expect(result.lines[2].filterLevel).toBeNull(); // chain already broken by the blank line
  });

  it("does not treat a plain capitalized sentence after an error as a continuation (only Error/Exception-suffixed bare headers qualify)", () => {
    const text = ["ERROR something broke", "Note: see the runbook for next steps"].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBeNull();
    expect(result.lines[1].isContinuation).toBe(false);
  });

  it("only inherits the bare-exception-header shape right after an ERROR, not after warn/info/etc.", () => {
    const text = ["WARN disk space low", "SomeException: not really related"].join("\n");
    const result = parseLog(text);
    // The bare-exception-header heuristic is gated to error parents only (per the ticket's steer to be
    // conservative); it still isn't swept in as a continuation of a WARN.
    expect(result.lines[1].filterLevel).toBeNull();
  });

  it("chains a 'Caused by:' line and its own indented frames onto the original error", () => {
    const text = [
      "ERROR outer failure",
      "Caused by: java.lang.NullPointerException",
      "    at com.example.Service.call(Service.java:10)",
    ].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBe("error");
    expect(result.lines[2].filterLevel).toBe("error");
  });
});

describe("filterLines", () => {
  const lines: LogLine[] = [
    { index: 0, text: "info line", level: "info", truncated: false, filterLevel: "info", isContinuation: false },
    { index: 1, text: "warn line", level: "warn", truncated: false, filterLevel: "warn", isContinuation: false },
    { index: 2, text: "error line", level: "error", truncated: false, filterLevel: "error", isContinuation: false },
    { index: 3, text: "plain line", level: null, truncated: false, filterLevel: null, isContinuation: false },
  ];

  it("shows everything when all levels + unleveled are active", () => {
    const shown = filterLines(lines, { levels: new Set(ALL_LEVELS), showUnleveled: true });
    expect(shown).toHaveLength(4);
  });

  it("hides non-matching levels", () => {
    const shown = filterLines(lines, { levels: new Set<LogLevel>(["error"]), showUnleveled: false });
    expect(shown.map((l) => l.text)).toEqual(["error line"]);
  });

  it("supports a 'warn and above' style filter", () => {
    const shown = filterLines(lines, { levels: new Set<LogLevel>(["warn", "error"]), showUnleveled: false });
    expect(shown.map((l) => l.text)).toEqual(["warn line", "error line"]);
  });

  it("hides unleveled lines when showUnleveled is false", () => {
    const shown = filterLines(lines, { levels: new Set(ALL_LEVELS), showUnleveled: false });
    expect(shown.find((l) => l.level === null)).toBeUndefined();
  });

  it("shows only unleveled lines when no levels are active", () => {
    const shown = filterLines(lines, { levels: new Set<LogLevel>(), showUnleveled: true });
    expect(shown.map((l) => l.text)).toEqual(["plain line"]);
  });

  it("returns an empty array when nothing is active", () => {
    const shown = filterLines(lines, { levels: new Set<LogLevel>(), showUnleveled: false });
    expect(shown).toHaveLength(0);
  });
});

// CPE-1644 B′: `LogPreview`'s `pages` cache grew without bound as a user paged backward through a huge
// file ("... a slower, opt-in version of the exact problem CPE-1637's windowed reads exist to fix").
// `pushLogPage` didn't exist before this fix — the unbounded growth lived inline in the component as
// `pages = [...pages, w]` with no cap — so this whole suite is a negative control: it fails to even
// import/compile against the pre-fix module.
describe("pushLogPage — bounded page cache (CPE-1644 B′)", () => {
  it("caps the cache at MAX_CACHED_LOG_PAGES, evicting the oldest/shallowest pages first", () => {
    let pages: number[] = [];
    for (let i = 0; i < MAX_CACHED_LOG_PAGES + 5; i++) pages = pushLogPage(pages, i);
    expect(pages).toHaveLength(MAX_CACHED_LOG_PAGES);
    // The 5 oldest pushes (0..4) were evicted; the most-recently-fetched pages survive, in order.
    expect(pages[0]).toBe(5);
    expect(pages[pages.length - 1]).toBe(MAX_CACHED_LOG_PAGES + 4);
  });

  it("never exceeds the cap no matter how many pages are pushed (exhaustively paging a huge file)", () => {
    let pages: number[] = [];
    for (let i = 0; i < 500; i++) pages = pushLogPage(pages, i);
    expect(pages.length).toBeLessThanOrEqual(MAX_CACHED_LOG_PAGES);
    expect(pages[pages.length - 1]).toBe(499); // always keeps the just-fetched page
  });

  it("does not evict anything while under the cap", () => {
    let pages: string[] = [];
    for (const p of ["a", "b", "c"]) pages = pushLogPage(pages, p);
    expect(pages).toEqual(["a", "b", "c"]);
  });

  it("does not mutate the array passed in (pure)", () => {
    const original = [1, 2, 3];
    const next = pushLogPage(original, 4);
    expect(original).toEqual([1, 2, 3]);
    expect(next).toEqual([1, 2, 3, 4]);
  });
});
