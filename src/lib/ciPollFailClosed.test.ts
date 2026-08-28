// CPE-1906 — the fail-open paths in `scripts/ci-poll.mjs`, driven END TO END.
//
// WHY THESE ARE SUBPROCESS TESTS AND NOT UNIT TESTS
//   `ci-poll.mjs` already has thorough unit coverage of its pure functions (`sprintStallControls.test.ts`),
//   and every bug this ticket closes survived that coverage — because none of them lives in a pure
//   function. They live in what `main()` does with an exception, and in what it prints and exits with.
//   The verdict LINE and the EXIT CODE are the entire interface the Foreman consumes, so those are what
//   is asserted here, against a real child process running the real script.
//
//   A test that only checked "it didn't crash" would have passed on every one of these bugs. That is
//   the point: the old code did not crash. It reported `CI VERDICT: pending` and exited 2, which the
//   file's own documentation calls a NORMAL outcome — on a `gh` that had never once answered.
//
// THE THREE FAILURES, AND WHAT EACH ONE PROVES
//   · errors    — `gh` exits non-zero (bad token, wrong PR number). Old: pending/2. New: unknown/3.
//   · hangs     — `gh` blocks for ten minutes, past the 600 s harness cap this file exists to stay
//                 under. Old: the call was unbounded and the whole run was auto-backgrounded. New: the
//                 per-call timeout kills it and the process returns inside its own budget. The elapsed
//                 assertion is the red-proof — remove `timeout` from `gh()` and this test hangs until
//                 vitest kills it.
//   · garbage   — `gh` exits 0 and prints something that is not JSON. Old: same silent `continue` as an
//                 error. New: unknown/3, classified `unparseable output`.
//   · skips     — a check that DID NOT RUN. Old: folded into the success test, `completed success`, 0.
//                 New: a distinct `completed did-not-run` verdict and exit 4 — but ONLY for a skip no
//                 job-level `if:` explains, because this repo skips one check on every PR by design.
//
// ROUND 2 ADDED TWO MORE, AND BOTH WERE THE SAME BUG WEARING A DIFFERENT COAT
//   · shape     — `gh` exits 0 and prints well-formed JSON that is not a board. It threw nothing, so
//                 round 1's failure counter never saw it, and the readers' defensive
//                 `Array.isArray(json?.x) ? … : []` turned it into `total_count=0` — "no checks
//                 scheduled yet". In `--pr` that was a wrong wait (exit 2); in `--run` it was exit 0,
//                 GREEN. New: `assertReadableShape` → the same classified failure path → exit 3.
//   · one red   — the verdict LINE and the EXIT CODE were computed by two different predicates and
//     predicate  disagreed on two real boards. They now come out of `verdictClass`, and the last test
//                in that block pins prefix→code across the whole stub matrix rather than case by case.
import { describe, it, expect } from "vitest";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  scanWorkflowJobs,
  explainableSkipMatchers,
  classifySkips,
  workflowTriggersPullRequest,
  readBaseWorkflowSources,
  coverageOf,
} from "../../scripts/ci-poll.mjs";

const REPO = process.cwd();
const CI_POLL = join(REPO, "scripts", "ci-poll.mjs");
const STALL_CHECK = join(REPO, "scripts", "stall-check.mjs");
const GH_STUB = join(REPO, "src", "lib", "fixtures", "ghStub.mjs");
/** CPE-1970 — `main`'s workflow set as the poll should see it. See the fixture dirs' own headers. */
const BASE_WORKFLOWS = join(REPO, "src", "lib", "fixtures", "workflows-base");
const BASE_WORKFLOWS_AHEAD = join(REPO, "src", "lib", "fixtures", "workflows-base-ahead");

/**
 * Run the REAL script as a child process against a stubbed `gh`, exactly as a caller would.
 *
 * CPE-1970 pins the BASE WORKFLOWS too, and that is not incidental. The poll now derives "which jobs
 * must have judged this PR" from `origin/main`, so without a seam every one of the assertions below
 * would depend on this repo's live history and would flip the day a job is added to `ci.yml` — a suite
 * that reds for a reason unrelated to what it tests. `workflows-base` therefore declares exactly the
 * one job the legacy stub boards carry, which keeps those tests measuring what they were written to
 * measure. Tests that are ABOUT coverage pass `base` explicitly.
 */
function runPoll(mode: string, args: string[], base: string | null = BASE_WORKFLOWS) {
  const startedAt = Date.now();
  const env: NodeJS.ProcessEnv = { ...process.env, CI_POLL_GH_SCRIPT: GH_STUB, GH_STUB_MODE: mode };
  if (base === null) delete env.CI_POLL_BASE_WORKFLOWS;
  else env.CI_POLL_BASE_WORKFLOWS = base;
  const res = spawnSync(process.execPath, [CI_POLL, ...args], {
    cwd: REPO,
    encoding: "utf8",
    env,
    timeout: 120_000,
  });
  const stdout = res.stdout ?? "";
  const verdict = stdout.split(/\r?\n/).find((l) => l.startsWith("CI VERDICT:")) ?? "";
  return { status: res.status, stdout, stderr: res.stderr ?? "", verdict, elapsedMs: Date.now() - startedAt };
}

