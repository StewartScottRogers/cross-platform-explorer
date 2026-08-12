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

  it(
    "no longer detects a BARE pid-bracket prefix with nothing else before it (round 3, PR #842): " +
      "structurally identical to a citation marker (\"[1] WARNING...\") once you strip the digits out, " +
      "and no real log format emits a PID bracket with nothing else ahead of it (RFC3164 syslog's own " +
      "PID-bracket shape always has a timestamp+hostname lead first — see the documented gap below). See " +
      "leadHasIsolatedLetterWord's timestamp-corroboration gate and CPE-1636's Work Log.",
    () => {
      expect(detectLevel("[1234] ERROR worker crashed")).toBeNull();
    },
  );

  it("still detects a pid-bracket prefix once a real timestamp corroborates it", () => {
    expect(detectLevel("2026-08-11T09:14:05Z [1234] ERROR worker crashed")).toBe("error");
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
    // NOT "[1234] ERROR worker crashed" — round 3 (PR #842) intentionally stopped trusting a bare
    // pid-bracket with nothing before it; see the dedicated describe block above for the rationale.
    expect(detectLevel("[2026-08-11] ERR disk write failed")).toBe("error");
    expect(detectLevel("WARNING: certificate expires soon")).toBe("warn");
    expect(detectLevel("E/NetworkClient: Failed to reach api.example.com")).toBe("error");
  });

  it("still detects a level immediately followed by a pipe separator", () => {
    expect(detectLevel("ERROR| worker crashed")).toBe("error");
  });
});

// F1 (PR #842 review): two mainstream real-world log formats the original detector never recognized —
// confirmed by the reviewer as a PRE-EXISTING gap (not a regression of CPE-1636/1638's own changes).
describe("detectLevel — F1: previously-undetected mainstream formats", () => {
  it("detects the Logback manual's own documented '%d [%thread] %level' pattern (bracketed thread name)", () => {
    // Fixed: a bracket-wrapped token with no internal whitespace ("[main]") is a logger/thread-name tag,
    // not an isolated prose word — see BRACKET_TOKEN_REGEX / leadHasIsolatedLetterWord.
    expect(detectLevel("17:04:22.123 [main] ERROR c.e.MyService - Failed to connect")).toBe("error");
  });

  it("detects the Logback pattern with a more realistic multi-word-shaped thread-pool tag", () => {
    expect(
      detectLevel("17:04:22.123 [http-nio-8080-exec-1] WARN c.e.MyService - Slow response"),
    ).toBe("warn");
  });

  it("still does not flag a bracketed prose remark that contains whitespace (not a bare tag token)", () => {
    // The exemption requires the WHOLE bracket span to contain no whitespace — a parenthetical aside like
    // "[see the docs]" never qualifies, so this must stay unclassified same as before the fix.
    expect(detectLevel("[see the docs] WARNING word only, not a real level marker here")).not.toBe(
      "warn",
    );
  });

  it(
    "documents a gap deliberately left open: RFC3164 syslog/journald with a month-name prefix AND a bare " +
      "hostname token (not bracket-wrapped, doesn't touch a digit) is indistinguishable from an isolated " +
      "prose word without materially raising the risk of flagging real prose — per the ticket's explicit " +
      "'leave the gap rather than guess' guidance. Left unclassified; not a regression, since this shape " +
      "was never detected before either.",
    () => {
      expect(
        detectLevel("Aug 11 17:04:22 myhost myapp[1234]: ERROR Failed to connect to database"),
      ).toBeNull();
    },
  );
});

