// CPE-1917 — two structural ratchets over release plumbing that failed silently for 27 days.
//
// ── What happened ──────────────────────────────────────────────────────────────────────────────
// Every run of `.github/workflows/release.yml` from 2026-08-04 to 2026-08-23 failed on all three
// matrix legs with `verify-release-artifacts: no latest.json found under ../../src-tauri/target`,
// and the dependent `catalog` job was `skipped` on every one. The manifest was never missing:
// tauri-action writes `latest.json` to its own cwd (the repo root) and uploads it straight to the
// release — re-confirmed for this ticket against `v0.57.69`'s real draft, whose 7,206-byte
// latest.json names 11 platforms and verifies clean (`OK: verified 11 of 11`) under today's gate.
// The verifier was simply pointed at a directory the manifest has never been written to.
//
// ── Ratchet 1: the two halves of "where latest.json is" must agree ─────────────────────────────
// CPE-1872 moved the check into the post-matrix `verify-published-manifest` job, which downloads the
// PUBLISHED manifest plus every asset it names into `release-assets/` and verifies that. That makes
// the manifest's location a fact split across two steps — the `gh release download --dir <DIR>` and
// the `--manifest <DIR>/latest.json --search <DIR>` — with nothing tying them together. Move either
// and the workflow breaks exactly as before, discoverable only on the next version tag someone
// happens to push (the plain channel's own cadence is ~monthly). The executable half of this pin
// lives in `crates/updater-verify/tests/release_workflow_wiring.rs`, which reads both halves out of
// the YAML and runs the real binary with the real argv; this file asserts the structural facts a
// running binary cannot see — which job the gate lives in, what gates that job, and that the two
// steps share a runner at all.
//
// ── Ratchet 2: the watchdog that was supposed to notice must stay wired to its subjects ────────
// The more expensive half of this bug is that nothing surfaced it for 27 days. CPE-1872 added
// `release-pipeline-watchdog.yml` for exactly that, and it selects its subjects by workflow DISPLAY
// NAME (`workflows: ["Release", "Release (sidecar-enabled)"]`) — a `workflow_run` trigger silently
// matches nothing if a name drifts, and a workflow that never fires looks identical to one with
// nothing to report. That is the same month-long silence reproducing one level up, inside the alarm
// itself. The tests below resolve those strings against the `name:` fields in the real workflow
// files instead of hard-coding them, so renaming a release workflow without updating the watchdog
// fails here rather than going quiet in production.
//
// Structural assertions go through `parseYaml` (src/lib/preview/yaml.ts, CPE-1617) — the same
// approach releaseHangHardening.test.ts (CPE-1824) and catalogPublishFreshnessGuard.test.ts
// (CPE-1893) use, adopted after a review round found a regex-over-raw-text guard could be satisfied
// by an unrelated neighbouring comment rather than the key it claimed to check.
import { describe, it, expect } from "vitest";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";

const ROOT = process.cwd();
const WORKFLOWS = join(ROOT, ".github", "workflows");

function read(fileName: string): string {
  return readFileSync(join(WORKFLOWS, fileName), "utf8");
}

