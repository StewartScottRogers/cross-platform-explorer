// CPE-1967 — every job in every workflow must declare its own `timeout-minutes`.
//
// ## What went wrong
//
// `ci.yml` had ELEVEN jobs and only one (`ci-verdict`) carried a job-level cap. The other ten had
// step-level timeouts at best, which bound a step and say nothing about a job wedged between steps,
// in `actions/cache`, or on a runner that stopped answering. Those ten therefore sat under the GitHub
// Actions default of **360 minutes**.
//
// CORRECTION, and it is the point. The first version of this docblock said "TEN jobs and not one of
// them carried a job-level cap". Both halves were wrong, and wrong in the exact way this file exists
// to prevent: the "ten" was carried forward from the ticket's prose instead of re-derived from the
// file, which is CLAUDE.md's CPE-1932 — *enumerate, don't recall* — failing inside the PR whose whole
// thesis is that enumerations beat recall. A false claim sitting beside a green test reads as vouched
// for by it, so the numbers below are no longer written down at all: `describe("the counts this
// file's rationale quotes are DERIVED …")` measures every one of them at run time and reds if the
// prose and the file disagree.
//
// The pre-CPE-1967 half is the one claim here that CANNOT be derived at run time, so it is not
// asserted — it is made REPRODUCIBLE instead, which is the honest fallback (CPE-1933):
//
//     git show 337ac334:.github/workflows/ci.yml | grep -nE '^    timeout-minutes:'
//
// returns exactly one line — `2042:    timeout-minutes: 10`, which is `ci-verdict` — and
//
//     git show 337ac334:.github/workflows/ci.yml | grep -cE '^  [a-z0-9-]+:$'
//
// returns 12 (the eleven job ids plus `push:` under `on:`). Deriving that inside this test was
// considered and rejected on a measured reason, not a hunch: this suite runs in `ci.yml`'s `frontend`
// job, whose `actions/checkout@v4` has NO `fetch-depth: 0` (only `ratchet-guard` sets it, and says at
// its own site why it must). A shallow clone has no object for that revision, so a `git show` leg
// would either red on every CI run or — far worse — be written to tolerate the miss and pass
// vacuously, which is the same fail-open one level up.
//
// That is the fail-open family in its purest form: a process that never finishes never reports, and
// "never reported" is the one state no verdict can classify. The sprint that filed this ticket spent
// over an hour unable to tell a *slow* `Server crates (windows-latest)` job from a *hung* one, with
// two approved PRs blocked behind it, and settled it by comparing start timestamps against the same
// job on a sibling PR by hand. `gui-smoke.yml` had capped its jobs since CPE-1171 — the practice
// existed in this repo, and the main CI workflow was the one that skipped it.
//
// ## Why this file exists rather than a note in the workflow
//
// The drift is "someone adds a job and forgets the cap", which is invisible in review precisely
// because the job works. CLAUDE.md's pattern is that an enumerated invariant gets a test.
//
// ## Two things this guard deliberately does NOT do
//
//   1. **No allowlist, no stored count of offenders.** There is nothing to burn down: every job in
//      the repo is capped as of CPE-1967, so the invariant is total. A ratchet here would be a
//      standing licence to add an uncapped job, which is the thing being prevented. (`MIN_EXPECTED_JOBS`
//      below is an enumeration sanity floor, not a ratchet — it can only ever cause a failure, never
//      excuse one.)
//   2. **It does not read the workflow text.** The job list and every value come out of the PARSED
//      document via `parseWorkflowFile` (CPE-1933: anchor on code, never on prose). Today's `ci.yml`
//      shows why, and the shape of the problem is stated here while the digits are left to the
//      derived describe below: the string `timeout-minutes` appears in that file far more often than
//      there are job caps, split three ways — job-level keys, an equal number of STEP-level keys
//      (which a text scan must not count as a job cap), and comment prose, including a fully-indented
//      `#   timeout-minutes: 6` inside a worked example that a naive line filter reads as a key.
//      A text scan has to get all three distinctions right. Reading `job["timeout-minutes"]` off the
//      parsed object cannot be satisfied by a comment and cannot mistake a step's cap for a job's.
//
// ## Red-proof (CPE-1933 rule 3), run by hand and recorded here rather than only in the PR body
//
// Deleting `timeout-minutes: 105` from `ci.yml`'s `crates` job:
//   → "every job in every workflow declares a job-level timeout-minutes" FAILS, naming
//     `.github/workflows/ci.yml [crates]` in the offender list. Restored, green again.
// Replacing `ci.yml`'s `frontend` cap with `timeout-minutes: 400`:
//   → "no job's cap is at or above the Actions default" FAILS, naming the job, 400 and 360.
// Replacing `ci.yml`'s `msrv` cap with `timeout-minutes: "30"` (a YAML string):
//   → "every cap is a positive whole number of minutes" FAILS, naming the job and the string.
// Both numbers were re-read from a real run of this file, not predicted.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { discoverWorkflows, parseWorkflowFile } from "./workflowShellSources";