// Round 3 (PR #842 review, attempt 3/3): F1's bracket exemption above reopened CPE-1636's own prose
// false-positive bug. When the bracket token is the ONLY letter content in a lead-in — or contains no
// letters at all — exempting it (or simply never reaching the letter-run loop) left the line looking like
// a clean logger prefix, even though every one of these openers is ordinary prose/markdown: a `[TODO]` or
// `[FIXME]` tag, a markdown checkbox (`[x]`/`[ ]`), or a citation marker (`[1]`, `[2]`). Fixed by requiring
// a genuine timestamp-shaped token elsewhere in the lead-in before ANY bracket is trusted — see
// leadHasIsolatedLetterWord's gate. These four are the exact reviewer-reproduced cases plus the
// markdown-checkbox/citation-marker variants the reviewer asked for explicitly.
describe("detectLevel — CPE-1636 round 3: bracket exemption reopened the prose false-positive class", () => {
  it("does not flag a bracketed logger-tag-shaped word opening a sentence", () => {
    expect(detectLevel("[main] ERROR handling is disabled in this build.")).toBeNull();
  });

  it("does not flag a TODO tag opening a sentence", () => {
    expect(detectLevel("[TODO] ERROR handling needs review before ship.")).toBeNull();
  });

  it("does not flag a bracketed citation-marker-shaped number opening a sentence", () => {
    expect(detectLevel("[1] WARNING signs were ignored by the team.")).toBeNull();
  });

  it("does not flag a checked markdown checkbox opening a sentence", () => {
    expect(detectLevel("[x] ERROR checking disabled for this test.")).toBeNull();
  });

  it("does not flag an unchecked markdown checkbox opening a sentence", () => {
    expect(detectLevel("[ ] ERROR checking disabled for this test (unchecked box).")).toBeNull();
  });

  it("does not flag a second citation-marker-shaped number opening a sentence", () => {
    expect(detectLevel("[2] ERROR handling notes go here (citation).")).toBeNull();
  });

  it("does not flag a FIXME tag opening a sentence", () => {
    expect(detectLevel("[FIXME] WARNING message needs a real implementation.")).toBeNull();
  });

  it("still detects [LEVEL] at line start — the level word INSIDE the bracket is a different code path " +
    "(no complete bracket pair ever appears in the lead-in, since the level word itself is what's between " +
    "the brackets) and must not be collateral damage from the round-3 gate", () => {
    expect(detectLevel("[ERROR] msg")).toBe("error");
    expect(detectLevel("[WARN] disk space low")).toBe("warn");
  });

  it("still detects the real Logback pattern once a genuine timestamp corroborates the bracket", () => {
    expect(detectLevel("17:04:22.123 [main] ERROR c.e.MyService - Failed to connect")).toBe("error");
  });
});

// CPE-1655/1656/1657: one coherent design pass over three tickets that pull in opposite directions —
// CPE-1655 WIDENS detection (real errors with no level word were invisible to the Errors filter);
// CPE-1657 TIGHTENS it (a loose digit-shape check let bracket-tagged prose slip through as a level);
// CPE-1656 does both (a UTF-16 false positive in the Rust crate, plus Go/Ruby/Rust trace grouping gaps).
// Every fixture below marked "real" is a literal transcript of genuine captured output (python -c, a live
// node crash, a real RUST_BACKTRACE=1 panic compiled+run for this ticket, and real lines pulled from
// C:\Windows\Logs\DISM\dism.log on the machine this was verified on) — not an invented approximation. Go
// and Ruby fixtures are marked synthetic where noted: this environment has no local Go or Ruby toolchain,
// so those use the ticket's own literal documented shape rather than a captured transcript.