describe("ci-poll: a gh that ERRORS is never reported as pending (CPE-1906)", () => {
  // The budget is deliberately far larger than this run needs: the poll BAILS at three consecutive
  // failures, so it returns in a few seconds regardless. A tight budget instead lets the DEADLINE end
  // the run first on a loaded box — observed at `--budget 4` with six sibling agents running, where the
  // node start-up cost per tick stretched past a second and only 2 of the 3 failures fit. That is a
  // flake in the harness, not a finding about the tool, and the fix is to let the bound under test be
  // the one that fires.
  const run = runPoll("error", ["--pr", "999999", "--budget", "40", "--interval", "1"]);

  it("exits 3 — 'could not ask' — not 2 ('still pending') and not 0", () => {
    expect(run.status).toBe(3);
  });

  it("prints CI VERDICT: unknown, and the word `pending` never appears as the verdict's state", () => {
    expect(run.verdict).toMatch(/^CI VERDICT: unknown —/);
    expect(run.verdict).not.toMatch(/^CI VERDICT: pending/);
    // The old line ended `CI still pending on unknown — re-invoke this poll…`, which is precisely the
    // sentence that told the Foreman to keep waiting. It must not be reachable from an error.
    expect(run.verdict).not.toMatch(/CI still pending on/);
  });

  it("says what happened and what the caller should do about it", () => {
    expect(run.verdict).toContain("could not ask GitHub (gh exited non-zero)");
    expect(run.verdict).toContain("Could not resolve to a PullRequest");
    expect(run.verdict).toMatch(/do not merge and do not\s+wait on it/);
    expect(run.verdict).toContain("gh auth status");
    expect(run.verdict).toContain("No successful read was ever obtained.");
  });

  it("bails after the bounded number of consecutive failures instead of burning the whole budget", () => {
    expect(run.stdout).toContain("gh read failed (3/3");
    expect(run.verdict).toMatch(/3 consecutive `gh` failure\(s\)/);
  });
});

describe("ci-poll: a gh that HANGS cannot outlive the budget (CPE-1906)", () => {
  // The stub blocks for 600 s — the harness cap itself. Before the fix, `execFileSync` had no `timeout`
  // and this single call ran to completion, putting the process past the cap and getting it
  // auto-backgrounded, which is the exact defect `ci-poll.mjs` was written to make impossible.
  const run = runPoll("hang", ["--pr", "1", "--budget", "3", "--interval", "1"]);

  it("returns in seconds, not in ten minutes", () => {
    // Budget 3 s + the 5 s per-call floor + node start-up. 45 s is generous head-room for a loaded CI
    // box and still two orders of magnitude below the 600 s the unbounded call would have taken.
    expect(run.elapsedMs).toBeLessThan(45_000);
  });

  it("classifies the hang as a timeout rather than pretending CI is slow", () => {
    expect(run.status).toBe(3);
    expect(run.verdict).toMatch(/^CI VERDICT: unknown —/);
    expect(run.verdict).toContain("could not ask GitHub (timed out)");
    expect(run.verdict).toContain("gh exceeded its per-call timeout");
  });

  it("advertises a structural bound, not just a modelled one", () => {
    expect(run.stdout).toMatch(/structural bound \d+s, harness cap 600s/);
  });
}, 120_000);

describe("ci-poll: a gh that returns GARBAGE is an error, not an empty board (CPE-1906)", () => {
  const run = runPoll("garbage", ["--pr", "1", "--budget", "4", "--interval", "1"]);

  it("exits 3 and names the reason", () => {
    expect(run.status).toBe(3);
    expect(run.verdict).toContain("could not ask GitHub (unparseable output)");
  });

  it("does not read a 502 page as `total_count=0`, which decideFromReads would call 'not scheduled yet'", () => {
    expect(run.verdict).not.toMatch(/^CI VERDICT: pending/);
    expect(run.verdict).not.toContain("an empty board is NOT a green one");
  });
});

