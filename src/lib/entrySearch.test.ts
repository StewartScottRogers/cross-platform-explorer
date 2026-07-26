import { describe, it, expect } from "vitest";
import { makeEntryMatcher, type EntryLike } from "./entrySearch";

/** Build a small fixture entry with sensible defaults, overridable per test. */
function entry(overrides: Partial<EntryLike> = {}): EntryLike {
  return {
    name: "file.txt",
    path: "/work/file.txt",
    extension: "txt",
    size: 0,
    modified: null,
    ...overrides,
  };
}

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;

// A fixed "now" so relative-date tests are deterministic: 2024-07-25 12:00:00 UTC.
const NOW_MS = Date.UTC(2024, 6, 25, 12, 0, 0);
const DAY_MS = 24 * 60 * 60 * 1000;

describe("makeEntryMatcher — empty query", () => {
  it("matches everything for an empty or whitespace query", () => {
    const m = makeEntryMatcher("", NOW_MS);
    expect(m(entry())).toBe(true);
    const ws = makeEntryMatcher("   ", NOW_MS);
    expect(ws(entry())).toBe(true);
  });
});

describe("bare name term (reuses makeMatcher)", () => {
  it("substring matches by name, case-insensitively", () => {
    const m = makeEntryMatcher("report", NOW_MS);
    expect(m(entry({ name: "Q3-Report.pdf" }))).toBe(true);
    expect(m(entry({ name: "budget.pdf" }))).toBe(false);
  });

  it("supports glob syntax identically to the plain name matcher", () => {
    const m = makeEntryMatcher("*.{jpg,png}", NOW_MS);
    expect(m(entry({ name: "photo.jpg" }))).toBe(true);
    expect(m(entry({ name: "photo.png" }))).toBe(true);
    expect(m(entry({ name: "photo.gif" }))).toBe(false);
  });
});

describe("size: filter", () => {
  it("supports > and < with 1024-based units", () => {
    const gt = makeEntryMatcher("size:>1mb", NOW_MS);
    expect(gt(entry({ size: MB + 1 }))).toBe(true);
    expect(gt(entry({ size: MB }))).toBe(false);
    expect(gt(entry({ size: MB - 1 }))).toBe(false);

    const lt = makeEntryMatcher("size:<1mb", NOW_MS);
    expect(lt(entry({ size: MB - 1 }))).toBe(true);
    expect(lt(entry({ size: MB }))).toBe(false);
  });

  it("supports >= and <=", () => {
    const ge = makeEntryMatcher("size:>=1k", NOW_MS);
    expect(ge(entry({ size: KB }))).toBe(true);
    expect(ge(entry({ size: KB - 1 }))).toBe(false);

    const le = makeEntryMatcher("size:<=500k", NOW_MS);
    expect(le(entry({ size: 500 * KB }))).toBe(true);
    expect(le(entry({ size: 500 * KB + 1 }))).toBe(false);
  });

  it("supports an inclusive lo..hi range", () => {
    const m = makeEntryMatcher("size:1mb..1gb", NOW_MS);
    expect(m(entry({ size: MB }))).toBe(true); // lower bound inclusive
    expect(m(entry({ size: GB }))).toBe(true); // upper bound inclusive
    expect(m(entry({ size: MB - 1 }))).toBe(false);
    expect(m(entry({ size: GB + 1 }))).toBe(false);
  });

  it("supports a decimal mantissa", () => {
    const m = makeEntryMatcher("size:=2.5g", NOW_MS);
    expect(m(entry({ size: 2_684_354_560 }))).toBe(true);
    expect(m(entry({ size: 2_684_354_559 }))).toBe(false);
  });

  it("a bare amount with no operator defaults to exact equality", () => {
    const m = makeEntryMatcher("size:1k", NOW_MS);
    expect(m(entry({ size: KB }))).toBe(true);
    expect(m(entry({ size: KB - 1 }))).toBe(false);
  });

  it("a garbage size token matches nothing rather than throwing", () => {
    expect(() => makeEntryMatcher("size:abc", NOW_MS)).not.toThrow();
    const m = makeEntryMatcher("size:abc", NOW_MS);
    expect(m(entry({ size: 0 }))).toBe(false);
    expect(m(entry({ size: 999_999_999 }))).toBe(false);

    const bad2 = makeEntryMatcher("size:1xb", NOW_MS);
    expect(bad2(entry({ size: 1024 }))).toBe(false);

    // A range with hi < lo is also rejected -> matches nothing.
    const badRange = makeEntryMatcher("size:1gb..1mb", NOW_MS);
    expect(badRange(entry({ size: MB }))).toBe(false);
  });
});

