// CPE-1956 — no test job in `ci.yml` can be skipped into a green verdict.
//
// The measurement: `ci.yml`'s `backend`, `crates`, `net-e2e`, `sidecar` and `msrv` all carried
// `needs: lockfile-preflight` with no `if:`. GitHub SKIPS a job whose `needs:` failed — it does not
// fail it — and a skipped job is grey, satisfies a required status check, and is neither `pending`
// nor `failure` to `scripts/ci-poll.mjs`. So one preflight failure silently removed the entire Rust
// suite from a PR's verdict while the PR still looked like it had run CI. Branch protection was
// measured off on 2026-08-27 (`branches/main/protection` -> 404, `rulesets` -> `[]`), so the
// required-check half of that hazard is latent rather than live; the reviewer-misreads-grey half is
// live today.
//
// The fix is a terminal `ci-verdict` job with `if: always()`, mirroring `gui-smoke-linux-verdict`
// (CPE-1753). This file is that job's red-proof, in two halves, both of which have to be DERIVED
// rather than asserted (CPE-1933 — a comment claiming a workflow behaves a certain way is untested
// by construction):
//
//   1. WIRING, derived from `ci.yml` itself. The set of jobs `ci-verdict` needs is compared against
//      the set of jobs that actually carry `needs: lockfile-preflight`, read out of the parsed
//      workflow at run time. A sixth job added behind the preflight and forgotten here reds. This is
//      CPE-1932's rule: the list is enumerated from the file, never recalled into a literal.
//   2. BEHAVIOUR, by EXECUTING the job's real `run:` body. The `run:` line is pulled out of the
//      parsed workflow (not retyped) and spawned with synthetic `CI_VERDICT_NEEDS` payloads, so
//      "a skipped job reds the verdict" is observed, not described. Both directions are proven: an
//      all-success payload must exit 0, or this file would be a guard only ever seen to fail.

import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { parseYaml } from "./preview/yaml";
import { judge, MIN_DEPENDENT_JOBS } from "../../scripts/ci-verdict.mjs";

const ROOT = process.cwd();
const CI_YML = join(ROOT, ".github", "workflows", "ci.yml");

interface WorkflowStep {
  name?: string;
  run?: string;
  env?: Record<string, string>;
  [key: string]: unknown;
}
interface WorkflowJob {
  needs?: string | string[];
  if?: string;
  steps?: WorkflowStep[];
  [key: string]: unknown;
}
interface WorkflowDoc {
  jobs: Record<string, WorkflowJob>;
  [key: string]: unknown;
}

function parseCi(): WorkflowDoc {
  const result = parseYaml(readFileSync(CI_YML, "utf8"));
  if (!result.ok) throw new Error(`.github/workflows/ci.yml did not parse as YAML: ${result.error}`);
  return result.value as WorkflowDoc;
}

const ci = parseCi();

/** `needs:` normalised — the YAML accepts a bare scalar or a list. */
function needsOf(job: WorkflowJob): string[] {
  const n = job.needs;
  if (n === undefined) return [];
  return Array.isArray(n) ? n.map(String) : [String(n)];
}

const VERDICT_JOB_NAME = "ci-verdict";

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// 1. Wiring, derived from ci.yml
// ─────────────────────────────────────────────────────────────────────────────────────────────────

