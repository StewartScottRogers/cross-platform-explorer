#!/usr/bin/env node
// CPE-1906 — a stand-in for `gh`, so `ci-poll.mjs`'s failure paths can be driven end to end.
//
// WHY A STUB AND NOT A PATH SHIM. The obvious way to fake `gh` is to put a script called `gh` first on
// PATH. It does not work on Windows: since the CVE-2024-27980 fix Node refuses to spawn a `.bat`/`.cmd`
// without `shell: true`, so a Windows developer could not run the test at all — and this repo's crew
// runs on Windows. `ci-poll.mjs` therefore honours `CI_POLL_GH_SCRIPT`, a documented test seam naming a
// Node script to run in place of `gh`, and this is that script.
//
// The mode comes from `GH_STUB_MODE`. Every mode reproduces something `gh` really does.

import { existsSync, readFileSync, writeFileSync } from "node:fs";

const mode = process.env.GH_STUB_MODE ?? "pending";
const minutesAgo = (m) => new Date(Date.now() - m * 60_000).toISOString();

/** A completed CheckRun that passed. */
const ok = (name) => ({
  __typename: "CheckRun",
  name,
  status: "COMPLETED",
  conclusion: "SUCCESS",
  startedAt: minutesAgo(30),
  completedAt: minutesAgo(5),
});

/** A completed CheckRun that GitHub marks SKIPPED — it DID NOT RUN. */
const skipped = (name) => ({
  __typename: "CheckRun",
  name,
  status: "COMPLETED",
  conclusion: "SKIPPED",
  startedAt: minutesAgo(30),
  completedAt: minutesAgo(30),
});

const failed = (name) => ({
  __typename: "CheckRun",
  name,
  status: "COMPLETED",
  conclusion: "FAILURE",
  startedAt: minutesAgo(30),
  completedAt: minutesAgo(2),
});

/** Still running, started `m` minutes ago — the age the poll now reports. */
const running = (name, m) => ({
  __typename: "CheckRun",
  name,
  status: "IN_PROGRESS",
  conclusion: null,
  startedAt: minutesAgo(m),
});

const prPayload = (rollup) => ({
  mergeable: "MERGEABLE",
  headRefOid: "0123456789abcdef0123456789abcdef01234567",
  statusCheckRollup: rollup,
});

// `ci.yml`'s five Rust test jobs, whose real check names are what a `needs: lockfile-preflight` cascade
// would skip. None of them carries a job-level `if:` — `ciPollFailClosed.test.ts` derives that from the
// workflow file rather than trusting this comment.
const RUST_JOBS = [
  "Backend — cargo test (ubuntu-latest)",
  "Server crates (windows-latest)",
  "Network E2E (ubuntu-latest)",
  "Sidecar — cargo test (ubuntu-latest)",
  "MSRV check",
];

// The one check this repo skips on EVERY pull request on purpose: gui-smoke.yml's windows job carries
// `if: github.event_name != 'push' && github.event_name != 'pull_request'` (CPE-1594).
const BY_DESIGN_SKIP = "GUI smoke (windows-latest) — tauri-driver + WebdriverIO";