interface WorkflowStep {
  name?: string;
  run?: string;
  if?: string;
  "working-directory"?: string;
  [key: string]: unknown;
}
interface WorkflowJob {
  needs?: string | string[];
  if?: string;
  steps?: WorkflowStep[];
  [key: string]: unknown;
}
interface WorkflowDoc {
  name?: string;
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

/** `on:` is YAML 1.1's boolean `true` under some loaders; go through the string key explicitly. */
function triggers(doc: WorkflowDoc): Record<string, unknown> {
  return doc["on" as keyof WorkflowDoc] as Record<string, unknown>;
}

function steps(job: WorkflowJob | undefined): WorkflowStep[] {
  return (job?.steps ?? []) as WorkflowStep[];
}

function stepRunning(job: WorkflowJob | undefined, needle: string): WorkflowStep | undefined {
  return steps(job).find((s) => typeof s.run === "string" && s.run.includes(needle));
}

describe("release.yml's updater gate is wired to the manifest it actually downloads (CPE-1917)", () => {
  const doc = parseWorkflow("release.yml");
  const verifyJob = doc.jobs["verify-published-manifest"];

  it("the gate lives in the post-matrix verify-published-manifest job", () => {
    expect(
      verifyJob,
      "release.yml has no verify-published-manifest job -- the release gate CPE-1058 exists to " +
        "provide is gone",
    ).toBeDefined();
    expect(stepRunning(verifyJob, "verify-release-artifacts")).toBeDefined();
  });

  it("the matrix `release` job no longer runs the verifier itself", () => {
    // The per-leg check CPE-1872 deleted verified a fragment of a manifest that is the UNION of all
    // three legs, and reported success on it. It is also the step whose wrong --search root failed
    // every run for 27 days. Neither may come back.
    expect(stepRunning(doc.jobs.release, "verify-release-artifacts")).toBeUndefined();
  });

  it("the verifier runs from the repo root, so its relative paths mean what they say", () => {
    const verify = stepRunning(verifyJob, "verify-release-artifacts");
    // The original failure was `--search ../../src-tauri/target` resolved from
    // `working-directory: crates/updater-verify`. Every path in this step is repo-root-relative;
    // reintroducing a working-directory silently re-points all of them at once.
    expect(verify?.["working-directory"]).toBeUndefined();
  });

  it("the download and the verify happen in the same job, so the downloaded files still exist", () => {
    // Jobs get fresh runners with empty workspaces. Splitting these two across jobs would leave the
    // verifier searching an empty directory -- "no latest.json found", verbatim, all over again.
    const download = stepRunning(verifyJob, "gh release download");
    expect(download, "the manifest download step must live alongside the verify step").toBeDefined();
    const verify = stepRunning(verifyJob, "verify-release-artifacts");
    expect(verify).toBeDefined();
    expect(steps(verifyJob).indexOf(download!)).toBeLessThan(steps(verifyJob).indexOf(verify!));
  });

  it("the download and verify steps share one secret gate, so neither can run without the other", () => {
    // The download is guarded on the signing key being present (a fork/PR run has nothing signed to
    // verify). If the two guards ever diverge, the verify step runs against a directory the download
    // step skipped filling -- a red release for a reason that has nothing to do with the release.
    const download = stepRunning(verifyJob, "gh release download");
    const verify = stepRunning(verifyJob, "verify-release-artifacts");
    expect(verify?.if).toBe(download?.if);
    expect(verify?.if).toContain("steps.sig.outputs.has");
  });

  it("the gate runs on any outcome of the matrix except an outright cancellation", () => {
    // CPE-1872 round 3 finding A: a bare `needs: release` SKIPS this job the moment one fail-fast:
    // false leg fails -- and the surviving legs have already uploaded installers and a merged
    // manifest to the draft by then. Skipped reads like "nothing to do", which is how a fully
    // populated, entirely unverified draft went unnoticed.
    expect(verifyJob?.needs).toBe("release");
    expect(verifyJob?.if).toBe("${{ !cancelled() }}");
  });

  it("catalog is gated the same way, so a red gate can never silently skip the publish again", () => {
    // CPE-1893 fixed this; asserted here too because `catalog` skipping on all ~15 runs of this
    // outage is half of what CPE-1917 is about, and the two facts belong in one place.
    expect(doc.jobs.catalog?.if).toBe(verifyJob?.if);
  });
});

describe("release-pipeline-watchdog.yml is still pointed at the workflows it watches (CPE-1917)", () => {
  const watchdog = parseWorkflow("release-pipeline-watchdog.yml");
  const watched = (triggers(watchdog).workflow_run as Record<string, unknown>)
    ?.workflows as string[];

  it("watches by workflow_run completion", () => {
    const wr = triggers(watchdog).workflow_run as Record<string, unknown>;
    expect(wr, "the watchdog no longer reacts to workflow_run at all").toBeDefined();
    expect(wr.types).toEqual(["completed"]);
    expect(Array.isArray(watched)).toBe(true);
  });

  it.each([
    ["release.yml", "the plain release channel -- the one that failed silently for 27 days"],
    ["release-sidecar.yml", "the channel that actually ships the installed app"],
  ])("names %s's real `name:` field, resolved from the file rather than hard-coded", (file) => {
    // `workflow_run` matches on the DISPLAY NAME. If a workflow is renamed and this list is not,
    // GitHub matches nothing, the watchdog never runs, and its absence from the Actions tab reads
    // exactly like "no failures to report" -- the precise silence CPE-1917 is about, one level up.
    const name = parseWorkflow(file).name;
    expect(typeof name).toBe("string");
    expect(
      watched,
      `release-pipeline-watchdog.yml watches ${JSON.stringify(watched)}, which does not include ` +
        `${file}'s name ${JSON.stringify(name)}. A workflow_run trigger naming a workflow that ` +
        `does not exist fails SILENTLY -- rename one without the other and the only alarm on the ` +
        `release pipeline goes dark with nothing anywhere saying so.`,
    ).toContain(name);
  });

  it("every name it watches resolves to a workflow that actually exists", () => {
    // The mirror image of the test above: a leftover entry for a deleted/renamed workflow is dead
    // weight that reads like coverage.
    const realNames = [
      "release.yml",
      "release-sidecar.yml",
      "ci.yml",
      "catalog-freshness.yml",
      "ffmpeg-pin-freshness.yml",
      "gui-smoke.yml",
      "model-snapshot.yml",
      "release-pipeline-watchdog.yml",
    ].map((f) => parseWorkflow(f).name);
    for (const w of watched) {
      expect(realNames, `watchdog watches ${JSON.stringify(w)}, which no workflow is named`).toContain(w);
    }
  });

  it("fires on every non-success conclusion, not just `failure`", () => {
    // CPE-1872 round-2 finding 4: startup_failure / timed_out / cancelled are the three ways a
    // pipeline goes dark without a conclusion of `failure`.
    const job = watchdog.jobs["notify-on-failure"];
    expect(job?.if).toContain("!= 'success'");
    expect(job?.if).not.toContain("== 'failure'");
  });

  it("can actually raise the alarm it exists to raise", () => {
    const perms = watchdog.permissions as Record<string, string>;
    expect(perms.issues, "without issues: write the watchdog cannot file its tracking issue").toBe("write");
  });
});

describe("release.yml still fires on the tags it is supposed to fire on (CPE-1917)", () => {
  // The watchdog can only report on a run that HAPPENED. A tag filter that stops matching plain
  // version tags produces no run at all -- no red X, no issue, no entry in the Actions tab, nothing
  // for anyone to notice. That is a strictly quieter failure than the 27-day outage this ticket is
  // about, so the trigger CPE-1894 landed is pinned here rather than left as three characters of
  // YAML nobody checks.
  const doc = parseWorkflow("release.yml");
  const push = (triggers(doc).push as Record<string, unknown>) ?? {};

  it("triggers on plain version tags", () => {
    expect(push.tags).toContain("v*");
  });

  it("excludes the sidecar channel's tags, in the one form GitHub allows", () => {
    // CPE-1894: a bare `v*` also matched `v0.57.69-sidecar`, so the plain workflow fired on the
    // sidecar channel's tag and merged plain installers into the sidecar draft -- v0.57.69 shipped a
    // manifest naming assets from two different products. The negation must live INSIDE `tags:`;
    // GitHub explicitly forbids combining `tags` with `tags-ignore` for the same event, and a config
    // it rejects means the workflow does not run at all.
    expect(push.tags).toContain("!v*-sidecar");
    expect(push["tags-ignore"]).toBeUndefined();
    expect((doc["on" as keyof WorkflowDoc] as Record<string, unknown>)["tags-ignore"]).toBeUndefined();
  });

  it("the sidecar workflow still has no push trigger of its own", () => {
    // The two channels' triggers are disjoint only as long as this stays true: release-sidecar.yml
    // is workflow_dispatch-only, so the overreach was ever one-directional.
    const sidecar = parseWorkflow("release-sidecar.yml");
    expect(triggers(sidecar).push).toBeUndefined();
  });
});

// ── CPE-1923 finding 4 — the absent-secret vacuous success, in BOTH release workflows ──────────
//
// Both real steps of each verify job are gated on `steps.sig.outputs.has == 'true'`. With
// `TAURI_SIGNING_PRIVATE_KEY` unset the job ran, skipped both, and concluded `success` — and
// RELEASING.md's publish gate reads that conclusion as proof the manifest was verified. Deleting one
// repo secret silently converted BOTH channels' release-integrity gates into green no-ops,
// indistinguishable in the run summary from gates that actually checked something.
//
// The fix is that each sig-detect step FAILS when a run that is cutting a release finds no key,
// leaving both downstream `if:` conditions untouched (they are pinned by the tests above and by
// CPE-1908's `channelPurityCoverage.test.ts` via `SIGNING_KEY_STEP_IF`).
//
// These tests do not read the scripts' text and take their word for it — a guard asserting on a
// string while the process still exits 0 is the exact defect this ticket is about. They EXTRACT
// each step's `run` out of its YAML and EXECUTE it, asserting on the process exit code and on what
// it actually wrote to `$GITHUB_OUTPUT`.
//
// Both workflows are covered explicitly rather than one being tested and the other assumed to
// mirror it. "Mirrors the other workflow" is a provenance claim, and this repo has CPE-1933 open
// about exactly that kind of claim decaying into a lie — the two workflows' triggers genuinely
// differ (tag push vs `workflow_dispatch`), so a verbatim copy of the plain workflow's
// `ref_type == 'tag'` predicate would be DEAD CODE in the sidecar one.
describe("neither release workflow can report a green no-op when the signing secret is missing (CPE-1923)", () => {
  interface SigCase {
    /** Workflow file to read. */
    file: string;
    /** The job whose gate this step feeds. */
    job: string;
    /**
     * The `env:` key holding "is this run cutting a release?", and the expression each workflow
     * computes it from. They differ because the triggers differ; the `run:` script does not.
     */
    releaseBuildExpr: (value: string) => boolean;
    /** How the workflow answers "not cutting a release", if it can at all. */
    nonReleaseValue: string | null;
  }

  const CASES: SigCase[] = [
    {
      file: "release.yml",
      job: "verify-published-manifest",
      // Tag-push triggered: a tag ref IS a release being cut.
      releaseBuildExpr: (v) => v.includes("github.ref_type") && v.includes("'tag'"),
      nonReleaseValue: "false",
    },
    {
      file: "release-sidecar.yml",
      job: "verify-published-manifest-sidecar",
      // workflow_dispatch-only with a REQUIRED release-tag input, so every run cuts a release and
      // the predicate is hardcoded true. `github.ref_type` is `branch` on every run here, which is
      // why copying the plain workflow's predicate verbatim would silently disarm this.
      releaseBuildExpr: (v) => v.trim() === "true" || v.trim() === '"true"',
      nonReleaseValue: null,
    },
  ];

  /** The `sig` step of a workflow's verify job. */
  function sigStepOf(c: SigCase): WorkflowStep | undefined {
    const doc = parseWorkflow(c.file);
    return steps(doc.jobs[c.job]).find((s) => s.id === "sig");
  }

  /**
   * Execute a step's own `run` script under bash with a controlled environment, and report what the
   * process actually did: its exit status and whatever it appended to `$GITHUB_OUTPUT`.
   *
   * Both steps declare `shell: bash`, so running them under bash is running them the way the runner
   * does. The frontend test job is ubuntu-only (see ci.yml), and bash is present on Windows dev
   * machines via Git Bash; a missing bash is reported as a failure rather than skipped, because a
   * silently-skipped security guard is the thing this file exists to prevent.
   */
  function runScript(script: string, env: Record<string, string>): { status: number | null; outputs: string } {
    const dir = mkdtempSync(join(tmpdir(), "cpe-1923-sig-"));
    const outputFile = join(dir, "github_output");
    writeFileSync(outputFile, "");
    try {
      const result = spawnSync("bash", ["-c", script], {
        env: { ...process.env, ...env, GITHUB_OUTPUT: outputFile },
        encoding: "utf8",
      });
      if (result.error) {
        throw new Error(
          `could not execute the sig step under bash (${result.error.message}). This guard must ` +
            `run the script, not read it -- fix the environment rather than weakening the test.`,
        );
      }
      return { status: result.status, outputs: readFileSync(outputFile, "utf8") };
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }

  it("both verify jobs still have an id'd `sig` detection step", () => {
    for (const c of CASES) {
      const step = sigStepOf(c);
      expect(
        step,
        `${c.file}'s ${c.job} has no \`sig\` step -- the step whose outputs gate the whole job is ` +
          `gone, and whatever replaced it needs its own executable guard, because this one can no ` +
          `longer find it`,
      ).toBeDefined();
      expect(typeof step?.run, `${c.file}: the sig step has no run: script`).toBe("string");
    }
  });

  it("the two workflows run a byte-identical script, so neither can drift into a weaker rule", () => {
    // This is what turns "the sidecar half mirrors release.yml" from a comment into a check. The
    // per-workflow difference is confined to the RELEASE_BUILD env value, asserted separately below.
    const [plain, sidecar] = CASES.map((c) => sigStepOf(c)?.run);
    expect(sidecar).toBe(plain);
  });

  it("each workflow reads the key, and computes 'is this cutting a release?' from its own trigger", () => {
    // The executable tests below inject KEY/RELEASE_BUILD as plain process env. That only proves
    // anything if each real step is wired to real context values, so pin the mapping too -- and pin
    // that each workflow's predicate suits ITS trigger rather than being copied from the other.
    for (const c of CASES) {
      const env = (sigStepOf(c)?.env ?? {}) as Record<string, string>;
      expect(env.KEY, `${c.file}`).toContain("secrets.TAURI_SIGNING_PRIVATE_KEY");
      expect(typeof env.RELEASE_BUILD, `${c.file} declares no RELEASE_BUILD`).toBe("string");
      expect(
        c.releaseBuildExpr(String(env.RELEASE_BUILD)),
        `${c.file}'s RELEASE_BUILD (${env.RELEASE_BUILD}) is not the right predicate for its own ` +
          `trigger. release.yml is tag-push triggered so it asks github.ref_type; ` +
          `release-sidecar.yml is workflow_dispatch-only (ref_type is always 'branch' there) with a ` +
          `required tag input, so it is unconditionally true. Copying one into the other produces a ` +
          `branch that never fires -- a guard that looks stronger than it is.`,
      ).toBe(true);
    }
  });

  for (const c of CASES) {
    it(`${c.file}: FAILS (non-zero exit) when cutting a release with no signing key`, () => {
      const script = sigStepOf(c)?.run as string;
      const { status, outputs } = runScript(script, { KEY: "", RELEASE_BUILD: "true" });
      expect(
        status,
        `${c.file}: a release-cutting run with no signing key must FAIL here. Exiting 0 with ` +
          `has=false makes the two verification steps skip and the job conclude \`success\` -- a ` +
          `release-integrity gate that verified nothing while reporting green (CPE-1923 finding 4).`,
      ).not.toBe(0);
      expect(
        outputs,
        `${c.file}: it must not also emit has=true/false -- the run is over, and a downstream step ` +
          `reading an output from a failed detection is exactly the ambiguity being removed`,
      ).not.toContain("has=");
    });

    it(`${c.file}: passes and arms the gate when the signing key is present`, () => {
      const script = sigStepOf(c)?.run as string;
      const { status, outputs } = runScript(script, { KEY: "a-signing-key", RELEASE_BUILD: "true" });
      expect(status, `${c.file}: the normal, correctly-configured release path must still succeed`).toBe(0);
      expect(outputs).toContain("has=true");
    });
  }

  it("release.yml still lets a non-tag run skip, so the rule is about cutting a release, not about the secret existing", () => {
    // release.yml is tag-triggered today, so this arm is not reachable in production -- it is what a
    // future non-tag trigger (a pull_request smoke run, a manual dispatch) would take. Keeping it is
    // what makes the failure above precise ("a release is being cut without a key") rather than
    // "this workflow now needs a secret to exist at all".
    //
    // release-sidecar.yml has no such arm to exercise: every dispatch of it cuts a release, so its
    // RELEASE_BUILD is hardcoded true. That asymmetry is real and is asserted above, not papered over.
    const plain = CASES[0];
    expect(plain.nonReleaseValue).not.toBeNull();
    const script = sigStepOf(plain)?.run as string;
    const { status, outputs } = runScript(script, { KEY: "", RELEASE_BUILD: plain.nonReleaseValue as string });
    expect(status).toBe(0);
    expect(outputs).toContain("has=false");
  });
});
