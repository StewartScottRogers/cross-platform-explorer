import { describe, it, expect } from "vitest";
import { detectContexts } from "./folderContext";
import type { DirEntry } from "./types";

const entry = (name: string, is_dir = false): DirEntry => ({
  name,
  path: `/proj/${name}`,
  is_dir,
  size: 0,
  modified: 0,
  extension: is_dir ? "" : (name.includes(".") ? name.split(".").pop()!.toLowerCase() : ""),
  hidden: name.startsWith("."),
});

const ids = (entries: DirEntry[]) =>
  detectContexts({ path: "/proj", entries }).map((c) => c.id);

describe("folder-context recognizers (CPE-235/239)", () => {
  it("aggregates multiple contexts for one folder", () => {
    const got = ids([entry(".git", true), entry("package.json"), entry("Cargo.toml")]);
    expect(got).toContain("git");
    expect(got).toContain("node");
    expect(got).toContain("rust");
  });

  it("Git uses the open-github action against the folder", () => {
    const ctx = detectContexts({ path: "/proj", entries: [entry(".git", true)] });
    const git = ctx.find((c) => c.id === "git")!;
    expect(git.actions[0].kind).toBe("open-github");
    expect(git.actions[0].target).toBe("/proj");
  });

  it("recognises a solution by extension and opens the marker file", () => {
    const ctx = detectContexts({ path: "/proj", entries: [entry("App.sln"), entry("App.csproj")] });
    const sln = ctx.find((c) => c.id === "vs-sln")!;
    expect(sln.actions[0].kind).toBe("open-path");
    expect(sln.actions[0].target).toBe("/proj/App.sln");
  });

  it("offers Web page only for a standalone index.html (not a bundler entry)", () => {
    expect(ids([entry("index.html")])).toContain("web-static");
    expect(ids([entry("index.html"), entry("package.json")])).not.toContain("web-static");
  });

  it("a plain folder matches nothing", () => {
    expect(ids([entry("readme.txt"), entry("photo.png"), entry("sub", true)])).toHaveLength(0);
  });

  it("detects a directory-marker context (Unity)", () => {
    expect(ids([entry("Assets", true), entry("ProjectSettings", true)])).toContain("unity");
  });
});
