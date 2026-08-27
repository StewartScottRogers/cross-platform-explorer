// CPE-1945: the repo has TWO npm projects (the root and `gui-smoke/`), and every Dependency Steward
// pass up to 2026-08-27 audited one of them and reported its number as the repo's. `gui-smoke/` had
// never been audited once; it was carrying 17 advisories including the same `brace-expansion` high the
// root pass had just fixed, in a project `gui-smoke.yml` runs on CI.
//
// `scripts/audit-npm-projects.mjs` closes that by sweeping every project `git ls-files` finds. These
// tests guard the sweep's own foundations — the parts that, if they quietly broke, would turn it into a
// green job that checks nothing:
//
//   1. Discovery really finds every tracked `package-lock.json` (compared against an INDEPENDENT
//      `git ls-files`, not the script's own call).
//   2. The count has not dropped below the floor, and — the tripwire — a NEW npm project fails this
//      test until someone acknowledges it. Discovery is dynamic so the sweep covers a third project
//      automatically; this assertion exists so a human also finds out, since "how many npm projects
//      are there" is the question nobody asked.
//   3. ci.yml still wires the sweep up. A guard nothing runs is not a guard.
//
// Same class as `sectionDocs.test.ts` and `epicsQueueLayout.test.ts`: cheap, deterministic, offline —
// no `npm audit` here, no registry access, no network. The audit itself belongs in CI.
import { describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
// Imported from the plain-JS sweep script itself, not re-implemented here: that script is the single
// implementation of the enumeration, so this test pins the real thing rather than a TS copy that could
// drift out of agreement with what CI actually runs. (`npm run check` type-checks the .mjs through
// this import, which is why it carries JSDoc types.)
import { discoverNpmProjects, MIN_EXPECTED_NPM_PROJECTS } from "../../scripts/audit-npm-projects.mjs";

const REPO_ROOT = process.cwd();

/** Every npm project directory ("" = root), enumerated independently of the script under test. */
function trackedNpmProjects(): string[] {
  return execFileSync("git", ["ls-files", "*package-lock.json"], { cwd: REPO_ROOT, encoding: "utf8" })
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    .map((lock) => {
      const slash = lock.lastIndexOf("/");
      return slash === -1 ? "" : lock.slice(0, slash);
    })
    .sort();
}

/**
 * The npm projects known to this repo as of CPE-1945. Deliberately a literal.
 *
 * This is NOT how the sweep discovers projects — that is dynamic, so a third project is audited the
 * day it lands with nothing here to update. This literal is the tripwire for the human half: adding an
 * npm project reds this test, which is the moment to ask whether the Steward's picture of the repo,
 * and any statement of "the repo's advisory count", still holds. Update it in the same commit that
 * adds the project.
 */
const KNOWN_NPM_PROJECTS = ["", "gui-smoke"];

describe("npm project enumeration (CPE-1945)", () => {
  it("discovers exactly the tracked package-lock.json set", () => {
    expect(discoverNpmProjects(REPO_ROOT)).toEqual(trackedNpmProjects());
  });

  it("finds at least the floor the sweep refuses to run below", () => {
    expect(discoverNpmProjects(REPO_ROOT).length).toBeGreaterThanOrEqual(MIN_EXPECTED_NPM_PROJECTS);
  });

  it("matches the known project list — a new npm project must be acknowledged here", () => {
    expect(discoverNpmProjects(REPO_ROOT)).toEqual(KNOWN_NPM_PROJECTS);
  });

  it("every project with a lockfile also has a package.json", () => {
    for (const dir of discoverNpmProjects(REPO_ROOT)) {
      expect(existsSync(join(REPO_ROOT, dir, "package.json")), `${dir || "<root>"}/package.json`).toBe(true);
    }
  });

  it("ci.yml still runs the sweep", () => {
    const ci = readFileSync(join(REPO_ROOT, ".github", "workflows", "ci.yml"), "utf8");
    expect(ci).toContain("scripts/audit-npm-projects.mjs");
  });
});
