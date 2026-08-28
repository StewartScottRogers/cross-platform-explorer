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
