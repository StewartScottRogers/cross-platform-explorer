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
//                 New: a distinct `completed skipped` verdict and exit 4 — but ONLY for a skip no
//                 job-level `if:` explains, because this repo skips one check on every PR by design.
import { describe, it, expect } from "vitest";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { scanWorkflowJobs, explainableSkipMatchers, classifySkips } from "../../scripts/ci-poll.mjs";

const REPO = process.cwd();
const CI_POLL = join(REPO, "scripts", "ci-poll.mjs");
const STALL_CHECK = join(REPO, "scripts", "stall-check.mjs");
const GH_STUB = join(REPO, "src", "lib", "fixtures", "ghStub.mjs");

/** Run the REAL script as a child process against a stubbed `gh`, exactly as a caller would. */
function runPoll(mode: string, args: string[]) {
  const startedAt = Date.now();
  const res = spawnSync(process.execPath, [CI_POLL, ...args], {
    cwd: REPO,
    encoding: "utf8",
    env: { ...process.env, CI_POLL_GH_SCRIPT: GH_STUB, GH_STUB_MODE: mode },
    timeout: 120_000,
  });
  const stdout = res.stdout ?? "";
  const verdict = stdout.split(/\r?\n/).find((l) => l.startsWith("CI VERDICT:")) ?? "";
  return { status: res.status, stdout, stderr: res.stderr ?? "", verdict, elapsedMs: Date.now() - startedAt };
}

describe("ci-poll: a gh that ERRORS is never reported as pending (CPE-1906)", () => {
  const run = runPoll("error", ["--pr", "999999", "--budget", "4", "--interval", "1"]);

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
    expect(cascade.verdict).toMatch(/^CI VERDICT: completed skipped —/);
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
});

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
