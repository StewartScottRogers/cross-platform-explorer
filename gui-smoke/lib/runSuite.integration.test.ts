// CPE-1910 — THE RED-PROOF, and the only test here that runs the real scripts.
//
// Everything else about the retry is pure and unit-tested in `sessionRetry.test.ts`. This file exists
// because the acceptance criterion is not "the policy function returns false", it is *"make a spec fail
// for real and confirm it is not retried and the job stays red"* — a claim about two CLI scripts, an
// exit code, and a `.results/` directory, none of which a pure test can speak for.
//
// So it executes the ACTUAL `scripts/run-suite.ts` and, on the results it leaves behind, the ACTUAL
// `scripts/run-ratchet.ts`, against a throwaway spec tree. What is stubbed is only the suite command
// itself (`GUI_SMOKE_SUITE_CMD` — a tiny node script that writes real reporter JSON and prints a real
// captured log), because the genuine article needs tauri-driver, WebKitWebDriver, xvfb and a compiled
// Linux app binary. Everything between that stub and the exit code is production code: the same
// `classifyLog`, the same `readResultChunks`, the same `decideSuiteRetry`, the same ratchet.
//
// Not in `scripts/` on purpose: `test:unit`'s glob is `lib/*.test.ts`, so a test living beside the
// script it drives would never run in CI (CPE-1694 paid for that lesson once already).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { after, describe, it } from "node:test";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const GUI_SMOKE = path.resolve(HERE, "..");
const RUN_SUITE = path.join(GUI_SMOKE, "scripts", "run-suite.ts");
const RUN_RATCHET = path.join(GUI_SMOKE, "scripts", "run-ratchet.ts");
/** RESOLVED, not hard-coded to `node_modules/tsx/dist/cli.mjs`: that path is tsx's private build layout
 *  and has moved between majors, and a wrong path here would fail every case below for a reason that
 *  looks like the feature being broken. `tsx/cli` is the package's own declared entry. */
const TSX_CLI = createRequire(import.meta.url).resolve("tsx/cli");
const SPEC_NAMES = ["alpha.smoke.ts", "beta.smoke.ts", "gamma.smoke.ts"];

const made: string[] = [];
after(() => {
  for (const dir of made) fs.rmSync(dir, { recursive: true, force: true });
});

interface Fixture {
  root: string;
  results: string;
  summary: string;
}

/**
 * A throwaway gui-smoke world: three spec files, an empty known-failing list, and a stub suite whose
 * behaviour per attempt is read from `script.json`.
 *
 * Each entry is one attempt: `report` names the spec files that manage to report, `pass` whether their
 * single case passes, and `log` the captured output that attempt prints. The stub appends to a counter so
 * attempt 2 can differ from attempt 1 — which is the only way to prove a retry actually re-ran anything
 * rather than re-reading the first attempt's leftovers.
 */
function fixture(script: { report: string[]; pass: boolean; log: string }[]): Fixture {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "cpe-1910-e2e-"));
  made.push(root);
  const specs = path.join(root, "specs");
  const results = path.join(root, ".results");
  fs.mkdirSync(specs, { recursive: true });
  fs.mkdirSync(results, { recursive: true });
  for (const name of SPEC_NAMES) fs.writeFileSync(path.join(specs, name), "// stub\n");
  fs.writeFileSync(path.join(root, "known-failing.json"), JSON.stringify({ cases: [] }, null, 2));
  fs.writeFileSync(path.join(root, "script.json"), JSON.stringify(script));
  fs.writeFileSync(path.join(root, "attempts.txt"), "");

  // The stub suite. Deliberately writes the SAME `@wdio/json-reporter` shape `reduceResultChunks`
  // consumes, so the ratchet downstream is judging real data.
  fs.writeFileSync(
    path.join(root, "stub-suite.mjs"),
    [
      `import fs from "node:fs";`,
      `import path from "node:path";`,
      `const root = ${JSON.stringify(root)};`,
      `const script = JSON.parse(fs.readFileSync(path.join(root, "script.json"), "utf8"));`,
      `const seen = fs.readFileSync(path.join(root, "attempts.txt"), "utf8").split("\\n").filter(Boolean).length;`,
      `fs.appendFileSync(path.join(root, "attempts.txt"), "x\\n");`,
      `const step = script[Math.min(seen, script.length - 1)];`,
      `for (const spec of step.report) {`,
      `  fs.writeFileSync(path.join(root, ".results", "wdio-" + spec + ".json"), JSON.stringify({`,
      `    specs: ["/abs/specs/" + spec],`,
      `    suites: [{ name: spec, tests: [{ name: "the only case", state: step.pass ? "passed" : "failed" }] }],`,
      `  }));`,
      `}`,
      `process.stdout.write(step.log + "\\n");`,
      `process.exit(step.pass ? 0 : 1);`,
    ].join("\n"),
  );

  return { root, results, summary: path.join(root, "step-summary.md") };
}