describe("ci-poll: a SKIPPED check did not run, and is never a green verdict (CPE-1906)", () => {
  // The Foreman's find, in the shape that made it live: `ci.yml`'s five Rust test jobs sit behind
  // `needs: lockfile-preflight` with no `if:`, so a preflight failure skips every one of them. The old
  // success test listed `SKIPPED` alongside `SUCCESS`, so this rollup reported `completed success` on a
  // run where the entire Rust suite never executed — on a repo whose `main` has no branch protection,
  // making this verdict the only gate.
  const cascade = runPoll("skip-cascade", ["--pr", "1", "--budget", "6", "--interval", "1"]);

  it("exits 4 — a distinct code, because 'did not run' and 'failed' need different responses", () => {
    expect(cascade.status).toBe(4);
    expect(cascade.status).not.toBe(0);
    expect(cascade.status).not.toBe(1);
  });

  it("names every check that did not run, so the caller can act without opening the browser", () => {
    expect(cascade.verdict).toMatch(/^CI VERDICT: completed did-not-run —/);
    expect(cascade.verdict).toContain("5 check(s) DID NOT RUN");
    expect(cascade.verdict).toContain("Server crates (windows-latest)");
    expect(cascade.verdict).toContain("MSRV check");
    expect(cascade.verdict).toContain("Do not merge");
  });

  it("counts the skips in the machine-readable totals too", () => {
    expect(cascade.verdict).toContain("skipped=5");
  });

  // The other half, and the reason a blanket rule was rejected: measured on PR #1068 (2026-08-27), this
  // repo skips `GUI smoke (windows-latest)` on EVERY pull request because the job carries a job-level
  // `if:`. A tool that reds every PR is a tool nobody runs.
  const byDesign = runPoll("skip-by-design", ["--pr", "1", "--budget", "6", "--interval", "1"]);

  it("stays green for a skip the workflow's own `if:` explains", () => {
    expect(byDesign.status).toBe(0);
    expect(byDesign.verdict).toMatch(/^CI VERDICT: completed success —/);
    expect(byDesign.verdict).toContain("Skipped by design: GUI smoke (windows-latest)");
  });

  it("a real FAILURE still outranks a skip — the caller's next move is the logs", () => {
    const both = runPoll("failure-and-skips", ["--pr", "1", "--budget", "6", "--interval", "1"]);
    expect(both.status).toBe(1);
    expect(both.verdict).toMatch(/^CI VERDICT: completed failure —/);
    expect(both.verdict).toContain("Lockfile pre-flight");
  });
});

describe("ci-poll: a gh that exits 0 with the WRONG SHAPE is 'could not ask', not 'pending' (CPE-1906 r2)", () => {
  // ROUND 1 CLOSED ONLY THE THROWN HALF. Counting `gh` failures catches a `gh` that throws. A `gh` that
  // exits 0 and prints well-formed JSON of the wrong shape throws nothing, so it walked into
  // `readFromPrJson`, whose defensive `Array.isArray(json?.statusCheckRollup) ? … : []` turned an absent
  // rollup into `total_count=0` — which `decideFromReads` reports as "no checks scheduled yet".
  // Measured on the pre-fix script: every payload below printed
  //   `CI VERDICT: pending — total_count=0 … CI still pending on unknown`  → exit 2
  // i.e. "did not run" reported as "not finished". It is the same defect CLAUDE.md records for
  // `audit-npm-projects.mjs` (npm's `--json` error path is well-formed JSON with no `metadata` key), one
  // layer down, inside the guard built to close that class.
  const cases: Array<{ label: string; mode: string; flag: string }> = [
    { label: "a REST error body", mode: "rest-error", flag: "--pr" },
    { label: "a GraphQL 200 with errors and a null rollup", mode: "graphql-partial", flag: "--pr" },
    { label: "a --run payload with no `jobs` key", mode: "run-no-jobs", flag: "--run" },
  ];

  for (const c of cases) {
    it(`${c.label} → exit 3, classified as a shape problem`, () => {
      const run = runPoll(c.mode, [c.flag, "1", "--budget", "4", "--interval", "1"]);
      expect(run.status).toBe(3);
      expect(run.verdict).toMatch(/^CI VERDICT: unknown —/);
      expect(run.verdict).toContain("could not ask GitHub (unexpected payload shape)");
      // The two sentences that told the Foreman to keep waiting.
      expect(run.verdict).not.toMatch(/^CI VERDICT: pending/);
      expect(run.verdict).not.toContain("CI still pending on");
    });
  }

  it("the --run shape was WORSE than a wrong wait: it was exit 0, green, on a board never seen", () => {
    // Pre-fix, `{"status":"completed","conclusion":"success"}` with no `jobs` read as total_count=0 +
    // terminal + conclusion success and exited 0. This is the merge-unsafe one, and the reason the
    // `--run` guard demands BOTH `jobs` and `status` rather than only failing when both are absent.
    const run = runPoll("run-no-jobs", ["--run", "1", "--budget", "4", "--interval", "1"]);
    expect(run.status).not.toBe(0);
    expect(run.verdict).not.toMatch(/completed success/);
  });

  it("but a genuinely check-less PR is still a PENDING board, not a shape error", () => {
    // The control that makes the guard safe to ship: a real PR with nothing scheduled yet returns the
    // rollup ARRAY (empty) and a real SHA. `sha=unknown` alongside `total_count=0` is what no real board
    // produces, and it is the discriminator.
    const run = runPoll("no-checks-yet", ["--pr", "1", "--budget", "3", "--interval", "1"]);
    expect(run.status).toBe(2);
    expect(run.verdict).toMatch(/^CI VERDICT: pending —/);
    expect(run.verdict).toContain("an empty board is NOT a green one");
    expect(run.verdict).toMatch(/sha=[0-9a-f]{7}/);
  });
});

