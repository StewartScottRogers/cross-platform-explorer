import { describe, it, expect } from "vitest";
import { filterNavCommands } from "./navCommandLine";
import type { Command } from "./commandPalette";

const cmd = (id: string, label: string, extra: Partial<Command> = {}): Command => ({ id, label, run: () => {}, ...extra });

describe("filterNavCommands (CPE-1554)", () => {
  const commands = [
    cmd("home", "Go Home"),
    cmd("newFolder", "New folder", { keywords: "create directory mkdir" }),
    cmd("settings", "Settings"),
    cmd("refresh", "Refresh"),
  ];

  it("strips a leading ':' before matching", () => {
    const withColon = filterNavCommands(commands, ":fol").map((c) => c.id);
    const withoutColon = filterNavCommands(commands, "fol").map((c) => c.id);
    expect(withColon).toEqual(withoutColon);
    expect(withColon).toEqual(["newFolder"]);
  });

  it("an empty query (after stripping ':') returns every enabled command, unfiltered, in declaration order", () => {
    expect(filterNavCommands(commands, ":").map((c) => c.id)).toEqual(["home", "newFolder", "settings", "refresh"]);
    expect(filterNavCommands(commands, "").map((c) => c.id)).toEqual(["home", "newFolder", "settings", "refresh"]);
  });

  it("a whitespace-only query (after stripping ':') also returns every enabled command", () => {
    expect(filterNavCommands(commands, ":   ").map((c) => c.id)).toEqual(["home", "newFolder", "settings", "refresh"]);
  });

  it("a query matching a subset returns only those, ranked", () => {
    expect(filterNavCommands(commands, ":settings").map((c) => c.id)).toEqual(["settings"]);
  });

  it("a query matching nothing returns an empty array", () => {
    expect(filterNavCommands(commands, ":zzz")).toEqual([]);
  });

  it("excludes disabled commands even on an exact label match", () => {
    const withDisabled = [...commands, cmd("locked", "Locked Action", { enabled: () => false })];
    expect(filterNavCommands(withDisabled, ":Locked Action")).toEqual([]);
  });

  it("excludes disabled commands from the blank-query 'show everything' case too", () => {
    const withDisabled = [...commands, cmd("locked", "Locked Action", { enabled: () => false })];
    expect(filterNavCommands(withDisabled, "").map((c) => c.id)).not.toContain("locked");
  });

  it("unwraps ScoredCommand back to a plain Command (no score/wrapper leaking through)", () => {
    const result = filterNavCommands(commands, ":home");
    expect(result).toEqual([commands[0]]);
    expect(result[0]).not.toHaveProperty("score");
  });

  it("resolves a verb by keyword alias, not just its label", () => {
    expect(filterNavCommands(commands, ":mkdir").map((c) => c.id)).toContain("newFolder");
  });
});
