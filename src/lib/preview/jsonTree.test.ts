/**
 * Unit tests for the JSON tree's pure logic (CPE-1573, epic CPE-1568) — parse/format, type
 * classification, child enumeration, and path/value copy formatting. Kept framework-free (no Svelte
 * mount) so these run fast and cover the large-file-safety constants the components consume.
 */
import { describe, it, expect } from "vitest";
import {
  typeOf,
  isExpandable,
  childEntries,
  formatPrimitive,
  copyableValue,
  pathToString,
  parseJson,
  formatJson,
  defaultExpanded,
  AUTO_COLLAPSE_DEPTH,
  MAX_CHILDREN,
} from "./jsonTree";

describe("typeOf", () => {
  it.each([
    [null, "null"],
    [[1, 2], "array"],
    [{ a: 1 }, "object"],
    ["s", "string"],
    [42, "number"],
    [true, "boolean"],
  ] as const)("classifies %p as %s", (value, kind) => {
    expect(typeOf(value)).toBe(kind);
  });
});

describe("isExpandable", () => {
  it("is true for object/array, false for primitives", () => {
    expect(isExpandable({})).toBe(true);
    expect(isExpandable([])).toBe(true);
    expect(isExpandable("x")).toBe(false);
    expect(isExpandable(1)).toBe(false);
    expect(isExpandable(null)).toBe(false);
    expect(isExpandable(true)).toBe(false);
  });
});

describe("childEntries", () => {
  it("enumerates array entries with numeric segments", () => {
    expect(childEntries(["a", "b"])).toEqual([
      { key: "0", segment: 0, value: "a" },
      { key: "1", segment: 1, value: "b" },
    ]);
  });

  it("enumerates object entries with string segments, preserving insertion order", () => {
    expect(childEntries({ b: 1, a: 2 })).toEqual([
      { key: "b", segment: "b", value: 1 },
      { key: "a", segment: "a", value: 2 },
    ]);
  });

  it("returns no entries for a primitive", () => {
    expect(childEntries("x")).toEqual([]);
    expect(childEntries(42)).toEqual([]);
    expect(childEntries(null)).toEqual([]);
  });
});

describe("formatPrimitive", () => {
  it("quotes/escapes strings like JSON source", () => {
    expect(formatPrimitive("hi")).toBe('"hi"');
    expect(formatPrimitive('a"b')).toBe('"a\\"b"');
  });

  it("prints number/boolean/null as their literal", () => {
    expect(formatPrimitive(42)).toBe("42");
    expect(formatPrimitive(true)).toBe("true");
    expect(formatPrimitive(null)).toBe("null");
  });
});

describe("copyableValue", () => {
  it("copies a string unquoted (directly usable text)", () => {
    expect(copyableValue("hello world")).toBe("hello world");
  });

  it("copies number/boolean/null as their literal", () => {
    expect(copyableValue(42)).toBe("42");
    expect(copyableValue(false)).toBe("false");
    expect(copyableValue(null)).toBe("null");
  });

  it("copies an object/array subtree as pretty JSON", () => {
    expect(copyableValue({ a: 1 })).toBe('{\n  "a": 1\n}');
    expect(copyableValue([1, 2])).toBe("[\n  1,\n  2\n]");
  });
});

describe("pathToString", () => {
  it("is $ at the root", () => {
    expect(pathToString([])).toBe("$");
  });

  it("dots into identifier-like object keys and brackets array indices", () => {
    expect(pathToString(["a", 0, "b"])).toBe("$.a[0].b");
  });

  it("brackets+quotes a key that isn't a valid identifier", () => {
    expect(pathToString(["odd key", "2bad"])).toBe('$["odd key"]["2bad"]');
  });
});

describe("parseJson", () => {
  it("parses valid JSON", () => {
    const r = parseJson('{"a":1}');
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ a: 1 });
  });

  it("reports a clear error for malformed JSON instead of throwing", () => {
    const r = parseJson("{not json}");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.length).toBeGreaterThan(0);
  });
});

describe("formatJson", () => {
  it("normalizes/pretty-prints valid JSON", () => {
    const r = formatJson('{"a":1,"b":[1,2]}');
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.text).toBe('{\n  "a": 1,\n  "b": [\n    1,\n    2\n  ]\n}');
  });

  it("surfaces the parse error for malformed JSON instead of throwing", () => {
    const r = formatJson("{bad");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.length).toBeGreaterThan(0);
  });
});

describe("large-file safety constants (CPE-1573)", () => {
  it("defaultExpanded is true up to AUTO_COLLAPSE_DEPTH, false past it", () => {
    for (let d = 0; d < AUTO_COLLAPSE_DEPTH; d++) expect(defaultExpanded(d)).toBe(true);
    expect(defaultExpanded(AUTO_COLLAPSE_DEPTH)).toBe(false);
    expect(defaultExpanded(AUTO_COLLAPSE_DEPTH + 5)).toBe(false);
  });

  it("MAX_CHILDREN is a sane finite cap", () => {
    expect(MAX_CHILDREN).toBeGreaterThan(0);
    expect(Number.isFinite(MAX_CHILDREN)).toBe(true);
  });
});