describe("ci.yml's terminal verdict job is wired to every job it must judge (CPE-1956/CPE-1932)", () => {
  it("parses ci.yml and finds a plausible number of jobs — a broken parse must not pass vacuously", () => {
    // Without this, a parser regression that returned `{ jobs: {} }` would make every derived
    // expectation below an empty-set-equals-empty-set tautology.
    expect(Object.keys(ci.jobs).length).toBeGreaterThanOrEqual(6);
    expect(Object.keys(ci.jobs)).toContain("lockfile-preflight");
  });

  it("the verdict job exists and carries if: always(), so it cannot be skipped by what it reports on", () => {
    const job = ci.jobs[VERDICT_JOB_NAME];
    expect(job, `ci.yml has no \`${VERDICT_JOB_NAME}\` job — the CPE-1956 gate was removed`).toBeDefined();
    expect(
      /always\(\)/.test(String(job.if ?? "")),
      `${VERDICT_JOB_NAME}'s if: is "${job.if ?? "<absent>"}" — without always() it is skipped by the ` +
        `same upstream failure it exists to report, and a skipped gate reads as a green PR`,
    ).toBe(true);
  });

  it("its needs: is exactly the set of jobs behind lockfile-preflight — derived, never recalled", () => {
    const behindPreflight = Object.entries(ci.jobs)
      .filter(([, job]) => needsOf(job).includes("lockfile-preflight"))
      .map(([name]) => name)
      .sort();

    // CPE-1932's near-empty backstop: a selector that silently stops matching would otherwise turn
    // this into `[] === []`.
    expect(
      behindPreflight.length,
      "no job in ci.yml carries `needs: lockfile-preflight` — either the preflight chain was " +
        "restructured (in which case this guard needs rewriting, not deleting) or the derivation broke",
    ).toBeGreaterThanOrEqual(3);

    expect(
      needsOf(ci.jobs[VERDICT_JOB_NAME]).sort(),
      `${VERDICT_JOB_NAME} must judge EVERY job behind lockfile-preflight. A job added behind the ` +
        `preflight but not listed in the verdict's needs: can still be skipped into a green run, ` +
        `which is exactly the defect CPE-1956 closed.`,
    ).toEqual(behindPreflight);
  });

  it("no job behind the preflight has quietly been given an if: that could skip it unnoticed", () => {
    // Not a prohibition on `if:` — it is a prohibition on an `if:` nobody recorded. If one of the
    // five ever grows a condition, the verdict's message ("SKIPPED — did not run") stops being the
    // right diagnosis and this test is where that gets re-decided.
    for (const [name, job] of Object.entries(ci.jobs)) {
      if (!needsOf(job).includes("lockfile-preflight")) continue;
      expect(
        job.if ?? null,
        `${name} grew a job-level if: (${job.if}) — decide what a legitimate skip means for the ` +
          `verdict job's message before allowing it`,
      ).toBeNull();
    }
  });

  it("the verdict step runs the real script and feeds it toJSON(needs), not a retyped literal", () => {
    const steps = ci.jobs[VERDICT_JOB_NAME].steps ?? [];
    const runner = steps.find((s) => typeof s.run === "string" && s.run.includes("ci-verdict.mjs"));
    expect(runner, `${VERDICT_JOB_NAME} has no step invoking scripts/ci-verdict.mjs`).toBeDefined();
    expect((runner as WorkflowStep).env?.CI_VERDICT_NEEDS).toContain("toJSON(needs)");
  });
});

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// 2. Behaviour of the judgement itself
// ─────────────────────────────────────────────────────────────────────────────────────────────────

const FIVE_GREEN = {
  backend: { result: "success" },
  crates: { result: "success" },
  "net-e2e": { result: "success" },
  sidecar: { result: "success" },
  msrv: { result: "success" },
};

