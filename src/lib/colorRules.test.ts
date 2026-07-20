import { describe, it, expect } from "vitest";
import { matchesCondition, evaluateRules, type ColorRule } from "./colorRules";
import type { DirEntry } from "./types";

const NOW = 1_700_000_000_000; // fixed "now" (epoch ms)
const DAY = 86_400_000;

function entry(over: Partial<DirEntry> = {}): DirEntry {
  return {
    name: "file.txt",
    path: "/x/file.txt",
    is_dir: false,
    size: 100,
    modified: NOW,
    ...(over as object),
  } as DirEntry;
}

describe("matchesCondition (CPE-774)", () => {
  it("ext: case-insensitive, leading dot optional, dotfiles have no ext", () => {
    expect(matchesCondition(entry({ name: "a.PNG" }), { kind: "ext", exts: ["png", "jpg"] }, NOW)).toBe(true);
    expect(matchesCondition(entry({ name: "a.png" }), { kind: "ext", exts: [".PNG"] }, NOW)).toBe(true);
    expect(matchesCondition(entry({ name: "a.txt" }), { kind: "ext", exts: ["png"] }, NOW)).toBe(false);
    expect(matchesCondition(entry({ name: ".gitignore" }), { kind: "ext", exts: ["gitignore"] }, NOW)).toBe(false);
  });

  it("glob: matches the name", () => {
    expect(matchesCondition(entry({ name: "report-2024.pdf" }), { kind: "glob", pattern: "report-*.pdf" }, NOW)).toBe(true);
    expect(matchesCondition(entry({ name: "notes.md" }), { kind: "glob", pattern: "report-*.pdf" }, NOW)).toBe(false);
  });

  it("size: inclusive min/max bounds", () => {
    expect(matchesCondition(entry({ size: 500 }), { kind: "size", min: 100, max: 1000 }, NOW)).toBe(true);
    expect(matchesCondition(entry({ size: 50 }), { kind: "size", min: 100 }, NOW)).toBe(false);
    expect(matchesCondition(entry({ size: 2000 }), { kind: "size", max: 1000 }, NOW)).toBe(false);
    expect(matchesCondition(entry({ size: 100 }), { kind: "size", min: 100, max: 100 }, NOW)).toBe(true);
  });

  it("olderThan / newerThan: relative to now, null modified never matches", () => {
    const old = entry({ modified: NOW - 10 * DAY });
    const fresh = entry({ modified: NOW - 1 * DAY });
    expect(matchesCondition(old, { kind: "olderThan", days: 7 }, NOW)).toBe(true);
    expect(matchesCondition(fresh, { kind: "olderThan", days: 7 }, NOW)).toBe(false);
    expect(matchesCondition(fresh, { kind: "newerThan", days: 7 }, NOW)).toBe(true);
    expect(matchesCondition(old, { kind: "newerThan", days: 7 }, NOW)).toBe(false);
    expect(matchesCondition(entry({ modified: null }), { kind: "olderThan", days: 7 }, NOW)).toBe(false);
    expect(matchesCondition(entry({ modified: null }), { kind: "newerThan", days: 7 }, NOW)).toBe(false);
  });

  it("isDir", () => {
    expect(matchesCondition(entry({ is_dir: true }), { kind: "isDir", value: true }, NOW)).toBe(true);
    expect(matchesCondition(entry({ is_dir: false }), { kind: "isDir", value: true }, NOW)).toBe(false);
  });
});

describe("evaluateRules (CPE-774)", () => {
  const rules: ColorRule[] = [
    { id: "1", when: { kind: "ext", exts: ["tmp"] }, color: "#888", enabled: false }, // disabled
    { id: "2", when: { kind: "isDir", value: true }, color: "#00f", label: "folder" },
    { id: "3", when: { kind: "ext", exts: ["png", "jpg"] }, color: "#0a0", label: "image" },
    { id: "4", when: { kind: "size", min: 1000 }, color: "#a00", label: "big" },
  ];

  it("returns the first enabled matching rule's style", () => {
    expect(evaluateRules(entry({ name: "a.png", size: 5000 }), rules, NOW)).toEqual({ color: "#0a0", label: "image" });
    // folder rule (2) wins over the size rule (4) by order
    expect(evaluateRules(entry({ is_dir: true, size: 5000 }), rules, NOW)).toEqual({ color: "#00f", label: "folder" });
    // only the size rule applies
    expect(evaluateRules(entry({ name: "a.bin", size: 5000 }), rules, NOW)).toEqual({ color: "#a00", label: "big" });
  });

  it("skips disabled rules and returns {} when nothing matches", () => {
    expect(evaluateRules(entry({ name: "a.tmp", size: 10 }), rules, NOW)).toEqual({}); // rule 1 disabled, nothing else matches
    expect(evaluateRules(entry({ name: "a.md", size: 10 }), [], NOW)).toEqual({});
  });
});