describe("ci-poll: the verdict PREFIX and the EXIT CODE come from one predicate (CPE-1906 r2)", () => {
  // There were two. `formatVerdict` branched on `failedNames`; the exit branched on
  // `failedNames || conclusion === "failure"`. Both shapes below are measured off the pre-fix script.
  it("a board of nothing but by-design skips is exit 4, not exit 1 ('at least one check FAILED')", () => {
    // Pre-fix: `CI VERDICT: completed skipped — … Skipped by design: …` and exit **1**, with zero
    // failures — and `completed skipped` was ALSO the exit-4 prefix, so the prefix discriminated
    // nothing. Nothing failed here, but nothing ran either: not red, not green.
    const run = runPoll("all-skipped-by-design", ["--pr", "1", "--budget", "6", "--interval", "1"]);
    expect(run.status).toBe(4);
    expect(run.verdict).toMatch(/^CI VERDICT: completed did-not-run —/);
    expect(run.verdict).toContain("nothing here verified this commit");
    expect(run.verdict).toContain("Skipped by design:");
  });

  it("a run-level `failure` with no failing job reads as RED in the line and in the code", () => {
    // Pre-fix: the unexplained-skip branch printed "This is neither red nor green" while the exit code
    // said 1. Red outranks a skip, so both now say red.
    const run = runPoll("run-failure-no-failing-job", ["--run", "1", "--budget", "4", "--interval", "1"]);
    expect(run.status).toBe(1);
    expect(run.verdict).toMatch(/^CI VERDICT: completed failure —/);
    expect(run.verdict).toContain("No individual check reported a failure");
    expect(run.verdict).not.toContain("neither red nor green");
  });

  it("every verdict prefix maps to exactly one exit code across the whole stub matrix", () => {
    const seen = new Map<string, Set<number>>();
    const matrix: Array<[string, string]> = [
      ["green", "--pr"],
      ["pending", "--pr"],
      ["skip-cascade", "--pr"],
      ["skip-by-design", "--pr"],
      ["failure-and-skips", "--pr"],
      ["all-skipped-by-design", "--pr"],
      ["no-checks-yet", "--pr"],
      ["rest-error", "--pr"],
      ["run-no-jobs", "--run"],
      ["run-failure-no-failing-job", "--run"],
    ];
    for (const [mode, flag] of matrix) {
      const run = runPoll(mode, [flag, "1", "--budget", "4", "--interval", "1"]);
      const prefix = /^CI VERDICT: ([a-z-]+(?: [a-z-]+)?)/.exec(run.verdict)?.[1] ?? "(none)";
      if (!seen.has(prefix)) seen.set(prefix, new Set());
      seen.get(prefix)!.add(run.status ?? -1);
    }
    for (const [prefix, codes] of seen) {
      expect([...codes], `prefix "${prefix}" maps to more than one exit code`).toHaveLength(1);
    }
    // …and the prefixes are actually distinct, or the assertion above would be vacuous.
    expect(seen.size).toBeGreaterThanOrEqual(4);
  });
}, 180_000);