function runSuite(f: Fixture): { status: number; out: string; attempts: number } {
  const r = spawnSync(process.execPath, [TSX_CLI, RUN_SUITE], {
    cwd: GUI_SMOKE,
    encoding: "utf8",
    env: {
      ...process.env,
      GUI_SMOKE_RESULTS_DIR: f.results,
      GUI_SMOKE_SPECS_DIR: path.join(f.root, "specs"),
      GUI_SMOKE_SUITE_CMD: `"${process.execPath}" "${path.join(f.root, "stub-suite.mjs")}"`,
      GITHUB_STEP_SUMMARY: f.summary,
      // Unsharded: the expectation is the whole three-file tree.
      GUI_SMOKE_SHARD_INDEX: "",
      GUI_SMOKE_SHARD_TOTAL: "",
    },
  });
  const attempts = fs.readFileSync(path.join(f.root, "attempts.txt"), "utf8").split("\n").filter(Boolean).length;
  return { status: r.status ?? -1, out: `${r.stdout}${r.stderr}`, attempts };
}

function runRatchet(f: Fixture): { status: number; out: string } {
  const r = spawnSync(
    process.execPath,
    [TSX_CLI, RUN_RATCHET],
    {
      cwd: GUI_SMOKE,
      encoding: "utf8",
      env: {
        ...process.env,
        GUI_SMOKE_RESULTS_DIR: f.results,
        GUI_SMOKE_SPECS_DIR: path.join(f.root, "specs"),
        GUI_SMOKE_KNOWN_FAILING: path.join(f.root, "known-failing.json"),
        GUI_SMOKE_SHARD_INDEX: "",
        GUI_SMOKE_SHARD_TOTAL: "",
        GUI_SMOKE_EXPECT_SHARDS: "",
      },
    },
  );
  return { status: r.status ?? -1, out: `${r.stdout}${r.stderr}` };
}

const SOCKET_DEATH = [
  'ERROR webdriver: WebDriverError: Request failed with error code UND_ERR_SOCKET when running "http://127.0.0.1:4444/session" with method "POST"',
  "Error serving connection: hyper::Error(User(Service), client error (Connect)",
  "    1: Connection refused (os error 111))",
  "no such element",
  "Could not get DRI3 device",
].join("\n");

describe("run-suite.ts — a session that dies before asserting", () => {
  it("retries once, the second attempt really runs, and the job ends green", () => {
    const f = fixture([
      { report: ["alpha.smoke.ts"], pass: true, log: SOCKET_DEATH },
      { report: SPEC_NAMES, pass: true, log: "all specs ran" },
    ]);
    const suite = runSuite(f);

    assert.equal(suite.status, 0, suite.out);
    assert.equal(suite.attempts, 2, "the stub suite must have been invoked twice");
    assert.match(suite.out, /decision: retry-session-died-before-asserting/);

    // The ratchet judges the SECOND attempt only, and it is complete, so the job is green.
    const ratchet = runRatchet(f);
    assert.equal(ratchet.status, 0, ratchet.out);
    assert.match(ratchet.out, /3\/3 spec file\(s\) reported/);

    // Attempt 1's evidence survives, out of the ratchet's flat glob's way.
    assert.ok(fs.existsSync(path.join(f.results, "attempt-1", "wdio-alpha.smoke.ts.json")));
    assert.ok(fs.existsSync(path.join(f.results, "suite-output.attempt-1.log")));
  });

  it("says so LOUDLY, with a count, in the job summary", () => {
    const f = fixture([
      { report: ["alpha.smoke.ts"], pass: true, log: SOCKET_DEATH },
      { report: SPEC_NAMES, pass: true, log: "all specs ran" },
    ]);
    runSuite(f);
    const summary = fs.readFileSync(f.summary, "utf8");
    assert.match(summary, /WebDriver session recovery happened on this run/);
    assert.match(summary, /job-level suite retries used \| \*\*1\*\*/);
    assert.match(summary, /suite attempts run \| \*\*2\*\*/);
  });

  it("a run that needed nothing writes no summary block at all", () => {
    const f = fixture([{ report: SPEC_NAMES, pass: true, log: "clean" }]);
    const suite = runSuite(f);
    assert.equal(suite.attempts, 1);
    assert.match(suite.out, /nothing to report/);
    assert.equal(fs.existsSync(f.summary), false);
  });

  it("stops after one retry and leaves the shard RED when the transport dies twice", () => {
    const f = fixture([{ report: ["alpha.smoke.ts"], pass: true, log: SOCKET_DEATH }]);
    const suite = runSuite(f);
    assert.equal(suite.attempts, 2, "one retry, not a loop");
    assert.match(suite.out, /decision: budget-spent/);
    assert.equal(runRatchet(f).status, 1, "an incomplete run must still red");
  });
});

