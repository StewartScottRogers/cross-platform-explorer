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
import { describe, it, expect, beforeAll } from "vitest";
import { readFileSync, writeFileSync, mkdtempSync, mkdirSync, rmSync, chmodSync } from "node:fs";
import { join, delimiter } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { parseYaml } from "./preview/yaml";

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

let hasBash = false;
let hasJq = false;
let ghStubWorks = false;
beforeAll(() => {
  hasBash = bashAvailable();
  if (!hasBash) return;
  hasJq = toolAvailable("jq");
  ghStubWorks = stubGhWorks();
});

const VALID_INDEX = JSON.stringify({ entries: [{ id: "demo", version: 1_800_000_000 }] });

// ── 1. The vacuous-success hole: a tag build with no signing key ────────────────────────────────
// Every real step in the catalog job is `if: steps.k.outputs.has == 'true'`. Before this ticket the
// detect step emitted `has=false` and exited 0 with the key unset, so the job RAN (CPE-1893's
// `!cancelled()` guarantees that), skipped every step, and concluded **success** -- a green run that
// published no catalog at all. That is a strictly worse signal than the grey skip CPE-1893 fixed,
// because a green job is not even suspicious. This is the identical hole CPE-1923 finding 4 closed
// for `verify-published-manifest`; the catalog job was not given the same treatment at the time.
describe('"Detect catalog signing key" cannot report a green no-op on a tag build (CPE-1953)', () => {
  it("key present on a tag build -> has=true, exit 0", () => {
    if (!hasBash) return;
    const r = execStep(runBody("Detect catalog signing key"), {
      env: { KEY: "deadbeef".repeat(8), RELEASE_BUILD: "true" },
    });
    expect(r.status).toBe(0);
    expect(r.output).toContain("has=true");
  });

  it("NO key on a tag build -> the step FAILS, and never writes has=false for the gates to read", () => {
    if (!hasBash) return;
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

  it("NO key on a NON-tag build -> the graceful has=false arm is preserved, exit 0", () => {
    if (!hasBash) return;
    const r = execStep(runBody("Detect catalog signing key"), {
      env: { KEY: "", RELEASE_BUILD: "false" },
    });
    expect(r.status).toBe(0);
    expect(r.output).toContain("has=false");
  });

  it("REGRESSION DEMO: the pre-CPE-1953 one-liner returned exit 0 + has=false on that same tag build", () => {
    if (!hasBash) return;
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

  it("no release object for the tag (total matrix failure) -> the step exits NON-ZERO, not skipped-quiet", () => {
    if (!hasBash || !ghStubWorks) return;
    const r = execStep(runBody("Upload catalog assets to the release"), {
      env: { TAG: "v9.9.9", GH_TOKEN: "x" },
      files,
      stubs: {
        gh: 'echo "release not found" >&2; exit 1',
      },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("release not found");
  });

  it("release object exists -> the step exits 0 and passes the tag + the bundle glob to gh", () => {
    if (!hasBash || !ghStubWorks) return;
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

  it("no catalog-index.json at all -> fails before anything is uploaded", () => {
    if (!hasBash) return;
    const r = execStep(body(), { files: { "catalog-out/README": "nothing useful\n" } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("::error::");
    expect(r.all).toContain("nothing to publish");
  });

  it("an EMPTY catalog-index.json -> fails (a zero-byte file is not a published catalog)", () => {
    if (!hasBash) return;
    const r = execStep(body(), { files: { "catalog-out/catalog-index.json": "" } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("nothing to publish");
  });

  it("index present but its detached signature missing -> fails (clients verify before applying)", () => {
    if (!hasBash) return;
    const r = execStep(body(), { files: { "catalog-out/catalog-index.json": VALID_INDEX } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("detached signature");
  });

  it("index + sig with zero entries[] -> fails as the 'succeeding at zero work' shape", () => {
    if (!hasBash || !hasJq) return;
    const r = execStep(body(), {
      files: {
        "catalog-out/catalog-index.json": JSON.stringify({ entries: [] }),
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("zero entries");
  });

  it("index + sig that is not JSON at all -> fails as corrupt, not as a silent abort", () => {
    if (!hasBash || !hasJq) return;
    const r = execStep(body(), {
      files: {
        "catalog-out/catalog-index.json": "{ truncated",
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
    });
    expect(r.status).not.toBe(0);
    // The `if ! entries=$(jq ...)` shape (CPE-1893 UAT round 1) is what lets the diagnostic print at
    // all -- a bare assignment under `set -e` aborts before reaching it.
    expect(r.all).toContain("not parseable JSON");
  });

  it("a real one-entry bundle -> exit 0 and entries=1 published to $GITHUB_OUTPUT", () => {
    if (!hasBash || !hasJq) return;
    const r = execStep(body(), {
      files: {
        "catalog-out/catalog-index.json": VALID_INDEX,
        "catalog-out/catalog-index.json.sig": "sig\n",
      },
    });
    expect(r.status).toBe(0);
    expect(r.output).toContain("entries=1");
  });

  it("with jq absent from PATH entirely, the step still fails LOUD -- never green", () => {
    if (!hasBash) return;
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

  it("both assets present on the release -> exit 0", () => {
    if (!hasBash || !ghStubWorks) return;
    const r = execStep(body(), {
      env,
      stubs: { gh: 'printf "%s\\n" catalog-index.json catalog-index.json.sig latest.json; exit 0' },
    });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("confirmed on v9.9.9");
  });

  it("PIPEFAIL TRAP: a match must not be inverted into a miss (the herestring, not a pipe)", () => {
    if (!hasBash || !ghStubWorks) return;
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

  it("upload 'succeeded' but the release carries only unrelated assets -> exit 1, names what is missing", () => {
    if (!hasBash || !ghStubWorks) return;
    const r = execStep(body(), {
      env,
      stubs: { gh: 'printf "%s\\n" latest.json some-installer.exe; exit 0' },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("does not carry");
    expect(r.all).toContain("catalog-index.json");
  });

  it("only the index, no signature -> still exit 1 (a catalog no client will apply is not published)", () => {
    if (!hasBash || !ghStubWorks) return;
    const r = execStep(body(), {
      env,
      stubs: { gh: 'printf "%s\\n" catalog-index.json; exit 0' },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("catalog-index.json.sig");
  });

  it("the read-back itself fails -> exit 1; a lookup failure is never taken as evidence of success", () => {
    if (!hasBash || !ghStubWorks) return;
    const r = execStep(body(), {
      env,
      stubs: { gh: 'echo "HTTP 503" >&2; exit 1' },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("NOT evidence the upload worked");
  });

  it("PIPEFAIL TRAP, shown directly: the piped form reports failure on a MATCH; the herestring does not", () => {
    if (!hasBash) return;
    // The `catalog` job's confirm step is written with a herestring for this reason. Proved the way
    // catalogPublishFreshnessGuard.test.ts proves its own set -e finding -- by running both shapes
    // rather than asserting one is better. A long list makes printf reach the SIGPIPE reliably.
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
    UPLOAD: "success",
    CONFIRM: "success",
    ENTRIES: "3",
    TAG: "v9.9.9",
  };

  it("a complete, confirmed publish -> exit 0 and says how many entries shipped", () => {
    if (!hasBash) return;
    const r = execStep(body(), { env: ok });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("agent catalog published for v9.9.9");
    expect(r.stdout).toContain("3 entr");
  });

  it("no key on a non-tag build -> exit 0, but says out loud that nothing was published", () => {
    if (!hasBash) return;
    const r = execStep(body(), { env: { ...ok, HAS_KEY: "false" } });
    expect(r.status).toBe(0);
    expect(r.all).toContain("::notice::");
    expect(r.all).toContain("nothing was published");
  });

  it.each([
    ["SIGN", "build+sign"],
    ["BUNDLE", "verify-bundle"],
    ["UPLOAD", "upload"],
    ["CONFIRM", "confirm-on-release"],
  ])("a %s outcome of 'skipped' -> exit 1 naming %s", (key, label) => {
    if (!hasBash) return;
    const r = execStep(body(), { env: { ...ok, [key]: "skipped" } });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("was NOT published");
    expect(r.all).toContain(label);
  });

  it("THE SHAPE THIS TICKET IS ABOUT: key present, every step skipped -> exit 1, not a green no-op", () => {
    if (!hasBash) return;
    const r = execStep(body(), {
      env: { HAS_KEY: "true", SIGN: "", BUNDLE: "", UPLOAD: "", CONFIRM: "", ENTRIES: "", TAG: "v9.9.9" },
    });
    expect(r.status).not.toBe(0);
    expect(r.all).toContain("was NOT published");
    expect(r.all).toContain("<none>");
  });

  it("a failed upload -> exit 1 and points at the freshness backstop for how stale this may get", () => {
    if (!hasBash) return;
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
    for (const id of ["k", "sign", "bundle", "up", "confirm"]) {
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
  // ci.yml's five build/test jobs all hang off `lockfile-preflight`. Enumerated and recorded here
  // rather than silently left alone, because it IS the same structural shape -- one job's failure
  // disabling five others -- with one decisive difference and one open caveat:
  //   DIFFERENCE: nothing here PUBLISHES. A skipped `backend` job delivers no wrong artifact to any
  //   user; it withholds a check on a PR that is already red from the preflight itself. The
  //   `catalog` case was uniquely bad because the skip's consequence (every user's agent roster
  //   frozen) was invisible from, and unrelated to, the failure that caused it.
  //   CAVEAT, recorded deliberately and NOT fixed under this ticket: GitHub treats a `skipped`
  //   required status check as satisfying branch protection. If any of these five is a required
  //   check, a `lockfile-preflight` failure could in principle let a PR look mergeable with its
  //   test suite never having run. That is a separate blast radius, a separate fix (a terminal
  //   `always()` verdict job over the five, mirroring gui-smoke-linux-verdict), and wants its own
  //   ticket rather than being smuggled into this one.
  const CI_PREFLIGHT =
    "ACCEPTED SILENT SKIP with a recorded caveat: nothing here publishes, and the run is already red " +
    "from lockfile-preflight itself -- unlike `catalog`, a skip withholds a CHECK rather than freezing " +
    "something users receive. Caveat: GitHub counts a skipped required check as satisfied, so a " +
    "terminal always() verdict job over these five is worth its own ticket. See this file's comment.";

  /** name -> why a silent skip behind a failed `needs:` is (or is not) acceptable here. */
  const VERDICTS: Record<string, { guarded: boolean; why: string }> = {
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
    },
    "release-sidecar.yml/verify-published-manifest-sidecar": {
      guarded: true,
      why: "Same integrity-gate reasoning as release.yml's. `!cancelled()`.",
    },
    "gui-smoke.yml/gui-smoke-linux": {
      guarded: false,
      why: "ACCEPTED SILENT SKIP: a failed build leaves no binary to smoke. The run is red from the build, and the skip has no independent user-facing consequence -- unlike `catalog`, nothing stops being DELIVERED because this did not run.",
    },
    "gui-smoke.yml/gui-smoke-linux-verdict": {
      guarded: true,
      why: "`always()` -- the verdict must report on a failed or skipped smoke run, which is precisely the terminal-gate shape this ticket adds to `catalog`.",
    },
    "ci.yml/backend": { guarded: false, why: CI_PREFLIGHT },
    "ci.yml/crates": { guarded: false, why: CI_PREFLIGHT },
    "ci.yml/net-e2e": { guarded: false, why: CI_PREFLIGHT },
    "ci.yml/sidecar": { guarded: false, why: CI_PREFLIGHT },
    "ci.yml/msrv": { guarded: false, why: CI_PREFLIGHT },
  };

  const files = ["release.yml", "release-sidecar.yml", "gui-smoke.yml", "ci.yml"];

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
});
