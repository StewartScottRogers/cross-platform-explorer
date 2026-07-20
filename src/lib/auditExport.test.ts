import { describe, it, expect } from "vitest";
import { filterEvents, toJson, toCsv, toMarkdown, type AuditEvent } from "./auditExport";

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
