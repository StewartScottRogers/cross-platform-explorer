// CPE-1617 (epic CPE-1568 slice 7): bounded-subset YAML parser — ordinary nested maps/sequences/scalars
// parse correctly; anything outside the documented subset degrades EXPLICITLY (unsupported:true) rather
// than being silently mis-parsed, and genuine syntax errors are structurally distinct (unsupported:false).
import { describe, it, expect } from "vitest";
import { parseYaml } from "./yaml";

function ok(raw: string): unknown {
  const r = parseYaml(raw);
  expect(r.ok, r.ok ? "" : `${(r as { error: string }).error}`).toBe(true);
  return (r as { ok: true; value: unknown }).value;
}

function unsupported(raw: string): string {
  const r = parseYaml(raw);
  expect(r.ok).toBe(false);
  expect((r as { unsupported: boolean }).unsupported, "expected unsupported:true").toBe(true);
  return (r as { error: string }).error;
}

function syntaxError(raw: string): string {
  const r = parseYaml(raw);
  expect(r.ok).toBe(false);
  expect((r as { unsupported: boolean }).unsupported, "expected unsupported:false (a real error)").toBe(false);
  return (r as { error: string }).error;
}

describe("parseYaml — happy path (ordinary config shapes)", () => {
  it("parses a flat mapping of scalars", () => {
    expect(ok("name: demo\nport: 8080\nratio: 1.5\nenabled: true\ndisabled: false\nnote: ~")).toEqual({
      name: "demo",
      port: 8080,
      ratio: 1.5,
      enabled: true,
      disabled: false,
      note: null,
    });
  });

  it("parses nested mappings by indentation", () => {
    expect(ok(["server:", "  host: localhost", "  port: 8080", "  tls:", "    enabled: true"].join("\n"))).toEqual({
      server: { host: "localhost", port: 8080, tls: { enabled: true } },
    });
  });

  it("parses a block sequence of scalars", () => {
    expect(ok(["fruits:", "  - apple", "  - banana", "  - cherry"].join("\n"))).toEqual({
      fruits: ["apple", "banana", "cherry"],
    });
  });

  it("parses a block sequence of mappings (the `- key: value` shorthand)", () => {
    const v = ok(
      ["users:", "  - name: ann", "    age: 30", "  - name: bo", "    age: 25"].join("\n"),
    );
    expect(v).toEqual({
      users: [
        { name: "ann", age: 30 },
        { name: "bo", age: 25 },
      ],
    });
  });

  it("parses a sequence nested under a bare dash", () => {
    const v = ok(["matrix:", "  -", "    - 1", "    - 2", "  -", "    - 3", "    - 4"].join("\n"));
    expect(v).toEqual({ matrix: [[1, 2], [3, 4]] });
  });

  it("parses single-line flow sequences and mappings", () => {
    expect(ok("nums: [1, 2, 3]")).toEqual({ nums: [1, 2, 3] });
    expect(ok("point: {x: 1, y: 2}")).toEqual({ point: { x: 1, y: 2 } });
  });

  it("parses quoted scalars with escapes", () => {
    expect(ok(String.raw`msg: "line1\nline2"`)).toEqual({ msg: "line1\nline2" });
    expect(ok("path: 'it''s a test'")).toEqual({ path: "it's a test" });
  });

  it("parses comments and blank lines", () => {
    expect(ok(["# leading comment", "", "a: 1  # trailing", "", "# another"].join("\n"))).toEqual({ a: 1 });
  });

  it("parses a single leading '---' document-start marker", () => {
    expect(ok(["---", "a: 1", "b: 2"].join("\n"))).toEqual({ a: 1, b: 2 });
  });

  it("parses a top-level scalar document", () => {
    expect(ok("42")).toBe(42);
    expect(ok('"hello"')).toBe("hello");
  });

  it("parses an empty document as null", () => {
    expect(ok("")).toBe(null);
  });

  it("a key whose value merely mentions '&' or '*' mid-string is fine (only a LEADING sigil is special)", () => {
    expect(ok('note: "call me * 2, or maybe & something"')).toEqual({
      note: "call me * 2, or maybe & something",
    });
  });
});