switch (mode) {
  case "hang": {
    // `gh` blocked on a stalled socket or an interactive prompt. Ten minutes — longer than the harness
    // cap this whole file exists to stay under, so an unbounded call would be visibly fatal.
    setTimeout(() => {}, 600_000);
    break;
  }
  case "error": {
    // `gh auth status` failure / a wrong PR number. Non-zero exit with a real message on stderr.
    process.stderr.write("gh: Could not resolve to a PullRequest with the number of 999999.\n");
    process.exit(1);
    break;
  }
  case "garbage": {
    // A proxy or captive portal answering with HTML, or a `gh` that printed a warning banner. Exit 0 —
    // the point is that a SUCCESSFUL call can still yield something unparseable.
    process.stdout.write("<html><head><title>502 Bad Gateway</title></head></html>\n");
    break;
  }
  case "pending": {
    process.stdout.write(
      JSON.stringify(prPayload([ok("Frontend — type-check and test"), running("Server crates (windows-latest)", 63)])),
    );
    break;
  }
  case "green": {
    process.stdout.write(JSON.stringify(prPayload([ok("Frontend — type-check and test"), ok("Ratchet guard")])));
    break;
  }
  case "skip-cascade": {
    // The live shape the Foreman found: `lockfile-preflight` did not fail *visibly* in the rollup this
    // poll can see, and the five Rust jobs came back SKIPPED. Nothing is red. The old success test
    // folded SKIPPED into "success" and reported `completed success` on a run where the entire Rust
    // suite never executed.
    process.stdout.write(
      JSON.stringify(prPayload([ok("Frontend — type-check and test"), ...RUST_JOBS.map(skipped)])),
    );
    break;
  }
  case "skip-by-design": {
    // Exactly what PR #1068 returned on 2026-08-27. Must stay exit 0, or the tool reds every PR and
    // gets switched off.
    process.stdout.write(JSON.stringify(prPayload([ok("Frontend — type-check and test"), skipped(BY_DESIGN_SKIP)])));
    break;
  }
  // ── CPE-1906 round 2: a `gh` that exits 0 and answers with something that is not a board ──────────
  case "rest-error": {
    // GitHub's REST error body, verbatim in shape. `gh` exits 0 having printed it when the API answered
    // — a wrong PR number against a token that CAN see the repo, a renamed repo, a 404 behind a proxy.
    process.stdout.write(
      JSON.stringify({ message: "Not Found", documentation_url: "https://docs.github.com/rest" }),
    );
    break;
  }
  case "graphql-partial": {
    // `gh pr view --json statusCheckRollup` is a GraphQL query, `statusCheckRollup` is a NULLABLE field,
    // and GitHub answers a field-level failure with HTTP 200, a partial `data` and an `errors` array.
    // Exit 0, well-formed JSON, no rollup — the shape that read as `total_count=0` i.e. "pending".
    process.stdout.write(
      JSON.stringify({ data: null, errors: [{ message: "Could not resolve to a PullRequest with the number of 1." }] }),
    );
    break;
  }
  case "no-checks-yet": {
    // The control: a REAL PR with no checks scheduled yet. Has the rollup ARRAY (empty) and a real SHA,
    // which is exactly what distinguishes it from the two payloads above. Must stay a pending board.
    process.stdout.write(JSON.stringify(prPayload([])));
    break;
  }
  case "all-skipped-by-design": {
    // Every finished check is a skip the workflows explain. Nothing failed — and nothing RAN either, so
    // it is not a pass. This used to print `completed skipped` and exit 1, "at least one check FAILED".
    process.stdout.write(JSON.stringify(prPayload([skipped(BY_DESIGN_SKIP)])));
    break;
  }
  case "run-no-jobs": {
    // `gh run view --json status,conclusion,headSha,jobs` answering without `jobs`. This one was not a
    // wrong wait — it was `CI VERDICT: completed success`, exit 0, GREEN, on a board never seen.
    process.stdout.write(JSON.stringify({ status: "completed", conclusion: "success", headSha: "deadbeef" }));
    break;
  }
  case "run-failure-no-failing-job": {
    // Run-level `conclusion: failure` with no failing JOB, plus one unexplained skip. The two red
    // predicates disagreed here: the line said "neither red nor green", the exit code said 1.
    process.stdout.write(
      JSON.stringify({
        status: "completed",
        conclusion: "failure",
        headSha: "deadbeef",
        jobs: [
          { name: "Frontend — type-check and test", status: "completed", conclusion: "success" },
          { name: "MSRV check", status: "completed", conclusion: "skipped" },
        ],
      }),
    );
    break;
  }
  case "flaky": {
    // Reads 1, 4, 7… succeed and the rest fail, so the CONSECUTIVE-failure counter never reaches its
    // bail threshold of 3 and the poll ends on the deadline with a genuinely pending board — plus a
    // pile of `gh` failures the verdict used to be completely silent about. The invocation count lives
    // in the file named by `GH_STUB_COUNTER` because each read is a fresh process.
    const counter = process.env.GH_STUB_COUNTER ?? "";
    const n = counter && existsSync(counter) ? Number(readFileSync(counter, "utf8")) || 0 : 0;
    if (counter) writeFileSync(counter, String(n + 1));
    if (n % 3 === 0) {
      process.stdout.write(JSON.stringify(prPayload([running("Server crates (windows-latest)", 12)])));
    } else {
      process.stderr.write("gh: HTTP 502 Bad Gateway (https://api.github.com/graphql)\n");
      process.exit(1);
    }
    break;
  }
  case "failure-and-skips": {
    // A red build that also cascaded. Failure outranks skips: the caller's next move is the logs.
    process.stdout.write(
      JSON.stringify(
        prPayload([failed("Lockfile pre-flight — cargo metadata --locked (no compilation)"), ...RUST_JOBS.map(skipped)]),
      ),
    );
    break;
  }
  default:
    process.stderr.write(`gh-stub: unknown GH_STUB_MODE ${mode}\n`);
    process.exit(2);
}
