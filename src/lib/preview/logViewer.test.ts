import { describe, it, expect } from "vitest";
import {
  detectLevel,
  parseLog,
  filterLines,
  ALL_LEVELS,
  MAX_LINES,
  MAX_LINE_CHARS,
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

describe("filterLines", () => {
  const lines: LogLine[] = [
    { index: 0, text: "info line", level: "info", truncated: false },
    { index: 1, text: "warn line", level: "warn", truncated: false },
    { index: 2, text: "error line", level: "error", truncated: false },
    { index: 3, text: "plain line", level: null, truncated: false },
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
