// CPE-1967 — every job in every workflow must declare its own `timeout-minutes`.
//
// ## What went wrong
//
// `ci.yml` had TEN jobs and not one of them carried a job-level cap. Only individual STEPS had
// timeouts, which bound a step and say nothing about a job wedged between steps, in `actions/cache`,
// or in a runner that stopped answering. Everything therefore sat under the GitHub Actions default of
// **360 minutes**.
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
//      document via `parseWorkflowFile` (CPE-1933: anchor on code, never on prose). That matters
//      concretely, and the numbers are measured on today's `ci.yml` rather than asserted: the string
//      `timeout-minutes` occurs **30** times in that file, of which **22** are real keys (nearly all
//      of them STEP-level, which this guard must not count as a job cap) and **8** are comment prose
//      — including a fully-indented `#   timeout-minutes: 6` inside a worked example. A text scan has
//      to get all three of those distinctions right; reading `job["timeout-minutes"]` off the parsed
//      object cannot be satisfied by a comment, and cannot mistake a step's cap for a job's.
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
