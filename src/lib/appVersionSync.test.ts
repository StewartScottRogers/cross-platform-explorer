// CPE-1904 — the app version lives in SIX places across FIVE files, and until this guard nothing in a
// normal push, PR or local run compared them.
//
// CLAUDE.md's "Versioning — keep five files in sync" records the real incident in the repo's own voice:
// `package-lock.json` had been **three releases behind** (`0.57.64` vs `0.57.67`), observed 2026-08-20.
// It also names the mechanism, and the mechanism is what makes this a *fail-open* defect rather than an
// ordinary oversight: nothing fails when they drift, so the drift surfaces as "a dirty working tree the
// moment anyone runs `npm install` or a local `cargo build` — which reads as unrelated noise and gets
// committed by accident or discarded along with real work".
//
// ## What was measured before this file existed (2026-08-27, on b5658d93)
//
// With BOTH of `package-lock.json`'s version fields deliberately drifted (root `"version"` → 0.57.66,
// `packages[""]."version"` → 0.57.64, against a `package.json` of 0.57.69):
//
//   * `npm ci`                → **exit 0**, "added 191 packages", no warning of any kind.
//   * `npm test`              → **exit 0**, 349 files / 5003 passed / 2 skipped.
//   * `npm run check`         → **exit 0**, "0 errors and 0 warnings".
//   * `npm install --package-lock-only` → **exit 0**, and it SILENTLY REPAIRED both fields. That is the
//     laundering step CLAUDE.md describes: the evidence of the drift is destroyed by the same command
//     that reveals it, and what is left behind is a dirty working tree with no message attached.
//
// `npm ci` is not simply blind, and the distinction is the reason this check has to exist separately
// rather than as a stricter invocation of something already running. Add `left-pad` to `package.json`
// without touching the lockfile and `npm ci` exits **1**:
//
//     npm error `npm ci` can only install packages when your package.json and package-lock.json …
//     npm error Missing: left-pad@1.3.0 from lock file
//
// So it does enforce the **dependency graph** — loudly, and it names what is missing. It just does not
// look at the `version` fields at all, and the two properties are orthogonal: the recorded incident was
// a lockfile whose graph was perfectly consistent and whose version was three releases old. No npm flag
// closes that; a separate check does.
//
// The fifth file is a different story and was measured too. `src-tauri/Cargo.lock` with its
// `cross-platform-explorer` entry drifted to 0.57.66 makes `cargo metadata --locked` exit **101** —
// CPE-1865's `--locked` and CPE-1932's `lockfile-preflight` really do back it. It is covered here as
// well anyway, for three reasons: the cargo failure needs a Rust toolchain and an hour-long matrix to
// be reached, its message names neither the version field nor a fix ("remove the `--locked` flag and
// use `--offline` instead" is precisely the newcomer trap CPE-1855's UAT flagged), and a guard that
// enumerates five of the six places is the same defect with extra steps.
//
// ## Why a vitest guard, and what the alternatives would have bought
//
// **Chosen: a vitest guard.** It runs on every push and PR inside ci.yml's `frontend` job (`npm test`),
// AND on every local `npm test` — which matters more than it looks, because the drift is *introduced*
// and then *laundered* locally, before anything is pushed. It needs no toolchain, costs ~200ms, and it
// is the shape this repo already uses for exactly this class of check (`lockfileLockedGuard.test.ts`,
// `npmProjects.test.ts`, `msrvSync.test.ts`, `sectionDocs.test.ts`).
//
// **A `--locked`-style build failure** was the obvious candidate and is the one this ticket names first.
// It was tried and it does not exist on the npm side: `npm ci` IS npm's `--locked`, it is already what
// CI runs, and the measurement above is it exiting 0 on a three-release drift. npm treats the lockfile's
// `version` fields as metadata to be rewritten, not as a constraint to be honoured, so there is no
// stricter invocation to reach for. It would have bought a failure at the earliest possible moment and
// on every consumer of the lockfile; it is simply not on offer. (For `src-tauri/Cargo.lock` it *is* on
// offer and already landed — see above.)
//
// **A dedicated node-only CI job**, alongside `ratchet-guard` and `npm-audit-sweep`, would have bought
// two things: its own red X in the PR checks list (more visible than one failing test inside
// `frontend`), and independence from `npm ci` succeeding first. Neither is worth a third job here. The
// second is moot — version drift provably does not break `npm ci`, so the `frontend` lane is reachable
// for exactly the defect at hand — and the first costs the local run, which is where this guard earns
// its keep. If the failure message below ever proves too easy to miss in a 5000-test log, promoting
// this to a job is a five-line change; the enumeration and the verdict are exported for that reason.
//
// ## Deriving the file list rather than recalling it (CPE-1932)
//
// The five files are NOT hard-coded. `git ls-files` supplies the candidate set and every candidate is
// keyed on the app's own package IDENTITY — the `name` seeded from the npm project root — so
// `gui-smoke/package.json` (`cpe-gui-smoke`), `gui-smoke/package-lock.json`, and the sixteen other
// `Cargo.lock`s that do not pin this app are excluded by what they say about themselves, not by a path
// this file happens to know. Sixteen of the seventeen tracked `Cargo.lock`s are skipped that way, and a
// seventeenth that started pinning `cross-platform-explorer` would be picked up the day it landed.
//
// `tauri.conf.json` is the one family that cannot be identity-keyed: Tauri's config carries
// `productName`/`identifier` but no package `name`, so it is matched on filename and that is stated
// here rather than papered over. Every Tauri config in this tree describes this app.
//
// The enumeration is guarded on both sides. `MIN_VERSION_PLACES` refuses a near-empty sweep (a green
// "0 of 0 places agree" is the exact failure this file exists to stop), and `KNOWN_VERSION_PLACES` is
// the human tripwire — the same two-layer shape `npmProjects.test.ts` uses: discovery stays dynamic so
// a sixth place is CHECKED automatically, and the literal reds so a sixth place is also NOTICED.
//
// ## Fail closed
//
// Every reader here throws on a file it cannot read or parse, naming the file and the reason. A guard
// that skips what it cannot understand reports "all six places agree" while checking four. All 34
// tracked Cargo manifests parse with the app's own `parseToml` today, measured at 55ms for the set, so
// strictness is affordable as well as correct.
//
// Measured on the REAL tree, not only on a fixture: truncating `src-tauri/Cargo.lock` mid-entry gives
//
//     Error: src-tauri/Cargo.lock: did not parse as TOML (Line 1609: expected end of line)
//     Tests  no tests        vitest exit 1
//
// — a hard, named failure at collection time. Not "5 of 5 places agree".
//
// ## Red-proofs, and their results
//
// Each of the six places was drifted **in the real working tree** to 0.57.66 (the shape of the recorded
// incident) and this file re-run, then the tree restored and verified byte-identical with `cmp`:
//
//   package.json               "version"                   → 1 failed / 18 passed; names all FIVE others
//   package-lock.json          root "version"              → 1 failed / 18 passed; names it alone
//   package-lock.json          packages[""]."version"      → 1 failed / 18 passed; names it alone
//   src-tauri/Cargo.toml       version in [package]        → 1 failed / 18 passed; names it alone
//   src-tauri/Cargo.lock       [[package]] entry           → 1 failed / 18 passed; names it alone
//   src-tauri/tauri.conf.json  "version"                   → 1 failed / 18 passed; names it alone
//
// Two things in that table were earned rather than assumed, and both are worth knowing:
//
//   * **"1 failed", not "8 failed".** The first round produced eight, because the fixture-based tests
//     copied the drifted working tree and faithfully reproduced the sabotage. `syncedFixture` below
//     normalises the fixture first, so a real drift gives the developer ONE clear failure instead of a
//     wall of collateral noise to read past.
//   * **The `package.json` row nearly read as a pass.** The harness aimed its `sed` at line 3; the
//     version is on line 4. The edit was a silent no-op, the run came back green, and it looked exactly
//     like the guard failing to fire — a fail-open red-proof, inside the ticket about fail-open guards.
//     A red-proof that does not verify its own sabotage landed proves nothing.
import { describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { tmpdir } from "node:os";
import { parseToml } from "./preview/toml";

const ROOT = process.cwd();

/** One concrete place in one file that carries the app's own version. */
export interface VersionPlace {
  /** Repo-relative, forward-slashed. */
  file: string;
  /** How to find the field inside that file, in the words a human would use to look for it. */
  where: string;
  /** The version literal actually sitting there. */
  version: string;
  /**
   * Which copy of that literal this is within its own file, counting from the top — 0 for the first.
   * Recorded at discovery, in the order the fields are read, because `package-lock.json` carries the
   * version twice and the two lines are textually identical. The red-proofs below use it to sabotage
   * one field without touching the other; nothing in the guard's own verdict depends on it.
   */
  occurrence: number;
  /** The cheapest command that puts this particular place back in sync. */
  fix: string;
}

/**
 * Places found in the working tree today. A literal on purpose, and NOT how discovery works — see the
 * header. Adding a sixth place reds this test, which is the moment to ask whether CLAUDE.md's
 * "keep five files in sync" section and `scripts/release.ps1`'s bump plan still describe the repo.
 * Update it in the same commit that adds the place.
 */
const KNOWN_VERSION_PLACES: ReadonlyArray<{ file: string; where: string }> = [
  { file: "package-lock.json", where: 'root "version"' },
  { file: "package-lock.json", where: 'packages[""]."version"' },
  { file: "package.json", where: '"version"' },
  { file: "src-tauri/Cargo.lock", where: 'version in the [[package]] entry named "cross-platform-explorer"' },
  { file: "src-tauri/Cargo.toml", where: "version in [package]" },
  { file: "src-tauri/tauri.conf.json", where: '"version"' },
];

/**
 * The sweep refuses to render a verdict on fewer places than this. Set at the real count rather than
 * comfortably below it: unlike npm projects or crates, version places do not come and go — the set has
 * been these six for the life of the repo, and CLAUDE.md pins them by name. If one legitimately
 * disappears, lowering this is a deliberate edit made in the same diff, with the reason in the commit.
 */
const MIN_VERSION_PLACES = 6;

/** Fix advice, kept next to the places it applies to so a message can never name a command for the
 *  wrong file. Each was run against a real drift on 2026-08-27 before being written down here. */
const FIX_ALL = "pwsh -File scripts/release.ps1 -Version <version> -BumpOnly   (rewrites every place at once)";
const FIX_NPM_LOCK = "npm install --package-lock-only";

/**
 * The value the red-proofs write into one place to check the guard notices.
 *
 * Deliberately not a plausible version. The first round used `0.0.1`, which is a REAL dependency pin
 * inside `package-lock.json` (3 copies) and `src-tauri/Cargo.lock` (1 copy) — the fixture helper's
 * "which literal is which" check refused to guess, correctly, and the red-proofs went red for the
 * wrong reason. A sabotage value has to be one that cannot already be in the file.
 */
const DRIFTED = "0.0.0-cpe1904-sabotage";

class VersionPlaceError extends Error {}

/** Reads a tracked file, failing closed with the path in the message. */
function readTracked(root: string, file: string): string {
  try {
    return readFileSync(join(root, file), "utf8");
  } catch (e) {
    throw new VersionPlaceError(`${file}: could not be read (${(e as Error).message})`);
  }
}

function readJson(root: string, file: string): Record<string, unknown> {
  const raw = readTracked(root, file);
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (e) {
    throw new VersionPlaceError(`${file}: is not valid JSON (${(e as Error).message})`);
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new VersionPlaceError(`${file}: parsed to ${Array.isArray(parsed) ? "an array" : typeof parsed}, expected an object`);
  }
  return parsed as Record<string, unknown>;
}

function readToml(root: string, file: string): Record<string, unknown> {
  const result = parseToml(readTracked(root, file));
  if (!result.ok) throw new VersionPlaceError(`${file}: did not parse as TOML (${result.error})`);
  return result.value;
}

/** A version literal, or a fail-closed throw. Never a silent skip and never an empty string. */
function requireVersion(file: string, where: string, value: unknown): string {
  if (typeof value !== "string" || !/^\d+\.\d+\.\d+/.test(value)) {
    throw new VersionPlaceError(
      `${file}: ${where} is ${value === undefined ? "missing" : JSON.stringify(value)}, ` +
        `which is not a version literal. This guard cannot compare what it cannot read, so it fails ` +
        `rather than skipping the field.`,
    );
  }
  return value;
}

/**
 * The app's own identity and the version every other place must agree with.
 *
 * Seeded from the npm project root's `package.json`. That single path is the one thing not derived, and
 * it cannot be: npm itself defines the root manifest's location, `package.json`'s `version` is the field
 * `npm install` propagates into the lockfile, and `scripts/release.ps1` treats it as file 1 of 5. Every
 * other file in the sweep is then found by asking it who it is.
 */
export function appIdentity(root: string): { name: string; version: string } {
  const pkg = readJson(root, "package.json");
  const name = pkg.name;
  if (typeof name !== "string" || name.length === 0) {
    throw new VersionPlaceError(`package.json: "name" is ${JSON.stringify(name)}; the app's identity is unreadable, so nothing can be matched against it`);
  }
  return { name, version: requireVersion("package.json", '"version"', pkg.version) };
}

/** `tauri.conf.json` and its per-platform siblings (`tauri.windows.conf.json`, …). */
const TAURI_CONF = /^tauri\.(.+\.)?conf\.json$/;

/**
 * Every place under `root` that carries the app's own version, given the tracked file list.
 *
 * `tracked` is a parameter rather than an internal `git ls-files` call so the red-proofs below can aim
 * the identical code at a throwaway fixture tree — the guard that runs in CI and the guard being
 * sabotaged are then provably the same function, not a copy of it.
 *
 * Throws on any candidate it cannot read, parse, or find a version literal in.
 */
export function appVersionPlaces(root: string, tracked: string[]): VersionPlace[] {
  const { name: app } = appIdentity(root);
  const out: VersionPlace[] = [];
  const seenInFile = new Map<string, number>();
  /** Stamps the file-order occurrence index before the list is sorted for display. */
  const push = (p: Omit<VersionPlace, "occurrence">): void => {
    const n = seenInFile.get(p.file) ?? 0;
    seenInFile.set(p.file, n + 1);
    out.push({ ...p, occurrence: n });
  };

  for (const file of tracked) {
    const base = basename(file);

    if (base === "package.json") {
      const json = readJson(root, file);
      if (json.name !== app) continue;
      push({ file, where: '"version"', version: requireVersion(file, '"version"', json.version), fix: FIX_ALL });
      continue;
    }

    if (base === "package-lock.json") {
      const json = readJson(root, file);
      if (json.name !== app) continue;
      push({ file, where: 'root "version"', version: requireVersion(file, 'root "version"', json.version), fix: FIX_NPM_LOCK });
      // The second place, and the one CLAUDE.md says gets missed "because it does not look like a
      // version field at a glance". It drifts independently of the first: `npm install` writes both,
      // but a hand edit, a merge resolution or a partial revert writes either.
      const packages = json.packages;
      if (packages === null || typeof packages !== "object" || Array.isArray(packages)) {
        throw new VersionPlaceError(`${file}: has no "packages" object, so the root package's own entry cannot be located`);
      }
      const rootEntry = (packages as Record<string, unknown>)[""];
      if (rootEntry === null || typeof rootEntry !== "object" || Array.isArray(rootEntry)) {
        throw new VersionPlaceError(`${file}: packages[""] is missing or is not an object; this lockfile does not describe its own root package`);
      }
      push({
        file,
        where: 'packages[""]."version"',
        version: requireVersion(file, 'packages[""]."version"', (rootEntry as Record<string, unknown>).version),
        fix: FIX_NPM_LOCK,
      });
      continue;
    }

    if (base === "Cargo.toml") {
      const toml = readToml(root, file);
      const pkg = toml.package;
      if (pkg === null || typeof pkg !== "object" || Array.isArray(pkg)) continue; // a virtual workspace manifest
      if ((pkg as Record<string, unknown>).name !== app) continue;
      push({
        file,
        where: "version in [package]",
        version: requireVersion(file, "version in [package]", (pkg as Record<string, unknown>).version),
        fix: FIX_ALL,
      });
      continue;
    }

    if (base === "Cargo.lock") {
      const toml = readToml(root, file);
      const packages = toml.package;
      if (!Array.isArray(packages)) continue; // a lockfile with no packages resolves nothing to check
      for (const entry of packages) {
        if (entry === null || typeof entry !== "object" || (entry as Record<string, unknown>).name !== app) continue;
        const where = `version in the [[package]] entry named ${JSON.stringify(app)}`;
        push({
          file,
          where,
          version: requireVersion(file, where, (entry as Record<string, unknown>).version),
          fix: `cargo check --locked --manifest-path ${dirname(file)}/Cargo.toml   (reports it; drop --locked to rewrite), or ${FIX_ALL}`,
        });
      }
      continue;
    }

    if (TAURI_CONF.test(base)) {
      const json = readJson(root, file);
      // Tauri falls back to the crate's version when `version` is absent, so an absent key is in sync
      // by construction. A key that is PRESENT and unreadable throws — see `requireVersion`.
      if (json.version === undefined) continue;
      push({ file, where: '"version"', version: requireVersion(file, '"version"', json.version), fix: FIX_ALL });
    }
  }

  return out.sort((a, b) => a.file.localeCompare(b.file) || a.where.localeCompare(b.where));
}

/**
 * The verdict. `""` means every place agrees; anything else is the failure message, and it names the
 * file, the field, both values, and what to run.
 */
export function describeDrift(places: VersionPlace[], expected: string): string {
  if (places.length < MIN_VERSION_PLACES) {
    return (
      `Only ${places.length} version place(s) were found, and this guard refuses to pass a verdict on ` +
      `fewer than ${MIN_VERSION_PLACES}. An enumeration that comes back near-empty is a silent all-green, ` +
      `not a clean bill of health. Check that the sweep is running from the repo root and that ` +
      `\`git ls-files\` still lists the manifests.`
    );
  }
  const drifted = places.filter((p) => p.version !== expected);
  if (drifted.length === 0) return "";

  const lines = [
    `The app version has drifted. package.json says ${expected}; ${drifted.length} of ${places.length} ` +
      `place(s) that must match it do not:`,
    "",
  ];
  for (const p of drifted) {
    lines.push(`  ${p.file}  —  ${p.where}`);
    lines.push(`      is       ${p.version}`);
    lines.push(`      expected ${expected}`);
    lines.push(`      fix:     ${p.fix}`);
  }
  lines.push(
    "",
    "This is CLAUDE.md's five-files rule. Nothing else fails when these drift — `npm ci`, `npm test` and",
    "`npm run check` all exit 0 on a three-release-stale lockfile, and `npm install` silently repairs it,",
    "leaving only a dirty working tree that reads as unrelated noise. That is how package-lock.json ended",
    "up three releases behind (0.57.64 vs 0.57.67, observed 2026-08-20). This test is the backstop.",
  );
  return lines.join("\n");
}

/** Every file git tracks, forward-slashed. */
function trackedFiles(root: string): string[] {
  const out = execFileSync("git", ["ls-files"], { cwd: root, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 })
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    .map((l) => l.split("\\").join("/"));
  if (out.length < 100) {
    throw new VersionPlaceError(`git ls-files returned only ${out.length} path(s) from ${root}; the enumeration did not run`);
  }
  return out;
}

/**
 * Copies just the version-bearing files into a throwaway tree, preserving relative paths, and returns
 * the root plus the file list. Fixtures live in an OS temp directory, never in the repo — same
 * convention as `releaseVersionBump.test.ts` and `mojibakeGuard.test.ts`.
 */
function fixtureTree(files: string[]): { root: string; cleanup: () => void } {
  const root = mkdtempSync(join(tmpdir(), "cpe-1904-"));
  for (const f of files) {
    const dest = join(root, f);
    mkdirSync(dirname(dest), { recursive: true });
    writeFileSync(dest, readFileSync(join(ROOT, f)));
  }
  return { root, cleanup: () => rmSync(root, { recursive: true, force: true }) };
}

/**
 * Rewrites exactly the one version literal this place refers to, leaving the file byte-identical
 * otherwise — so a red-proof of `packages[""]."version"` cannot accidentally be a red-proof of the
 * root `"version"` sitting six lines above it.
 *
 * Fails closed rather than sabotaging the wrong field: if the file contains more copies of the version
 * literal than the sweep found places in it, the occurrence index no longer identifies a field and the
 * helper says so instead of guessing.
 */
function driftPlace(root: string, place: VersionPlace, places: VersionPlace[], to: string): void {
  // The places in the same file that currently read the same literal, in file order. Only these are
  // indistinguishable from each other in the text, so only these have to be counted past.
  const siblings = places
    .filter((p) => p.file === place.file && p.version === place.version)
    .sort((a, b) => a.occurrence - b.occurrence);
  const index = siblings.findIndex((p) => p.where === place.where);
  expect(index, `${place.where} is not among the places discovered in ${place.file}`).toBeGreaterThanOrEqual(0);

  const path = join(root, place.file);
  const raw = readFileSync(path, "utf8");
  const pattern = new RegExp(place.version.replace(/\./g, "\\."), "g");
  const total = raw.match(pattern)?.length ?? 0;
  expect(
    total,
    `${place.file} contains ${total} copies of ${place.version} but ${siblings.length} discovered ` +
      `place(s) claim that literal; the fixture cannot tell which one is which`,
  ).toBe(siblings.length);

  let seen = -1;
  const next = raw.replace(pattern, (m) => {
    seen++;
    return seen === index ? to : m;
  });
  expect(next, `fixture drift of ${place.file} (${place.where}) changed nothing`).not.toBe(raw);
  writeFileSync(path, next);
}

/**
 * A fixture tree holding every version-bearing file, normalised so that every place reads `expected`
 * before the test sabotages one of them.
 *
 * The normalisation is the point, and it was added after the first round of red-proofs. Without it,
 * each of these tests inherits whatever the working tree currently says — so drifting one real place
 * to check the guard fires made **eight** tests in this file red instead of one, the seven extras being
 * the fixtures faithfully reproducing the sabotage they were built on. The guard is about the
 * MECHANISM, not about the tree it happens to be run in; when a real drift lands, the developer should
 * get one clear failure ("every place agrees with package.json") and not a wall of collateral noise
 * they have to read past to find it.
 */
function syncedFixture(files: string[], expected: string): { root: string; cleanup: () => void } {
  const { root, cleanup } = fixtureTree(files);
  try {
    // Re-read after every write: rewriting one literal changes the `version` recorded for that place,
    // and the next write has to search for the value that is there now.
    for (let guard = 0; guard < files.length * 4; guard++) {
      const places = appVersionPlaces(root, files);
      const off = places.find((p) => p.version !== expected);
      if (!off) return { root, cleanup };
      driftPlace(root, off, places, expected);
    }
    throw new Error("could not bring the fixture into sync — a version literal is resisting rewrite");
  } catch (e) {
    cleanup();
    throw e;
  }
}

describe("app version sync — the five files CLAUDE.md says must move together (CPE-1904)", () => {
  const tracked = trackedFiles(ROOT);
  const places = appVersionPlaces(ROOT, tracked);
  const { version: expected } = appIdentity(ROOT);
  /** The version-bearing files, deduplicated — `package.json` is both the seed and a place. */
  const versionFiles = [...new Set(places.map((p) => p.file))];

  it("discovers exactly the version places this repo is known to have", () => {
    expect(places.map((p) => ({ file: p.file, where: p.where }))).toEqual(
      [...KNOWN_VERSION_PLACES].sort((a, b) => a.file.localeCompare(b.file) || a.where.localeCompare(b.where)),
    );
  });

  it("refuses to render a verdict on a near-empty enumeration", () => {
    expect(places.length).toBeGreaterThanOrEqual(MIN_VERSION_PLACES);
    // The refusal itself, not just the count that avoids it.
    expect(describeDrift(places.slice(0, MIN_VERSION_PLACES - 1), expected)).toMatch(
      /refuses to pass a verdict on fewer than 6/,
    );
  });

  it("keys on the app's package identity, so the other npm project and the other 16 lockfiles are out", () => {
    // Derived, not asserted from memory: these ARE tracked and ARE of a family the sweep reads, and
    // they are excluded because of what they say their name is.
    const lockfiles = tracked.filter((f) => basename(f) === "Cargo.lock");
    expect(lockfiles.length).toBeGreaterThan(10);
    expect(new Set(places.map((p) => p.file))).not.toContain("gui-smoke/package.json");
    expect(new Set(places.map((p) => p.file))).not.toContain("gui-smoke/package-lock.json");
    expect(places.filter((p) => basename(p.file) === "Cargo.lock").map((p) => p.file)).toEqual(["src-tauri/Cargo.lock"]);
  });

  // ── The guard ──────────────────────────────────────────────────────────────────────────────────
  it("every place agrees with package.json", () => {
    expect(describeDrift(places, expected)).toBe("");
  });

  // ── Red-proofs. One per discovered place, driven off the discovery itself, so a sixth place is
  //    red-proofed the day it lands rather than the day someone remembers to add a case. ──────────
  describe("red-proof: drifting each place independently", () => {
    for (const place of places) {
      it(`reds on ${place.file} — ${place.where}`, () => {
        const { root, cleanup } = syncedFixture(versionFiles, expected);
        try {
          const synced = appVersionPlaces(root, versionFiles);
          const target = synced.find((p) => p.file === place.file && p.where === place.where);
          expect(target, `${place.where} vanished from the synced fixture`).toBeDefined();

          driftPlace(root, target!, synced, DRIFTED);
          const drifted = appVersionPlaces(root, versionFiles);
          const verdict = describeDrift(drifted, expected);

          expect(verdict).not.toBe("");
          expect(verdict).toContain(place.file);
          expect(verdict).toContain(place.where);
          expect(verdict).toContain(DRIFTED); // the value that is there
          expect(verdict).toContain(expected); // the value that should be
          expect(verdict).toContain(place.fix); // and what to run
          // Exactly one place moved, so exactly one place is named — the other five stay quiet.
          expect(drifted.filter((p) => p.version !== expected)).toHaveLength(1);
        } finally {
          cleanup();
        }
      });
    }
  });

  it("stays quiet when a dependency changes but no version does (the false-alarm case)", () => {
    const files = versionFiles;
    const { root, cleanup } = syncedFixture(files, expected);
    try {
      const lockPath = join(root, "package-lock.json");
      const lock = JSON.parse(readFileSync(lockPath, "utf8")) as Record<string, unknown>;
      const packages = lock.packages as Record<string, Record<string, unknown>>;
      // Exactly what adding a dependency does: a new entry, and a new dependency edge on the root —
      // a large lockfile diff that touches neither version field.
      packages["node_modules/cpe-1904-fixture-dep"] = { version: "9.9.9", resolved: "https://example.invalid/x.tgz", integrity: "sha512-x" };
      (packages[""].dependencies as Record<string, string>)["cpe-1904-fixture-dep"] = "^9.9.9";
      writeFileSync(lockPath, JSON.stringify(lock, null, 2) + "\n");

      expect(describeDrift(appVersionPlaces(root, files), expected)).toBe("");
    } finally {
      cleanup();
    }
  });

  // ── Fail closed. Each of these would be a silent skip in a guard that swallowed its own errors,
  //    and a silent skip here reads as "all six places agree" while checking five. ────────────────
  describe("fails closed on anything it cannot read", () => {
    const cases: Array<{ name: string; file: string; write: string | null; expected: RegExp }> = [
      {
        name: "a Cargo.lock that is not valid TOML",
        file: "src-tauri/Cargo.lock",
        write: '[[package]]\nname = "cross-platform-explorer"\nversion = """oops\n',
        expected: /src-tauri\/Cargo\.lock: did not parse as TOML/,
      },
      {
        name: "a package-lock.json that is not valid JSON",
        file: "package-lock.json",
        write: '{ "name": "cross-platform-explorer", ',
        expected: /package-lock\.json: is not valid JSON/,
      },
      {
        name: "a package-lock.json with no packages[\"\"] entry",
        file: "package-lock.json",
        write: '{ "name": "cross-platform-explorer", "version": "9.9.9", "packages": {} }',
        expected: /packages\[""\] is missing or is not an object/,
      },
      {
        name: "a tauri.conf.json whose version is present but unreadable",
        file: "src-tauri/tauri.conf.json",
        write: '{ "version": { "from": "package.json" } }',
        expected: /is not a version literal/,
      },
      {
        name: "a Cargo.toml whose version key is gone",
        file: "src-tauri/Cargo.toml",
        write: '[package]\nname = "cross-platform-explorer"\n',
        expected: /version in \[package\] is missing/,
      },
      {
        name: "a file that has vanished from under the enumeration",
        file: "src-tauri/Cargo.lock",
        write: null,
        expected: /src-tauri\/Cargo\.lock: could not be read/,
      },
    ];

    for (const c of cases) {
      it(`throws on ${c.name}`, () => {
        const files = versionFiles;
        const { root, cleanup } = fixtureTree(files);
        try {
          if (c.write === null) rmSync(join(root, c.file));
          else writeFileSync(join(root, c.file), c.write);
          expect(() => appVersionPlaces(root, files)).toThrow(c.expected);
        } finally {
          cleanup();
        }
      });
    }

    it("throws rather than reporting an empty sweep when the identity cannot be read", () => {
      const files = ["package.json"];
      const { root, cleanup } = fixtureTree(files);
      try {
        writeFileSync(join(root, "package.json"), '{ "version": "1.2.3" }');
        expect(() => appVersionPlaces(root, files)).toThrow(/the app's identity is unreadable/);
      } finally {
        cleanup();
      }
    });
  });

  it("names a fix command that actually exists (CPE-1933: derive, do not claim)", () => {
    // `FIX_ALL` tells a reader to run `scripts/release.ps1 -BumpOnly`. Read the script and check both
    // halves of that instruction, so the advice cannot rot into folklore while this file stays green.
    const script = readFileSync(join(ROOT, "scripts", "release.ps1"), "utf8");
    expect(FIX_ALL).toContain("scripts/release.ps1");
    expect(FIX_ALL).toContain("-BumpOnly");
    expect(script).toMatch(/\[switch\]\s*\$BumpOnly/);
    // And that it really is the all-places bump: every file this guard checks must appear in the
    // script's plan, or the "rewrites every place at once" claim is false.
    for (const file of new Set(places.map((p) => p.file))) {
      expect(script, `${file} is not in release.ps1's bump plan`).toContain(basename(file));
    }
  });
});