describe("detectLevel — CPE-1657: the timestamp gate rejects a coincidental digit run, not just any timestamp", () => {
  it("rejects a partial-date digit run coincidentally sitting in front of a bracket (adversarial input #4 from the ticket)", () => {
    // Pre-fix: TIMESTAMP_SHAPE_REGEX (`/\d{1,4}[:-]\d{2}/`) matched "2026-08" even though it has no day
    // component — not a real date — which wrongly exempted the "[draft]" bracket and let this classify as
    // "error". This is the WIDEN-vs-TIGHTEN pairing's tighten half: a regression guard against ever
    // loosening the timestamp shape back to "any digit-separator-digit run".
    expect(detectLevel("2026-08 [draft] ERROR budget")).toBeNull();
  });

  it("rejects an IP:port octet's coincidental colon-digit run (adversarial input #5 from the ticket)", () => {
    // Pre-fix: "1:80" (the tail of "10.0.0.1:8080") satisfied the old regex — one digit, a colon, two
    // digits — even though it's not a timestamp. The tightened regex requires two digits before the colon
    // AND a valid minute (00-59) after it; "1:80"'s minute "80" fails immediately.
    expect(detectLevel("10.0.0.1:8080 [proxy] ERROR rate")).toBeNull();
  });

  it("still rejects the three inputs that already held pre-fix, via the same fallback prose-word mechanism", () => {
    // The ticket's own finding: these three held only because of an incidental prose word outside the
    // bracket ("the", "Version", "See section"), not because the gate recognized the digits as bogus. They
    // must stay null after the fix too — this is the non-regression half of the tighten.
    expect(detectLevel("At 14:30 the [main] ERROR handling was disabled.")).toBeNull();
    expect(detectLevel("Version 1.2-30 [beta] ERROR counts rose.")).toBeNull();
    expect(detectLevel("See section 3-14 [note] WARNING signs.")).toBeNull();
  });

  it("still detects every positive control the ticket lists, including a level word directly inside the bracket", () => {
    expect(detectLevel("17:04:22.123 [main] ERROR c.e.MyService - Failed to connect")).toBe("error");
    expect(detectLevel("2026-08-11T09:14:05Z [1234] ERROR worker crashed")).toBe("error");
    expect(detectLevel("[ERROR] msg")).toBe("error");
  });

  it("still stays null for the four original CPE-1636 round-3 prose cases", () => {
    expect(detectLevel("[main] ERROR handling is disabled in this build.")).toBeNull();
    expect(detectLevel("[TODO] ERROR handling needs review before ship.")).toBeNull();
    expect(detectLevel("[1] WARNING signs were ignored by the team.")).toBeNull();
    expect(detectLevel("[x] ERROR checking disabled for this test.")).toBeNull();
  });

  it("still detects the bracket-only-date shape ([2026-08-11] ERR ...) via the full-ISO-date alternative", () => {
    expect(detectLevel("[2026-08-11] ERR disk write failed")).toBe("error");
  });
});

describe("detectLevel — CPE-1655: the markdown ATX-heading false positive", () => {
  it("does not flag a markdown heading as a level (found in this repo's own src/docs/*.md prose)", () => {
    // Pre-fix: "## " contains no letters, so leadHasIsolatedLetterWord's letter-run loop never even sees
    // it, and it sailed through as if it were a real logger prefix. 1 hit in 3,859 real doc lines.
    expect(detectLevel("## Error handling")).toBeNull();
  });

  it("still flags nothing for the deeper heading levels either", () => {
    expect(detectLevel("# ERROR")).toBeNull();
    expect(detectLevel("### WARNING")).toBeNull();
    expect(detectLevel("###### INFO")).toBeNull();
  });
});

describe("detectLevel — CPE-1655: DISM's native status-line shape, real captured lines", () => {
  // Real lines pulled from C:\Windows\Logs\DISM\dism.log (64,499 lines) on the verifying machine. All 816
  // lines matching this shape in that real file used one of exactly these three hex codes, every one with
  // the HRESULT failure bit set (top nibble 8-f) — never a 0x0... success code.
  it("classifies the real dism.log error line as error", () => {
    expect(
      detectLevel(
        "[31692.9108] [0x8007007b] FIOReadFileIntoBuffer:(1454): The filename, directory name, or volume label syntax is incorrect.",
      ),
    ).toBe("error");
  });

  it("classifies a real dism.log line with no trailing message text (just the status shape)", () => {
    expect(detectLevel("[31692.9108] [0xc142011c] UnmarshallImageHandleFromDirectory:(641)")).toBe("error");
    expect(detectLevel("[14116.31140] [0x80070002] SomeOtherFunc:(200)")).toBe("error");
  });

  it("does not flag an ordinary DISM info line sharing the [pid.tid] prefix but no hex-status shape", () => {
    expect(
      detectLevel(
        "[31692.9108] Info                  DISM   API: PID=31692 TID=9108 Enter CCommandThread::ExecuteLoop",
      ),
    ).toBeNull();
  });

  it("does not flag a hex-looking bracket whose top nibble is NOT in the HRESULT-failure range", () => {
    // 0x00000000-0x7FFFFFFF never appeared in the real file's matches (Win32 success/non-failure range);
    // the shape stays deliberately narrow to that observed real-world signal.
    expect(detectLevel("[100.200] [0x00000001] SomeFunc:(10): informational")).toBeNull();
  });
});

