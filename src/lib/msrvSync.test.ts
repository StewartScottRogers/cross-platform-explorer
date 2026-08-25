// CPE-1855: the declared MSRV is only honest if (a) every manifest in the tree agrees on the same
// number and (b) the CI job that actually compiles at that floor covers every one of them. The real
// enforcement — compiling with rustc pinned to exactly the declared version — runs in CI's `msrv` job
// (.github/workflows/ci.yml) and cannot be reproduced here without installing that exact toolchain.
// This test is the local, no-toolchain-required half: the same "a static list can silently go stale"
// ratchet `releaseVersionBump.test.ts` (CLAUDE.md's five-files list vs. release.ps1) and
// `epicsQueueLayout.test.ts` (folder vs. frontmatter) already use elsewhere in this repo, applied to
// Cargo manifests instead. A crate directory added later, or dropped from the CI job's coverage, fails
// this test on its own — nobody has to remember to update a hand-typed list.
import { describe, it, expect } from "vitest";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = process.cwd();
const CI_YML_PATH = join(ROOT, ".github", "workflows", "ci.yml");

/** Every subdirectory of `parent` that contains its own Cargo.toml — one entry per real,
 *  independently-buildable Rust crate. This repo deliberately keeps each crate standalone, out of any
 *  shared Cargo workspace (see `crates/server/Cargo.toml`'s own comment on why), so "has a Cargo.toml"
 *  is the correct, and only, membership test — there is no root workspace manifest to enumerate
 *  members from instead. */
function crateDirsUnder(parent: string): string[] {
  const parentPath = join(ROOT, parent);
  return readdirSync(parentPath)
    .filter((name) => {
      const full = join(parentPath, name);
      return statSync(full).isDirectory() && existsSync(join(full, "Cargo.toml"));
    })
    .map((name) => `${parent}/${name}`)
    .sort();
}

/** Every real Cargo manifest's directory in the tree: `crates/*`, `sidecar/*`, and `src-tauri` itself
 *  (which has no parent grouping directory, so it is listed directly rather than discovered). */
function allCrateDirs(): string[] {
  return [...crateDirsUnder("crates"), ...crateDirsUnder("sidecar"), "src-tauri"].sort();
}

/** The `rust-version = "…"` value declared in a manifest, or undefined if it declares none. */
function declaredRustVersion(crateDir: string): string | undefined {
  const toml = readFileSync(join(ROOT, crateDir, "Cargo.toml"), "utf8");
  const m = /^rust-version\s*=\s*"([^"]+)"/m.exec(toml);
  return m?.[1];
}

/** The text of the `msrv:` job in ci.yml, from its `  msrv:` header to the next line that starts a
 *  sibling top-level job (two-space indent, not a comment, not blank) or end of file. */
function msrvJobText(): string {
  const ci = readFileSync(CI_YML_PATH, "utf8");
  const start = ci.indexOf("\n  msrv:\n");
  expect(start, "ci.yml has no `  msrv:` job at all — CPE-1855's MSRV CI leg is missing").toBeGreaterThanOrEqual(0);
  const rest = ci.slice(start + 1);
  const nextJob = /\n {2}[A-Za-z][\w-]*:\n/.exec(rest.slice(1));
  const end = nextJob ? nextJob.index + 1 : rest.length;
  return rest.slice(0, end);
}

/** The rustc version the `msrv` job pins via `dtolnay/rust-toolchain@<version>` — deliberately NOT
 *  `@stable` (that's every other toolchain-install step in this file; pinning a real number is the
 *  whole point of this job). */
function pinnedToolchainVersion(jobText: string): string | undefined {
  const m = /dtolnay\/rust-toolchain@(\S+)/.exec(jobText);
  return m?.[1];
}

/** The directory list the job's `for dir in ... ; do` loop actually iterates, parsed the same way bash
 *  would split it: whitespace- and backslash-continuation–separated tokens. */
function loopedDirs(jobText: string): string[] {
  const m = /for dir in([\s\S]*?); do/.exec(jobText);
  expect(m, "the msrv job has no `for dir in ... ; do` loop in the shape this test expects").not.toBeNull();
  return m![1]
    .split(/[\s\\]+/)
    .map((s) => s.trim())
    .filter(Boolean)
    .sort();
}

describe("MSRV declaration + enforcement stay in sync (CPE-1855)", () => {
  const crateDirs = allCrateDirs();

  it("finds real Cargo manifests to check (the discovery itself isn't silently empty)", () => {
    expect(crateDirs.length).toBeGreaterThanOrEqual(17);
    expect(crateDirs).toContain("crates/server");
    expect(crateDirs).toContain("sidecar/host");
    expect(crateDirs).toContain("src-tauri");
  });

  it("every manifest declares a rust-version (none silently opts out)", () => {
    const missing = crateDirs.filter((d) => declaredRustVersion(d) === undefined);
    expect(missing, `these manifests declare no rust-version at all: ${missing.join(", ")}`).toEqual([]);
  });

  it("every manifest declares the SAME rust-version — one honest floor, not a per-crate guess", () => {
    const byVersion = new Map<string, string[]>();
    for (const dir of crateDirs) {
      const v = declaredRustVersion(dir) ?? "(none)";
      byVersion.set(v, [...(byVersion.get(v) ?? []), dir]);
    }
    expect(
      byVersion.size,
      `rust-version has drifted across manifests: ${JSON.stringify(Object.fromEntries(byVersion), null, 2)}`,
    ).toBe(1);
  });

  it("ci.yml's msrv job pins that EXACT version via dtolnay/rust-toolchain@<version>, not @stable", () => {
    const declared = declaredRustVersion("src-tauri");
    const pinned = pinnedToolchainVersion(msrvJobText());
    expect(pinned).toBeDefined();
    expect(pinned).not.toBe("stable");
    expect(pinned, "the msrv job's pinned toolchain and the manifests' declared rust-version have drifted apart").toBe(
      declared,
    );
  });

  it("ci.yml's msrv job checks EVERY real crate directory — no partial sweep", () => {
    const looped = loopedDirs(msrvJobText());
    expect(looped, "the msrv job's for-loop directory list has drifted from the real crate directories on disk").toEqual(
      crateDirs,
    );
  });
});