describe("date:/modified: filter", () => {
  it("matches a fixed relative window (<7d) against a pinned now", () => {
    const m = makeEntryMatcher("modified:<7d", NOW_MS);
    expect(m(entry({ modified: NOW_MS }))).toBe(true); // now itself
    expect(m(entry({ modified: NOW_MS - 3 * DAY_MS }))).toBe(true); // within window
    expect(m(entry({ modified: NOW_MS - 7 * DAY_MS }))).toBe(true); // boundary: inclusive
    expect(m(entry({ modified: NOW_MS - 7 * DAY_MS - 1000 }))).toBe(false); // just past boundary
    expect(m(entry({ modified: NOW_MS - 10 * DAY_MS }))).toBe(false);
  });

  it("matches an older-than window (>Nunit)", () => {
    const m = makeEntryMatcher("date:>1w", NOW_MS);
    expect(m(entry({ modified: NOW_MS - 10 * DAY_MS }))).toBe(true);
    expect(m(entry({ modified: NOW_MS - 3 * DAY_MS }))).toBe(false);
  });

  it("matches absolute year/month/day spans", () => {
    const year = makeEntryMatcher("date:2024", NOW_MS);
    expect(year(entry({ modified: Date.UTC(2024, 0, 1) }))).toBe(true);
    expect(year(entry({ modified: Date.UTC(2024, 11, 31, 23, 59, 59) }))).toBe(true);
    expect(year(entry({ modified: Date.UTC(2023, 11, 31, 23, 59, 59) }))).toBe(false);
    expect(year(entry({ modified: Date.UTC(2025, 0, 1) }))).toBe(false);

    const month = makeEntryMatcher("date:2024-07", NOW_MS);
    expect(month(entry({ modified: Date.UTC(2024, 6, 1) }))).toBe(true);
    expect(month(entry({ modified: Date.UTC(2024, 6, 31, 23, 59, 59) }))).toBe(true);
    expect(month(entry({ modified: Date.UTC(2024, 7, 1) }))).toBe(false);

    const day = makeEntryMatcher("date:2024-07-25", NOW_MS);
    expect(day(entry({ modified: Date.UTC(2024, 6, 25, 0, 0, 0) }))).toBe(true);
    expect(day(entry({ modified: Date.UTC(2024, 6, 25, 23, 59, 59) }))).toBe(true);
    expect(day(entry({ modified: Date.UTC(2024, 6, 26, 0, 0, 0) }))).toBe(false);
  });

  it("matches today/yesterday relative to the pinned now", () => {
    const today = makeEntryMatcher("date:today", NOW_MS);
    expect(today(entry({ modified: Date.UTC(2024, 6, 25, 0, 0, 0) }))).toBe(true);
    expect(today(entry({ modified: Date.UTC(2024, 6, 24, 23, 59, 59) }))).toBe(false);

    const yesterday = makeEntryMatcher("date:yesterday", NOW_MS);
    expect(yesterday(entry({ modified: Date.UTC(2024, 6, 24, 12, 0, 0) }))).toBe(true);
    expect(yesterday(entry({ modified: Date.UTC(2024, 6, 25, 0, 0, 0) }))).toBe(false);
  });

  it("a null modified never matches a date filter", () => {
    const m = makeEntryMatcher("modified:<30d", NOW_MS);
    expect(m(entry({ modified: null }))).toBe(false);
  });

  it("malformed date tokens match nothing rather than throwing", () => {
    expect(() => makeEntryMatcher("date:2024-13", NOW_MS)).not.toThrow();
    const m = makeEntryMatcher("date:2024-13", NOW_MS); // month 13
    expect(m(entry({ modified: NOW_MS }))).toBe(false);

    const garbage = makeEntryMatcher("date:garbage", NOW_MS);
    expect(garbage(entry({ modified: NOW_MS }))).toBe(false);

    // A syntactically-huge "year" must not throw (overflow guard, ported from date_filter.rs).
    const huge = makeEntryMatcher("date:9223372036854775807", NOW_MS);
    expect(() => huge(entry({ modified: NOW_MS }))).not.toThrow();
    expect(huge(entry({ modified: NOW_MS }))).toBe(false);
  });
});

describe("type: filter", () => {
  it("matches a single class", () => {
    const m = makeEntryMatcher("type:image", NOW_MS);
    expect(m(entry({ extension: "png" }))).toBe(true);
    expect(m(entry({ extension: "PNG" }))).toBe(true); // case-insensitive
    expect(m(entry({ extension: "mp3" }))).toBe(false);
  });

  it("matches a comma list of classes (any-of)", () => {
    const m = makeEntryMatcher("type:image,video", NOW_MS);
    expect(m(entry({ extension: "png" }))).toBe(true);
    expect(m(entry({ extension: "mp4" }))).toBe(true);
    expect(m(entry({ extension: "mp3" }))).toBe(false);
  });

  it("an unrecognised class matches nothing", () => {
    const m = makeEntryMatcher("type:bogus", NOW_MS);
    expect(m(entry({ extension: "png" }))).toBe(false);
    expect(m(entry({ extension: "bogus" }))).toBe(false);
  });
});

