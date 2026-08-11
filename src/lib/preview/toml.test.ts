// CPE-1617 (epic CPE-1568 slice 7): hand-rolled TOML parser — tables, dotted keys, arrays (incl.
// multi-line), inline tables, array-of-tables, scalar types, and honest errors on out-of-scope syntax.
import { describe, it, expect } from "vitest";
import { parseToml } from "./toml";

function ok(raw: string): Record<string, unknown> {
  const r = parseToml(raw);
  expect(r.ok, r.ok ? "" : (r as { error: string }).error).toBe(true);
  return (r as { ok: true; value: Record<string, unknown> }).value;
}

function fails(raw: string): string {
  const r = parseToml(raw);
  expect(r.ok).toBe(false);
  return (r as { ok: false; error: string }).error;
}

describe("parseToml — happy path", () => {
  it("parses top-level scalars of every type", () => {
    const v = ok(
      [
        'title = "hello"',
        "count = 42",
        "ratio = 3.14",
        "enabled = true",
        "disabled = false",
        "empty_ok = 0",
      ].join("\n"),
    );
    expect(v).toEqual({ title: "hello", count: 42, ratio: 3.14, enabled: true, disabled: false, empty_ok: 0 });
  });

  it("parses negative numbers, underscores, and exponents", () => {
    const v = ok(["a = -17", "b = 1_000_000", "c = 6.022e23", "d = -0.5"].join("\n"));
    expect(v).toEqual({ a: -17, b: 1000000, c: 6.022e23, d: -0.5 });
  });

  it("parses hex/octal/binary integers", () => {
    const v = ok(["hex = 0xFF", "oct = 0o17", "bin = 0b1010"].join("\n"));
    expect(v).toEqual({ hex: 255, oct: 15, bin: 10 });
  });

  it("parses inf/nan", () => {
    const v = ok(["a = inf", "b = -inf", "c = nan"].join("\n"));
    expect(v.a).toBe(Infinity);
    expect(v.b).toBe(-Infinity);
    expect(Number.isNaN(v.c as number)).toBe(true);
  });

  it("parses basic strings with escapes", () => {
    const v = ok(String.raw`s = "line1\nline2\t\"quoted\""`);
    expect(v.s).toBe('line1\nline2\t"quoted"');
  });

  it("parses literal strings with no escaping", () => {
    const v = ok(String.raw`path = 'C:\Users\no\escapes'`);
    expect(v.path).toBe(String.raw`C:\Users\no\escapes`);
  });

  it("parses a single-line array", () => {
    const v = ok('nums = [1, 2, 3]\nnames = ["a", "b"]');
    expect(v).toEqual({ nums: [1, 2, 3], names: ["a", "b"] });
  });

  it("parses a multi-line array with comments (pyproject.toml shape)", () => {
    const v = ok(
      [
        "dependencies = [",
        '  "requests>=2",  # http client',
        '  "click",',
        "  # a comment-only line",
        '  "rich",',
        "]",
      ].join("\n"),
    );
    expect(v.dependencies).toEqual(["requests>=2", "click", "rich"]);
  });

  it("parses nested arrays", () => {
    const v = ok("matrix = [[1, 2], [3, 4]]");
    expect(v.matrix).toEqual([[1, 2], [3, 4]]);
  });

  it("parses an inline table", () => {
    const v = ok('point = { x = 1, y = 2, label = "origin" }');
    expect(v.point).toEqual({ x: 1, y: 2, label: "origin" });
  });

  it("parses dotted keys as nested tables", () => {
    const v = ok('server.host = "localhost"\nserver.port = 8080');
    expect(v.server).toEqual({ host: "localhost", port: 8080 });
  });

  it("parses [table] headers, including nested dotted paths", () => {
    const v = ok(
      ['[package]', 'name = "demo"', "", "[package.metadata]", "ci = true"].join("\n"),
    );
    expect(v).toEqual({ package: { name: "demo", metadata: { ci: true } } });
  });

  it("parses [[array.of.tables]] headers with multiple entries", () => {
    const v = ok(
      [
        "[[workers]]",
        'name = "a"',
        "threads = 2",
        "",
        "[[workers]]",
        'name = "b"',
        "threads = 4",
      ].join("\n"),
    );
    expect(v.workers).toEqual([
      { name: "a", threads: 2 },
      { name: "b", threads: 4 },
    ]);
  });

  it("parses a nested array-of-tables under a table", () => {
    const v = ok(
      ["[server]", 'name = "s1"', "", "[[server.routes]]", 'path = "/a"', "", "[[server.routes]]", 'path = "/b"'].join(
        "\n",
      ),
    );
    expect(v.server).toEqual({
      name: "s1",
      routes: [{ path: "/a" }, { path: "/b" }],
    });
  });

  it("parses quoted keys", () => {
    const v = ok('"weird key" = 1\n\'also weird\' = 2');
    expect(v).toEqual({ "weird key": 1, "also weird": 2 });
  });

  it("ignores comments and blank lines", () => {
    const v = ok(["# a leading comment", "", "a = 1  # trailing comment", "", "# another"].join("\n"));
    expect(v).toEqual({ a: 1 });
  });

  it("represents date/time values as their literal source string", () => {
    const v = ok("dob = 1979-05-27T07:32:00-08:00\ntod = 07:32:00");
    expect(v.dob).toBe("1979-05-27T07:32:00-08:00");
    expect(v.tod).toBe("07:32:00");
  });

  it("parses an empty document to an empty object", () => {
    expect(ok("")).toEqual({});
    expect(ok("  \n\n# just a comment\n")).toEqual({});
  });
});