describe("ci-poll: the skip discrimination is DERIVED from the workflows, not recalled (CPE-1932/1933)", () => {
  const ciYml = readFileSync(join(REPO, ".github", "workflows", "ci.yml"), "utf8");
  const guiSmokeYml = readFileSync(join(REPO, ".github", "workflows", "gui-smoke.yml"), "utf8");

  it("reads ci.yml and finds the five Rust test jobs behind lockfile-preflight with NO job-level `if:`", () => {
    const jobs = scanWorkflowJobs(ciYml);
    // If this list ever stops matching the workflow, the assertion below fails rather than the comment
    // quietly going stale — the whole reason CPE-1933 exists.
    for (const id of ["backend", "crates", "net-e2e", "sidecar", "msrv"]) {
      const job = jobs.get(id);
      expect(job, `ci.yml no longer has a job called ${id}`).toBeTruthy();
      expect(job!.needs, `${id} should still depend on lockfile-preflight`).toContain("lockfile-preflight");
      // No `if:` means GitHub can only skip it as a cascade — so a SKIPPED here is always "did not run".
      expect(job!.conditional, `${id} gained a job-level if: — re-examine the skip verdict`).toBe(false);
    }
  });

  it("reads gui-smoke.yml and finds the windows job IS conditional, which is why a blanket rule fails", () => {
    const jobs = scanWorkflowJobs(guiSmokeYml);
    expect(jobs.get("gui-smoke")?.conditional).toBe(true);
    expect(jobs.get("gui-smoke")?.name).toContain("GUI smoke (windows-latest)");
  });

  it("the matcher set built from the real workflows explains the by-design skip and nothing else", () => {
    const matchers = explainableSkipMatchers([ciYml, guiSmokeYml]);
    const { explained, unexplained } = classifySkips(
      ["GUI smoke (windows-latest) — tauri-driver + WebdriverIO", "Server crates (windows-latest)", "MSRV check"],
      matchers,
    );
    expect(explained).toEqual(["GUI smoke (windows-latest) — tauri-driver + WebdriverIO"]);
    expect(unexplained).toEqual(["Server crates (windows-latest)", "MSRV check"]);
  });

  it("matches a bare job id EXACTLY, because a prefix is the fail-open direction (CPE-1906 r2)", () => {
    // Four of the six matchers this repo derives are ids of `name:`-less jobs. As `startsWith` prefixes
    // the id `catalog` excused BOTH of the names below — checks nothing had declared skippable. No live
    // collision today (they are release-workflow jobs that never reach a PR rollup), but silently
    // excusing a future `catalog-*` job is precisely the defect this file exists to remove.
    const releaseLike = `jobs:\n  catalog:\n    needs: release\n    if: \${{ !cancelled() }}\n    runs-on: x\n`;
    const matchers = explainableSkipMatchers([releaseLike]);
    expect(matchers).toEqual([{ text: "catalog", prefix: false }]);
    const { explained, unexplained } = classifySkips(
      ["catalog", "catalog-freshness nightly", "catalogue rebuild"],
      matchers,
    );
    expect(explained).toEqual(["catalog"]);
    expect(unexplained).toEqual(["catalog-freshness nightly", "catalogue rebuild"]);
  });

  it("keeps a PREFIX only where it is forced: a `${{ matrix.… }}` name GitHub expands at run time", () => {
    const matrixJob =
      `jobs:\n  shard:\n    if: github.event_name != 'push'\n` +
      `    name: GUI smoke shard \${{ matrix.shard }} of 4\n    runs-on: x\n`;
    expect(explainableSkipMatchers([matrixJob])).toEqual([{ text: "GUI smoke shard", prefix: true }]);
    const { explained } = classifySkips(["GUI smoke shard 3 of 4"], explainableSkipMatchers([matrixJob]));
    expect(explained).toEqual(["GUI smoke shard 3 of 4"]);
  });

  it("sees a job-level `if:` in the block-mapping form too, which used to read as unconditional", () => {
    // `if:` alone on its line with the expression indented beneath is legal YAML (a multi-line plain
    // scalar). It scanned as `conditional=false`, which fails CLOSED — the job's skips stop being
    // explainable and the poll over-blocks — but reformatting gui-smoke.yml's `if:` onto two lines
    // would then have exited 4 on every PR. The folded form (`if: >-`) already worked; it is the `>`
    // that satisfies the same-line test.
    const at = (body: string) => scanWorkflowJobs(`jobs:\n  a:\n${body}    runs-on: x\n`).get("a")?.conditional;
    expect(at("    if: github.event_name != 'push'\n")).toBe(true);
    expect(at("    if:\n      github.event_name != 'push'\n")).toBe(true);
    expect(at("    if: >-\n      github.event_name != 'push'\n")).toBe(true);
    expect(at("")).toBe(false);
  });

  it("fails CLOSED when the workflow scan comes back empty — every skip is then unexplained", () => {
    const { explained, unexplained } = classifySkips(["GUI smoke (windows-latest) — anything"], null);
    expect(explained).toEqual([]);
    expect(unexplained).toHaveLength(1);
  });
});

describe("ci-poll: job age is reported so 'slow or hung?' stops being a manual comparison (CPE-1906)", () => {
  const run = runPoll("pending", ["--pr", "1", "--budget", "2", "--interval", "1"]);

  it("still exits 2 for a genuinely pending board — the normal, expected outcome", () => {
    expect(run.status).toBe(2);
    expect(run.verdict).toMatch(/^CI VERDICT: pending —/);
    expect(run.verdict).toContain("CI still pending on");
  });

  it("carries the age and the NAME of the longest-running unfinished check", () => {
    expect(run.verdict).toMatch(/oldest_pending_min=6[23]/);
    expect(run.verdict).toContain('Oldest pending check: "Server crates (windows-latest)"');
    expect(run.verdict).toMatch(/running 6[23]m/);
    expect(run.verdict).toContain("compare that against the same job on a sibling PR");
  });

  it("keeps the existing output contract that sprint.md's runbooks quote", () => {
    for (const key of ["total_count=", "pending=", "mergeable=", "sha="]) expect(run.verdict).toContain(key);
  });

  it("a clean pending board reports gh_failures=0 rather than leaving the key out", () => {
    expect(run.verdict).toContain("gh_failures=0");
  });
});