describe("run-suite.ts — RED-PROOF: a spec that fails for real is never retried", () => {
  it("does not retry, and the ratchet still reds the job", () => {
    // A complete run in which a real case fails, and — the trap this whole design turns on — the log
    // carries NO `AssertionError`, exactly like the 24 shard-2 failures measured on 2026-08-28. The
    // log-signature verdict is therefore `environment-signature-only`, and retrying on that verdict
    // alone (which the ticket's own summary invites) would re-run a genuine regression.
    const f = fixture([
      {
        report: SPEC_NAMES,
        pass: false,
        log: ["no such element", "could not inhibit screen lock", "Error: expected the dialog after 10000ms"].join(
          "\n",
        ),
      },
    ]);
    const suite = runSuite(f);

    assert.equal(suite.attempts, 1, "a real failure must be run exactly once");
    assert.match(suite.out, /decision: suite-completed/);
    assert.equal(fs.existsSync(f.summary), false, "no recovery happened, so nothing is claimed");

    const ratchet = runRatchet(f);
    assert.equal(ratchet.status, 1, "the job must stay red");
    assert.match(ratchet.out, /NEW GUI REGRESSION/);
  });

  it("does not retry a chai AssertionError even when the shard also died early", () => {
    // Both retry conditions would disagree here: the run IS incomplete (1 of 3 reported), but a real
    // assertion fired. The assertion wins, always.
    const f = fixture([
      { report: ["alpha.smoke.ts"], pass: false, log: "AssertionError: expected 0 to equal 3\nno such element" },
    ]);
    const suite = runSuite(f);
    assert.equal(suite.attempts, 1);
    assert.match(suite.out, /decision: assertion-evidence-present/);
    assert.equal(runRatchet(f).status, 1);
  });
});

describe("run-suite.ts — fails closed", () => {
  it("exits NON-ZERO when the suite command cannot be spawned at all", () => {
    // 'did not run' must fail the step, never be reported as a clean run. The Ratchet step's
    // `if: always()` still runs after it, so the job's verdict is never lost either way.
    const f = fixture([{ report: SPEC_NAMES, pass: true, log: "clean" }]);
    const r = spawnSync(
      process.execPath,
      [TSX_CLI, RUN_SUITE],
      {
        cwd: GUI_SMOKE,
        encoding: "utf8",
        env: {
          ...process.env,
          GUI_SMOKE_RESULTS_DIR: f.results,
          GUI_SMOKE_SPECS_DIR: path.join(f.root, "specs"),
          GUI_SMOKE_MAX_ATTEMPTS: "not-a-number",
          GUI_SMOKE_SUITE_CMD: `"${process.execPath}" "${path.join(f.root, "stub-suite.mjs")}"`,
          GUI_SMOKE_SHARD_INDEX: "",
          GUI_SMOKE_SHARD_TOTAL: "",
        },
      },
    );
    assert.equal(r.status, 1);
    assert.match(`${r.stdout}${r.stderr}`, /GUI_SMOKE_MAX_ATTEMPTS must be a positive integer/);
    assert.match(`${r.stdout}${r.stderr}`, /must never read as 'ran and found nothing'/);
  });
});
