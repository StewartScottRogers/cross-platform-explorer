// CPE-1953 — the agent catalog stopped reaching users and nothing said so.
//
// The measurement: `release.yml`'s `catalog` job is `needs: release`, and across 23 consecutive
// `release.yml` runs (v0.57.35-sidecar 2026-07-26 → v0.57.69-sidecar 2026-08-23) `run=failure`
// implied `catalog=skipped`, without exception. The last run that actually published was
// **v0.57.33 on 2026-07-25** — a 33-day gap by the time this ticket was worked, not the four days
// the ticket title guessed, and not v0.57.32 (whose release is a *draft* no client ever fetched;
// v0.57.33's release was later deleted, which is why an asset scan mistakes v0.57.32 for the last
// good one). CPE-1893 closed the job-level half of that on 2026-08-26 by adding
// `if: ${{ !cancelled() }}`, but NO tag has been cut since, so every claim it makes about what
// happens on a failed release was untested when this ticket was filed.
//
// This file is the execution-level red-proof for both halves. Unlike its sibling
// `catalogPublishFreshnessGuard.test.ts` (which asserts the SHAPE of the workflow), every test
// below extracts a step's real `run:` body out of the parsed workflow and EXECUTES it under bash
// with a controlled environment and stubbed `gh`, asserting on exit codes and `$GITHUB_OUTPUT`.
// A workflow edit that cannot be demonstrated is a provenance claim (CPE-1933); the shape used
// here follows `releaseHangHardening.test.ts` (parse, never regex raw text) and
// `catalogPublishFreshnessGuard.test.ts` (probe for bash, skip gracefully where it is absent).
//
// What is deliberately NOT proven here: that a real tagged release publishes a real catalog
// end to end. That requires cutting a release — an outward-facing publishing action — and was not
// done. See the PR body for exactly what that leaves open.
//
// CPE-1978 extends this file with §8. Same subject (this job's honesty), same harness: the step
// that says "Verify" now runs the verifier instead of checking a `.sig` is present, and every claim
// about it below is either executed or derived from the file it names.
import { describe, it, expect } from "vitest";
import { readFileSync, writeFileSync, readdirSync, mkdtempSync, mkdirSync, rmSync, chmodSync } from "node:fs";
import { join, delimiter } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { parseYaml } from "./preview/yaml";
import { stripRustComments, rustStrSliceAfter } from "./rustSource";
import { logicalLines } from "./shellScriptLines";
import { allShellUnits } from "./workflowShellSources";

const WORKFLOWS = join(process.cwd(), ".github", "workflows");

interface WorkflowStep {
  id?: string;
  name?: string;
  if?: string;
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
  const result = parseYaml(readFileSync(join(WORKFLOWS, fileName), "utf8"));
  if (!result.ok) {
    throw new Error(`${fileName} did not parse as YAML: ${result.error}`);
  }
  return result.value as WorkflowDoc;
}

const release = parseWorkflow("release.yml");
const catalogJob = release.jobs.catalog;

function step(name: string): WorkflowStep {
  const found = (catalogJob.steps ?? []).find((s) => s.name === name);
  if (!found) {
    throw new Error(
      `release.yml's catalog job has no step named "${name}" -- it was renamed or removed, and the ` +
        `red-proof below is no longer proving anything about the shipped workflow.`,
    );
  }
  return found;
}

/** The step's real `run:` body, with CRLF normalised (release.yml is a CRLF file, and a stray \r
 *  makes bash report `$'\r': command not found` on lines that are otherwise fine). Asserting the
 *  body is non-empty here means a YAML-parser regression shows up as a clear failure rather than as
 *  a vacuously-passing empty script -- the exact class of silent pass this whole ticket is about. */
function runBody(stepName: string): string {
  const body = step(stepName).run;
  if (typeof body !== "string" || body.trim().length === 0) {
    throw new Error(`step "${stepName}" has no run: body after parsing`);
  }
  return body.replace(/\r\n/g, "\n");
}

function bashAvailable(): boolean {
  const probe = spawnSync("bash", ["--version"], { stdio: "ignore" });
  return !probe.error && probe.status === 0;
}

function toolAvailable(tool: string): boolean {
  const probe = spawnSync("bash", ["-c", `command -v ${tool}`], { stdio: "ignore" });
  return !probe.error && probe.status === 0;
}

interface ExecResult {
  status: number | null;
  stdout: string;
  stderr: string;
  /** Everything the step wrote to $GITHUB_OUTPUT, as raw text. */
  output: string;
  /** stdout + stderr, for message assertions that don't care which stream carried the line. */
  all: string;
}

interface ExecOptions {
  /** Environment the step sees (on top of a minimal inherited PATH). */
  env?: Record<string, string>;
  /** Files to create in the working directory before running, as path -> contents. */
  files?: Record<string, string>;
  /** Shell-script stubs to place first on PATH, as command name -> script body (no shebang). */
  stubs?: Record<string, string>;
}

/** Runs a workflow step's own `run:` body in a throwaway directory, exactly as `bash -e`-less
 *  GitHub Actions would (each `run:` is handed to bash as a script; any `set -e` is the step's own,
 *  which is why the bodies below carry their own). Returns the exit code plus whatever the step
 *  wrote to $GITHUB_OUTPUT, which is what the workflow's downstream `if:` conditions read. */
