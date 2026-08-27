#!/usr/bin/env node
// CPE-1945 — run `npm audit` across EVERY npm project in this repo, discovered rather than recalled.
//
// THE FAILURE THIS CLOSES
//   This repo has two independent npm projects: the root, and `gui-smoke/`. Every Dependency Steward
//   pass up to 2026-08-27 audited the root and stopped, then reported its number as the repo's
//   dependency position. `gui-smoke/` had never been audited once — it was carrying 17 advisories,
//   including the same `brace-expansion` high that CPE-1926 had just fixed at the root, in a project
//   `gui-smoke.yml` runs on CI with repo credentials.
//
//   The worker who ran that pass named the defect exactly:
//
//     "The failure wasn't that I skipped gui-smoke/, it's that I never asked how many npm projects
//      there were. I read 'run npm audit' and executed it where I happened to be standing."
//
//   That is CPE-1932 in a different costume. CPE-1932 was seventeen `Cargo.lock` files while the rule
//   was being applied to two, and its fix was ci.yml's `lockfile-preflight` job — which enumerates with
//   `git ls-files '*Cargo.lock'` and never a hardcoded list. This script is the npm half of that same
//   fix, and it is deliberately shaped like it.
//
//   Note what is NOT the fix here: writing "remember to also audit gui-smoke/" into the Steward's
//   procedure. A written procedure is precisely what failed — the Steward's charter already said
//   "runs cargo audit + npm audit", and a worker read that and ran it in one directory. The enumerate
//   step has to be executable, not remembered. It costs one `git ls-files`.
//
// WHAT THIS DOES
//   1. Discovers every tracked `package-lock.json` via `git ls-files` — never a hardcoded list, so a
//      third npm project is covered the day it lands with no step here to update.
//   2. Refuses to run on a near-empty enumeration (see MIN_EXPECTED_NPM_PROJECTS). A sweep that finds
//      nothing and reports "0 vulnerabilities across 0 projects" is a zero-enumeration false green —
//      the exact failure this file exists to stop.
//   3. Runs `npm audit --json` in each project directory. That reads the lockfile and queries the
//      registry; it needs NO `node_modules`, so the whole sweep costs seconds.
//   4. Prints per-project totals AND a repo-wide total explicitly labelled as a sum over N projects,
//      so the number can never again be quoted as repo-wide when it is one project's.
//   5. Fails when a project has an UNAPPLIED non-major fix — see the next block for why that is
//      measured rather than read off npm's `fixAvailable`.
//
// WHY THE FAILURE CONDITION IS MEASURED, NOT READ OFF `fixAvailable`
//   The obvious predicate is "`fixAvailable` is `true` or a non-major object". It is wrong, and
//   CPE-1945 caught it being wrong on the very tree that motivated this script. After the gui-smoke
//   fix landed, `@puppeteer/browsers` and `extract-zip` BOTH still reported `fixAvailable: true` while
//   `npm audit fix` was a proven no-op — `extract-zip`'s advisory range is `<=2.0.1` and 2.0.1 is its
//   latest release, so there is nothing to upgrade to and npm says otherwise anyway.
//
//   Shipping the optimistic predicate would have red-flagged CI on day one for an advisory nobody can
//   act on, and a guard that cries wolf on its first run is a guard that gets ignored by its third.
//   So the question "is there an unapplied non-major fix?" is answered by ASKING npm to produce one:
//   copy the project's `package.json` + `package-lock.json` into a scratch dir, run
//   `npm audit fix --package-lock-only` (resolve-only, no install, no `--force`), and compare. If the
//   lockfile moves, a real non-major fix was going begging. If it does not, there is nothing to do
//   regardless of what `fixAvailable` claims. `fixAvailable` is still reported, as information.
//
//   Advisories whose only remedy is a semver-major are reported but never fail this script: those are
//   a deliberate, tracked migration decision (CPE-1443 at the root, the `@wdio/*` 9.x line in
//   `gui-smoke/`), not something an unrelated PR should be blocked on.
//
// USAGE
//   node scripts/audit-npm-projects.mjs            # sweep; fail on an unapplied non-major fix
//   node scripts/audit-npm-projects.mjs --report   # sweep; always exit 0 (report-only)
//
// `discoverNpmProjects` and `MIN_EXPECTED_NPM_PROJECTS` are what `src/lib/npmProjects.test.ts` pins.

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Repo root — this file lives in `<root>/scripts/`. */
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * Scratch space for the `--package-lock-only` probe. Kept INSIDE the repo on purpose (this project's
 * standing rule: temp work lives in the working tree, not the system temp dir) and removed after each
 * probe. `.claude/` is already gitignored for scratch of this kind.
 */