/**
 * GitHub Actions' own default job timeout. A declared cap that is not BELOW this bounds nothing —
 * it is the status quo with a number written next to it, which reads like a decision and is not one.
 *
 * Documented by GitHub as 360 minutes for `jobs.<job_id>.timeout-minutes`.
 */
const ACTIONS_DEFAULT_TIMEOUT_MINUTES = 360;

/**
 * Enumeration sanity floor for "how many jobs did we find at all", modelled on
 * `MIN_EXPECTED_WORKFLOWS` in `workflowShellSources.ts` (CPE-1932/CPE-1969). A guard that scans
 * nothing reports clean, and "0 uncapped jobs across 0 jobs" is a zero-enumeration false green.
 *
 * 20 rather than today's 28: `discoverWorkflows` already refuses a near-empty FILE list at its own
 * floor of 8, so what is left for this number to catch is a parse that returned a document with no
 * `jobs` key — a partial or empty result, not a repo that genuinely shrank. Jobs are added and
 * retired far more often than whole workflows, so pinning this at today's exact count would red on
 * ordinary work with nothing wrong. If it ever falls below 20 something is broken, not tidied.
 */
const MIN_EXPECTED_JOBS = 20;

interface Job {
  id: string;
  file: string;
  where: string;
  timeout: unknown;
}

/** Every job in every workflow, derived at run time from the parsed YAML. Never a remembered list. */
function allJobs(): Job[] {
  const out: Job[] = [];
  for (const file of discoverWorkflows()) {
    const doc = parseWorkflowFile(file);
    const jobs = doc.jobs ?? {};
    // A workflow with no jobs is not a thing GitHub runs; it means the parse came back shaped wrong.
    // Refuse it here rather than let it thin the total out silently.
    expect(
      Object.keys(jobs).length,
      `${file} parsed to zero jobs. A workflow with no \`jobs:\` does not run at all, so this is ` +
        `almost certainly a parse that returned the wrong shape — fix the parse, do not skip the file.`,
    ).toBeGreaterThan(0);
    for (const [id, job] of Object.entries(jobs)) {
      out.push({
        id,
        file,
        where: `${file} [${id}]`,
        timeout: (job as Record<string, unknown>)["timeout-minutes"],
      });
    }
  }
  expect(
    out.length,
    `job enumeration came back near-empty: ${out.length}, floor is ${MIN_EXPECTED_JOBS}. A guard ` +
      `that scans nothing reports clean (CPE-1932).`,
  ).toBeGreaterThanOrEqual(MIN_EXPECTED_JOBS);
  return out;
}

describe("every workflow job is bounded by its own timeout-minutes (CPE-1967)", () => {
  const jobs = allJobs();

  it("every job in every workflow declares a job-level timeout-minutes", () => {
    const uncapped = jobs.filter((j) => j.timeout === undefined || j.timeout === null).map((j) => j.where);
    expect(
      uncapped,
      `${uncapped.length} job(s) declare no \`timeout-minutes:\`, so they run to the GitHub Actions ` +
        `default of ${ACTIONS_DEFAULT_TIMEOUT_MINUTES} minutes: ${uncapped.join(", ")}.\n` +
        `A job that never finishes never reports, and "never reported" is the one state no verdict ` +
        `can classify. Add a cap sized from MEASURED duration — see the rule and the worked ` +
        `arithmetic in the block above \`jobs:\` in .github/workflows/ci.yml, and record the sample ` +
        `beside the value so the next reader can tell "this hung" from "we guessed low".`,
    ).toEqual([]);
  });

  it("every cap is a positive whole number of minutes", () => {
    const bad = jobs
      .filter((j) => j.timeout !== undefined && j.timeout !== null)
      .filter((j) => typeof j.timeout !== "number" || !Number.isInteger(j.timeout) || (j.timeout as number) < 1)
      .map((j) => `${j.where} = ${JSON.stringify(j.timeout)}`);
    expect(
      bad,
      `${bad.length} job(s) declare a \`timeout-minutes:\` that is not a positive whole number of ` +
        `minutes: ${bad.join(", ")}. A quoted value ("30") or an expression is not a number to this ` +
        `guard and is a good sign the value is not what was intended.`,
    ).toEqual([]);
  });

  it("no job's cap is at or above the Actions default, so every cap actually bounds something", () => {
    const useless = jobs
      .filter((j) => typeof j.timeout === "number" && (j.timeout as number) >= ACTIONS_DEFAULT_TIMEOUT_MINUTES)
      .map((j) => `${j.where} = ${String(j.timeout)}`);
    expect(
      useless,
      `${useless.length} job(s) declare a cap at or above the ${ACTIONS_DEFAULT_TIMEOUT_MINUTES}-minute ` +
        `Actions default: ${useless.join(", ")}. That is the status quo with a number written next to ` +
        `it — it reads like a decision and bounds nothing.`,
    ).toEqual([]);
  });
});

