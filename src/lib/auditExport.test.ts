import { describe, it, expect } from "vitest";
import { filterEvents, redactEvents, toJson, toCsv, toMarkdown, type AuditEvent } from "./auditExport";

const ev = (over: Partial<AuditEvent> = {}): AuditEvent => ({
  ts: 1000,
  session: "s1",
  kind: "modified",
  path: "/a/b.txt",
  ...over,
});

describe("filterEvents (CPE-799)", () => {
  const events = [
    ev({ ts: 100, kind: "created", path: "/src/x.ts" }),
    ev({ ts: 200, kind: "modified", path: "/src/y.ts" }),
    ev({ ts: 300, kind: "read", path: "/docs/z.md" }),
  ];
  it("filters by kind, time range, and path substring (combinable)", () => {
    expect(filterEvents(events, { kinds: ["read"] }).map((e) => e.ts)).toEqual([300]);
    expect(filterEvents(events, { since: 150, until: 250 }).map((e) => e.ts)).toEqual([200]);
    expect(filterEvents(events, { pathIncludes: "/SRC/" }).map((e) => e.ts)).toEqual([100, 200]);
    expect(filterEvents(events, { kinds: ["created", "modified"], pathIncludes: "y." }).map((e) => e.ts)).toEqual([200]);
    expect(filterEvents(events, {})).toHaveLength(3);
  });
});

describe("redactEvents (CPE-801)", () => {
  it("collapses the home dir to ~ (case-insensitive, separator-aware, only at start)", () => {
    const events = [
      ev({ path: "C:\\Users\\alice\\secret.txt" }),
      ev({ path: "C:\\Users\\alice" }), // exact home, no trailing sep
      ev({ path: "D:\\Users\\alicebob\\x" }), // must NOT match: alice is not a full segment
    ];
    const out = redactEvents(events, { home: "C:\\Users\\alice" });
    expect(out.map((e) => e.path)).toEqual([
      "~\\secret.txt",
      "~",
      "D:\\Users\\alicebob\\x",
    ]);
  });

  it("masks usernames and custom patterns in path and detail; does not mutate input", () => {
    const events = [ev({ path: "/home/alice/k.txt", detail: "token=alice-KEY-123" })];
    const out = redactEvents(events, {
      home: "/home/alice",
      usernames: ["alice"],
      patterns: ["KEY-\\d+"],
    });
    expect(out[0].path).toBe("~/k.txt");
    expect(out[0].detail).toBe("token=<user>-<redacted>"); // username + pattern both masked
    expect(events[0].path).toBe("/home/alice/k.txt"); // original untouched
  });

  it("respects redactDetail:false and skips invalid regex patterns", () => {
    const events = [ev({ path: "/home/alice/x", detail: "/home/alice/x" })];
    const out = redactEvents(events, { home: "/home/alice", redactDetail: false, patterns: ["("] });
    expect(out[0].path).toBe("~/x");
    expect(out[0].detail).toBe("/home/alice/x"); // detail left alone
  });

  it("is a no-op with empty options and preserves undefined detail", () => {
    const events = [ev({ path: "/a/b", detail: undefined })];
    const out = redactEvents(events, {});
    expect(out[0].path).toBe("/a/b");
    expect(out[0].detail).toBeUndefined();
  });
});

describe("exporters (CPE-799)", () => {
  it("JSON round-trips", () => {
    const events = [ev(), ev({ ts: 2000, kind: "read" })];
    expect(JSON.parse(toJson(events))).toEqual(events);
  });

  it("CSV has a header and quotes fields with commas/quotes/newlines", () => {
    const csv = toCsv([ev({ path: "/a,b.txt", detail: 'say "hi"' }), ev({ path: "/multi\nline" })]);
    const lines = csv.split("\n");
    expect(lines[0]).toBe("ts,session,kind,path,detail");
    expect(lines[1]).toContain('"/a,b.txt"'); // comma → quoted
    expect(lines[1]).toContain('"say ""hi"""'); // quotes doubled
    // the embedded newline field is quoted, so the row spans two output lines
    expect(csv).toContain('"/multi\nline"');
  });

  it("Markdown builds a table and escapes pipes / newlines", () => {
    const md = toMarkdown([ev({ path: "/a|b", detail: "line1\nline2" })]);
    const rows = md.split("\n");
    expect(rows[0]).toBe("| ts | session | kind | path | detail |");
    expect(rows[1]).toBe("| --- | --- | --- | --- | --- |");
    expect(rows[2]).toContain("/a\\|b"); // pipe escaped
    expect(rows[2]).toContain("line1 line2"); // newline collapsed
  });

  it("handles an empty list", () => {
    expect(toJson([])).toBe("[]");
    expect(toCsv([])).toBe("ts,session,kind,path,detail");
    expect(toMarkdown([]).split("\n")).toHaveLength(2); // header + separator only
  });
});
