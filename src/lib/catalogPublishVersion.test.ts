// CPE-1941 — the guard on WHERE the agent catalog's `version` number comes from.
//
// `.github/workflows/release.yml`'s `catalog` job used to compute it inline as `VERSION=$(date +%s)`
// — a wall-clock reading taken at PUBLISH time, stamped uniformly on every entry of
// catalog-index.json. So the number recorded when the workflow ran, not what it published, and
// re-running the workflow on an OLD TAG republished that tag's old manifests under a version newer
// than anything installed. The trust engine's anti-rollback rule
// (`sidecar/host/src/catalog.rs::VersionStanding::refusal`) compares nothing but that number, so it
// accepted them: a content downgrade with every signature and hash intact, reachable by anyone who
// can press "Re-run jobs" on an old tag — no signing-key compromise required.
//
// The engine-side before/after is demonstrated in Rust
// (`sidecar/host/tests/catalog_republish_downgrade.rs`). THIS file guards the publish side, in two
// layers, because each on its own has a known way to rot:
//
//   1. Structural, via `parseYaml` (the approach catalogPublishFreshnessGuard.test.ts and
//      releaseHangHardening.test.ts settled on): the workflow really does derive its version from
//      the shared script, really does feed that value to the signer, and no step of that job reads a
//      clock. A regex over the raw file would be satisfied by a neighbouring comment — and this file
//      is full of comments quoting `date +%s` verbatim, so that failure mode is not hypothetical.
//   2. Executed: the script itself is run against fabricated inputs and against a REAL, purpose-built
//      git repository with two commits at controlled committer dates, so "an older ref yields a
//      smaller number, deterministically, no matter when you ask" is proven rather than asserted.
//      A structural check alone would still pass if the script's arithmetic were wrong.
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { readFileSync, mkdtempSync, mkdirSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { parseYaml } from "./preview/yaml";

const ROOT = resolve(__dirname, "..", "..");
const SCRIPT = join(ROOT, ".github", "workflows", "scripts", "catalog-version.sh");

interface WorkflowStep {
  name?: string;
  id?: string;
  run?: string;
  env?: Record<string, string>;
  "continue-on-error"?: unknown;
  [key: string]: unknown;
}
interface WorkflowJob {
  steps?: WorkflowStep[];
  [key: string]: unknown;
}
interface WorkflowDoc {
  jobs: Record<string, WorkflowJob>;
  [key: string]: unknown;
}

function parseWorkflow(fileName: string): WorkflowDoc {
  const result = parseYaml(readFileSync(join(ROOT, ".github", "workflows", fileName), "utf8"));
  if (!result.ok) throw new Error(`${fileName} did not parse as YAML: ${result.error}`);
  return result.value as WorkflowDoc;
}

function catalogSteps(): WorkflowStep[] {
  const steps = parseWorkflow("release.yml").jobs.catalog?.steps;
  expect(steps, "release.yml must still have a `catalog` job with steps").toBeTruthy();
  return steps as WorkflowStep[];
}

describe("release.yml stamps the catalog with a version derived from the tag, not from publish time (CPE-1941)", () => {
  it("the shared version script exists and is the only place the rule lives", () => {
    expect(existsSync(SCRIPT)).toBe(true);
    const text = readFileSync(SCRIPT, "utf8");
    expect(text).toContain("--format=%ct"); // committer timestamp of the tagged commit
    expect(text).toMatch(/^CATALOG_VERSION_FLOOR=\d+/m);
  });

  it("a step derives the version by invoking that script", () => {
    const derive = catalogSteps().filter((s) =>
      (s.run ?? "").includes(".github/workflows/scripts/catalog-version.sh"),
    );
    expect(derive.length, "exactly one step derives the catalog version").toBe(1);
    // It must publish the result for the signing step rather than computing it a second time.
    expect(derive[0].id).toBeTruthy();
    expect(derive[0].run).toContain("$GITHUB_OUTPUT");
  });

  it("the signing step consumes that step's output and never recomputes a version", () => {
    const steps = catalogSteps();
    const derive = steps.find((s) => (s.run ?? "").includes("catalog-version.sh"));
    const sign = steps.find((s) => (s.run ?? "").includes("--bin catalog-sign"));
    expect(sign, "the catalog job must still sign a bundle").toBeTruthy();
    const version = sign?.env?.VERSION ?? "";
    // Bound to the derive step BY ITS ID -- renaming that step's id without repointing this one
    // fails here rather than silently signing with an empty version.
    expect(version).toBe("${{ steps." + derive?.id + ".outputs.version }}");
    expect(sign?.run).toContain('test -n "$VERSION"'); // never sign an empty version
  });

  // The actual defect, stated as an invariant over the job rather than over one line of it: NO step
  // of the catalog job may read a clock to produce the version. `date +%s` inside this job is the
  // bug itself, in any step, however it is spelled.
  it("no step of the catalog job reads a wall clock to produce the version", () => {
    for (const step of catalogSteps()) {
      const run = step.run ?? "";
      expect(
        run,
        `step "${step.name ?? step.id ?? "?"}" must not derive a version from the clock`,
      ).not.toMatch(/VERSION\s*=\s*\$\(\s*date\b/);
    }
  });

  // `date -u -d "@$VERSION"` in the derive step's log line is a *rendering* of the derived number,
  // not a source for it -- so the guard above targets assignment, and this one pins that the only
  // remaining `date` use cannot flow into the signer.
  it("the version that reaches the signer comes from the script's stdout alone", () => {
    const derive = catalogSteps().find((s) => (s.run ?? "").includes("catalog-version.sh"));
    expect(derive?.run).toMatch(
      /VERSION=\$\(bash \.github\/workflows\/scripts\/catalog-version\.sh\b/,
    );
    // Whatever is written to $GITHUB_OUTPUT is that same VERSION, unmodified.
    expect(derive?.run).toMatch(/version=\$\{VERSION\}"?\s*>>\s*"?\$GITHUB_OUTPUT/);
  });

  it("neither the derive nor the sign step is allowed to fail softly", () => {
    for (const step of catalogSteps()) {
      const run = step.run ?? "";
      if (!run.includes("catalog-version.sh") && !run.includes("--bin catalog-sign")) continue;
      // A non-fatal verification step is how a broken version derivation would ship a bundle anyway.
      expect(step["continue-on-error"], `${step.name} must stay fatal`).toBeUndefined();
      expect(run).toContain("set -euo pipefail");
    }
  });

  // model-snapshot.yml deliberately KEEPS `date -u +%s`: its content is scraped live from the
  // reseller endpoints on each run, so publish time genuinely is content time there, and a commit
  // timestamp would freeze the snapshot at its first published version. Pinning that difference here
  // stops a future "consistency" pass from breaking one workflow while fixing the other.
  it("model-snapshot.yml keeps publish-time versions, and says why", () => {
    const raw = readFileSync(join(ROOT, ".github", "workflows", "model-snapshot.yml"), "utf8");
    expect(raw).toContain("VERSION=$(date -u +%s)");
    expect(raw).toContain("CPE-1941");
  });
});

// --- Executed: the script's own arithmetic -------------------------------------------------------

/**
 * bash is a REQUIREMENT here, not a probe-and-skip (CPE-1941 review, F4).
 *
 * The sibling catalogPublishFreshnessGuard.test.ts skips when bash is absent, and copying that was
 * the obvious move -- but this file argues, a few describes down, that a test which silently skips
 * "would guard nothing in the place it matters most", and it would be incoherent to reject that
 * pattern for the fixture repository and then use it for bash. Nine vacuous passes on a bash-less
 * machine is exactly the failure mode this repo keeps re-finding.
 *
 * Requiring it is safe: every environment that can check this repo out already has bash (it ships
 * with Git for Windows; CI's frontend job is ubuntu-latest), and the thing under test is a shell
 * script that only ever runs on ubuntu-latest. So a missing bash is a broken environment, worth one
 * loud failure rather than a green suite that proved nothing.
 */
function requireBash(): void {
  const probe = spawnSync("bash", ["--version"], { stdio: "ignore" });
  if (probe.error || probe.status !== 0) {
    throw new Error(
      "bash is required to execute .github/workflows/scripts/catalog-version.sh -- these tests " +
        "run the real script rather than asserting about it, so a missing bash is a broken " +
        "environment, not a reason to pass.",
    );
  }
}

function runScript(...args: string[]) {
  return spawnSync("bash", [SCRIPT, ...args], { encoding: "utf8" });
}

/** The floor, read from the script -- never copied, so this file cannot drift away from it. */
function floorFromScript(): number {
  const m = /^CATALOG_VERSION_FLOOR=(\d+)/m.exec(readFileSync(SCRIPT, "utf8"));
  expect(m, "catalog-version.sh must define CATALOG_VERSION_FLOOR").toBeTruthy();
  return Number(m![1]);
}

// The measured high-water mark of the installed base, across all 65 releases carrying a catalog
// index (read 2026-08-27). The highest version on any PUBLISHED release is 1784894333
// (2026-07-24T11:58:53Z, v0.57.31-sidecar) -- the true installed base, since clients fetch
// releases/latest/download/. The value below, 1784951108, comes from v0.57.32, which is a DRAFT
// (isDraft: true, published_at: null), so no client ever fetched it. It is used here deliberately:
// it errs HIGH -- a floor clearing it necessarily clears the real one -- and it is what a plain max
// over the releases API returns, so anyone re-measuring this later lands on the same value.
const HIGHEST_INSTALLED_UNDER_THE_OLD_SCHEME = 1_784_951_108;

describe("catalog-version.sh, executed (CPE-1941)", () => {
  beforeAll(() => requireBash());

  it("the floor clears the installed base, so the first commit-derived release is an upgrade", () => {
    expect(floorFromScript()).toBeGreaterThan(HIGHEST_INSTALLED_UNDER_THE_OLD_SCHEME);
  });

  it("the floor is in the past, so a commit made today can satisfy it", () => {
    // The opposite failure and just as fatal: a floor set into the future fails every release.
    expect(floorFromScript()).toBeLessThanOrEqual(Math.floor(Date.now() / 1000));
  });

  it("a version at the floor is accepted", () => {
    const floor = floorFromScript();
    const r = runScript("--validate", String(floor), String(floor + 60));
    expect(r.status).toBe(0);
    expect(r.stdout.trim()).toBe(String(floor));
  });

  it("the highest pre-existing catalog version is now BELOW the floor and is refused (exit 3)", () => {
    const r = runScript("--validate", String(HIGHEST_INSTALLED_UNDER_THE_OLD_SCHEME));
    expect(r.status).toBe(3);
    expect(r.stderr).toContain("BELOW the floor");
    expect(r.stdout.trim()).toBe(""); // never echoes a fallback a caller could use anyway
  });

  it("a far-future version is refused rather than published (exit 4)", () => {
    // A commit date can be set outright (GIT_COMMITTER_DATE), and a version far in the future would
    // block every LATER release as a rollback. Fatal at publish time, not a warning.
    const now = floorFromScript() + 1000;
    const r = runScript("--validate", String(now + 40 * 86_400), String(now));
    expect(r.status).toBe(4);
    expect(r.stderr).toContain("ahead of now");
    expect(r.stdout.trim()).toBe("");
  });

  it("a non-integer, an empty value, and a leading-zero value are all refused (exit 2)", () => {
    for (const bad of ["abc", "", "01787000001", "17.87e9", "-1787000001"]) {
      const r = runScript("--validate", bad);
      expect(r.status, `"${bad}" must be refused`).toBe(2);
      expect(r.stdout.trim()).toBe("");
    }
  });

  it("an unresolvable ref yields no version at all (exit 5)", () => {
    const r = runScript("refs/tags/definitely-not-a-real-tag", ROOT);
    expect(r.status).toBe(5);
    expect(r.stdout.trim()).toBe("");
  });
});

// --- Executed against real git: the property the whole ticket is about ---------------------------
//
// A purpose-built repository with two commits at controlled committer dates, rather than this
// checkout's own history: CI's frontend job checks out shallow, so `HEAD~1` is not reliably present,
// and a test that silently skips there would guard nothing in the place it matters most.
describe("an older ref always yields a smaller version, however late you ask (CPE-1941)", () => {
  let tmp = "";
  let ok = false;
  const older = 1_787_100_000; // both above the floor, so the floor check is not what is under test
  const newer = 1_787_200_000;

  function git(args: string[], dateEpoch?: number) {
    const stamp = dateEpoch === undefined ? {} : {
      GIT_AUTHOR_DATE: `${dateEpoch} +0000`,
      GIT_COMMITTER_DATE: `${dateEpoch} +0000`,
    };
    // `-c` overrides so a developer's global config (commit signing, a default branch name) cannot
    // make this fixture fail for reasons that have nothing to do with what it is testing.
    return spawnSync("git", ["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main", ...args], {
      cwd: tmp,
      encoding: "utf8",
      env: {
        ...process.env,
        ...stamp,
        GIT_AUTHOR_NAME: "cpe",
        GIT_AUTHOR_EMAIL: "cpe@example.invalid",
        GIT_COMMITTER_NAME: "cpe",
        GIT_COMMITTER_EMAIL: "cpe@example.invalid",
      },
    });
  }

  beforeAll(() => {
    requireBash();
    // Kept inside the repo (and inside the gitignored worktrees dir) per house rule; removed after.
    // `.claude/worktrees/` is gitignored, so it does NOT exist in a fresh CI checkout — create it,
    // or `mkdtempSync` throws ENOENT there and this whole describe fails on the one machine that
    // matters most.
    const holder = join(ROOT, ".claude", "worktrees");
    mkdirSync(holder, { recursive: true });
    tmp = mkdtempSync(join(holder, "cpe1941-"));
    if (git(["init", "-q", "-b", "main"]).status !== 0) return;
    writeFileSync(join(tmp, "a.txt"), "old release\n");
    git(["add", "a.txt"]);
    if (git(["commit", "-q", "-m", "old tag"], older).status !== 0) return;
    if (git(["tag", "v-old"]).status !== 0) return;
    writeFileSync(join(tmp, "a.txt"), "new release\n");
    git(["add", "a.txt"]);
    if (git(["commit", "-q", "-m", "new tag"], newer).status !== 0) return;
    if (git(["tag", "v-new"]).status !== 0) return;
    ok = true;
  });

  afterAll(() => {
    // Best-effort: a read-only pack file can make this throw on Windows, and failing the suite over
    // cleanup of a gitignored scratch dir would be a false red.
    try {
      if (tmp) rmSync(tmp, { recursive: true, force: true });
    } catch {
      /* leftover scratch dir under the gitignored .claude/worktrees/ */
    }
  });

  function versionFor(ref: string, now = newer + 3600) {
    return spawnSync("bash", [SCRIPT, ref, tmp, String(now)], { encoding: "utf8" });
  }

  it("the two tags produce their own commits' timestamps", () => {
    expect(ok, "fixture repo must have been created").toBe(true);
    expect(versionFor("v-old").stdout.trim()).toBe(String(older));
    expect(versionFor("v-new").stdout.trim()).toBe(String(newer));
  });

  it("re-running the OLD tag much later reproduces the old number -- it never advances", () => {
    expect(ok).toBe(true);
    // The re-run, a year on. Under the old scheme this is where `date +%s` handed the stale bundle a
    // number bigger than everything installed; the whole fix is that this value does not move.
    const muchLater = newer + 365 * 86_400;
    const rerun = versionFor("v-old", muchLater).stdout.trim();
    expect(rerun).toBe(String(older));
    expect(Number(rerun)).toBeLessThan(newer); // strictly older => anti-rollback refuses it
  });

  it("the number is a property of the ref, not of when it is asked for", () => {
    expect(ok).toBe(true);
    const a = versionFor("v-old", newer + 10).stdout.trim();
    const b = versionFor("v-old", newer + 10_000_000).stdout.trim();
    expect(a).toBe(b);
    expect(a).not.toBe("");
  });
});