// -------------------------------------------------------------------------------------------------
// The counts this file's rationale quotes are DERIVED, not recalled (CPE-1932 / CPE-1948).
//
// This describe exists because the first version of this PR got its own headline number wrong twice:
// "`ci.yml` had TEN jobs" (it has eleven) and "not one of them carried a job-level cap"
// (`ci-verdict` did). Both were carried forward from the ticket's prose rather than re-derived, which
// is the precise failure the guard above is about, one level out — and a wrong number sitting beside
// a green test reads as vouched for by it.
//
// CPE-1948's lesson applied: the fix is not a better sentence, it is to stop keeping an unguarded
// second copy of a measurement. Every number the prose above leans on is measured here, so the file
// cannot contradict itself again without reddening.
//
// Red-proofed (CPE-1933 rule 3), run by hand on 2026-08-28 and the result recorded here rather than
// only in a PR body: inserting a twelfth job (`extra-job`, with its own cap) into `ci.yml` reds TWO
// of these three — the parsed-count leg naming the full job list and `expected 12 to be 11`, and the
// step-vs-job leg at `expected 11 to be 12`. Removed, green again at 6/6. So the count really is
// re-read from the file on every run; it is not a literal agreeing with another literal.
describe("the counts this file's rationale quotes are DERIVED from ci.yml, not recalled", () => {
  const CI = join(process.cwd(), ".github", "workflows", "ci.yml");
  const text = readFileSync(CI, "utf8");
  const lines = text.split(/\r?\n/);

  // Anchored on INDENTATION, which is what distinguishes the three populations in a workflow file:
  // a job key sits at four spaces under `jobs: <id>:`, a step key at eight or more under `steps:`,
  // and a comment line starts with `#` at whatever indent. Every one of these is a TEXT measurement
  // on purpose — the whole point is to show what a text scan would have to get right, and to compare
  // it against the parsed answer below.
  const jobLevelKeys = lines.filter((l) => /^ {4}timeout-minutes:/.test(l));
  const stepLevelKeys = lines.filter((l) => /^ {8,}timeout-minutes:/.test(l));
  const anyKey = lines.filter((l) => /^ *timeout-minutes:/.test(l));
  const allMentions = lines.filter((l) => l.includes("timeout-minutes"));
  const commentMentions = allMentions.filter((l) => !/^ *timeout-minutes:/.test(l));

  it("`ci.yml` has ELEVEN jobs, and the parsed count agrees with the job-level key count", () => {
    const parsed = Object.keys((parseWorkflowFile(".github/workflows/ci.yml").jobs ?? {}) as object);
    expect(parsed.length, `ci.yml's parsed job list is ${parsed.join(", ")}`).toBe(11);
    // The prose says "all eleven carry a cap". If the text scan and the parser ever disagree on how
    // many job-level caps there are, one of them is reading the file wrong and the sentence is
    // unsupported either way.
    expect(jobLevelKeys.length, "job-level `timeout-minutes:` lines found by text scan").toBe(11);
    expect(parsed.includes("ci-verdict"), "`ci-verdict` is the job named in the prose above").toBe(true);
  });

  it("step-level caps exist in the same file and are NOT job caps — the distinction the prose claims", () => {
    // The sentence in section 2 says there is "an equal number of STEP-level keys". Asserted, so it
    // cannot quietly stop being true.
    expect(stepLevelKeys.length).toBe(jobLevelKeys.length);
    expect(anyKey.length).toBe(jobLevelKeys.length + stepLevelKeys.length);
  });

  it("`timeout-minutes` also appears in COMMENT prose, which is what defeats a naive text scan", () => {
    // The prose's third population. It must be non-empty or the argument for parsing over grepping is
    // hypothetical rather than demonstrated in this very file.
    expect(commentMentions.length).toBeGreaterThan(0);
    expect(allMentions.length).toBe(anyKey.length + commentMentions.length);
    // And specifically the worked example the prose names: a fully-indented comment line that a
    // filter keyed on "the line contains `timeout-minutes:`" would read as a key.
    expect(
      commentMentions.some((l) => /^ *#\s+timeout-minutes:\s*\d/.test(l)),
      "ci.yml no longer contains the indented `#   timeout-minutes: N` example the docblock cites",
    ).toBe(true);
  });
});