describe("judge()", () => {
  it("passes when every needed job succeeded", () => {
    expect(judge(FIVE_GREEN).ok).toBe(true);
  });

  it("reds on a SKIPPED job and says so in those words — the CPE-1956 case", () => {
    const v = judge({ ...FIVE_GREEN, crates: { result: "skipped" } });
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toContain("crates");
    expect(v.errors.join("\n")).toContain("SKIPPED");
    // The message has to say a skip is not a pass, because "grey means not applicable" is the exact
    // misreading this gate exists to stop.
    expect(v.errors.join("\n")).toMatch(/did not run|NOT pass/);
  });

  it("reds on a failed job and on a cancelled one", () => {
    expect(judge({ ...FIVE_GREEN, msrv: { result: "failure" } }).ok).toBe(false);
    expect(judge({ ...FIVE_GREEN, msrv: { result: "cancelled" } }).ok).toBe(false);
  });

  it("reds on an unknown result rather than treating anything-not-failure as fine", () => {
    const v = judge({ ...FIVE_GREEN, sidecar: { result: "neutral" } });
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toContain("sidecar");
  });

  it("reds when a job reports no result field at all", () => {
    const v = judge({ ...FIVE_GREEN, backend: {} });
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toContain("backend");
  });

  it("reds on an empty or near-empty needs payload instead of reporting 0-of-0 success", () => {
    // The vacuous-green case: `toJSON(needs)` evaluating to `{}` must not read as "nothing failed".
    expect(judge({}).ok).toBe(false);
    expect(judge({ backend: { result: "success" } }).ok).toBe(false);
    expect(judge({}).errors.join("\n")).toContain(String(MIN_DEPENDENT_JOBS));
  });

  it("reds when the payload is not an object at all", () => {
    expect(judge(null).ok).toBe(false);
    expect(judge([]).ok).toBe(false);
    expect(judge("success").ok).toBe(false);
  });

  it("the floor sits below the number of jobs actually behind the preflight", () => {
    // A floor at or above the real count would red on every green run; a floor of 0 would never red.
    const behind = Object.values(ci.jobs).filter((j) => needsOf(j).includes("lockfile-preflight")).length;
    expect(MIN_DEPENDENT_JOBS).toBeGreaterThan(0);
    expect(MIN_DEPENDENT_JOBS).toBeLessThanOrEqual(behind);
  });
});

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// 3. Red-proof: execute the workflow's OWN run: body
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/** The `run:` body of the verdict step, taken from the parsed workflow rather than retyped. */
function verdictRunBody(): string {
  const steps = ci.jobs[VERDICT_JOB_NAME].steps ?? [];
  const runner = steps.find((s) => typeof s.run === "string" && s.run.includes("ci-verdict.mjs"));
  const body = (runner as WorkflowStep | undefined)?.run;
  if (typeof body !== "string" || body.trim() === "") {
    throw new Error(
      `${VERDICT_JOB_NAME} has no non-empty run: body after parsing — the red-proof below would ` +
        `otherwise execute nothing and pass vacuously`,
    );
  }
  return body.trim();
}

function runVerdict(needs: unknown): { status: number; out: string } {
  const body = verdictRunBody();
  // The body is `node scripts/ci-verdict.mjs`. Split on whitespace rather than assuming the exact
  // string, so a `--flag` added in the workflow is carried through here too.
  const argv = body.split(/\s+/);
  expect(argv[0], `the verdict step's run: body is "${body}" — expected it to invoke node`).toBe("node");
  const r = spawnSync(process.execPath, argv.slice(1), {
    cwd: ROOT,
    encoding: "utf8",
    env: { ...process.env, CI_VERDICT_NEEDS: JSON.stringify(needs) },
  });
  return { status: r.status ?? -1, out: `${r.stdout ?? ""}\n${r.stderr ?? ""}` };
}

describe("the shipped ci-verdict step, executed (CPE-1933 red-proof)", () => {
  it("exits 0 when all five jobs succeeded", () => {
    const r = runVerdict(FIVE_GREEN);
    expect(r.out).toContain("every job behind lockfile-preflight ran and succeeded");
    expect(r.status).toBe(0);
  });

  it("exits 1 and emits ::error:: naming every skipped job when the preflight failure skipped them", () => {
    // Exactly what GitHub hands the job when `lockfile-preflight` fails: all five `skipped`.
    const r = runVerdict({
      backend: { result: "skipped" },
      crates: { result: "skipped" },
      "net-e2e": { result: "skipped" },
      sidecar: { result: "skipped" },
      msrv: { result: "skipped" },
    });
    expect(r.status).toBe(1);
    for (const name of ["backend", "crates", "net-e2e", "sidecar", "msrv"]) {
      expect(r.out, `the verdict must name ${name} in its error output`).toContain(name);
    }
    expect(r.out).toContain("::error::");
    expect(r.out).toContain("SKIPPED");
  });

  it("exits 1 when its own input is missing, rather than treating silence as success", () => {
    const argv = verdictRunBody().split(/\s+/);
    const env = { ...process.env };
    delete env.CI_VERDICT_NEEDS;
    const r = spawnSync(process.execPath, argv.slice(1), { cwd: ROOT, encoding: "utf8", env });
    expect(r.status).toBe(1);
    expect(`${r.stderr}`).toContain("::error::");
  });

  it("exits 1 on a malformed payload", () => {
    const argv = verdictRunBody().split(/\s+/);
    const r = spawnSync(process.execPath, argv.slice(1), {
      cwd: ROOT,
      encoding: "utf8",
      env: { ...process.env, CI_VERDICT_NEEDS: "{not json" },
    });
    expect(r.status).toBe(1);
    expect(`${r.stderr}`).toContain("::error::");
  });
});