describe("parseLog — CPE-1655: whole-file crash dumps with no level word anywhere are reachable via the Errors filter", () => {
  it("a real python -c KeyError traceback (captured for this ticket) is fully reachable, all 13 lines", () => {
    // Real output: `python C:\...\python_crash_test.py` raising KeyError('missing_key'), captured verbatim
    // (path shortened here for readability; the shape — not the exact path — is what's under test).
    const PYTHON_TRACEBACK = [
      "Traceback (most recent call last):",
      '  File "script.py", line 12, in <module>',
      "    deep1()",
      '  File "script.py", line 9, in deep1',
      "    return deep2()",
      "           ^^^^^^^",
      '  File "script.py", line 6, in deep2',
      "    return deep3()",
      "           ^^^^^^^",
      '  File "script.py", line 3, in deep3',
      '    return d["missing_key"]',
      "           ~^^^^^^^^^^^^^^~",
      "KeyError: 'missing_key'",
    ].join("\n");
    const result = parseLog(PYTHON_TRACEBACK);
    expect(result.lines[0].level).toBe("error");
    const shown = filterLines(result.lines, { levels: new Set<LogLevel>(["error"]), showUnleveled: false });
    expect(shown.length).toBe(result.lines.length); // the WHOLE file is reachable, not just the header
    // The terminal exception-summary line is grouped (inherited), not double-badged with its own level —
    // the CPE-1638 "wall of red" guard still holds for a continuation of an already-classified header.
    expect(result.lines[result.lines.length - 1].level).toBeNull();
    expect(result.lines[result.lines.length - 1].filterLevel).toBe("error");
  });

  it("a real node TypeError crash (captured for this ticket) with NO level word anywhere is reachable", () => {
    const NODE_CRASH = [
      "C:\\scratch\\node_crash_test.js:3",
      "  return obj.foo;",
      "             ^",
      "",
      "TypeError: Cannot read properties of undefined (reading 'foo')",
      "    at deep3 (C:\\scratch\\node_crash_test.js:3:14)",
      "    at deep2 (C:\\scratch\\node_crash_test.js:6:10)",
      "    at deep1 (C:\\scratch\\node_crash_test.js:9:10)",
      "    at Object.<anonymous> (C:\\scratch\\node_crash_test.js:12:1)",
      "    at Module._compile (node:internal/modules/cjs/loader:1781:14)",
      "",
      "Node.js v22.22.3",
    ].join("\n");
    const result = parseLog(NODE_CRASH);
    // The header (a ROOT bare-exception header, nothing classified above it) gets its own real level.
    expect(result.lines[4].level).toBe("error");
    expect(result.lines[4].text).toContain("TypeError");
    for (let i = 5; i <= 9; i++) {
      expect(result.lines[i].filterLevel, `line ${i}`).toBe("error");
      expect(result.lines[i].isContinuation, `line ${i}`).toBe(true);
    }
    const shown = filterLines(result.lines, { levels: new Set<LogLevel>(["error"]), showUnleveled: false });
    expect(shown.map((l) => l.text)).toEqual(NODE_CRASH.split("\n").slice(4, 10));
    // The pre-crash source-excerpt lines and the trailing "Node.js v22.22.3" line are correctly NOT swept
    // in — they precede/follow the finding, they aren't part of it.
    expect(result.lines[0].filterLevel).toBeNull();
    expect(result.lines[11].filterLevel).toBeNull();
  });

  it("a real RUST_BACKTRACE=1 panic (compiled and run for this ticket) is reachable, header through every frame", () => {
    // Real output from a small Rust program panicking on an out-of-bounds Vec index, compiled with `rustc
    // -g` and run with RUST_BACKTRACE=1 (paths shortened for readability; shape is what's under test).
    const RUST_PANIC = [
      "thread 'main' (27836) panicked at src\\main.rs:3:6:",
      "index out of bounds: the len is 3 but the index is 10",
      "stack backtrace:",
      "   0: std::panicking::panic_handler",
      "             at /rustc/abc123/library\\std\\src\\panicking.rs:689",
      "   1: core::panicking::panic_fmt",
      "             at /rustc/abc123/library\\core\\src\\panicking.rs:80",
      "   2: core::panicking::panic_bounds_check",
      "             at /rustc/abc123/library\\core\\src\\panicking.rs:271",
      "   5: main::deep3",
      "             at src\\main.rs:3",
      "   6: main::deep2",
      "             at src\\main.rs:5",
      "note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.",
    ].join("\n");
    const result = parseLog(RUST_PANIC);
    expect(result.lines[0].level).toBe("error"); // the panic header itself
    expect(result.lines[2].level).toBe("error"); // "stack backtrace:" re-anchors after the free-text message
    for (let i = 3; i <= 12; i++) {
      expect(result.lines[i].filterLevel, `line ${i}`).toBe("error");
      expect(result.lines[i].isContinuation, `line ${i}`).toBe(true);
    }
    // The one free-text message line between the panic header and "stack backtrace:" doesn't itself match
    // any known shape — an accepted, documented, narrow gap (still under-grouping, never over-grouping).
    expect(result.lines[1].filterLevel).toBeNull();
    // The trailing "note:" line after the backtrace is correctly not swept in either.
    expect(result.lines[13].filterLevel).toBeNull();
  });

  it("a Go panic (ticket's own documented shape — no local Go toolchain available to capture live output) is reachable", () => {
    const GO_PANIC = [
      "panic: runtime error: index out of range [3] with length 3",
      "",
      "goroutine 1 [running]:",
      "main.main()",
      "\t/app/main.go:10 +0x1b",
    ].join("\n");
    const result = parseLog(GO_PANIC);
    expect(result.lines[0].level).toBe("error"); // "panic: ..." header
    // The blank line breaks the chain (existing, tested behavior) — "goroutine ...:" re-anchors as its own
    // header rather than relying on the chain to bridge the gap.
    expect(result.lines[1].filterLevel).toBeNull();
    expect(result.lines[2].level).toBe("error"); // "goroutine 1 [running]:"
    expect(result.lines[3].filterLevel).toBe("error"); // "main.main()"
    expect(result.lines[3].isContinuation).toBe(true);
    expect(result.lines[4].filterLevel).toBe("error"); // "\t/app/main.go:10 +0x1b"
    expect(result.lines[4].isContinuation).toBe(true);
  });
});