describe("parseYaml — deliberately unsupported constructs degrade explicitly with a reason", () => {
  it("anchors", () => {
    expect(unsupported("base: &anchor\n  a: 1")).toMatch(/anchor/i);
  });

  it("aliases", () => {
    expect(unsupported("a: &x 1\nb: *x")).toMatch(/anchor|alias/i);
  });

  it("explicit tags", () => {
    expect(unsupported("a: !!str 123")).toMatch(/tag/i);
  });

  it("block scalars (literal |)", () => {
    expect(unsupported(["desc: |", "  line one", "  line two"].join("\n"))).toMatch(/block scalar/i);
  });

  it("block scalars (folded >)", () => {
    expect(unsupported(["desc: >", "  folded text"].join("\n"))).toMatch(/block scalar/i);
  });

  it("complex mapping keys", () => {
    expect(unsupported("? complex key\n: value")).toMatch(/complex/i);
  });

  it("multiple documents", () => {
    expect(unsupported(["a: 1", "---", "b: 2"].join("\n"))).toMatch(/multiple.*document/i);
  });

  it("tab-indented content", () => {
    expect(unsupported("a:\n\tb: 1")).toMatch(/tab/i);
  });

  it("a flow collection spanning multiple lines", () => {
    expect(unsupported("nums: [1, 2,")).toMatch(/multiple lines|multi-line/i);
  });
});

describe("parseYaml — genuine syntax errors are structurally distinct from 'unsupported'", () => {
  it("bad indentation reports a real error, not unsupported", () => {
    const err = syntaxError(["a: 1", "  b: 2"].join("\n"));
    expect(err).toMatch(/unexpected indentation/i);
  });

  it("an unterminated quoted scalar reports a real error", () => {
    expect(syntaxError('a: "unterminated')).toMatch(/unterminated/i);
  });

  it("a line that isn't a mapping entry inside a mapping block reports a real error", () => {
    expect(syntaxError(["a: 1", "not a mapping line"].join("\n"))).toMatch(/expected "key: value"/i);
  });

  it("a duplicate key reports a real error", () => {
    expect(syntaxError("a: 1\na: 2")).toMatch(/duplicate key/i);
  });
});

describe("parseYaml — untrusted-input safety", () => {
  it("a '__proto__' key never pollutes Object.prototype", () => {
    const v = ok("__proto__:\n  polluted: true") as Record<string, unknown>;
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
    expect(v["__proto__"]).toEqual({ polluted: true });
  });

  it("deeply nested block sequences fail cleanly instead of overflowing the stack", () => {
    const depth = 3000;
    const lines: string[] = [];
    for (let i = 0; i < depth; i++) lines.push("  ".repeat(i) + "-");
    lines.push("  ".repeat(depth) + "1");
    expect(() => parseYaml(lines.join("\n"))).not.toThrow();
    const r = parseYaml(lines.join("\n"));
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toMatch(/nested too deeply/i);
  });

  it("deeply nested flow collections on one line fail cleanly instead of overflowing the stack", () => {
    const depth = 5000;
    const raw = "a: " + "[".repeat(depth) + "1" + "]".repeat(depth);
    expect(() => parseYaml(raw)).not.toThrow();
    const r = parseYaml(raw);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toMatch(/nested too deeply/i);
  });

  it("a large flat sequence (many siblings, not nesting) parses without error", () => {
    const lines = ["items:"];
    for (let i = 0; i < 20000; i++) lines.push(`  - ${i}`);
    const r = parseYaml(lines.join("\n"));
    expect(r.ok).toBe(true);
    if (r.ok) expect((r.value as { items: unknown[] }).items.length).toBe(20000);
  });
});
