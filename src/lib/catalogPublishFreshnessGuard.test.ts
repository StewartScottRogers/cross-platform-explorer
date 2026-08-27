// CPE-1893: the signed agent catalog (CPE-308/377) went unpublished for 31 days
// (2026-07-25 → 2026-08-25) because release.yml's `catalog` job was `needs: release` with no
// `if:`, so ANY failing leg of `release`'s fail-fast:false matrix silently SKIPPED it -- indistin-
// guishable in the run summary from a job that correctly had nothing to do. Two independent guards
// close this, and this file is the structural ratchet for both so neither can silently regress:
//
//   1. release.yml's `catalog` job now runs with `if: ${{ !cancelled() }}` (matching the
//      already-established `verify-published-manifest` pattern, CPE-1872 finding A) -- a bare
//      `needs: release` with no `if:` can never sneak back in.
//   2. catalog-freshness.yml is a scheduled, independent backstop: it checks the wall-clock age of
//      whatever catalog is actually live right now, so a DIFFERENT future failure (not just a
//      `release`-job failure -- e.g. this job "succeeding" at zero work because the signing-key
//      secret got unset) still surfaces on its own instead of waiting for someone to trip over it.
//
// Structural assertions go through `parseYaml` (src/lib/preview/yaml.ts, CPE-1617), the same
// approach releaseHangHardening.test.ts (CPE-1824) and its siblings use, adopted after a review
// round found a regex-over-raw-text guard could be satisfied by an unrelated neighbouring comment
// rather than the key it claimed to check.
//
// .github/workflows/scripts/catalog-freshness-check.sh's own age/staleness arithmetic is red-proofed
// separately below by actually EXECUTING it (probed, skipped gracefully where `bash` isn't on PATH
// -- same probe-and-skip shape releaseVersionBump.test.ts uses for pwsh/powershell) against fixed
// fabricated epochs, so the fresh/boundary/stale verdicts are proven, not merely asserted to exist.
import { describe, it, expect, beforeAll } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { parseYaml } from "./preview/yaml";

const ROOT = process.cwd();
const WORKFLOWS = join(ROOT, ".github", "workflows");

function read(fileName: string): string {
  return readFileSync(join(WORKFLOWS, fileName), "utf8");
}