describe("parseLog — CPE-1656 B: Ruby backtrace frames group under an already-classified header", () => {
  // The ticket's own literal frame shape (`\tfrom /path:N:in \`method'`). No local Ruby toolchain is
  // available in this environment to capture a live crash, so this uses an explicit ERROR-leveled header
  // (unlike the Python/Node/Rust cases above, which are fully real end-to-end) — the header classification
  // itself isn't in question here, only whether the `from ...` frame shape groups once a header exists.
  it("groups two Ruby-style 'from ...' frames under a preceding ERROR line", () => {
    const text = [
      "ERROR uncaught exception in worker",
      "\tfrom /app/lib/worker.rb:6:in `deep2'",
      "\tfrom /app/lib/worker.rb:9:in `deep1'",
    ].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBe("error");
    expect(result.lines[1].isContinuation).toBe(true);
    expect(result.lines[2].filterLevel).toBe("error");
    expect(result.lines[2].isContinuation).toBe(true);
  });
});

describe("parseLog — CPE-1656 B: over-grouping guard still holds under the widened shapes", () => {
  it("does not sweep an indented line that merely LOOKS path-like into an unrelated preceding error", () => {
    // The widened CONTINUATION_SOURCE_LOCATION_REGEX/CONTINUATION_RUST_FRAME_INDEX_REGEX/GO_FRAME shapes
    // must not resurrect the CPE-1638 F2 "bare indentation is enough" bug — an indented line only counts
    // once it clears one of the real corroborating shapes.
    const text = [
      "ERROR Connection failed on worker-thread-1",
      "  Now processing next item in queue for worker-thread-2",
    ].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBeNull();
    expect(result.lines[1].isContinuation).toBe(false);
  });

  it("does not sweep a real source path followed by ordinary prose (PR #846 review, near-miss shape)", () => {
    // The reviewer's demonstrated over-grouping: the first CONTINUATION_SOURCE_LOCATION_REGEX allowed
    // ARBITRARY trailing text after `file.ext:NN`, so an indented sentence that merely opens with a real
    // source location was swept into an unrelated preceding error. Only Go's own `+0xHEX` offset (or
    // nothing at all) may follow now. This is the near-miss the widened shape left unpinned — bare
    // indentation without any shape match was already covered, this one wasn't.
    const text = [
      "ERROR Connection failed on worker-thread-1",
      "\tsrc/main.rs:42 was recently modified by CPE-1656",
    ].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBeNull();
    expect(result.lines[1].isContinuation).toBe(false);
  });

  it("does not sweep a bare function call that lacks Go's package qualifier (PR #846 review, near-miss shape)", () => {
    // Same class, other regex: CONTINUATION_GO_FRAME_REGEX matched ANY call-shaped line. A real Go frame
    // is always package-qualified (`main.main()`), so an unqualified call is not a frame.
    const text = ["ERROR Connection failed on worker-thread-1", "processRequest(ctx)"].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBeNull();
    expect(result.lines[1].isContinuation).toBe(false);
  });

  it("still groups the GENUINE Go frame shapes the tightening had to preserve", () => {
    // The other half of the tightening: both real shapes must survive it, or the fix traded one bug for
    // another. Package-qualified call + tab-indented `file.go:NN +0xHEX` location.
    const text = [
      "panic: runtime error: index out of range [3] with length 3",
      "",
      "goroutine 1 [running]:",
      "main.main()",
      "\t/app/main.go:10 +0x1b",
    ].join("\n");
    const result = parseLog(text);
    expect(result.lines[3].filterLevel, "main.main()").toBe("error");
    expect(result.lines[4].filterLevel, "/app/main.go:10 +0x1b").toBe("error");
  });

  it("still does not sweep several interleaved indented lines from unrelated threads (re-run of the CPE-1638 guard)", () => {
    const text = [
      "2026-08-11T09:00:00Z ERROR worker-1 failed to acquire lock",
      "  worker-2: heartbeat ok, queue depth 4",
      "  worker-3: heartbeat ok, queue depth 1",
      "\tworker-4: starting batch 17",
    ].join("\n");
    const result = parseLog(text);
    for (const i of [1, 2, 3]) {
      expect(result.lines[i].filterLevel, `line ${i}`).toBeNull();
      expect(result.lines[i].isContinuation, `line ${i}`).toBe(false);
    }
  });
});