describe("parseToml — malformed input reports a real, specific error", () => {
  it("reports an unterminated table header", () => {
    expect(fails("[server\nname = 1")).toMatch(/malformed table header/i);
  });

  it("reports an unterminated array-of-tables header", () => {
    expect(fails("[[workers]\nname = 1")).toMatch(/malformed array-of-tables header/i);
  });

  it("reports a missing '=' in a key/value line", () => {
    expect(fails("key value")).toMatch(/expected '='/i);
  });

  it("reports an unterminated basic string", () => {
    expect(fails('s = "unterminated')).toMatch(/unterminated string/i);
  });

  it("reports an unterminated array", () => {
    expect(fails("nums = [1, 2")).toMatch(/unterminated array/i);
  });

  it("reports a multi-line string as explicitly unsupported (not silently mis-parsed)", () => {
    expect(fails('s = """\nmulti\nline\n"""')).toMatch(/multi-line strings/i);
  });

  it("reports a duplicate key", () => {
    expect(fails("a = 1\na = 2")).toMatch(/duplicate key/i);
  });

  it("reports redefining a table as a non-table", () => {
    expect(fails("a = 1\n[a]\nb = 2")).toMatch(/cannot redefine/i);
  });

  it("reports an invalid bare value", () => {
    expect(fails("a = not_a_valid_value_at_all!!")).toMatch(/invalid value/i);
  });

  it("reports trailing garbage after a value", () => {
    expect(fails("a = 1 2")).toMatch(/expected end of line/i);
  });
});

describe("parseToml — untrusted-input safety", () => {
  it("a '__proto__' key never pollutes Object.prototype", () => {
    const v = ok('"__proto__" = { polluted = true }');
    // The key exists as an OWN property of the null-prototype root object, not as a real prototype
    // mutation — a plain {} object elsewhere in the app must not suddenly have `.polluted`.
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
    expect((v as Record<string, unknown>)["__proto__"]).toEqual({ polluted: true });
  });

  it("deeply nested arrays fail cleanly instead of overflowing the stack", () => {
    const depth = 5000;
    const raw = "a = " + "[".repeat(depth) + "1" + "]".repeat(depth);
    expect(() => parseToml(raw)).not.toThrow();
    const r = parseToml(raw);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toMatch(/nested too deeply/i);
  });

  it("a large flat array (many siblings, not nesting) parses without error", () => {
    const items = Array.from({ length: 20000 }, (_, i) => i).join(", ");
    const r = parseToml(`nums = [${items}]`);
    expect(r.ok).toBe(true);
    if (r.ok) expect((r.value.nums as unknown[]).length).toBe(20000);
  });
});