const PROBE_ROOT = join(REPO_ROOT, ".claude", "tmp", "npm-audit-sweep");

/**
 * Enumeration sanity floor. If discovery finds fewer than this many npm projects, something about
 * discovery itself is broken (git missing, wrong working directory, a future refactor of this file)
 * and the sweep must fail loudly rather than pass vacuously.
 *
 * Set to 2 — today's real count — deliberately, not below it. Unlike the 17 `Cargo.lock` files, which
 * churn as crates come and go, an npm project is a heavyweight, rarely-added thing. Two is both the
 * current count and the count below which this script has demonstrably stopped doing its job, because
 * "1" is exactly the number the failing Steward passes were operating on.
 */
export const MIN_EXPECTED_NPM_PROJECTS = 2;

/**
 * Every npm project in the repo, as a directory path relative to the repo root ("" = the root
 * project), sorted. Discovered from git, never hardcoded.
 */
export function discoverNpmProjects(repoRoot = REPO_ROOT) {
  const out = execFileSync("git", ["ls-files", "*package-lock.json"], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  return out
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((lockfile) => {
      const dir = dirname(lockfile);
      return dir === "." ? "" : dir;
    })
    .sort();
}

/**
 * A human label for a project directory, since the root project's path is the empty string.
 * @param {string} dir
 */
export function projectLabel(dir) {
  return dir === "" ? "<root>" : dir;
}

/**
 * npm's own claim about an advisory: does it say a fix exists that needs no semver-major?
 *
 * npm encodes three shapes: `false` (no fix at all), `true` (fixable in place — no top-level
 * dependency has to change), or an object `{name, version, isSemVerMajor}` naming the top-level
 * package that would have to move. Only the last shape can be a major, and only when it says so.
 *
 * Reported for information only. See the header: this is optimistic and cannot be the failure
 * condition — `npm audit fix` being a no-op is measured instead.
 */
export function isNonMajorFixable(/** @type {any} */ vuln) {
  const fix = vuln?.fixAvailable;
  if (fix === false || fix === undefined) return false;
  if (fix === true) return true;
  return fix.isSemVerMajor !== true;
}

/**
 * Is this parsed `npm audit --json` output an actual audit REPORT, as opposed to npm's error shape?
 *
 * The distinction is the whole point — see the long comment inside `npmAuditJson`. Exported so
 * `src/lib/npmProjects.test.ts` can pin it against npm's real error payload offline, because the
 * version of this check that shipped first ("is it parseable JSON?") passed that payload happily.
 *
 * @param {any} report
 * @returns {boolean}
 */
export function isUsableAuditReport(report) {
  return !!report && typeof report === "object" && typeof report.metadata?.vulnerabilities === "object";
}

/**
 * Run `npm audit --json` in `cwd` and return the parsed report.
 * @param {string} cwd
 * @param {string} label
 * @returns {any}
 */
function npmAuditJson(cwd, label) {
  let stdout;
  try {
    // `npm audit` exits non-zero when it FINDS vulnerabilities, which is the normal case here — the
    // report is on stdout either way, so the exit code is NOT the signal. What makes output a real
    // report is `metadata.vulnerabilities`, checked below.
    stdout = execFileSync("npm", ["audit", "--json"], {
      cwd,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      shell: process.platform === "win32",
    });
  } catch (err) {
    stdout = /** @type {any} */ (err).stdout ?? "";
  }

  let report;
  try {
    report = JSON.parse(stdout);
  } catch {
    report = undefined;
  }

  // THE CHECK THAT MUST NOT BE WEAKENED BACK TO "is it parseable JSON?".
  //
  // That is what this function did when CPE-1945 first shipped, and it was a false green with the
  // right error message wired to a condition that could never fire: npm's `--json` FAILURE path emits
  // perfectly well-formed JSON —
  //
  //     $ npm audit --json --registry=http://127.0.0.1:9/
  //     { "message": "request to ... failed, reason: connect ECONNREFUSED", "error": {...} }
  //
  // — so `JSON.parse` succeeded, `metadata` was simply absent, the totals summed as zero, and the
  // sweep printed "0 vulnerabilities across 2 npm projects" and exited 0. A flaky registry is the
  // single most likely failure mode of this job on CI, so that is not a corner case; it is the case.
  //
  // Worse, with one project's lockfile corrupt (merge-conflict markers — a realistic state) the same
  // hole re-emitted CPE-1945 ITSELF: the broken project silently contributed `{}` while the surviving
  // project's ROOT-ONLY number was printed as the repo-wide sum. The exact defect this file exists to
  // close, reproduced by the guard meant to close it.
  //
  // `metadata.vulnerabilities` is present in every successful audit and absent from every failure
  // shape, so it — not parseability — is what separates "audited, clean" from "did not audit".
  if (!isUsableAuditReport(report)) {
    throw new Error(
      `npm audit did not return a usable report for ${label} (no metadata.vulnerabilities). This is an ` +
        `audit FAILURE (no registry access? unreadable or corrupt lockfile?), not a clean project — ` +
        `treating it as "no vulnerabilities" would be a false green, and summing it into a repo-wide ` +
        `total would misreport one project's number as the repo's.\n` +
        `First 500 chars of output:\n${String(stdout).slice(0, 500)}`,
    );
  }
  return report;
}

/**
 * Ask npm whether a non-major fix is actually available and unapplied, by having it try.
 *
 * Non-destructive: works on a copy, and `--package-lock-only` never installs. Returns the names the
 * fix would clear (empty when `npm audit fix` would change nothing).
 */
function unappliedNonMajorFix(/** @type {string} */ dir) {
  const src = join(REPO_ROOT, dir);
  const probe = join(PROBE_ROOT, dir === "" ? "root" : dir.replace(/[\\/]/g, "_"));
  try {
    rmSync(probe, { recursive: true, force: true });
    mkdirSync(probe, { recursive: true });
  } catch (err) {
    // Fail CLOSED, and with a message rather than a raw ENOTDIR stack trace: an unusable scratch dir
    // means the probe cannot run, and a probe that cannot run must never be reported as "nothing to
    // fix here".
    throw new Error(
      `could not create the scratch dir for ${projectLabel(dir)} at ${probe}: ` +
        `${/** @type {any} */ (err).message}. The "is a non-major fix available?" probe cannot run ` +
        `without it, and skipping the probe would understate what needs doing.`,
    );
  }
  try {
    copyFileSync(join(src, "package.json"), join(probe, "package.json"));
    copyFileSync(join(src, "package-lock.json"), join(probe, "package-lock.json"));
    const before = readFileSync(join(probe, "package-lock.json"), "utf8");
    const beforeNames = new Set(Object.keys(npmAuditJson(probe, `${projectLabel(dir)} (probe)`).vulnerabilities ?? {}));

    // NO `--force`: accepting a semver-major has to be structurally impossible here, not merely
    // avoided by convention. Measured on a scratch copy, `--force` here downgrades @wdio/local-runner
    // 9.31.4 -> 7.40.0, @wdio/cli 9.31.4 -> 8.14.6, webdriverio -> 8.14.6 and @wdio/mocha-framework
    // -> 8.14.0, REWRITES package.json's pins, leaves an incoherent v7/v8/v9 mix (@wdio/types stays
    // at 9.29.1), and takes gui-smoke from 15 advisories to 29.
    let fixStderr = "";
    try {
      execFileSync("npm", ["audit", "fix", "--package-lock-only", "--no-fund", "--no-audit"], {
        cwd: probe,
        encoding: "utf8",
        maxBuffer: 64 * 1024 * 1024,
        shell: process.platform === "win32",
      });
    } catch (err) {
      // `npm audit fix` exits non-zero merely because vulnerabilities REMAIN, which is the normal
      // case here — measured, that path writes nothing at all to stderr. A genuine failure (registry
      // unreachable, unresolvable tree) is what puts npm's own `npm error` marker there. Swallowing
      // both alike would report a FAILED probe as "no unapplied fix", an understatement in exactly
      // the direction this script exists to prevent.
      fixStderr = String(/** @type {any} */ (err).stderr ?? "");
    }
    if (/npm error/i.test(fixStderr)) {
      throw new Error(
        `npm audit fix --package-lock-only FAILED for ${projectLabel(dir)} rather than merely leaving ` +
          `vulnerabilities behind, so whether a non-major fix is available is UNKNOWN, not "none".\n` +
          `npm stderr:\n${fixStderr.slice(0, 500)}`,
      );
    }

    if (readFileSync(join(probe, "package-lock.json"), "utf8") === before) return [];
    const afterNames = new Set(Object.keys(npmAuditJson(probe, `${projectLabel(dir)} (probe, fixed)`).vulnerabilities ?? {}));
    return [...beforeNames].filter((n) => !afterNames.has(n)).sort();
  } finally {
    rmSync(probe, { recursive: true, force: true });
  }
}

function main() {
  const reportOnly = process.argv.includes("--report");
  const projects = discoverNpmProjects();
  process.on("exit", () => rmSync(PROBE_ROOT, { recursive: true, force: true }));

  console.log(`Found ${projects.length} npm project(s) via 'git ls-files *package-lock.json':`);
  for (const dir of projects) console.log(`  ${projectLabel(dir)}`);
  console.log();

  if (projects.length < MIN_EXPECTED_NPM_PROJECTS) {
    console.error(
      `::error::only ${projects.length} npm project(s) found via 'git ls-files *package-lock.json' -- ` +
        `expected at least ${MIN_EXPECTED_NPM_PROJECTS}. This almost always means enumeration itself is ` +
        `broken (git missing or misconfigured, wrong working directory) rather than a project actually ` +
        `having been removed. A sweep that finds nothing and reports success is exactly the failure ` +
        `CPE-1945 exists to prevent -- fix the enumeration, don't relax this floor to make it pass.`,
    );
    process.exit(1);
  }

  /** @type {Record<string, number>} */
  const totals = { info: 0, low: 0, moderate: 0, high: 0, critical: 0, total: 0 };
  /** @type {{ dir: string, names: string[] }[]} */
  const actionable = [];

  for (const dir of projects) {
    if (!existsSync(join(REPO_ROOT, dir, "package.json"))) {
      console.error(`::error::${projectLabel(dir)} has a package-lock.json but no package.json`);
      process.exit(1);
    }

    const report = npmAuditJson(join(REPO_ROOT, dir), projectLabel(dir));
    const meta = report.metadata?.vulnerabilities ?? {};
    const vulns = report.vulnerabilities ?? {};

    for (const key of Object.keys(totals)) totals[key] += meta[key] ?? 0;

    const claimedNonMajor = Object.entries(vulns).filter(([, v]) => isNonMajorFixable(v)).map(([n]) => n).sort();
    const majorOnly = Object.entries(vulns).filter(([, v]) => !isNonMajorFixable(v)).map(([n]) => n).sort();
    const unapplied = unappliedNonMajorFix(dir);

    console.log(`::group::npm audit -- ${projectLabel(dir)}`);
    console.log(`  totals: ${JSON.stringify(meta)}`);
    console.log(`  npm claims fixable without a major (${claimedNonMajor.length}): ${claimedNonMajor.join(", ") || "(none)"}`);
    console.log(`  needs a semver-major or has no fix (${majorOnly.length}): ${majorOnly.join(", ") || "(none)"}`);
    console.log(`  UNAPPLIED non-major fix, measured (${unapplied.length}): ${unapplied.join(", ") || "(none -- npm audit fix is a no-op here)"}`);
    console.log("::endgroup::");

    if (unapplied.length > 0) actionable.push({ dir, names: unapplied });
  }

  console.log();
  console.log(
    `Repo-wide total across all ${projects.length} npm project(s) (a SUM, not any one project's ` +
      `number): ${JSON.stringify(totals)}`,
  );
  console.log();

  if (actionable.length === 0) {
    console.log(
      "No npm project has an unapplied non-major fix. Anything still listed above either needs a " +
        "tracked semver-major migration (CPE-1443 at the root) or is gated behind an upstream " +
        "dependency range -- in gui-smoke/, WebdriverIO's and mocha's own pins. See CPE-1443's Notes.",
    );
    return;
  }

  for (const { dir, names } of actionable) {
    console.error(
      `::error::${projectLabel(dir)}: \`npm audit fix\` would clear ${names.length} advisory/advisories ` +
        `without a semver-major, and has not been run: ${names.join(", ")}`,
    );
  }
  console.error(
    "\nTo fix: run `npm audit fix` (NEVER `--force`) in each project named above and commit that " +
      "project's package-lock.json. `--force` accepts semver-majors, which is a migration decision " +
      "belonging in its own reviewed ticket, not in a supply-chain patch.\n" +
      "Advisories needing a major are NOT counted here and do not fail this job.",
  );

  if (!reportOnly) process.exit(1);
}

// Only sweep when run as a CLI. `src/lib/npmProjects.test.ts` imports the enumeration helpers above,
// and that test must stay offline and instant — an unconditional `main()` here would fire a full
// registry-hitting audit on import.
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (err) {
    // Every throw in this file means "the sweep could not establish the facts" — a dead registry, a
    // corrupt lockfile, an unusable scratch dir. Each must FAIL CLOSED and say why in a line CI will
    // surface, rather than dying in a raw stack trace or, far worse, being mistaken for a clean run.
    console.error(`::error::npm audit sweep FAILED -- it did not audit, which is not the same as ` +
      `finding nothing: ${/** @type {any} */ (err).message}`);
    process.exit(1);
  }
}