describe("ci-poll: a pending verdict is not silent about `gh` failures it survived (CPE-1906 r2)", () => {
  // Measured pre-fix: one good read, then failed reads that never reached the 3-in-a-row bail, then the
  // deadline → `CI VERDICT: pending … exit 2` with NO mention of the failures anywhere on the line.
  // "Still pending" and "still pending, and I stopped being able to ask two thirds of the time" are
  // different situations and the caller was shown the same sentence for both.
  const dir = mkdtempSync(join(tmpdir(), "cpe-1906-flaky-"));
  const counter = join(dir, "n.txt");
  const run = spawnSync(process.execPath, [CI_POLL, "--pr", "1", "--budget", "8", "--interval", "1"], {
    cwd: REPO,
    encoding: "utf8",
    env: { ...process.env, CI_POLL_GH_SCRIPT: GH_STUB, GH_STUB_MODE: "flaky", GH_STUB_COUNTER: counter },
    timeout: 60_000,
  });
  rmSync(dir, { recursive: true, force: true });
  const verdict = (run.stdout ?? "").split(/\r?\n/).find((l) => l.startsWith("CI VERDICT:")) ?? "";

  it("still ends pending — the failures stayed under the bail threshold, so nothing was concluded", () => {
    expect(run.status).toBe(2);
    expect(verdict).toMatch(/^CI VERDICT: pending —/);
  });

  it("counts them on the machine-readable totals line", () => {
    const n = Number(/gh_failures=(\d+)/.exec(verdict)?.[1] ?? "0");
    expect(n).toBeGreaterThan(0);
  });

  it("appends the key rather than inserting it, so the pinned relative order is untouched", () => {
    const order = ["total_count=", "pending=", "oldest_pending_min=", "skipped=", "neutral=", "mergeable=", "sha=", "gh_failures="];
    let at = -1;
    for (const key of order) {
      const next = verdict.indexOf(key, at + 1);
      expect(next, `${key} missing from the verdict line`).toBeGreaterThan(at);
      at = next;
    }
  });
}, 120_000);

describe("bad usage exits 64 with one line, never a stack trace and never 'CI failed' (CPE-1906)", () => {
  const cases: Array<{ label: string; argv: string[]; expect: RegExp }> = [
    { label: "--interval 0", argv: ["--pr", "1", "--interval", "0"], expect: /--interval must be a positive number/ },
    { label: "a negative interval", argv: ["--pr", "1", "--interval", "-5"], expect: /--interval must be a positive number/ },
    { label: "a non-numeric interval", argv: ["--pr", "1", "--interval", "abc"], expect: /--interval must be a positive number/ },
    { label: "a zero budget", argv: ["--pr", "1", "--budget", "0"], expect: /budget must be a positive number/ },
    { label: "no target at all", argv: ["--budget", "60"], expect: /one of --run <run-id> or --pr <number> is required/ },
    { label: "an unknown flag", argv: ["--pr", "1", "--watch"], expect: /unknown argument: --watch/ },
    { label: "a flag with no value", argv: ["--pr"], expect: /--pr needs a value/ },
  ];

  for (const c of cases) {
    it(`${c.label} → exit 64`, () => {
      const res = spawnSync(process.execPath, [CI_POLL, ...c.argv], {
        cwd: REPO,
        encoding: "utf8",
        env: { ...process.env, CI_POLL_GH_SCRIPT: GH_STUB, GH_STUB_MODE: "green" },
        timeout: 60_000,
      });
      // 1 is "CI failed" in this file's own table. Reporting bad input as a red build is how someone
      // spends an hour debugging the wrong thing — the reason this ticket lists it as a defect.
      expect(res.status, `stderr: ${res.stderr}`).toBe(64);
      expect(res.stderr).toMatch(c.expect);
      expect(res.stderr).toContain("usage: node scripts/ci-poll.mjs");
      // A raw Node stack trace has `    at ` frames. One line of explanation plus one of usage, no more.
      expect(res.stderr).not.toMatch(/^\s+at .+:\d+:\d+/m);
      expect(res.stdout).not.toContain("CI VERDICT:");
    });
  }
});

describe("stall-check: bad input is a usage error, not an ENOENT stack trace (CPE-1906)", () => {
  const run = (argv: string[]) =>
    spawnSync(process.execPath, [STALL_CHECK, ...argv], { cwd: REPO, encoding: "utf8", timeout: 60_000 });

  it("a nonexistent report file exits 64 with one line", () => {
    const res = run([join(REPO, "no", "such", "report.txt")]);
    expect(res.status).toBe(64);
    expect(res.stderr).toMatch(/cannot read .*report\.txt/);
    expect(res.stderr).toContain("usage: node scripts/stall-check.mjs");
    expect(res.stderr).not.toMatch(/^\s+at .+:\d+:\d+/m);
  });

  it("a bad --prior exits 64 with the usage line too", () => {
    const res = run(["--prior", "not-a-number"]);
    expect(res.status).toBe(64);
    expect(res.stderr).toContain("usage: node scripts/stall-check.mjs");
  });

  it("an unexpected second positional exits 64", () => {
    const res = run(["a.txt", "b.txt"]);
    expect(res.status).toBe(64);
    expect(res.stderr).toContain("unexpected argument b.txt");
  });
});