function execStep(body: string, options: ExecOptions = {}): ExecResult {
  const dir = mkdtempSync(join(tmpdir(), "cpe-1953-catalog-"));
  try {
    const outputFile = join(dir, "github_output");
    writeFileSync(outputFile, "");
    const scriptFile = join(dir, "step.sh");
    writeFileSync(scriptFile, body, "utf8");

    for (const [rel, contents] of Object.entries(options.files ?? {})) {
      const full = join(dir, rel);
      mkdirSync(join(full, ".."), { recursive: true });
      writeFileSync(full, contents, "utf8");
    }

    const pathParts: string[] = [];
    if (options.stubs && Object.keys(options.stubs).length > 0) {
      const binDir = join(dir, "stub-bin");
      mkdirSync(binDir, { recursive: true });
      for (const [cmd, script] of Object.entries(options.stubs)) {
        const file = join(binDir, cmd);
        writeFileSync(file, `#!/usr/bin/env bash\n${script}\n`, "utf8");
        chmodSync(file, 0o755);
      }
      pathParts.push(binDir);
    }
    if (process.env.PATH) pathParts.push(process.env.PATH);

    const result = spawnSync("bash", [scriptFile], {
      cwd: dir,
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: pathParts.join(delimiter),
        GITHUB_OUTPUT: outputFile,
        ...(options.env ?? {}),
      },
    });
    const output = readFileSync(outputFile, "utf8");
    const stdout = result.stdout ?? "";
    const stderr = result.stderr ?? "";
    return { status: result.status, stdout, stderr, output, all: `${stdout}\n${stderr}` };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** A stub `gh` whose PATH entry is an extensionless bash script. On Windows a real `gh.exe` may sit
 *  further along PATH; prepending the stub dir wins, but this probe confirms it rather than
 *  assuming, so a stubbed test can never quietly exercise the REAL gh against a live repo. */
function stubGhWorks(): boolean {
  const r = execStep('gh --stub-probe\n', { stubs: { gh: 'echo "STUB-GH"; exit 0' } });
  return r.status === 0 && r.stdout.includes("STUB-GH");
}

// CPE-1953 review, non-blocking finding 4: these probes used to run in `beforeAll` and each test
// began `if (!hasBash) return;`. An early `return` inside a test body makes vitest report a **green
// PASS for a test that never ran** -- three of the tests here did exactly that on a machine without
// `jq`, so "33 tests" silently meant 30 and the mutation-kill count was a lower bound nobody could
// see. That is the same "green means nothing happened" shape this entire file exists to eliminate,
// reproduced in the file eliminating it. Probing at MODULE scope (spawnSync is synchronous, so this
// is safe at collection time) lets `it.skipIf(...)` mark the un-runnable cases as **skipped** in the
// reporter, where they are visibly not-run rather than indistinguishable from a pass.
const hasBash = bashAvailable();
const hasJq = hasBash && toolAvailable("jq");
const ghStubWorks = hasBash && stubGhWorks();

const VALID_INDEX = JSON.stringify({ entries: [{ id: "demo", version: 1_800_000_000 }] });

// ── 1. The vacuous-success hole: a tag build with no signing key ────────────────────────────────
// Every real step in the catalog job is `if: steps.k.outputs.has == 'true'`. Before this ticket the
// detect step emitted `has=false` and exited 0 with the key unset, so the job RAN (CPE-1893's
// `!cancelled()` guarantees that), skipped every step, and concluded **success** -- a green run that
// published no catalog at all. That is a strictly worse signal than the grey skip CPE-1893 fixed,
// because a green job is not even suspicious. This is the identical hole CPE-1923 finding 4 closed
// for `verify-published-manifest`; the catalog job was not given the same treatment at the time.
describe('"Detect catalog signing key" cannot report a green no-op on a tag build (CPE-1953)', () => {
  it.skipIf(!hasBash)("key present on a tag build -> has=true, exit 0", () => {
    const r = execStep(runBody("Detect catalog signing key"), {
      env: { KEY: "deadbeef".repeat(8), RELEASE_BUILD: "true" },
    });
    expect(r.status).toBe(0);
    expect(r.output).toContain("has=true");
  });

  it.skipIf(!hasBash)("NO key on a tag build -> the step FAILS, and never writes has=false for the gates to read", () => {
    const r = execStep(runBody("Detect catalog signing key"), {
      env: { KEY: "", RELEASE_BUILD: "true" },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("::error::");
    expect(r.all).toContain("CPE_CATALOG_SIGNING_KEY is not set");
    // The whole point: `has=false` is what silently disarmed every downstream step. It must not be
    // produced on the release path at all, not merely be accompanied by a warning.
    expect(r.output).not.toContain("has=false");
  });

  it.skipIf(!hasBash)("NO key on a NON-tag build -> the graceful has=false arm is preserved, exit 0", () => {
    const r = execStep(runBody("Detect catalog signing key"), {
      env: { KEY: "", RELEASE_BUILD: "false" },
    });
    expect(r.status).toBe(0);
    expect(r.output).toContain("has=false");
  });

  it.skipIf(!hasBash)("REGRESSION DEMO: the pre-CPE-1953 one-liner returned exit 0 + has=false on that same tag build", () => {
    // The literal body that shipped until this ticket. Executed here so the fix above is a measured
    // change in behaviour, not an assertion about a body nobody ever ran.
    const old = 'if [ -n "$KEY" ]; then echo "has=true" >> "$GITHUB_OUTPUT"; else echo "has=false" >> "$GITHUB_OUTPUT"; fi';
    const r = execStep(old, { env: { KEY: "", RELEASE_BUILD: "true" } });
    expect(r.status).toBe(0);
    expect(r.output).toContain("has=false");
  });
});

// ── 2. CPE-1893's "FAILS LOUDLY at gh release upload" claim, forced ─────────────────────────────
// release.yml's own comment asserts that with `if: ${{ !cancelled() }}`, a total-matrix `release`
// failure leaves no release object and this job "runs and FAILS LOUDLY at `gh release upload`
// rather than skipping quietly". Every skip in the run history PREDATES that change, so the claim
// had never been exercised. It is forced here against a stub `gh` reproducing GitHub's real
// not-found response.
describe('CPE-1893\'s "fail loudly at gh release upload" claim, exercised (CPE-1953)', () => {
  const files = {
    "catalog-out/catalog-index.json": VALID_INDEX,
    "catalog-out/catalog-index.json.sig": "sig\n",
  };

  it.skipIf(!hasBash || !ghStubWorks)("no release object for the tag (total matrix failure) -> the step exits NON-ZERO, not skipped-quiet", () => {
    const r = execStep(runBody("Upload catalog assets to the release"), {
      env: { TAG: "v9.9.9", GH_TOKEN: "x" },
      files,
      stubs: {
        gh: 'echo "release not found" >&2; exit 1',
      },
    });
    // The fact under test is the EXIT CODE: the step propagates gh's failure instead of skipping.
    // The message assertion below is safe only because that text is this test's OWN stub speaking
    // (see `stubs.gh` above), standing in for gh's real not-found response -- it is not a claim about
    // how any released `gh` words it. Real `gh` output is never asserted on anywhere in this file;
    // it would be exactly the portability trap the MECHANISM test below documents.
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("release not found");
  });

  it.skipIf(!hasBash || !ghStubWorks)("release object exists -> the step exits 0 and passes the tag + the bundle glob to gh", () => {
    const r = execStep(runBody("Upload catalog assets to the release"), {
      env: { TAG: "v9.9.9", GH_TOKEN: "x" },
      files,
      stubs: { gh: 'echo "ARGS: $*"; exit 0' },
    });
    expect(r.status).toBe(0);
    // The glob must have EXPANDED -- a literal `catalog-out/*` reaching gh is the shape that
    // uploads nothing while looking like it tried.
    expect(r.stdout).toContain("v9.9.9");
    expect(r.stdout).toContain("catalog-out/catalog-index.json");
    expect(r.stdout).not.toContain("catalog-out/*");
  });
});

// ── 3. Zero-work sign: catalog-sign exiting 0 having produced nothing usable ────────────────────
describe('"Verify the signed bundle before uploading it" catches a zero-work sign (CPE-1953)', () => {
  const body = () => runBody("Verify the signed bundle before uploading it");

  it.skipIf(!hasBash)("no catalog-index.json at all -> fails before anything is uploaded", () => {
    const r = execStep(body(), { files: { "catalog-out/README": "nothing useful\n" } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("::error::");
    expect(r.all).toContain("nothing to publish");
  });

  it.skipIf(!hasBash)("an EMPTY catalog-index.json -> fails (a zero-byte file is not a published catalog)", () => {
    const r = execStep(body(), { files: { "catalog-out/catalog-index.json": "" } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("nothing to publish");
  });

  it.skipIf(!hasBash)("index present but its detached signature missing -> fails (clients verify before applying)", () => {
    const r = execStep(body(), { files: { "catalog-out/catalog-index.json": VALID_INDEX } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("detached signature");
  });

  // The three cases below turn on what `jq` REPORTS, and `jq` is absent from many dev machines
  // (confirmed absent on this author's). Gating them on `hasJq` is what produced the review's
  // finding-4 vacuous passes -- and worse than reported: on a jq-less machine the "real one-entry
  // bundle -> exit 0" case would have FAILED had it run, because the step correctly reds when jq is
  // missing. So they are driven by a faithful `jq` stub instead, reproducing the only contract this
  // step has with jq (print the count on stdout; exit non-zero when the document will not parse).
  // That makes them run everywhere. The genuine article is exercised separately, below, on any
  // machine that has it -- and CI's ubuntu runner always does.
  const jqStub = (stdout: string, code = 0) =>
    code === 0 ? `printf '%s\\n' "${stdout}"; exit 0` : `echo "jq: parse error" >&2; exit ${code}`;

  it.skipIf(!hasBash)("index + sig with zero entries[] -> fails as the 'succeeding at zero work' shape", () => {
    const r = execStep(body(), {
      files: {
        "catalog-out/catalog-index.json": JSON.stringify({ entries: [] }),
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
      stubs: { jq: jqStub("0") },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("zero entries");
  });

  it.skipIf(!hasBash)("index + sig that is not JSON at all -> fails as corrupt, not as a silent abort", () => {
    const r = execStep(body(), {
      files: {
        "catalog-out/catalog-index.json": "{ truncated",
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
      stubs: { jq: jqStub("", 5) },
    });
    expect(r.status).not.toBe(0);
    // The `if ! entries=$(jq ...)` shape (CPE-1893 UAT round 1) is what lets the diagnostic print at
    // all -- a bare assignment under `set -e` aborts before reaching it.
    expect(r.all).toContain("not parseable JSON");
  });

  it.skipIf(!hasBash)("a one-entry bundle -> exit 0 and entries=1 published to $GITHUB_OUTPUT", () => {
    const r = execStep(body(), {
      files: {
        "catalog-out/catalog-index.json": VALID_INDEX,
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
      stubs: { jq: jqStub("1") },
    });
    expect(r.status).toBe(0);
    expect(r.output).toContain("entries=1");
  });

  it.skipIf(!hasBash || !hasJq)("REAL JQ corroboration: a genuine one-entry bundle -> exit 0, entries=1", () => {
    const r = execStep(body(), {
      files: {
        "catalog-out/catalog-index.json": VALID_INDEX,
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
    });
    expect(r.status).toBe(0);
    expect(r.output).toContain("entries=1");
  });

  // ── Review blocker 1: the entry-count comparison must not fail OPEN ─────────────────────────
  // `[ "$x" -lt 1 ]` exits **2** when $x is not a single integer, and inside an `if` that reads as
  // plain "false" -- so the zero-entry check was skipped and the step exited 0 and went on to
  // upload. Both halves are proved: the raw bash mechanism (so the claim about `[` is not taken on
  // faith), and the shipped step against real jq output that triggers it.
  it.skipIf(!hasBash)('MECHANISM: `[ "0\\n1" -lt 1 ]` exits 2, and an `if` cannot tell 2 from false', () => {
    const r = spawnSync(
      "bash",
      [
        "-c",
        'entries=$(printf "0\\n1"); [ "${entries:-0}" -lt 1 ]; echo "cmp_rc=$?"; ' +
          'if [ "${entries:-0}" -lt 1 ]; then echo TOOK-LT-BRANCH; fi; echo "step-continued"',
      ],
      { encoding: "utf8" },
    );
    // The comparison runs TWICE on purpose: once bare, to capture `[`'s own exit status, and once as
    // an `if` condition, to show that status being flattened into "false". The captured status is
    // the fact under test -- not anything bash prints about it.
    //
    // The whole defect in three observable, platform-stable facts: `[` reports 2 (an ERROR, not a
    // verdict), the guard branch is nonetheless not taken, and the script carries on to what would
    // have been the upload.
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("cmp_rc=2");
    expect(r.stdout).not.toContain("TOOK-LT-BRANCH");
    expect(r.stdout).toContain("step-continued");
    // LOOSE ON PURPOSE, and saying why is the point (CPE-1953 review round 2): this string is
    // BASH's incidental complaint, not a diagnostic this repo emits, and the two bash builds this
    // file runs on word it differently — Git Bash on Windows says "integer expected", GNU bash on
    // ubuntu says "integer expression expected". An earlier version pinned the Windows spelling and
    // went red on CI: the same "asserting on another tool's human-readable output" mistake the
    // review flagged elsewhere, committed while following the rule to assert on diagnostics. That
    // rule means assert on OUR diagnostic. The shell's text is kept ONLY as corroboration that the
    // exit code above came from the integer parse rather than from something else, so it is matched
    // on the shape the two spellings share — `integer` … `expected` — and not on a literal
    // substring of either. (A first attempt at "loose" used `/integer expres/`, which matches the
    // Linux spelling and NOT the Windows one: a narrower pin wearing a wildcard, red on the very
    // machine the original was written on. If a third bash wording ever appears, widen this to the
    // exit code alone rather than growing an alternation of vendors' prose.)
    expect(r.stderr).toMatch(/integer\b.*\bexpected/);
  });

  const CONCATENATED_STREAM = '{"entries":[]}{"entries":[{"id":"demo","version":1800000000}]}';

  it.skipIf(!hasBash)("a multi-line count -- the step must REFUSE it, not upload", () => {
    // Two JSON documents back to back. Real jq prints one length per document and exits 0, so
    // `entries` arrives as "0\n1" -- an honest reading failure that the old comparison turned into a
    // pass. Driven by the stub so this runs everywhere; the real-jq version follows.
    const r = execStep(body(), {
      files: {
        "catalog-out/catalog-index.json": CONCATENATED_STREAM,
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
      stubs: { jq: 'printf \'%s\\n\' 0 1; exit 0' },
    });
    expect(r.status).not.toBe(0);
    // Assert on the step's OWN diagnostic, not merely on a nonzero exit -- a nonzero exit can be
    // earned for entirely the wrong reason (the review's own harness produced a wall of green PASSes
    // that were rc=127 "bash not found" and `set -u` aborts).
    expect(r.all).toContain("which is not one non-negative integer");
    expect(r.all).toContain("that is how a guard fails open");
    expect(r.output).not.toContain("entries=");
  });

  it.skipIf(!hasBash)("a count that is not a number at all is refused with the same diagnostic", () => {
    const r = execStep(body(), {
      files: {
        "catalog-out/catalog-index.json": VALID_INDEX,
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
      // Stub jq so this case runs everywhere, including where jq is absent. It stands in for any
      // future jq/schema change that stops yielding a bare integer.
      stubs: { jq: 'echo "null"; exit 0' },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("which is not one non-negative integer");
  });

  it.skipIf(!hasBash || !hasJq)("REAL JQ: a concatenated stream really does make jq print two counts at exit 0", () => {
    // Corroborates that the stub above is faithful rather than a convenient fiction: this is what
    // genuine jq does with the same bytes. Skipped, visibly, where jq is absent.
    const r = spawnSync("bash", ["-c", `printf '%s' '${CONCATENATED_STREAM}' | jq -r '.entries | length'`], {
      encoding: "utf8",
    });
    expect(r.status).toBe(0);
    expect(r.stdout.trim().split(/\r?\n/)).toEqual(["0", "1"]);
  });

  it.skipIf(!hasBash)("REGRESSION DEMO: the pre-review comparison accepted that same multi-line count and passed", () => {
    // The shipped body with only the shape-validation `case` removed -- i.e. exactly what this PR
    // originally proposed. It exits 0 and would have gone on to upload.
    const withoutShapeCheck = body().replace(/\s*case "\$entries" in[\s\S]*?esac\n/, "\n");
    expect(withoutShapeCheck, "the case block must still be findable for this demo to mean anything").not.toContain(
      'case "$entries" in',
    );
    const r = execStep(withoutShapeCheck, {
      files: {
        "catalog-out/catalog-index.json": CONCATENATED_STREAM,
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
      stubs: { jq: 'printf \'%s\\n\' 0 1; exit 0' },
    });
    // The regression is entirely behavioural, and every assertion below is on this repo's own
    // output: the step exits 0, never prints its zero-entry refusal or its shape-check refusal, and
    // reaches its own pre-upload success line — i.e. it would have gone on to upload a bundle whose
    // entry count it could not read. Bash's "integer expres…" complaint is incidental to that and is
    // deliberately NOT asserted here; see the MECHANISM test above for why its wording is not
    // portable across the two bash builds this file runs on.
    expect(r.status).toBe(0);
    expect(r.all).not.toContain("zero entries"); // the guard branch was never taken -- failing OPEN
    expect(r.all).not.toContain("which is not one non-negative integer"); // nor the shape check
    expect(r.stdout).toContain("signed catalog bundle carries"); // reached the pre-upload success line
  });

  it.skipIf(!hasBash)("with jq absent from PATH entirely, the step still fails LOUD -- never green", () => {
    // Property that holds on every machine regardless of what is installed: this step has no path
    // to exit 0 without having actually read the index. A missing tool is a red step, not a pass.
    const r = execStep(`PATH=/nonexistent-cpe-1953\n${body()}`, {
      files: {
        "catalog-out/catalog-index.json": VALID_INDEX,
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
    });
    expect(r.status).not.toBe(0);
  });
});

// ── 4. An upload that "succeeded" while attaching nothing a client asks for ─────────────────────
describe('"Confirm the catalog is actually on the release" reads back the PUBLISHED state (CPE-1953)', () => {
  const body = () => runBody("Confirm the catalog is actually on the release");
  const env = { TAG: "v9.9.9", GH_TOKEN: "x" };

  it.skipIf(!hasBash || !ghStubWorks)("both assets present on the release -> exit 0", () => {
    const r = execStep(body(), {
      env,
      stubs: { gh: 'printf "%s\\n" catalog-index.json catalog-index.json.sig latest.json; exit 0' },
    });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("confirmed on v9.9.9");
  });

  it.skipIf(!hasBash || !ghStubWorks)("PIPEFAIL TRAP: a match must not be inverted into a miss (the herestring, not a pipe)", () => {
    // Direct demonstration of the bug the herestring avoids: under `pipefail`, `grep -q` matching
    // early closes the pipe, printf takes SIGPIPE, and the PIPELINE reports 141 -- so the piped
    // form would report the asset MISSING precisely when it is present. A long asset list makes the
    // SIGPIPE race reliable rather than incidental.
    const many = Array.from({ length: 4000 }, (_, i) => `filler-asset-${i}.bin`);
    const listing = ["catalog-index.json", "catalog-index.json.sig", ...many].join("\\n");
    const r = execStep(body(), {
      env,
      stubs: { gh: `printf "%s\\n" "$(printf '${listing}')"; exit 0` },
    });
    expect(r.status).toBe(0);
    expect(r.all).not.toContain("does not carry");
  });

  it.skipIf(!hasBash || !ghStubWorks)("upload 'succeeded' but the release carries only unrelated assets -> exit 1, names what is missing", () => {
    const r = execStep(body(), {
      env,
      stubs: { gh: 'printf "%s\\n" latest.json some-installer.exe; exit 0' },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("does not carry");
    expect(r.all).toContain("catalog-index.json");
  });

  it.skipIf(!hasBash || !ghStubWorks)("only the index, no signature -> still exit 1 (a catalog no client will apply is not published)", () => {
    const r = execStep(body(), {
      env,
      stubs: { gh: 'printf "%s\\n" catalog-index.json; exit 0' },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("catalog-index.json.sig");
  });

  it.skipIf(!hasBash || !ghStubWorks)("the read-back itself fails -> exit 1; a lookup failure is never taken as evidence of success", () => {
    const r = execStep(body(), {
      env,
      stubs: { gh: 'echo "HTTP 503" >&2; exit 1' },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("NOT evidence the upload worked");
  });

  it.skipIf(!hasBash)("PIPEFAIL TRAP, shown directly: the piped form can report failure on a MATCH; the herestring cannot", () => {
    // The `catalog` job's confirm step is written with a herestring for this reason. Proved the way
    // catalogPublishFreshnessGuard.test.ts proves its own set -e finding -- by running both shapes
    // rather than asserting one is better.
    //
    // SCOPE, stated honestly (CPE-1953 review): this inversion is SIZE-GATED. It needs roughly
    // 28,912-88,912 bytes of asset names -- about 2,000-6,000 assets -- for printf to still be
    // writing when grep exits early. A real release carries ~20, so the piped form would have worked
    // in practice. This is a latent shape avoided before it shipped, not a bug that bit; the huge
    // generated list below exists precisely because the realistic input would NOT reproduce it.
    const gen = "for i in $(seq 1 50000); do echo filler-$i; done";
    const piped = spawnSync(
      "bash",
      ["-c", `set -uo pipefail; names=$(${gen}; echo catalog-index.json); printf '%s\\n' "$names" | grep -Fxq catalog-index.json; echo "exit=$?"`],
      { encoding: "utf8" },
    );
    const here = spawnSync(
      "bash",
      ["-c", `set -uo pipefail; names=$(${gen}; echo catalog-index.json); grep -Fxq catalog-index.json <<< "$names"; echo "exit=$?"`],
      { encoding: "utf8" },
    );
    // The herestring form is unconditionally correct: the asset IS present, so exit must be 0.
    expect(here.stdout.trim()).toBe("exit=0");
    // The piped form is at best unreliable here -- 0 when printf happens to finish first, 141 when
    // it takes SIGPIPE. Asserting only that the herestring never has that failure mode keeps this
    // test deterministic while still documenting the shape being avoided.
    expect(["exit=0", "exit=141"]).toContain(piped.stdout.trim());
  });
});

// ── 5. The terminal honesty gate ────────────────────────────────────────────────────────────────
// The one place that decides what "catalog: success" is allowed to mean. Executed across the full
// outcome matrix, including the exact shape this ticket exists to kill: key present, every step
// `skipped`, job otherwise green.
describe('"Catalog publish outcome" makes a non-publishing run RED (CPE-1953)', () => {
  const body = () => runBody("Catalog publish outcome");
  const ok = {
    HAS_KEY: "true",
    SIGN: "success",
    BUNDLE: "success",
    // CPE-1978's real signature check. The gate reads it like every other step, so a `skipped` or
    // `failure` here is a non-publishing run.
    SIGVERIFY: "success",
    UPLOAD: "success",
    CONFIRM: "success",
    ENTRIES: "3",
    TAG: "v9.9.9",
    // release.yml triggers on tag pushes only, so this is what a real run always carries.
    RELEASE_BUILD: "true",
  };

  it.skipIf(!hasBash)("a complete, confirmed publish -> exit 0 and says how many entries shipped", () => {
    const r = execStep(body(), { env: ok });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("agent catalog published for v9.9.9");
    expect(r.stdout).toContain("3 entr");
  });

  it.skipIf(!hasBash)("no key on a NON-tag build -> exit 0, and the notice is true: nothing was published or expected", () => {
    const r = execStep(body(), { env: { ...ok, HAS_KEY: "false", RELEASE_BUILD: "false" } });
    expect(r.status).toBe(0);
    expect(r.all).toContain("::notice::");
    expect(r.all).toContain("nothing was published, and nothing was expected to be");
  });

  // ── Review blocker 2: the gate must not describe a tag build as "not a tag build" ────────────
  // On a tag with no signing key the detect step exits 1, so `HAS_KEY` reaches this gate EMPTY --
  // not "false". The first version of this branch printed a ::notice:: reading "this is not a tag
  // build — nothing was published, and nothing was expected to be", both clauses false, in the exact
  // scenario the ticket was filed for. The job was still red via step `k`, so the outcome was right
  // and only the SUMMARY LINE lied -- which, for the step commented as "the single place this job's
  // honesty is decided", is the same defect in a smaller place.
  it.skipIf(!hasBash)("THIS TICKET'S HEADLINE SCENARIO: tag build, key unset, HAS_KEY empty -> red, and the message is true", () => {
    const r = execStep(body(), {
      env: { HAS_KEY: "", SIGN: "", BUNDLE: "", UPLOAD: "", CONFIRM: "", ENTRIES: "", TAG: "v9.9.9", RELEASE_BUILD: "true" },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("::error::");
    expect(r.all).toContain("on a TAG build (v9.9.9)");
    expect(r.all).toContain("a publish WAS expected");
    // The two false clauses must be gone, not merely accompanied by a truer one.
    expect(r.all).not.toContain("this is not a tag build");
    expect(r.all).not.toContain("nothing was expected to be");
  });

  it.skipIf(!hasBash)("HAS_KEY=false on a tag build is treated the same way -- the trigger decides, not the missing output", () => {
    const r = execStep(body(), { env: { ...ok, HAS_KEY: "false", RELEASE_BUILD: "true" } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("a publish WAS expected");
  });

  it.skipIf(!hasBash).each([
    ["SIGN", "build+sign"],
    ["BUNDLE", "verify-bundle"],
    ["SIGVERIFY", "verify-signatures"],
    ["UPLOAD", "upload"],
    ["CONFIRM", "confirm-on-release"],
  ])("a %s outcome of 'skipped' -> exit 1 naming %s", (key, label) => {
    const r = execStep(body(), { env: { ...ok, [key]: "skipped" } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("was NOT published");
    expect(r.all).toContain(label);
  });

  it.skipIf(!hasBash)("THE SHAPE THIS TICKET IS ABOUT: key present, every step skipped -> exit 1, not a green no-op", () => {
    const r = execStep(body(), {
      env: { HAS_KEY: "true", SIGN: "", BUNDLE: "", UPLOAD: "", CONFIRM: "", ENTRIES: "", TAG: "v9.9.9" },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("was NOT published");
    expect(r.all).toContain("<none>");
  });

  it.skipIf(!hasBash)("a failed upload -> exit 1 and points at the freshness backstop for how stale this may get", () => {
    const r = execStep(body(), { env: { ...ok, UPLOAD: "failure" } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("catalog-freshness.yml");
  });
});

// ── 6. Structural ratchet: the gate stays terminal, and every real step stays observable ────────
describe("release.yml's catalog job keeps the structure these red-proofs depend on (CPE-1953)", () => {
  it("the outcome gate is the LAST step and runs with if: always()", () => {
    const steps = catalogJob.steps ?? [];
    const last = steps[steps.length - 1];
    expect(last.name).toBe("Catalog publish outcome");
    expect(last.if).toBe("always()");
  });

  it("every step the gate reads an outcome from has the id the gate names", () => {
    const ids = new Set((catalogJob.steps ?? []).map((s) => s.id).filter(Boolean));
    for (const id of ["k", "sign", "bundle", "sigverify", "up", "confirm"]) {
      expect(ids.has(id), `catalog job is missing step id "${id}" that the outcome gate reads`).toBe(true);
    }
  });

  it("the detect step is fatal on a tag build -- the same mechanism verify-published-manifest uses", () => {
    // Both jobs answer "is this run cutting a release?" from `github.ref_type == 'tag'`. Asserting
    // they share the mechanism (not merely that each has one) is what stops a future edit relaxing
    // one of the two without the other.
    const catalogDetect = step("Detect catalog signing key");
    const verifySteps = release.jobs["verify-published-manifest"].steps ?? [];
    const verifyDetect = verifySteps.find((s) => s.name === "Detect updater signing key");
    const envOf = (s: WorkflowStep) => (s.env ?? {}) as Record<string, string>;
    expect(envOf(catalogDetect).RELEASE_BUILD).toBe(envOf(verifyDetect as WorkflowStep).RELEASE_BUILD);
    expect(envOf(catalogDetect).RELEASE_BUILD).toContain("github.ref_type == 'tag'");
  });
});

// ── 7. CPE-1932 enumeration: every OTHER job chained behind something that can silently disable it
// The ticket's finding generalises -- `catalog` was found by accident, so the question "what else is
// `needs:`-chained behind a failure with a different blast radius?" has to be ENUMERATED, not
// recalled. This ratchet lists every `needs:`-carrying job across every workflow with a recorded
// verdict, so a NEW one cannot be added without someone making the same decision explicitly.
describe("every needs:-chained job across the workflows has a recorded skip verdict (CPE-1932/CPE-1953)", () => {
  // ci.yml's five build/test jobs all hang off `lockfile-preflight`. CPE-1953 enumerated them here
  // and recorded the skip as ACCEPTED WITH A CAVEAT, deferring the caveat to its own ticket. That
  // ticket was CPE-1956, and it is now done: `ci.yml` grew a terminal `ci-verdict` job
  // (`if: always()`, `needs:` all five) that reds when any of them did not run. So the five stay
  // recorded as accepted silent skips -- the `needs:` edge is deliberately kept, because it is what
  // makes the preflight's fail-fast saving real -- but the skip is no longer SILENT, and the
  // `coveredBy` field below is the derivation that says so.
  //
  // The distinction that made the caveat worth a ticket, kept here because it is the reason the two
  // cases got different fixes:
  //   `catalog`'s skip was uniquely bad because its consequence (every user's agent roster frozen)
  //   was invisible from, and unrelated to, the failure that caused it. Nothing in ci.yml PUBLISHES:
  //   a skipped `backend` delivers no wrong artifact, it withholds a CHECK. But GitHub counts a
  //   skipped required status check as SATISFIED, and a grey check reads to a human exactly like a
  //   job that had nothing to do -- so "withholds a check" is only harmless while nobody is relying
  //   on that check being there. CPE-1956 measured branch protection OFF on 2026-08-27
  //   (`branches/main/protection` -> 404, `rulesets` -> `[]`), i.e. the hazard was latent, and fixed
  //   it while it was still cheap rather than after someone turned protection on.
  const CI_PREFLIGHT =
    "ACCEPTED SILENT SKIP, now covered by a terminal verdict (CPE-1956): the `needs:` edge is pure " +
    "ORDERING -- lockfile-preflight writes no output the five consume -- and it is kept because it is " +
    "what converts 'one stale lockfile per hour-long matrix run' into 'every stale lockfile in seconds'. " +
    "Deleting the edge would delete that saving and still not fix the real defect, which was the skip " +
    "being invisible. `ci.yml/ci-verdict` (always()) reds and names every job that did not run.";

  /**
   * name -> why a silent skip behind a failed `needs:` is (or is not) acceptable here, and -- for an
   * accepted skip -- which terminal job still examines the run so the skip cannot pass unnoticed.
   *
   * `coveredBy` is DERIVED, not decorative: the test below resolves the named job out of the parsed
   * workflow and asserts it really exists, really carries an `always()`/`!cancelled()` `if:`, and
   * really lists this job in its own `needs:`. A prose "it's fine, something else catches it" is the
   * exact untestable provenance claim CPE-1933 bans.
   */
  const VERDICTS: Record<string, { guarded: boolean; why: string; coveredBy?: string }> = {
    "release.yml/verify-published-manifest": {
      guarded: true,
      why: "CPE-1872 finding A: a fail-fast:false matrix can still have published assets, so the integrity gate must run on a failed release. `!cancelled()`.",
    },
    "release.yml/catalog": {
      guarded: true,
      why: "This ticket's subject. `!cancelled()` (CPE-1893) plus the terminal outcome gate above.",
    },
    "release-sidecar.yml/release-sidecar": {
      guarded: false,
      why: "ACCEPTED SILENT SKIP, and the blast radius is the same failure rather than a different one: it needs create-release's release OBJECT and verify-updater-pin's go-ahead. With either failed there is literally nothing to build into, and the run is already red from the upstream job -- nothing ships to users independently of it.",
      coveredBy: "release-sidecar.yml/verify-published-manifest-sidecar",
    },
    "release-sidecar.yml/verify-published-manifest-sidecar": {
      guarded: true,
      why: "Same integrity-gate reasoning as release.yml's. `!cancelled()`.",
    },
    "gui-smoke.yml/gui-smoke-linux": {
      guarded: false,
      why: "ACCEPTED SILENT SKIP: a failed build leaves no binary to smoke. The run is red from the build, and the skip has no independent user-facing consequence -- unlike `catalog`, nothing stops being DELIVERED because this did not run.",
      coveredBy: "gui-smoke.yml/gui-smoke-linux-verdict",
    },
    "gui-smoke.yml/gui-smoke-linux-verdict": {
      guarded: true,
      why: "`always()` -- the verdict must report on a failed or skipped smoke run, which is precisely the terminal-gate shape this ticket adds to `catalog`.",
    },
    "ci.yml/backend": { guarded: false, why: CI_PREFLIGHT, coveredBy: "ci.yml/ci-verdict" },
    "ci.yml/crates": { guarded: false, why: CI_PREFLIGHT, coveredBy: "ci.yml/ci-verdict" },
    "ci.yml/net-e2e": { guarded: false, why: CI_PREFLIGHT, coveredBy: "ci.yml/ci-verdict" },
    "ci.yml/sidecar": { guarded: false, why: CI_PREFLIGHT, coveredBy: "ci.yml/ci-verdict" },
    "ci.yml/msrv": { guarded: false, why: CI_PREFLIGHT, coveredBy: "ci.yml/ci-verdict" },
    "ci.yml/ci-verdict": {
      guarded: true,
      why: "CPE-1956's gate itself. `always()` -- without it the verdict is skipped by the very failure it exists to report, and a skipped gate is a green PR. Its own wiring (that its needs: covers EVERY job behind lockfile-preflight) is derived in src/lib/ciVerdict.test.ts.",
    },
  };

  // CPE-1953 review, non-blocking finding 3: this list was hard-coded to the four files that happen
  // to contain `needs:` chains today, which is precisely the "enumerate, don't recall" failure
  // CLAUDE.md's CPE-1932 rule names (and which this PR's own body invokes) -- a chain added in one of
  // the other four workflow files would simply not be looked at. Derived from the directory now, with
  // the near-empty backstop that rule also requires, so a glob that silently stops matching reads as
  // a failure rather than as a vacuously-satisfied table.
  const files = readdirSync(WORKFLOWS)
    .filter((f) => f.endsWith(".yml") || f.endsWith(".yaml"))
    .sort();

  it("derives the workflow list at run time and finds a plausible number of them (CPE-1932)", () => {
    expect(files.length).toBeGreaterThanOrEqual(6);
    expect(files).toContain("release.yml");
    expect(files).toContain("ci.yml");
  });

  it("enumerates the same set the verdict table records -- a new chained job fails this test", () => {
    const found: string[] = [];
    for (const file of files) {
      const doc = parseWorkflow(file);
      for (const [jobName, job] of Object.entries(doc.jobs)) {
        if (job.needs !== undefined) found.push(`${file}/${jobName}`);
      }
    }
    expect(found.sort()).toEqual(Object.keys(VERDICTS).sort());
  });

  it("every job recorded as guarded really does carry an if: that survives an upstream failure", () => {
    for (const [key, verdict] of Object.entries(VERDICTS)) {
      if (!verdict.guarded) continue;
      const [file, jobName] = key.split("/");
      const job = parseWorkflow(file).jobs[jobName];
      const cond = job.if ?? "";
      // The only two expressions that keep a job running when a `needs:` job failed.
      expect(
        /!cancelled\(\)|always\(\)/.test(cond),
        `${key} is recorded as guarded but its if: (${cond || "<absent>"}) would still skip on an upstream failure`,
      ).toBe(true);
    }
  });

  it("every accepted silent skip has a written reason, not an empty string", () => {
    for (const [key, verdict] of Object.entries(VERDICTS)) {
      if (verdict.guarded) continue;
      expect(verdict.why.length, `${key} needs a recorded reason`).toBeGreaterThan(60);
    }
  });

  // CPE-1956. The invariant the whole family of tickets (CPE-1753, CPE-1872, CPE-1893, CPE-1953)
  // has been converging on, stated once and DERIVED: a job may be silently skipped only if some
  // terminal job still runs and still has it in view. Concretely -- for every accepted silent skip,
  // `coveredBy` must name a job that (a) exists in the same workflow, (b) carries an `if:` that
  // survives an upstream failure, and (c) genuinely lists the skipped job in its own `needs:`.
  //
  // (c) is the load-bearing clause and the reason this is a derivation rather than a comment: a
  // terminal job that runs on `always()` but does NOT need the skipped job never sees it, so naming
  // it as cover would be false while reading as reassurance -- the precise failure mode CPE-1933
  // describes, where a green test vouches for an unchecked claim.
  //
  // What this deliberately does NOT claim: that each covering job reds *specifically because of*
  // the skip. That is true and demonstrated by execution for `ci.yml/ci-verdict`
  // (src/lib/ciVerdict.test.ts spawns its real run: body with an all-skipped payload and observes
  // exit 1) and for `gui-smoke-linux-verdict` (its ratchet reds on MISSING SHARD). For
  // `verify-published-manifest-sidecar` the claim is only the weaker, checkable one: the run does
  // not end with the skip unexamined.
  it("every accepted silent skip names a terminal job that really covers it (CPE-1956)", () => {
    const accepted = Object.entries(VERDICTS).filter(([, v]) => !v.guarded);

    // Near-empty backstop: if the filter ever matches nothing, this becomes a vacuous pass over the
    // exact population it is supposed to police.
    expect(
      accepted.length,
      "no accepted silent skips found in VERDICTS -- the table or this filter broke, and an " +
        "empty population is not the same as a clean one",
    ).toBeGreaterThanOrEqual(3);

    for (const [key, verdict] of accepted) {
      const [file, jobName] = key.split("/");
      expect(
        verdict.coveredBy,
        `${key} is an accepted silent skip with no coveredBy. Either name the terminal job that ` +
          `still examines the run, or guard the job itself -- "the run is red anyway" is not cover, ` +
          `because a skipped required check counts as satisfied and a grey check reads as N/A.`,
      ).toBeDefined();

      const [coverFile, coverJob] = String(verdict.coveredBy).split("/");
      expect(coverFile, `${key}'s coveredBy must name a job in the same workflow`).toBe(file);

      const cover = parseWorkflow(coverFile).jobs[coverJob];
      expect(cover, `${key} names ${verdict.coveredBy} as cover, but that job does not exist`).toBeDefined();

      const coverIf = cover.if ?? "";
      expect(
        /!cancelled\(\)|always\(\)/.test(coverIf),
        `${key}'s cover ${verdict.coveredBy} has if: (${coverIf || "<absent>"}) -- it would be ` +
          `skipped by the same upstream failure, so it covers nothing`,
      ).toBe(true);

      const coverNeeds = Array.isArray(cover.needs) ? cover.needs : cover.needs === undefined ? [] : [cover.needs];
      expect(
        coverNeeds.map(String),
        `${key}'s cover ${verdict.coveredBy} does not list ${jobName} in its needs:, so the skip is ` +
          `outside its field of view entirely`,
      ).toContain(jobName);
    }
  });
});

// ── 8. CPE-1978: the step named "Verify" runs the verifier ──────────────────────────────────────
// `release.yml`'s "Verify the signed bundle before uploading it" checked that a `.sig` FILE EXISTS.
// Its own comment named CPE-1954 as the enabler for the real check and said so honestly; CPE-1954
// landed (PR #1088), so `Verify the signed bundle's signatures under the trusted key (CPE-1978)`
// now runs `catalog-sign verify` before the upload.
//
// Everything below is executed or derived. The bash legs run the shipped `run:` bodies with a stub
// `cargo`; the stub is deliberately KEY-SENSITIVE (it accepts one pubkey and refuses every other),
// because key-sensitivity is the property the step's own control leg depends on. What a stub cannot
// speak for -- that the REAL binary refuses a bad bundle -- is pinned in
// `sidecar/host/tests/catalog_sign_verify_gate.rs`
// (`a_bundle_from_the_real_signer_still_verifies`, and the "signed by a key the operator did not
// name" / "a signature that is not hex" arms of
// `every_unusable_index_is_refused_rather_than_waved_through`), and was re-measured by hand against
// the real binary while writing this -- see the PR body for those four exit statuses.

const VERIFY_STEP = "Verify the signed bundle's signatures under the trusted key (CPE-1978)";

/** The `env:` map the workflow gives a step in the catalog job. */
function stepEnv(name: string): Record<string, string> {
  return (step(name).env ?? {}) as Record<string, string>;
}

/** The public key `release.yml` verifies under, read out of the shipped workflow. */
function workflowPubkey(): string {
  const value = stepEnv(VERIFY_STEP).CATALOG_PUBKEY;
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(
      `${VERIFY_STEP} has no CATALOG_PUBKEY in its env:. The step cannot verify anything without ` +
        `a key, and this guard cannot check a key it cannot find -- do not "fix" this by deleting ` +
        `the assertion.`,
    );
  }
  return value;
}

/** A stub `cargo` that models the one property that matters: it accepts exactly one pubkey. */
function keySensitiveCargo(accepts: string): string {
  return [
    'echo "CARGO-RAN: $*"',
    // Everything after the invocation's `--` is the binary's argv; the pubkey is its last token.
    'for a in "$@"; do last="$a"; done',
    `if [ "$last" = "${accepts}" ]; then echo "OK: index + 1 manifest(s) verify under the key"; exit 0; fi`,
    'echo "FAIL: index signature does not verify under the key" >&2',
    "exit 1",
  ].join("\n");
}

const A_BUNDLE = {
  "catalog-out/catalog-index.json": VALID_INDEX,
  "catalog-out/catalog-index.json.sig": "not a signature",
};

describe("the presence check is not a verification, and the workflow no longer relies on it (CPE-1978)", () => {
  // THE GAP, ASSERTED ON EXIT STATUS. This is the whole ticket in one measurement: hand the
  // presence-only step a bundle whose detached signature is the ASCII text `not a signature`, and
  // it exits 0 and the job proceeds to upload. It still does -- that step's scope is unchanged and
  // deliberately so -- which is exactly why the step below it had to start existing.
  it.skipIf(!hasBash)("a bundle whose .sig does NOT verify still passes the presence-only step, exit 0", () => {
    const r = execStep(runBody("Verify the signed bundle before uploading it"), {
      files: A_BUNDLE,
      stubs: { jq: "echo 1" },
    });
    expect(r.status).toBe(0);
  });

  it("the presence-only step is followed, in the same job and BEFORE the upload, by the real check", () => {
    const names = (catalogJob.steps ?? []).map((s) => s.name);
    const presence = names.indexOf("Verify the signed bundle before uploading it");
    const real = names.indexOf(VERIFY_STEP);
    const upload = names.indexOf("Upload catalog assets to the release");
    expect(presence, "the presence step is gone -- update this guard rather than dropping it").toBeGreaterThanOrEqual(0);
    expect(real, `the catalog job has no step named "${VERIFY_STEP}"`).toBeGreaterThan(presence);
    expect(
      upload,
      "the real verification must run BEFORE the upload -- a bundle that fails it must never reach " +
        "the release at all, the same ordering argument CPE-1953 made for the presence check",
    ).toBeGreaterThan(real);
  });
});

describe("the verify step's key is the one clients trust, derived not asserted (CPE-1978)", () => {
  // CPE-1933: the workflow could carry any 64 hex characters and still look right. The value that
  // makes the check MEAN something is the key installed clients trust, and that lives in exactly one
  // place -- `CATALOG_TRUSTED_KEYS` in `src-tauri/src/lib.rs`. Read it from there, comments stripped
  // (the const's own doc comment names the constant, so an anchored scan over raw source is the
  // documented trap), and require the workflow's literal to be one of its entries.
  //
  // RED-PROOFED BY EXECUTION, and the result belongs here rather than only in the PR body: with the
  // workflow literal's leading `5b` changed to `5c` and `src-tauri/src/lib.rs` untouched, this test
  // failed with "release.yml verifies under 5c18... which is not one of CATALOG_TRUSTED_KEYS";
  // with the workflow left alone and the Rust const's leading `5b` changed to `5c`, it failed the
  // same way. Both reverted.
  const TRUSTED_KEYS = rustStrSliceAfter(
    stripRustComments(readFileSync(join(process.cwd(), "src-tauri", "src", "lib.rs"), "utf8")),
    "const CATALOG_TRUSTED_KEYS",
  );

  it("CATALOG_TRUSTED_KEYS is a non-empty list of ed25519 public keys", () => {
    // A near-empty derivation is not a clean one: an empty list would make the membership check
    // below vacuous in the direction that matters.
    expect(TRUSTED_KEYS.length, "no keys derived from CATALOG_TRUSTED_KEYS").toBeGreaterThanOrEqual(1);
    for (const k of TRUSTED_KEYS) expect(k).toMatch(/^[0-9a-f]{64}$/);
  });

  it("release.yml verifies under a key CATALOG_TRUSTED_KEYS actually holds", () => {
    const wf = workflowPubkey();
    expect(wf, `release.yml's CATALOG_PUBKEY (${wf}) is not 64 lowercase hex characters`).toMatch(/^[0-9a-f]{64}$/);
    expect(
      TRUSTED_KEYS,
      `release.yml verifies under ${wf}, which is not one of CATALOG_TRUSTED_KEYS ` +
        `(${TRUSTED_KEYS.join(", ")}). Either the workflow is checking bundles against a key no ` +
        `installed client trusts -- so a catalog every client rejects would publish green -- or a ` +
        `key rotation updated one of the two files and not the other. Both are the failure this ` +
        `step exists to catch; fix the value, never this assertion.`,
    ).toContain(wf);
  });

  it("the key is a plain literal in the workflow, not an expression a secret could fill in", () => {
    // The costed decision (see the step's own comment): a PUBLIC key gains nothing from being a
    // secret and loses two things -- reviewability, and the fact that an unset secret expands to
    // the empty string, i.e. fails OPEN. `${{ ... }}` here would reintroduce both.
    expect(workflowPubkey()).not.toContain("${{");
  });
});

describe("the verify step fails closed on every way the check can fail to RUN (CPE-1978)", () => {
  const KEY = workflowPubkey();
  const DECOY = `0${KEY.slice(1)}` === KEY ? `1${KEY.slice(1)}` : `0${KEY.slice(1)}`;
  const body = () => runBody(VERIFY_STEP);
  /** The step's SHIPPED `env:`, so these runs see the same CATALOG_PUBKEY the release would. */
  const shippedEnv = () => stepEnv(VERIFY_STEP);
  const run = (options: ExecOptions = {}) =>
    execStep(body(), { ...options, env: { ...shippedEnv(), ...(options.env ?? {}) } });

  it.skipIf(!hasBash)("a bundle that verifies under the trusted key -> exit 0", () => {
    const r = run({ files: A_BUNDLE, stubs: { cargo: keySensitiveCargo(KEY) } });
    expect(r.status, r.all).toBe(0);
  });

  it.skipIf(!hasBash)("it really invokes `catalog-sign verify` against catalog-out, twice, with both keys", () => {
    const r = run({ files: A_BUNDLE, stubs: { cargo: keySensitiveCargo(KEY) } });
    // Derived from the step's own argv rather than asserted about it: the two lines the stub echoed
    // are the two invocations the shipped body made.
    const ran = r.all.split("\n").filter((l) => l.includes("CARGO-RAN:"));
    expect(ran.length, `expected two cargo invocations, got:\n${r.all}`).toBe(2);
    for (const line of ran) {
      expect(line).toContain("--bin catalog-sign");
      expect(line).toContain("-- verify catalog-out");
    }
    expect(ran[0]).toContain(KEY);
    expect(ran[1]).toContain(DECOY);
  });

  it.skipIf(!hasBash)("the verifier says no under the trusted key -> the step FAILS, before any upload", () => {
    const r = run({
      files: A_BUNDLE,
      stubs: { cargo: keySensitiveCargo("f".repeat(64)) },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("::error::");
    expect(r.all).toContain("did not succeed under CATALOG_TRUSTED_KEYS");
  });

  // THE "RAN AND FOUND NOTHING" vs "DID NOT RUN" LEG, which an exit code alone cannot cover: a
  // verifier that says yes to everything passes the positive run. The step therefore runs the check
  // a second time under a key that provably did not sign the bundle and requires a REFUSAL, so a
  // stub, a no-op, a short-circuit, or a future edit that drops the `verify` subcommand is caught.
  it.skipIf(!hasBash)("a verifier that approves EVERYTHING -> the step FAILS on its own control", () => {
    const r = run({
      files: A_BUNDLE,
      stubs: { cargo: 'echo "CARGO-RAN: $*"; echo "OK: everything is fine"; exit 0' },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("The verifier is not verifying");
  });

  it.skipIf(!hasBash)("cargo absent from PATH entirely -> the step FAILS (127 is not a pass)", () => {
    // No `cargo` stub at all. On a developer machine a real cargo may sit on PATH, in which case
    // this exercises a real `cargo run` against a bundle that cannot verify -- which still fails.
    const r = run({
      files: A_BUNDLE,
      stubs: { cargo: 'echo "cargo: command not found" >&2; exit 127' },
    });
    expect(r.status).not.toBe(0);
  });

  it.skipIf(!hasBash).each([
    ["empty", ""],
    ["not hex", "zzzz" + "0".repeat(60)],
    ["uppercase (a different spelling of the same key is still not the literal we pin)", "5B".repeat(32)],
    ["too short", "abc123"],
    ["too long", "a".repeat(65)],
  ])("an unusable CATALOG_PUBKEY (%s) -> refused BEFORE the verifier is ever run", (_label, key) => {
    const r = run({
      files: A_BUNDLE,
      env: { CATALOG_PUBKEY: key as string },
      stubs: { cargo: 'echo "CARGO-RAN: $*"; exit 0' },
    });
    expect(r.status).not.toBe(0);
    // The distinction CLAUDE.md asks for: this must read as "the check did not run", and it must
    // not have quietly run under a garbage key and come back with an ordinary-looking "no".
    expect(r.all).not.toContain("CARGO-RAN");
  });

  it.skipIf(!hasBash)("no bundle on disk at all -> the step FAILS rather than verifying nothing", () => {
    const r = run({
      // The real binary reads catalog-out/catalog-index.json and exits 1 when it cannot; the stub
      // models that by refusing when the file is absent.
      stubs: { cargo: 'if [ -s catalog-out/catalog-index.json ]; then exit 0; fi; echo "read: no such file" >&2; exit 1' },
    });
    expect(r.status).not.toBe(0);
  });
});

// CPE-1932: "does release-sidecar.yml carry the same step?" is a question about a REMEMBERED list of
// two files. Enumerate instead -- every workflow step and every extracted script CI runs -- and ask
// the general question: does anything that SIGNS a bundle also VERIFY it before publishing?
//
// Measured on this revision: `release-sidecar.yml` has no catalog job and signs nothing, so there is
// nothing there to diverge; the sign-family invocations are `release.yml`'s `catalog-sign` (sign +,
// now, verify) and `model-snapshot.yml`'s `model-snapshot-sign` (sign only).
//
// RED-PROOFED, and specifically against the trap CPE-1933 rule 2 names. The real verify invocation
// in `release.yml` was replaced with a `#` COMMENT carrying the identical text (`# cargo run …
// --bin catalog-sign -- verify catalog-out "$1"`) and a `true` in its place. Six tests went red,
// including "release.yml signs a catalog bundle and never verifies it" and the floor below -- i.e.
// the commented-out invocation was counted by nothing. A whole-line-comment filter would have
// caught that one shape; the trailing-comment shape it would NOT have caught is exactly why this
// delegates to `logicalLines` instead of filtering here. Reverted.
describe("every workflow that SIGNS a bundle also verifies it before publishing (CPE-1932/CPE-1978)", () => {
  interface SignFamilyCall {
    where: string;
    file: string;
    bin: string;
    manifest?: string;
    verifying: boolean;
  }

  /**
   * Every `cargo run … --bin <something>-sign …` across all the shell CI executes.
   *
   * Anchored on code, never on prose: `logicalLines` (the same stripper
   * `crates/updater-verify/src/workflow_scan.rs` is the Rust port of) removes whole-line AND
   * trailing comments, joins `\` continuations, and skips heredoc bodies -- so the four prose
   * comments in `release.yml` that mention `catalog-sign` are invisible here, which is the point.
   *
   * What this cannot see, stated rather than left to be discovered: a signer invoked as a built
   * binary path rather than through `cargo run --bin`, a bin name that does not end in `-sign`, and
   * a signer run from a composite action rather than a `run:` step. At least those; the list is
   * open. The near-empty backstop below is what stops any of them turning this into a vacuous pass
   * silently -- it cannot see them either, but it will not let the population collapse unnoticed.
   */
  function signFamilyCalls(): SignFamilyCall[] {
    const out: SignFamilyCall[] = [];
    for (const unit of allShellUnits()) {
      for (const line of logicalLines(unit.run)) {
        const toks = line.split(/\s+/).map((t) => t.replace(/^["']|["']$/g, ""));
        const at = toks.indexOf("--bin");
        if (at < 0) continue;
        const bin = toks[at + 1];
        if (!bin || !bin.endsWith("-sign")) continue;
        const dashdash = toks.indexOf("--", at);
        const argv = dashdash < 0 ? [] : toks.slice(dashdash + 1);
        const mp = toks.indexOf("--manifest-path");
        out.push({
          where: unit.where,
          file: unit.file,
          bin,
          manifest: mp < 0 ? undefined : toks[mp + 1],
          verifying: argv[0] === "verify",
        });
      }
    }
    return out;
  }

  /**
   * Whether `bin` even HAS a `verify` subcommand, derived from the crate manifest the invocation
   * itself names and the Rust file that manifest points at (comments stripped first -- the one
   * `verify` in `model_snapshot_sign.rs` is inside a comment, so a raw-source scan would answer
   * this question wrong in the direction that hides a gap).
   *
   * Blind spot, named: a verify path spelled some other way (`check`, a clap subcommand, a flag)
   * reads as absent here. That fails toward reporting a gap that is already closed -- loud, not
   * silent -- which is the safe direction for this particular scan.
   */
  function hasVerifySubcommand(manifest: string, bin: string): boolean {
    const toml = readFileSync(join(process.cwd(), manifest), "utf8");
    const blocks = toml.split("[[bin]]").slice(1);
    for (const block of blocks) {
      const name = /^\s*name\s*=\s*"([^"]+)"/m.exec(block)?.[1];
      const path = /^\s*path\s*=\s*"([^"]+)"/m.exec(block)?.[1];
      if (name !== bin || !path) continue;
      const crateDir = manifest.slice(0, manifest.lastIndexOf("/"));
      const src = stripRustComments(readFileSync(join(process.cwd(), crateDir, path), "utf8"));
      return src.includes('"verify"');
    }
    throw new Error(`${manifest} declares no [[bin]] named ${bin} -- the invocation and the manifest disagree`);
  }

  const CALLS = signFamilyCalls();

  it("the enumeration found the signers it is supposed to police", () => {
    expect(
      CALLS.length,
      "no `--bin *-sign` invocation found in any workflow step or script. This scan has stopped " +
        "seeing its own subject, which would make every assertion below a vacuous pass -- fix the " +
        "scan, never lower this floor.",
    ).toBeGreaterThanOrEqual(3);
    const bins = [...new Set(CALLS.map((c) => c.bin))].sort();
    expect(bins).toContain("catalog-sign");
  });

  it("release.yml's catalog-sign signing call is matched by a verify call in the same workflow", () => {
    const catalogCalls = CALLS.filter((c) => c.bin === "catalog-sign");
    expect(catalogCalls.some((c) => !c.verifying), "release.yml no longer signs a catalog").toBe(true);
    expect(
      catalogCalls.some((c) => c.verifying && c.file.endsWith("/release.yml")),
      "release.yml signs a catalog bundle and never verifies it -- CPE-1978 reopened",
    ).toBe(true);
  });

  it("no signer publishes unverified while its binary CAN verify", () => {
    const stillUnverified: string[] = [];
    for (const bin of [...new Set(CALLS.map((c) => c.bin))]) {
      const calls = CALLS.filter((c) => c.bin === bin);
      const signing = calls.filter((c) => !c.verifying);
      if (signing.length === 0) continue;
      const files = [...new Set(signing.map((c) => c.file))];
      for (const file of files) {
        if (calls.some((c) => c.verifying && c.file === file)) continue;
        const manifest = signing.find((c) => c.file === file)?.manifest;
        if (!manifest) {
          throw new Error(`${bin} is invoked in ${file} with no --manifest-path -- cannot derive whether it can verify`);
        }
        if (hasVerifySubcommand(manifest, bin)) stillUnverified.push(`${file} -> ${bin}`);
      }
    }
    expect(
      stillUnverified,
      `these workflows sign and publish a bundle without verifying it, using a binary that HAS a ` +
        `verify subcommand: ${stillUnverified.join(", ")}. That is CPE-1978's defect in a sibling ` +
        `workflow -- wire the verify call in rather than widening this test.`,
    ).toEqual([]);
  });

  // The open half, recorded as a derived fact rather than as prose so it cannot go stale silently.
  // `model-snapshot.yml` signs `models-index.json` with the SAME key and publishes it to the
  // `model-catalog` release with no signature check at all -- the identical shape this ticket
  // closes for the agent catalog. It is not closed here because `model-snapshot-sign` has no verify
  // path to call: closing it means adding one to the binary, which is its own change with its own
  // blast radius (a scheduled workflow) and belongs in its own ticket. The day someone adds that
  // subcommand, this test reds and the assertion above starts demanding the wiring.
  it("model-snapshot-sign still has no verify subcommand -- the reason its workflow is not covered", () => {
    const snapshot = CALLS.filter((c) => c.bin === "model-snapshot-sign");
    expect(snapshot.length, "model-snapshot.yml no longer signs a snapshot -- re-derive this note").toBeGreaterThanOrEqual(1);
    const manifest = snapshot[0].manifest;
    expect(manifest, "the model-snapshot-sign invocation names no --manifest-path").toBeDefined();
    expect(
      hasVerifySubcommand(String(manifest), "model-snapshot-sign"),
      "model-snapshot-sign has grown a `verify` subcommand. The reason model-snapshot.yml publishes " +
        "an unverified signed bundle no longer holds -- wire `verify` into that workflow the way " +
        "CPE-1978 did for release.yml, then delete this test.",
    ).toBe(false);
  });
});