describe("ext: filter", () => {
  it("matches an exact extension, comma list any-of, dot tolerated", () => {
    const single = makeEntryMatcher("ext:png", NOW_MS);
    expect(single(entry({ extension: "png" }))).toBe(true);
    expect(single(entry({ extension: "jpg" }))).toBe(false);

    const list = makeEntryMatcher("ext:png,jpg", NOW_MS);
    expect(list(entry({ extension: "jpg" }))).toBe(true);

    const dotted = makeEntryMatcher("ext:.png", NOW_MS);
    expect(dotted(entry({ extension: "png" }))).toBe(true);
  });
});

describe("path: filter", () => {
  it("matches a case-insensitive substring of the full path", () => {
    const m = makeEntryMatcher("path:reports", NOW_MS);
    expect(m(entry({ path: "/work/Reports/q3.pdf" }))).toBe(true);
    expect(m(entry({ path: "/work/other/q3.pdf" }))).toBe(false);
  });
});

describe("boolean precedence", () => {
  it("juxtaposition (implicit AND) requires both terms", () => {
    const m = makeEntryMatcher("size:>1mb type:image", NOW_MS);
    expect(m(entry({ size: 2 * MB, extension: "png" }))).toBe(true);
    expect(m(entry({ size: 2 * MB, extension: "mp3" }))).toBe(false);
    expect(m(entry({ size: 0, extension: "png" }))).toBe(false);
  });

  it("OR binds looser than AND: a OR b c == a OR (b AND c)", () => {
    const m = makeEntryMatcher("a OR b c", NOW_MS);
    expect(m(entry({ name: "a" }))).toBe(true);
    expect(m(entry({ name: "b-c" }))).toBe(true); // contains both b and c substrings
    expect(m(entry({ name: "b" }))).toBe(false); // b without c
    expect(m(entry({ name: "x" }))).toBe(false);
  });

  it("-x is equivalent to NOT x", () => {
    const dash = makeEntryMatcher("-tmp", NOW_MS);
    const not = makeEntryMatcher("NOT tmp", NOW_MS);
    expect(dash(entry({ name: "tmp" }))).toBe(false);
    expect(not(entry({ name: "tmp" }))).toBe(false);
    expect(dash(entry({ name: "file" }))).toBe(true);
    expect(not(entry({ name: "file" }))).toBe(true);
  });

  it("NOT binds tighter than AND: NOT a b == (NOT a) AND b", () => {
    const m = makeEntryMatcher("NOT type:archive size:>0", NOW_MS);
    expect(m(entry({ extension: "zip", size: 1 }))).toBe(false);
    expect(m(entry({ extension: "png", size: 1 }))).toBe(true);
    expect(m(entry({ extension: "png", size: 0 }))).toBe(false);
  });

  it("parentheses override the default precedence", () => {
    const m = makeEntryMatcher("(type:image OR type:video) size:>1mb", NOW_MS);
    expect(m(entry({ extension: "png", size: 2 * MB }))).toBe(true);
    expect(m(entry({ extension: "mp4", size: 2 * MB }))).toBe(true);
    expect(m(entry({ extension: "png", size: 0 }))).toBe(false);
    expect(m(entry({ extension: "mp3", size: 2 * MB }))).toBe(false);
  });

  it("deep nesting never throws or overflows the stack", () => {
    const parens = "(".repeat(10_000) + "a" + ")".repeat(10_000);
    expect(() => makeEntryMatcher(parens, NOW_MS)).not.toThrow();
    const parenMatcher = makeEntryMatcher(parens, NOW_MS);
    expect(() => parenMatcher(entry({ name: "a" }))).not.toThrow();

    const nots = "NOT ".repeat(10_000) + "x";
    expect(() => makeEntryMatcher(nots, NOW_MS)).not.toThrow();
    const notMatcher = makeEntryMatcher(nots, NOW_MS);
    expect(() => notMatcher(entry({ name: "x" }))).not.toThrow();

    const unbalanced = "(".repeat(5_000);
    expect(() => makeEntryMatcher(unbalanced, NOW_MS)).not.toThrow();
  });
});

describe("unrecognised key:value falls back to a bare name term", () => {
  it("treats an unknown prefix as a literal name/substring term, not a dropped token", () => {
    const m = makeEntryMatcher("foo:bar", NOW_MS);
    expect(m(entry({ name: "foo:bar-report.txt" }))).toBe(true);
    expect(m(entry({ name: "unrelated.txt" }))).toBe(false);
  });
});

describe("compiles once (no per-entry recompile)", () => {
  it("returns a matcher whose behavior is stable across many calls", () => {
    const m = makeEntryMatcher("size:>1mb type:image", NOW_MS);
    const entries = [
      entry({ size: 2 * MB, extension: "png", name: "a" }),
      entry({ size: 0, extension: "png", name: "b" }),
      entry({ size: 2 * MB, extension: "mp3", name: "c" }),
    ];
    expect(entries.map(m)).toEqual([true, false, false]);
  });
});