describe("parseLog — CPE-1655: real MSI verbose log's internal 'Error' table reference is not misclassified", () => {
  // A real line from a genuine MSI verbose install log on the verifying machine: MSI's own internal
  // SQL-style diagnostic chatter referencing the `Error` table by name — not an actual error condition.
  // Already correctly rejected pre-fix (via the pre-existing "isolated word" lead-in rule — "MSI" itself is
  // an isolated word); recorded here as a real-corpus non-regression guard for this ticket's pass.
  it("does not flag MSI's own 'Note: ... 3: Error' internal diagnostic line", () => {
    expect(detectLevel("MSI (c) (20:D4) [08:10:05:861]: Note: 1: 2205 2:  3: Error ")).toBeNull();
    expect(
      detectLevel("MSI (c) (20:00) [08:10:05:909]: Note: 1: 2228 2:  3: Error 4: SELECT `Message` FROM `Error` WHERE `Error` = 2898 "),
    ).toBeNull();
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

  // --- F2 (PR #842 review): bare leading whitespace alone is not a corroborating signal. An indented
  // line only counts as a continuation once its own leading whitespace is stripped away AND what's left
  // looks like an actual stack-frame/continuation shape — never indentation by itself, which is far too
  // common in ordinary interleaved multi-thread/multi-process log output to mean anything on its own. ---

  it("does NOT sweep an indented but otherwise unrelated line into a preceding error's group (reviewer's live reproduction)", () => {
    // Exact reproduction from the reviewer: two lines of interleaved multi-thread output where the second
    // line is merely indented (e.g. a nested/sub-status log convention) — not a stack frame, not a
    // continuation of the first line's message, just incidentally indented.
    const text = [
      "ERROR Connection failed on worker-thread-1",
      "  Now processing next item in queue for worker-thread-2",
    ].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBeNull();
    expect(result.lines[1].isContinuation).toBe(false);
  });

  it("does not sweep in several interleaved indented lines from unrelated threads, even mid-chain", () => {
    const text = [
      "2026-08-11T09:00:00Z ERROR worker-1 failed to acquire lock",
      "  worker-2: heartbeat ok, queue depth 4",
      "  worker-3: heartbeat ok, queue depth 1",
      "\tworker-4: starting batch 17",
    ].join("\n");
    const result = parseLog(text);
    for (const i of [1, 2, 3]) {
      expect(result.lines[i].filterLevel, `line ${i}`).toBeNull();
      expect(result.lines[i].isContinuation, `line ${i}`).toBe(false);
    }
  });

  it("still groups a genuine indented stack frame — the corroborating 'at ...' shape survives stripping the indentation", () => {
    const text = ["ERROR outer failure", "    at com.example.Service.call(Service.java:10)"].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBe("error");
    expect(result.lines[1].isContinuation).toBe(true);
  });

  it('still groups an indented Python-style File "..." traceback frame', () => {
    const text = ["ERROR unhandled exception", '  File "app.py", line 10, in <module>'].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBe("error");
    expect(result.lines[1].isContinuation).toBe(true);
  });

  it("still groups an indented trailing elision line ('... N more')", () => {
    const text = ["ERROR outer failure", "    ... 9 more lines omitted"].join("\n");
    const result = parseLog(text);
    expect(result.lines[1].filterLevel).toBe("error");
    expect(result.lines[1].isContinuation).toBe(true);
  });

  it("a real Node.js-style unhandled-rejection trace still shows fully under an errors-only filter", () => {
    // A realistic Node.js stack trace shape: header + several indented "at ..." frames, no level word on
    // any of the frame lines.
    const NODE_TRACE_LINES = [
      "2026-08-11T09:14:05.201Z ERROR Unhandled promise rejection",
      "TypeError: Cannot read properties of undefined (reading 'foo')",
      "    at Object.<anonymous> (/app/index.js:42:11)",
      "    at Module._compile (node:internal/modules/cjs/loader:1105:14)",
      "    at Module._extensions..js (node:internal/modules/cjs/loader:1159:10)",
      "    at Module.load (node:internal/modules/cjs/loader:981:32)",
      "Listening on port 3000", // unrelated — must not be swept in
    ];
    const result = parseLog(NODE_TRACE_LINES.join("\n"));
    for (const i of [1, 2, 3, 4, 5]) {
      expect(result.lines[i].filterLevel, `line ${i}`).toBe("error");
      expect(result.lines[i].isContinuation, `line ${i}`).toBe(true);
    }
    expect(result.lines[6].filterLevel).toBeNull();

    const shown = filterLines(result.lines, { levels: new Set<LogLevel>(["error"]), showUnleveled: false });
    expect(shown.map((l) => l.text)).toEqual(NODE_TRACE_LINES.slice(0, 6));
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
