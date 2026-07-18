import { describe, it, expect } from "vitest";
import { scoreMatch, filterCommands, isEnabled, type Command } from "./commandPalette";

const cmd = (id: string, label: string, extra: Partial<Command> = {}): Command => ({ id, label, run: () => {}, ...extra });

describe("scoreMatch (CPE-602)", () => {
  it("ranks exact > prefix > word-start > substring > subsequence > none", () => {
    expect(scoreMatch("New folder", "New folder")).toBeGreaterThan(scoreMatch("New folder", "new"));
    expect(scoreMatch("New folder", "new")).toBeGreaterThan(scoreMatch("New folder", "folder")); // prefix > word-start
    expect(scoreMatch("New folder", "folder")).toBeGreaterThan(scoreMatch("Rename folder", "ame")); // word-start > mid-substring
    expect(scoreMatch("New folder", "nf")).toBeGreaterThan(0); // subsequence matches
    expect(scoreMatch("New folder", "xyz")).toBe(0); // no match
  });
  it("is case-insensitive and treats a blank query as a match", () => {
    expect(scoreMatch("Settings", "SET")).toBeGreaterThan(0);
    expect(scoreMatch("anything", "")).toBe(1);
  });
});

describe("filterCommands (CPE-602)", () => {
  const commands = [
    cmd("home", "Go Home"),
    cmd("newFolder", "New folder", { keywords: "create directory mkdir" }),
    cmd("settings", "Settings"),
    cmd("refresh", "Refresh"),
  ];

  it("returns everything (declaration order) for a blank query", () => {
    expect(filterCommands(commands, "").map((s) => s.command.id)).toEqual(["home", "newFolder", "settings", "refresh"]);
  });
  it("filters and ranks by relevance", () => {
    const r = filterCommands(commands, "fol");
    expect(r.map((s) => s.command.id)).toEqual(["newFolder"]); // only "New folder" contains "fol"
  });
  it("matches keywords (synonyms) too, but below a direct label match", () => {
    expect(filterCommands(commands, "mkdir").map((s) => s.command.id)).toContain("newFolder");
    // A label hit outranks a keyword-only hit.
    const withDir = filterCommands([cmd("a", "Directory listing"), commands[1]], "directory");
    expect(withDir[0].command.id).toBe("a");
  });
  it("drops non-matches", () => {
    expect(filterCommands(commands, "zzz")).toHaveLength(0);
  });
});

describe("isEnabled (CPE-602)", () => {
  it("defaults to enabled; respects an enabled predicate", () => {
    expect(isEnabled(cmd("a", "A"))).toBe(true);
    expect(isEnabled(cmd("b", "B", { enabled: () => false }))).toBe(false);
  });
});