// ─────────────────────────────────────────────────────────────────────────────────────────────────────
// CPE-1970 — a GREEN board that a guard on `main` never appeared on.
//
// THE SHAPE, reconstructed from the real merge. PR #1056: 22 checks, zero failures, the newest one
// finished at 18:35:13Z and the PR merged at 18:36:20Z — sixty-seven seconds later, on a board that had
// only just gone green. `ratchet-guard` had landed on `main` at 17:42:59Z and is absent from all 22.
// Every predicate this file already has says that board is fine, because every one of them looks only
// at what IS on the board.
//
// WHY THE TWO DIRECTIONS ARE THE SAME PAYLOAD. `guard-gap` emits one identical rollup for both tests;
// only the base-workflow fixture changes. A refusal test that used a different board for the green leg
// would prove the tool can say both words, not that it says them about the same thing for the right
// reason.
describe("ci-poll: a guard `main` carries that never reached the board is not green (CPE-1970)", () => {
  const stale = runPoll("guard-gap", ["--pr", "1056", "--budget", "6", "--interval", "1"], BASE_WORKFLOWS_AHEAD);
  const fresh = runPoll("guard-gap", ["--pr", "1056", "--budget", "6", "--interval", "1"], BASE_WORKFLOWS);

  it("REFUSES the #1056 board: exit 5, and the prefix is its own, not `success` and not `failure`", () => {
    expect(stale.status).toBe(5);
    expect(stale.verdict).toMatch(/^CI VERDICT: completed stale-checks —/);
    expect(stale.verdict).not.toMatch(/^CI VERDICT: completed success/);
  });

  it("names the guard that did not judge it, and says what to do", () => {
    expect(stale.verdict).toContain("Ratchet guard — no baseline raised without a declaration");
    expect(stale.verdict).toContain("Nothing on this board is red, and that is the problem");
    expect(stale.verdict).toContain("Rebase onto `main` and let CI re-run before merging");
    expect(stale.verdict).toContain("Do not merge on this board");
  });

  it("PASSES the identical board when `main` has not moved: exit 0, coverage=ok", () => {
    expect(fresh.status).toBe(0);
    expect(fresh.verdict).toMatch(/^CI VERDICT: completed success —/);
    expect(fresh.verdict).toContain("coverage=ok");
    expect(fresh.verdict).not.toContain("stale-checks");
  });

  it("the two fixtures differ in exactly the one job — derived, so the pair cannot rot into two boards", () => {
    // CPE-1933: the claim "these are the same `main` plus one guard" is checked against the files
    // rather than written in a comment. Change either fixture's job list and this reds.
    const jobsIn = (dir: string) => [...scanWorkflowJobs(readFileSync(join(dir, "ci.yml"), "utf8")).keys()].sort();
    const before = jobsIn(BASE_WORKFLOWS);
    const after = jobsIn(BASE_WORKFLOWS_AHEAD);
    expect(after.filter((j) => !before.includes(j))).toEqual(["ratchet-guard"]);
    expect(before.filter((j) => !after.includes(j))).toEqual([]);
  });

  it("counts the gap on the machine-readable totals line", () => {
    expect(stale.verdict).toContain("coverage=1-unjudged");
    // The count is derived from the list it printed, not from a separate tally that could drift.
    const named = (stale.verdict.match(/Ratchet guard — no baseline raised without a declaration/g) ?? []).length;
    expect(named).toBe(Number(/coverage=(\d+)-unjudged/.exec(stale.verdict)?.[1]));
  });

  it("appends `coverage=` after `gh_failures=`, leaving the pinned key order untouched", () => {
    const order = [
      "total_count=",
      "pending=",
      "oldest_pending_min=",
      "skipped=",
      "neutral=",
      "mergeable=",
      "sha=",
      "gh_failures=",
      "coverage=",
    ];
    for (const line of [stale.verdict, fresh.verdict]) {
      let at = -1;
      for (const key of order) {
        const next = line.indexOf(key, at + 1);
        expect(next, `${key} missing from ${line}`).toBeGreaterThan(at);
        at = next;
      }
    }
  });

  it("says which `main` it read the guard set from, because a stale local ref under-reports", () => {
    for (const line of [stale.verdict, fresh.verdict]) expect(line).toContain("Guard set read from seam:");
  });
});

describe("ci-poll: the coverage check is narrow ON PURPOSE, and its carve-outs are asserted (CPE-1970)", () => {
  const fresh = runPoll("guard-gap", ["--pr", "1", "--budget", "6", "--interval", "1"], BASE_WORKFLOWS);

  it("a workflow that does not run on pull_request is never counted as missing", () => {
    // `release.yml` in the fixture has one job, `Publish installers`, and no `pull_request` trigger.
    // Without this carve-out this repo's four release/schedule workflows would put ~7 permanently
    // absent jobs on every verdict — a gate that reds every PR is a gate that gets aliased away.
    expect(fresh.status).toBe(0);
    expect(fresh.stdout).not.toContain("Publish installers");
  });

  it("a PR-triggered workflow that contributed NO check is not flagged — but it IS named", () => {
    // The `paths-ignore` case: a Ticketing-only PR legitimately gets no `ci.yml` checks. Silence here
    // is a deliberate fail-open, so the operator is shown it rather than left to infer it.
    expect(fresh.verdict).not.toContain("Nightly sweep A");
    expect(fresh.stdout).toContain("nightly.yml contributed no check to this board");
  });

  it("`--run` mode says the check did not apply rather than going quiet about it", () => {
    // One workflow run's job list cannot answer "did every job main requires across ALL workflows
    // appear". Out of scope — and out of scope must not read the same as passed.
    const run = runPoll("run-failure-no-failing-job", ["--run", "1", "--budget", "4", "--interval", "1"]);
    expect(run.verdict).toContain("coverage=n/a(run-mode)");
    expect(run.status).toBe(1); // unchanged by CPE-1970: red still outranks everything
  });

  it("a board still pending reports n/a, because jobs enter a rollup in waves", () => {
    const run = runPoll("pending", ["--pr", "1", "--budget", "4", "--interval", "1"]);
    expect(run.status).toBe(2);
    expect(run.verdict).toContain("coverage=n/a(board-pending)");
  });
});

