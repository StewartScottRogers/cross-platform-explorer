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
  readOnBlock,
  readBaseWorkflowSources,
  coverageOf,
  PR_EVENTS,
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
    // ROUND 2: this used to be asserted on the two lines someone remembered while the doc claimed it of
    // EVERY verdict — and `formatVerdict` appended the ref in exactly two of its seven branches. Now
    // derived: every stub whose board reaches the coverage check must carry it, whatever the verdict.
    for (const line of [stale.verdict, fresh.verdict]) expect(line).toContain("Guard set read from seam:");
    for (const mode of ["failure-and-skips", "skip-cascade", "all-skipped-by-design", "green", "guard-gap"]) {
      const run = runPoll(mode, ["--pr", "1", "--budget", "6", "--interval", "1"], BASE_WORKFLOWS);
      if (run.verdict.includes("coverage=n/a") || run.verdict.includes("CI VERDICT: pending")) continue;
      expect(run.verdict, `${mode} printed no guard-set ref: ${run.verdict}`).toContain("Guard set read from seam:");
    }
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

  it("a silent workflow is excused ONLY because its own `pull_request:` carries a path filter", () => {
    // Round 2 narrowed this from a blanket carve-out. `nightly.yml` in the fixture declares
    // `pull_request: { paths: [...] }`, so GitHub was entitled not to run it — that, and nothing else,
    // is what buys the excuse. The operator is still shown it rather than left to infer it, and the
    // machine-readable token says `ok(1-silent)` rather than a bare `ok`.
    expect(fresh.verdict).not.toContain("Nightly sweep A");
    expect(fresh.stdout).toContain("nightly.yml contributed no check to this board");
    expect(fresh.stdout).toContain("path-filtered on every PR trigger it declares");
    expect(fresh.verdict).toContain("coverage=ok(1-silent)");
    // …and the excuse really is derived from that key: strip it and the same board reds.
    const filtered = readFileSync(join(BASE_WORKFLOWS, "nightly.yml"), "utf8");
    expect(filtered, "the fixture no longer declares the `paths:` this test is about").toContain("paths:");
    expect(readOnBlock(filtered).prPathFiltered).toBe(true);
    expect(readOnBlock(filtered.replace(/^\s*paths:.*$/m, "    branches: [main]")).prPathFiltered).toBe(false);
  });

  it("a PR-triggered workflow with NO path filter that contributed nothing is UNJUDGED, not excused", () => {
    // The whole-workflow blind spot round 1 shipped: a board carrying zero `ci.yml` checks returned
    // `ok`, exit 0, with every `ci.yml` guard absent. Measured before narrowing the rule: across all
    // 186 merges in the window, 0 boards were missing the `CI` workflow and 0 were missing `GUI smoke`,
    // so this costs zero real firings. Asserted against THIS repo's real `ci.yml`, not a fixture.
    const ci = readFileSync(join(REPO, ".github", "workflows", "ci.yml"), "utf8");
    expect(readOnBlock(ci)).toMatchObject({ trigger: "pull-request", prPathFiltered: false });
    const gap = coverageOf(["some check from another workflow"], [{ file: "ci.yml", text: ci }]);
    expect(gap.state).toBe("unjudged");
    expect(gap.silentWorkflows).toEqual([]);
    expect(gap.unjudged.length).toBeGreaterThan(5);
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

// ─────────────────────────────────────────────────────────────────────────────────────────────────────
// CPE-1970 ROUND 2 — the `on:`-block scanner used to fail OPEN, silently, on legal YAML, and the shape
// was one reformat away from live. Every case below was reproduced against the REAL
// `.github/workflows/ci.yml` before the fix; each one removed the entire workflow from the required set
// with `coverage=ok` still printed. CLAUDE.md rule 2: a whole-line-comment filter is not enough, and
// this had no comment handling at all.
describe("ci-poll: an `on:` block the scanner cannot read fails CLOSED, never quietly false (CPE-1970)", () => {
  const CI = readFileSync(join(REPO, ".github", "workflows", "ci.yml"), "utf8");
  // A PR-scoped event name this classifier has never heard of — the "one GitHub adds later" case the
  // header's blind-spot bullet is about. Deliberately not a real event: the point is that the guard
  // fires on the SHAPE of the name, so nobody has to have heard of it first.
  const FUTURE_PR_EVENT = "on:\n  pull_request_v2:\n    branches: [main]\njobs:\n  f:\n    name: Future\n";

  it("a column-0 comment inside `on:` no longer deletes the workflow from the required set", () => {
    // The red-proof, run by hand before the fix: `true` for the real file, `false` for the same bytes
    // with `# a comment` at column 0 under `on:`, and `coverageOf` then returned `ok` for a one-check
    // board. Revert `readOnBlock`'s comment handling and the second expectation below goes red.
    expect(workflowTriggersPullRequest(CI)).toBe(true);
    const injected = CI.replace(/^on:\s*$/m, "on:\n# a comment");
    expect(injected).not.toBe(CI);
    expect(workflowTriggersPullRequest(injected)).toBe(true);
    expect(coverageOf(["Frontend — type-check and test"], [{ file: "ci.yml", text: injected }]).state).toBe("unjudged");
  });

  it("reads the legal spellings that used to answer `false`: quoted keys, deeper indent, flow seq", () => {
    for (const src of [
      'on:\n  "pull_request":\n    branches: [main]\n',
      '"on":\n  pull_request:\n    branches: [main]\n',
      "on:\n    pull_request:\n        branches: [main]\n",
      "on: [push, pull_request]\n",
      "on:\n  - push\n  - pull_request\n",
      "on: # a trailing comment\n  pull_request:\n",
      // Round 3. A trailing comment in the `on:` BLOCK BODY, which round 2's header claimed to strip
      // and did not — the SEQUENCE spelling answered `unknown` (fail-closed, but `coverage=unknown` on
      // every board until someone deleted the comment).
      "on:\n  - push\n  - pull_request  # only PRs\n",
      // ROUND 4 — the MAPPING spelling below never exhibited that bug, and round 3's comment implied it
      // did. Re-measured against round 2's own module (`git show <round-2>:scripts/ci-poll.mjs`):
      //   `on:\n  - push\n  - pull_request  # only PRs`   round 2 → `unknown`       round 3+ → `true`
      //   `on:\n  push:\n  pull_request:  # only PRs`     round 2 → `true` ALREADY  round 3+ → `true`
      // Round 2's key regex ended `(:.*)?$`, which swallowed `:  # only PRs` whole. Worth keeping as a
      // case; not worth attributing a failure it never had.
      "on:\n  push:\n  pull_request:  # only PRs\n",
    ]) {
      expect(workflowTriggersPullRequest(src), src).toBe(true);
    }
    for (const src of ["on:\n  push:\n    branches: [main]\n", "on: [push]\n", "on:\n  schedule:\n    - cron: '0 7 * * *'\n"]) {
      expect(workflowTriggersPullRequest(src), src).toBe(false);
    }
  });

  it("classifies `pull_request_target` as a PR trigger — it was a confident `false` with no trace", () => {
    // ROUND 3 BLOCKER. The block-key loop compared the key to `"pull_request"` exactly, so a
    // `pull_request_target`-only workflow answered `{trigger:"other"}` and `coverageOf` skipped the
    // file entirely: no `unjudged` row, no `silentWorkflows` entry, a bare `coverage=ok` at exit 0 with
    // that workflow's whole guard set absent. Reproduced below end to end.
    for (const src of [
      "on:\n  pull_request_target:\n    branches: [main]\n",
      "on: [pull_request_target]\n",
      "on: pull_request_target\n",
      'on:\n  "pull_request_target":\n',
      "on:\n  - pull_request_target\n",
    ]) {
      expect(workflowTriggersPullRequest(src), src).toBe(true);
    }
    // `\b` is why this hid: `\bpull_request\b` does NOT match inside `pull_request_target` (`_` is a
    // word character), so the inline branch missed it too, for a different reason than the block one.
    expect(/\bpull_request\b/.test("on: [pull_request_target]")).toBe(false);
    // The end-to-end shape: a board carrying every `ci.yml` check and nothing from `security.yml`.
    const security = "on:\n  pull_request_target:\n    branches: [main]\njobs:\n  codeql:\n    name: CodeQL\n";
    const cov = coverageOf(["Frontend — type-check and test"], [{ file: "security.yml", text: security }]);
    expect(cov.state).toBe("unjudged");
    expect(cov.unjudged.map((u) => u.label)).toEqual(["CodeQL"]);
    // …and it takes `paths:` on the same terms, so the silent-workflow carve-out needed no special case.
    expect(readOnBlock("on:\n  pull_request_target:\n    paths: [src/**]\n").prPathFiltered).toBe(true);
  });

  it("the PR-event list is a literal pair, named — the standing blind spot the header points at", () => {
    // Nothing in `readOnBlock` can notice a THIRD PR-scoped event name: it compares against a list.
    // ROUND 4 — and round 3's claim that "only the enumeration test's `toEqual` will" was FALSE, in
    // three places at once (here, the header, and §2d). That `toEqual` runs over a FILTERED array, so
    // an unrecognised event drops out and leaves it green; see the shape-based `unknownPrLike` guard
    // in `every real workflow in this repo still classifies …`, which is what actually notices now.
    expect([...PR_EVENTS].sort()).toEqual(["pull_request", "pull_request_target"]);
    // WHY `pull_request_review` IS NOT IN THE PAIR, stated rather than left to read as an oversight:
    // it fires on a review being submitted, NOT on `opened`/`synchronize`, so a `pull_request_review`
    // workflow legitimately produces no check run on a PR nobody has reviewed yet. Classing it as
    // PR-triggered would put its jobs in the required set and make `coverage` refuse EVERY unreviewed
    // PR — over-blocking, which is the outcome that gets the gate aliased away. Same for
    // `pull_request_review_comment`. This is a DECISION, not the blind spot bullet 1 describes; the
    // `unknownPrLike` guard still reds if one lands, so the decision gets re-taken with a file in hand
    // rather than silently inherited.
    for (const src of [
      "on:\n  pull_request_review:\n",
      "on:\n  pull_request_review_comment:\n",
      "on: [pull_request_review]\n",
    ]) {
      expect(workflowTriggersPullRequest(src), `${src} — a PR-adjacent event we deliberately do NOT class as one`).toBe(
        false,
      );
    }
  });

  it("`prPathFiltered` needs EVERY PR trigger filtered, not the first one found", () => {
    // The excuse for silence must not be bought by one of two triggers: a workflow with a path-filtered
    // `pull_request:` and an unfiltered `pull_request_target:` still runs on every diff. Round 2's loop
    // broke at the first PR key, so widening the class without this would have opened a new fail-open.
    const both = (a: string, b: string) => `on:\n  pull_request:\n    ${a}\n  pull_request_target:\n    ${b}\n`;
    expect(readOnBlock(both("paths: [src/**]", "paths-ignore: [docs/**]")).prPathFiltered).toBe(true);
    expect(readOnBlock(both("paths: [src/**]", "branches: [main]")).prPathFiltered).toBe(false);
    expect(readOnBlock(both("branches: [main]", "paths: [src/**]")).prPathFiltered).toBe(false);
  });

  it("a `#` inside a quoted scalar on the `on:` line is not a comment — round 2 said this and it wasn't", () => {
    // ROUND 3 BLOCKER 2. The header's tri-state paragraph named `on: ["push#1"]` as landing in
    // `unknown`; measured, it landed in a confident `false`. The regex it used, `/(^|\s)#.*$/`, is
    // quote-blind, so the worse spelling below cut at the `#` inside the quoted scalar and ate the
    // `pull_request` after it. Now parsed correctly rather than merely closed.
    expect(readOnBlock('on: ["a #b", pull_request]\n')).toMatchObject({ trigger: "pull-request" });
    expect(readOnBlock('on: ["push#1"]\n')).toMatchObject({ trigger: "other" });
    expect(readOnBlock("on: ['a #b', pull_request_target]\n")).toMatchObject({ trigger: "pull-request" });
    expect(/(^|\s)#.*$/.exec(' ["a #b", pull_request]')?.[0], "the regex that could not see the quoting").toBe(
      ' #b", pull_request]',
    );
  });

  it("an unclassifiable `on:` is `null`, and `coverageOf` turns that into exit-5 territory", () => {
    // Every shape the header's "cannot see" list says lands in `unknown` is RUN here rather than
    // asserted in prose — round 2's list named one that landed in `false` instead.
    for (const src of [
      "jobs:\n  a:\n    name: A\n",
      "on:\nname: X\n",
      "on: >\n  push\n",
      // Round 3 minor: a YAML anchor/alias is not resolved. Round 2 answered a confident `other` for
      // the first of these, silently dropping the whole workflow. GitHub rejects anchors, so this is
      // unreachable in practice — it is fail-closed and named in the header rather than left absent.
      "on: &trig\n  pull_request:\n",
      "on: *trig\n",
      'on: ["push\n',
      // Round 5: a flow collection that does not close on the `on:` line. See the dedicated test below
      // for why this used to be a confident `false` rather than a `null`.
      "on: [push,\n  pull_request]\n",
      "on: {push: null,\n  pull_request: null}\n",
    ]) {
      expect(workflowTriggersPullRequest(src), src).toBeNull();
    }
    expect(readOnBlock("on: &trig\n  pull_request:\n").why).toContain("anchor");
    const cov = coverageOf(["anything"], [{ file: "mystery.yml", text: "on:\nname: X\njobs:\n  a:\n    name: A\n" }]);
    expect(cov.state).toBe("unknown");
    expect(cov.detail).toContain("mystery.yml");
  });

  it("the flow-mapping path filter is missed — over-blocking, and by OMISSION not by construction", () => {
    // Round 3 minor 4. Round 2's comment said a trigger written inline "cannot carry a path filter", so
    // `prPathFiltered: false` was true by construction. It is legal and it does; the classifier just
    // does not read it. Pinned in the safe direction: `false` refuses the excuse for silence, so such a
    // workflow's absence is called unjudged rather than waved through. Flip this to `true` on the day
    // someone teaches the inline branch flow mappings — and delete the header's bullet with it.
    const flow = "on: {pull_request: {paths: ['src/**']}}\n";
    expect(workflowTriggersPullRequest(flow)).toBe(true);
    expect(readOnBlock(flow).prPathFiltered, "if this is now true the header's `cannot see` list is stale").toBe(false);
  });

  it("a multi-line flow `on:` was a confident `false` — the flow sweep UNDER-reports too, not only over", () => {
    // ROUND 5 BLOCKER. Round 4's comment on the flow branch said the token sweep's error direction was
    // over-reporting only ("reds a workflow that is fine rather than passing one that is not"). It is
    // TWO-SIDED. A YAML flow collection may span lines, and the `on:` scanner captures only the
    // remainder of the `on:` LINE, so the continuation was invisible to BOTH `trigger` and `events`.
    const multi = "on: [push,\n  pull_request]\njobs:\n  codeql:\n    name: CodeQL\n";
    // The one-line spelling is the control: same events, same file, one line break apart.
    const oneLine = "on: [push, pull_request]\njobs:\n  codeql:\n    name: CodeQL\n";
    expect(readOnBlock(oneLine)).toMatchObject({ trigger: "pull-request", events: ["push", "pull_request"] });
    // Before the refusal: `{trigger: "other", events: ["push"]}` and a confident `false`. Now `null`.
    expect(workflowTriggersPullRequest(multi)).toBeNull();
    expect(readOnBlock(multi).why).toContain("flow collection");
    // …and the round-4 `events` guard could not have caught it either: a `pull_request_v2` on the
    // continuation line is equally absent from the swept string.
    expect(readOnBlock("on: [push,\n  pull_request_v2]\n").events).not.toContain("pull_request_v2");
    expect(workflowTriggersPullRequest("on: {push: null,\n  pull_request: null}\n")).toBeNull();

    // END TO END, the reviewer's reproduction: a board carrying every real `ci.yml` check and nothing
    // from a `security.yml` spelled that way. Before the refusal this returned
    // `{state:"ok", unjudged:[], judgedWorkflows:["ci.yml"], silentWorkflows:[], detail:"every job
    // `main` requires from ci.yml produced a check here"}` — round 3's `pull_request_target` defect
    // character for character, detail string included, with CodeQL's guard set silently absent.
    const base = readBaseWorkflowSources("HEAD") as { files: { file: string; text: string }[] };
    const ci = base.files.find((f) => f.file === "ci.yml")!;
    const board = [...scanWorkflowJobs(ci.text)].map(([id, job]) => (job as { name?: string }).name ?? id);
    const cov = coverageOf(board, [ci, { file: "security.yml", text: multi }]);
    expect(cov.state, "a multi-line-flow workflow must not be waved through as `ok`").toBe("unknown");
    expect(cov.detail).toContain("security.yml");
    // `unknown`, not `unjudged`: round 3 could WIDEN the class because `pull_request_target` is a
    // well-understood event, but nothing here read the continuation line, so "did not run" is the
    // honest answer. `coverageOf` blocks on it by name either way.

    // NEITHER INSTRUMENT DOMINATES, asserted rather than argued (CLAUDE.md rule: derive, don't claim).
    // The brief for round 4 asked for a raw grep; the parse replaced it. Each catches what the other
    // misses, so the swap traded a false-positive class for a false-negative class.
    const greps = (s: string) => /pull_request/.test(s);
    const events = (s: string) => readOnBlock(s).events.join(" ");
    // (a) THE GREP WINS HERE: the continuation line is in the bytes, and was never in `events`.
    expect(greps(multi)).toBe(true);
    expect(events(multi)).toBe("");
    // (b) THE PARSE WINS HERE: all five comment positions naming a PR-ish event inside `on:` red a
    // grep and are correctly ignored by the parse. `ci.yml`'s real `on:` block carries ~60 lines of
    // commentary, so this is not hypothetical.
    const commented = [
      "on:\n# pull_request_review\n  push:\n", // column 0
      "on:\n  # pull_request_review\n  push:\n", // indented
      "on:\n  push:  # pull_request_review\n", // trailing on a block key
      "on:  # pull_request_review\n  push:\n", // trailing on the `on:` line
      "on: [push]  # pull_request_review\n", // trailing after a flow seq
    ];
    for (const src of commented) {
      expect(greps(src), `a raw grep reds on this comment: ${JSON.stringify(src)}`).toBe(true);
      expect(events(src), `the parse must ignore this comment: ${JSON.stringify(src)}`).not.toContain("pull_request");
      expect(workflowTriggersPullRequest(src), src).toBe(false);
    }

    // The refusal counts `[`/`{` in `splitInlineComment`'s QUOTE-AWARE loop, not over the returned
    // string, which still carries its quotes. A naive count answers 1 for the line below and would
    // refuse a workflow the classifier reads correctly — a new false positive bought with the fix.
    expect(readOnBlock('on: ["a[b", pull_request]\n')).toMatchObject({ trigger: "pull-request" });
    expect(readOnBlock('on: ["a]b", pull_request]\n')).toMatchObject({ trigger: "pull-request" });
  });

  it("a PR-triggered workflow whose jobs cannot be read is `unknown`, not an empty requirement", () => {
    const cov = coverageOf(["anything"], [{ file: "empty.yml", text: "on:\n  pull_request:\n" }]);
    expect(cov.state).toBe("unknown");
    expect(cov.detail).toContain("empty.yml");
  });

  it("a column-0 comment inside `jobs:` no longer truncates the job list", () => {
    // Pre-existing from CPE-1906 — `["a"]` before the fix. Harmless while the only consumer was the
    // skip matcher; the coverage check is the first consumer for which a short job list fails OPEN.
    expect([...scanWorkflowJobs("jobs:\n  a:\n    name: A\n# c\n  b:\n    name: B\n").keys()]).toEqual(["a", "b"]);
    // …and the same hole in a block `needs:` list, which shortens the skip closure instead.
    const jobs = scanWorkflowJobs("jobs:\n  a:\n    name: A\n  b:\n    name: B\n    needs:\n      - a\n# note\n      - c\n");
    expect(jobs.get("b")?.needs).toEqual(["a", "c"]);
  });

  it("every real workflow in this repo still classifies, so the tri-state is not hiding a regression", () => {
    // Enumerate, don't recall (CPE-1932): read the directory, refuse a near-empty answer, and require a
    // definite yes/no for every file. A `null` here means a live workflow the scanner cannot read —
    // which now blocks at exit 5 rather than going quiet, so it must never be normal.
    const base = readBaseWorkflowSources("HEAD");
    expect(base!.files.length).toBeGreaterThan(5);
    for (const f of base!.files as { file: string; text: string }[]) {
      expect(workflowTriggersPullRequest(f.text), `${f.file} could not be classified`).not.toBeNull();
    }
    const prTriggered = (base!.files as { file: string; text: string }[])
      .filter((f) => workflowTriggersPullRequest(f.text))
      .map((f) => f.file)
      .sort();
    expect(prTriggered).toEqual(["ci.yml", "gui-smoke.yml"]);
    // ROUND 4. That `toEqual` canNOT notice a new PR-scoped event arriving, which is what round 3
    // wrote here and in the header. `prTriggered` is a FILTER — a workflow classified `other` is
    // REMOVED from the array, so the `toEqual` still holds and the suite stays green; it can only red
    // on OVER-inclusion, or on `ci.yml`/`gui-smoke.yml` dropping out. Measured over this same
    // `readBaseWorkflowSources("HEAD")` set plus one hypothetical file: `review-gate.yml`
    // (`on: pull_request_review:`) and `future.yml` (`on: pull_request_v2:`) both classify `false` and
    // both leave the `toEqual` GREEN. Round 3's second backstop was a text grep for the
    // `pull_request_target` LITERAL, which caught that one name and nothing else — so for the case the
    // header is actually about, "one GitHub adds later", all three were silent at once.
    //
    // So the check is by SHAPE, off the parsed `on:` keys rather than the raw bytes: `readOnBlock`
    // now returns the `events` it read (comments already stripped, per CLAUDE.md rule 2 — a raw grep
    // would match `pull_request_review` sitting in one of `ci.yml`'s ~60 lines of `on:` commentary).
    // Anything that LOOKS PR-scoped and is not a name this module knows reds here, naming the file.
    const unknownPrLike = (base!.files as { file: string; text: string }[])
      .flatMap((f) => readOnBlock(f.text).events.map((e) => ({ file: f.file, event: e })))
      .filter((x) => /^pull_request[_a-z0-9]*$/.test(x.event) && !PR_EVENTS.includes(x.event))
      .map((x) => `${x.file}: ${x.event}`)
      .sort();
    expect(
      unknownPrLike,
      "a PR-SHAPED NAME this classifier does not know is now parsed out of an `on:` block. It may not be " +
        "an event at all: the flow branch is a token sweep, so `on: {push: {paths: ['pull_request_v2/**']}}` " +
        "lands its path glob here too (the only over-report the round-5 review could induce). Read the " +
        "named file's `on:` key, then decide whether it belongs in `PR_EVENTS` or whether the sweep " +
        "merely swept — and re-read `readOnBlock`'s header either way",
    ).toEqual([]);
    // POSITIVE CONTROL, inline rather than left to whoever remembers to red-proof: the same expression
    // over the same real files plus one hypothetical `pull_request_v2` workflow DOES fire. Without
    // this, an `events` that silently came back `[]` would leave the assertion above green forever —
    // which is precisely the failure mode round 3 shipped one guard earlier.
    const withFuture = [...(base!.files as { file: string; text: string }[]), { file: "future.yml", text: FUTURE_PR_EVENT }];
    expect(
      withFuture
        .flatMap((f) => readOnBlock(f.text).events.map((e) => ({ file: f.file, event: e })))
        .filter((x) => /^pull_request[_a-z0-9]*$/.test(x.event) && !PR_EVENTS.includes(x.event))
        .map((x) => `${x.file}: ${x.event}`),
    ).toEqual(["future.yml: pull_request_v2"]);
    // …and the classifier itself still says `false` for it, so the `toEqual` above genuinely could not
    // have caught it. Both halves of round 4's finding, asserted side by side.
    expect(workflowTriggersPullRequest(FUTURE_PR_EVENT)).toBe(false);
    expect([...withFuture].filter((f) => workflowTriggersPullRequest(f.text)).map((f) => f.file).sort()).toEqual([
      "ci.yml",
      "gui-smoke.yml",
    ]);
    // §2d of docs/design/CI-STALENESS.md rests on this: NEITHER carries a `pull_request:` path filter,
    // so no PR in this repo can legitimately be missing either workflow's checks.
    for (const f of base!.files as { file: string; text: string }[]) {
      if (!workflowTriggersPullRequest(f.text)) continue;
      expect(readOnBlock(f.text).prPathFiltered, `${f.file} now path-filters pull_request — §2d needs rewriting`).toBe(
        false,
      );
    }
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