interface WorkflowStep {
  name?: string;
  run?: string;
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

function parseWorkflow(fileName: string): WorkflowDoc {
  const result = parseYaml(read(fileName));
  if (!result.ok) {
    throw new Error(`${fileName} did not parse as YAML: ${result.error}`);
  }
  return result.value as WorkflowDoc;
}

describe("release.yml's catalog job can never silently skip behind a failed release job (CPE-1893)", () => {
  const doc = parseWorkflow("release.yml");

  it("catalog still needs release (ordering preserved -- this is not a full decoupling)", () => {
    expect(doc.jobs.catalog.needs).toBe("release");
  });

  it("catalog runs regardless of release's pass/fail outcome, same as verify-published-manifest", () => {
    // The exact expression GitHub Actions documents for "run unless the workflow was cancelled" --
    // NOT `always()` (which would also run after an explicit `cancelled()`, unlike this job's own
    // sibling) and NOT left absent (which is the original bug: absent `if:` + a failed `needs:` job
    // means SKIPPED).
    expect(doc.jobs.catalog.if).toBe("${{ !cancelled() }}");
  });

  it("catalog's if: condition matches verify-published-manifest's -- the one already-proven pattern in this file", () => {
    // CPE-1872 round 3 already solved this exact "fail-fast:false leaves surviving legs having
    // published something, but the job-level status still reads failure" problem for
    // verify-published-manifest. Asserting equality (not just presence) means a future edit that
    // relaxes one of the two sites without the other is caught here.
    expect(doc.jobs.catalog.if).toBe(doc.jobs["verify-published-manifest"].if);
  });

  it("catalog's own per-step secret gate is untouched -- the job now RUNS, but still does nothing without CPE_CATALOG_SIGNING_KEY", () => {
    const steps = doc.jobs.catalog.steps ?? [];
    const detect = steps.find((s) => s.name === "Detect catalog signing key");
    expect(detect, "the secret-detection step must still exist").toBeDefined();
    const gated = steps.filter((s) => typeof s["if"] === "string" && (s["if"] as string).includes("steps.k.outputs.has"));
    // Every real-work step (build/upload) stays gated on the secret; only the job-level `if:`
    // changed. This is the distinction the ticket draws between "make the skip loud" (job level)
    // and "still skip gracefully when catalog signing genuinely isn't configured" (step level).
    expect(gated.length).toBeGreaterThanOrEqual(3);
  });
});

describe("catalog-freshness.yml is a scheduled, independent staleness backstop (CPE-1893)", () => {
  const doc = parseWorkflow("catalog-freshness.yml");
  const on = doc["on" as keyof WorkflowDoc] as Record<string, unknown>;
  const job = doc.jobs["check-catalog-freshness"];

  it("has a schedule trigger (not just workflow_run/workflow_dispatch) so it checks even when no release is ever tagged", () => {
    expect(on).toBeDefined();
    expect(Array.isArray(on.schedule)).toBe(true);
    const entries = on.schedule as Array<Record<string, unknown>>;
    expect(entries.length).toBeGreaterThan(0);
    expect(typeof entries[0].cron).toBe("string");
    expect((entries[0].cron as string).length).toBeGreaterThan(0);
  });

  it("also has a workflow_dispatch test hook to dry-run the stale path without waiting", () => {
    const wd = on.workflow_dispatch as Record<string, unknown> | undefined;
    expect(wd).toBeDefined();
    const inputs = wd?.inputs as Record<string, unknown> | undefined;
    expect(inputs?.override_threshold_days).toBeDefined();
  });

  it("declares issues: write (needed to file/dedupe the alert issue) and contents: read only", () => {
    const perms = doc.permissions as Record<string, unknown>;
    expect(perms.issues).toBe("write");
    expect(perms.contents).toBe("read");
  });

  it("has a deliberately-chosen, non-zero default threshold, recorded in env", () => {
    const env = doc.env as Record<string, unknown>;
    expect(Number(env.DEFAULT_THRESHOLD_DAYS)).toBeGreaterThan(0);
  });

  it("sources the one shared freshness-arithmetic script rather than reimplementing the math inline", () => {
    const steps = job.steps ?? [];
    const evalStep = steps.find((s) => s.name === "Evaluate freshness");
    expect(evalStep, "the freshness-evaluation step must exist").toBeDefined();
    expect(evalStep?.run).toContain("catalog-freshness-check.sh");
  });

  it("checks the exact default URL a real client's do_fetch_catalog (src-tauri/src/lib.rs) requests", () => {
    const steps = job.steps ?? [];
    const fetchStep = steps.find((s) => s.name === "Fetch the live catalog index");
    expect(fetchStep?.run).toContain("releases/latest/download/catalog-index.json");
  });

  it("treats a confirmed 404 as its own alarm, distinct from mere staleness or an inconclusive status", () => {
    const steps = job.steps ?? [];
    const issueStep = steps.find((s) => s.name === "File an issue (deduped) on a confirmed problem");
    expect(issueStep?.if).toContain("http_code == '404'");
  });
});

// --- Execution-level red-proof of the shared script's age/staleness arithmetic -------------------
const SCRIPT = join(WORKFLOWS, "scripts", "catalog-freshness-check.sh");

function bashAvailable(): boolean {
  const probe = spawnSync("bash", ["--version"], { stdio: "ignore" });
  return !probe.error && probe.status === 0;
}

function runScript(publishedEpoch: number, thresholdDays: number, nowEpoch: number) {
  return spawnSync("bash", [SCRIPT, String(publishedEpoch), String(thresholdDays), String(nowEpoch)], {
    encoding: "utf8",
  });
}

describe("catalog-freshness-check.sh's age/staleness arithmetic (executed, not just asserted to exist)", () => {
  let hasBash = false;
  beforeAll(() => {
    hasBash = bashAvailable();
  });

  const NOW = 1_800_000_000; // fixed reference instant -- deterministic across machines/timezones
  const DAY = 86_400;

  it("a catalog published moments ago is fresh (exit 0)", () => {
    if (!hasBash) return; // no bash on PATH -- CI's frontend job (ubuntu-latest) always has one
    const r = runScript(NOW - 60, 7, NOW);
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("fresh");
  });

  it("a catalog published exactly at the threshold is still fresh -- strictly-older, not >= (exit 0)", () => {
    if (!hasBash) return;
    const r = runScript(NOW - 7 * DAY, 7, NOW);
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("fresh");
  });

  it("a catalog one day past the threshold is stale (exit 1)", () => {
    if (!hasBash) return;
    const r = runScript(NOW - 8 * DAY, 7, NOW);
    expect(r.status).toBe(1);
    expect(r.stdout).toContain("STALE");
  });

  it("this ticket's own real scenario -- a 40-day-old catalog against a 7-day threshold -- is stale (exit 1)", () => {
    if (!hasBash) return;
    const r = runScript(NOW - 40 * DAY, 7, NOW);
    expect(r.status).toBe(1);
    expect(r.stdout).toContain("STALE");
    expect(r.stdout).toContain("40d");
  });


  // CPE-1893 UAT round 1, reviewer finding 1: a naive `[ age -gt threshold ]` comparison lets a
  // NEGATIVE age (a future-dated `version` -- a clock skew on the signing runner, or a mis-stamped
  // catalog) slip through as "fresh", technically true since it is not GREATER than the threshold.
  // is_catalog_stale now returns a distinct exit code (2) for this case so no caller can collapse it
  // into "fresh". Both cases below are the reviewer's own red-proof inputs, reproduced exactly.
  it("a future-dated version (age -1d, the reviewer's exact red-proof case) is CLOCK SKEW, not fresh (exit 2)", () => {
    if (!hasBash) return;
    const r = runScript(1_800_086_400, 7, 1_800_000_000);
    expect(r.status).toBe(2);
    expect(r.stdout).toContain("CLOCK SKEW");
    expect(r.stdout).toContain("-1d");
    expect(r.stdout).not.toContain("fresh —"); // must never read as the healthy verdict
  });

  it("an extreme future-dated version (age far negative) is still CLOCK SKEW, not fresh (exit 2)", () => {
    if (!hasBash) return;
    const r = runScript(9_999_999_999, 7, 1_800_000_000);
    expect(r.status).toBe(2);
    expect(r.stdout).toContain("CLOCK SKEW");
    expect(r.stdout).toContain("-94907d");
    expect(r.stdout).not.toContain("fresh —");
  });
});

// --- Execution-level red-proof of the "don't let set -e swallow a parse failure" control flow ----
// CPE-1893 UAT round 1, reviewer finding 2: catalog-freshness.yml's "Evaluate freshness" step runs
// under `set -euo pipefail`. A BARE failing command inside `published=$(jq ...)` aborts the step
// immediately -- before `unparseable=true` is ever written to $GITHUB_OUTPUT -- so a corrupt
// catalog-index.json body (HTTP 200, invalid JSON) went red WITHOUT filing the alert issue, unlike
// the sibling case (valid JSON, empty entries[]) which correctly does. The fix wraps the assignment
// as an `if` CONDITION, which is exempt from `set -e` regardless of the guarded command's exit code.
// `jq` is not installed in every dev/CI environment this test runs in (confirmed absent locally), so
// this proves the underlying bash CONTROL-FLOW mechanism directly with a stand-in failing command
// (`false`) rather than depending on jq being present -- the defect and the fix are both about
// `set -e` interacting with a command-substitution assignment, not about jq specifically, and the
// exact same shape (`if ! var=$(cmd); then ...`) is what catalog-freshness.yml now uses verbatim.
describe('the "if ! var=$(cmd)" guard survives set -e where a bare "var=$(cmd)" does not (CPE-1893)', () => {
  let hasBash = false;
  beforeAll(() => {
    hasBash = bashAvailable();
  });

  it("OLD shape: a bare failing assignment under set -e aborts BEFORE the failure can be handled", () => {
    if (!hasBash) return;
    const r = spawnSync(
      "bash",
      ["-c", 'set -euo pipefail; published=$(false); echo "unparseable=true"; echo "reached-marker"'],
      { encoding: "utf8" },
    );
    // The script dies at the failing assignment -- neither the marker nor the "unparseable" line
    // it was meant to guard ever runs. This is the exact silent-abort bug the reviewer found.
    expect(r.status).not.toBe(0);
    expect(r.stdout).not.toContain("reached-marker");
    expect(r.stdout).not.toContain("unparseable=true");
  });

  it("NEW shape: guarding the assignment as an if-condition lets the failure be handled, then still exits non-zero", () => {
    if (!hasBash) return;
    const r = spawnSync(
      "bash",
      [
        "-c",
        'set -euo pipefail; if ! published=$(false); then echo "unparseable=true"; echo "reached-marker"; exit 1; fi',
      ],
      { encoding: "utf8" },
    );
    // The failure is caught -- both the "unparseable" line and the marker run before the step still
    // exits non-zero (the run stays red, matching the sibling empty-entries[] case's own behavior).
    expect(r.status).toBe(1);
    expect(r.stdout).toContain("reached-marker");
    expect(r.stdout).toContain("unparseable=true");
  });
});