describe("ci-poll: 'could not compute the coverage' is not 'nothing to check' (CPE-1970)", () => {
  // The eleventh instance of the house rule this file exists to enforce, in the guard added to enforce
  // it. A coverage check that goes quiet when it cannot read `main` is worse than no coverage check:
  // the verdict line then carries a `coverage=` field that means nothing and reads like assurance.
  it("an unreadable base → exit 5 with its own prefix, never exit 0", () => {
    const run = runPoll("guard-gap", ["--pr", "1", "--budget", "6", "--interval", "1"], join(REPO, "no", "such", "dir"));
    expect(run.status).toBe(5);
    expect(run.verdict).toMatch(/^CI VERDICT: completed coverage-unknown —/);
    expect(run.verdict).toContain("coverage=unknown");
    expect(run.verdict).toContain("Do not merge");
  });

  it("an EMPTY base directory is the same answer — zero workflows is not zero requirements", () => {
    const dir = mkdtempSync(join(tmpdir(), "cpe-1970-empty-"));
    try {
      const run = runPoll("guard-gap", ["--pr", "1", "--budget", "6", "--interval", "1"], dir);
      expect(run.status).toBe(5);
      expect(run.verdict).toMatch(/^CI VERDICT: completed coverage-unknown —/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("a red board is still reported red — coverage never outranks a failure", () => {
    const run = runPoll("failure-and-skips", ["--pr", "1", "--budget", "6", "--interval", "1"], join(REPO, "nope"));
    expect(run.status).toBe(1);
    expect(run.verdict).toMatch(/^CI VERDICT: completed failure —/);
  });
});

describe("ci-poll: the required-job set is DERIVED from real workflows, not from the fixtures (CPE-1950)", () => {
  // A fixture pair proves the two directions agree with each other; it cannot prove either matches the
  // repo. This leg reads THIS repo's own `.github/workflows` through the shipped functions, so a change
  // to how jobs or triggers are written reds here rather than passing on a fixture nobody updated.
  const base = readBaseWorkflowSources("HEAD");

  it("reads the real workflow set out of a git revision", () => {
    expect(base, "readBaseWorkflowSources('HEAD') could not read .github/workflows").toBeTruthy();
    expect(base!.files.map((f: { file: string }) => f.file)).toContain("ci.yml");
  });

  it("classifies this repo's real triggers: ci.yml and gui-smoke.yml are PR-triggered, release.yml is not", () => {
    const textOf = (name: string) => base!.files.find((f: { file: string }) => f.file === name)?.text ?? "";
    expect(workflowTriggersPullRequest(textOf("ci.yml"))).toBe(true);
    expect(workflowTriggersPullRequest(textOf("gui-smoke.yml"))).toBe(true);
    expect(workflowTriggersPullRequest(textOf("release.yml"))).toBe(false);
  });

  it("would have caught #1056 against the REAL ci.yml — the guard's own name read out of the file", () => {
    const ci = base!.files.find((f: { file: string }) => f.file === "ci.yml")!.text;
    const guardLabel = scanWorkflowJobs(ci).get("ratchet-guard")?.name;
    expect(guardLabel, "ci.yml no longer has a `ratchet-guard` job — re-derive this test").toBeTruthy();
    // #1056's board, minus that one job: every OTHER PR-triggered job's label, present and passing.
    const everyOtherLabel: string[] = [];
    for (const f of base!.files as { file: string; text: string }[]) {
      if (!workflowTriggersPullRequest(f.text)) continue;
      for (const [id, job] of scanWorkflowJobs(f.text)) {
        const label = job.name ?? id;
        if (label === guardLabel) continue;
        everyOtherLabel.push(label.includes("${{") ? `${label.split("${{")[0]}(ubuntu-latest)` : label);
      }
    }
    const gap = coverageOf(everyOtherLabel, base!.files);
    expect(gap.state).toBe("unjudged");
    expect(gap.unjudged.map((u: { label: string }) => u.label)).toEqual([guardLabel]);
    // …and the same board WITH it is clean, so the assertion above is about that job and not about a
    // scanner that flags everything.
    expect(coverageOf([...everyOtherLabel, guardLabel!], base!.files).state).toBe("ok");
  });
});
