// CPE-1951 — the publish side of "a release cut from an OLDER commit publishes a fully green
// catalog that every client silently refuses".
//
// CPE-1941 made each catalog entry's `version` the tagged commit's committer timestamp. Right
// number, and it made the version track **commit order, not release order**: tag a commit older
// than the last released one (a hotfix off a maintenance branch, a revert branch, `git tag` on a
// non-tip commit) and the number goes DOWN while the release job stays entirely green —
// `CATALOG_VERSION_FLOOR` is a *static* constant such a version clears by a mile, the future-date
// check passes, the signatures verify, the upload succeeds. Every client then answers
// `ApplyOutcome::Rollback`, writes nothing, and logs nothing.
//
// The client-and-disk half of that story is `sidecar/host/tests/catalog_offtip_release_lower_bound.rs`
// (it asserts on `ApplyOutcome` and on the manifest bytes, never on a verdict enum alone). THIS file
// is the publish half, in three layers, because each on its own has a known way to rot:
//
//   1. Structural, via `parseYaml`: `release.yml`'s `catalog` job really does run the guard, really
//      does feed it the derived version, really does run it BEFORE signing, and really does account
//      for its outcome. A regex over the raw file would be satisfied by a comment — and both this
//      workflow and the guard script are full of comments quoting the very strings a regex would
//      look for, so that failure mode is not hypothetical.
//   2. Derived, not claimed (CLAUDE.md): the URL the guard fetches is read out of `catalog_url()` in
//      `src-tauri/src/lib.rs` at run time, comments stripped, and compared against what the shipped
//      shell function actually prints. Change either side and this reds.
//   3. Executed: the real script is run against a real git fixture (three commits at controlled
//      committer dates) and stubbed `gh`/`curl`, once per failure mode, asserting the exit code AND
//      the distinct message. A guard whose failure paths were never executed is a provenance claim.
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  chmodSync,
  existsSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { parseYaml } from "./preview/yaml";
import { logicalLines } from "./shellScriptLines";
import { stripRustComments, rustStringLiteralAfter } from "./rustSource";

const ROOT = resolve(__dirname, "..", "..");
const SCRIPT_REL = ".github/workflows/scripts/catalog-lower-bound.sh";
/** Forward slashes throughout: this path is handed to bash (as argv and, in one case, inside a
 *  `bash -c` string), and a Windows `Z:\…` spelling is a backslash-escape minefield there. */
const SCRIPT = join(ROOT, ...SCRIPT_REL.split("/")).replace(/\\/g, "/");
const VERSION_SCRIPT = join(ROOT, ".github", "workflows", "scripts", "catalog-version.sh").replace(
  /\\/g,
  "/",
);
const RUST_SIBLING = join(ROOT, "sidecar", "host", "tests", "catalog_offtip_release_lower_bound.rs");

interface WorkflowStep {
  id?: string;
  name?: string;
  if?: string;
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

/** The index of the step whose (comment-stripped) body mentions `needle`, or -1. Comment-stripped
 *  via `logicalLines` — the shared shell splitter — because a *trailing* comment walks straight
 *  through a whole-line filter, which is how CPE-1933's first draft reintroduced the hole it was
 *  closing. */
function stepIndexRunning(needle: string): number {
  return catalogSteps().findIndex((s) =>
    logicalLines(s.run).some((l) => l.includes(needle)),
  );
}

// ── 1. Structural: the workflow really runs it, before signing, and cannot fail softly ──────────
//
// RED-PROOFED 2026-08-28, both directions of CLAUDE.md's "anchor on code, never on prose":
//   * replacing the step's invocation with `echo "lower bound skipped"` -> 5 of these 8 red.
//   * hiding the invocation in a TRAILING comment on that echo
//     (`echo … # bash .github/workflows/scripts/catalog-lower-bound.sh "$VERSION" "$REPO"`)
//     -> the same 5 stay red. `logicalLines` strips it; a whole-line-comment filter would not have.

describe("release.yml refuses a catalog version that is not newer than the published one (CPE-1951)", () => {
  it("the guard script exists and defines the two functions the workflow depends on", () => {
    expect(existsSync(SCRIPT)).toBe(true);
    const text = readFileSync(SCRIPT, "utf8");
    expect(text).toContain("catalog_published_lower_bound()");
    expect(text).toContain("catalog_lower_bound_check()");
  });

  it("exactly one step of the catalog job invokes it", () => {
    const steps = catalogSteps();
    const hits = steps.filter((s) => logicalLines(s.run).some((l) => l.includes(SCRIPT_REL)));
    expect(hits.length, `exactly one step must run ${SCRIPT_REL}`).toBe(1);
    expect(hits[0].id, "the step needs an id so the outcome step can account for it").toBeTruthy();
  });

  it("it is fed the derived version, bound to the derive step BY ID rather than recomputed", () => {
    const steps = catalogSteps();
    const derive = steps.find((s) =>
      logicalLines(s.run).some((l) => l.includes("catalog-version.sh")),
    );
    const guard = steps.find((s) => logicalLines(s.run).some((l) => l.includes(SCRIPT_REL)));
    expect(derive?.id, "the derive step must still have an id").toBeTruthy();
    // Renaming the derive step's id without repointing this one fails here, rather than silently
    // comparing an empty version against the published one.
    expect(guard?.env?.VERSION).toBe("${{ steps." + derive?.id + ".outputs.version }}");
    expect(guard?.env?.REPO).toBe("${{ github.repository }}");
    // No step may recompute the number it is about to check.
    expect(guard?.run).toContain('test -n "$VERSION"');
  });

  it("it runs BEFORE the bundle is signed and uploaded, so a refusal publishes nothing", () => {
    const guard = stepIndexRunning(SCRIPT_REL);
    const sign = stepIndexRunning("--bin catalog-sign");
    const upload = stepIndexRunning("gh release upload");
    expect(guard).toBeGreaterThanOrEqual(0);
    expect(sign).toBeGreaterThan(guard);
    expect(upload).toBeGreaterThan(guard);
  });

  it("it cannot fail softly", () => {
    const guard = catalogSteps().find((s) =>
      logicalLines(s.run).some((l) => l.includes(SCRIPT_REL)),
    );
    expect(guard?.["continue-on-error"], "the gate must stay fatal").toBeUndefined();
    expect(guard?.run).toContain("set -euo pipefail");
    // A hang on the release path reads as a stuck release, not a refusal.
    expect(guard?.["timeout-minutes"]).toBeTruthy();
  });

  it("the 'Catalog publish outcome' step accounts for the gate by its step id", () => {
    const steps = catalogSteps();
    const guardId = steps.find((s) => logicalLines(s.run).some((l) => l.includes(SCRIPT_REL)))?.id;
    const outcome = steps.find((s) => s.name === "Catalog publish outcome");
    expect(outcome, "the honesty step must still exist").toBeTruthy();
    const bound = Object.entries(outcome?.env ?? {}).find(
      ([, v]) => v === "${{ steps." + guardId + ".outcome }}",
    );
    expect(bound, `no env var in the outcome step reads steps.${guardId}.outcome`).toBeTruthy();
    const varName = bound![0];
    expect(logicalLines(outcome?.run).join("\n")).toContain(`\${${varName}:-}`);
  });

  it("CATALOG_VERSION_FLOOR is KEPT — the static floor and the monotonic bound are both wanted", () => {
    // They answer different questions and neither implies the other: the floor is about what the
    // installed base already holds (no fetch can see that — a client may sit on an old catalog for
    // months), the bound is about what is published right now. Deleting the floor "because the new
    // check subsumes it" is the one refactor this ticket must not license.
    expect(readFileSync(VERSION_SCRIPT, "utf8")).toMatch(/^CATALOG_VERSION_FLOOR=\d+/m);
    expect(stepIndexRunning("catalog-version.sh")).toBeGreaterThanOrEqual(0);
  });

  it("the client-side sibling test still exists and still names the cases this file leans on", () => {
    // Not a provenance claim about behaviour — a pointer, kept honest. This file proves the publish
    // side; the Rust file proves the client outcome and the on-disk state. A rename that orphans
    // half the demonstration reds here.
    expect(existsSync(RUST_SIBLING)).toBe(true);
    const rust = readFileSync(RUST_SIBLING, "utf8");
    for (const fn of [
      "a_release_cut_from_an_older_commit_is_refused_by_every_client_and_changes_nothing_on_disk",
      "the_publish_side_is_entirely_green_for_the_off_tip_release",
      "the_clients_acceptance_boundary_is_strictly_greater_than_the_installed_version",
    ]) {
      expect(rust, `${fn} must still exist in the Rust sibling`).toContain(`fn ${fn}(`);
    }
  });
});

// ── 2. Derived: the URL fetched is the one clients fetch ────────────────────────────────────────

/** `catalog_url()`'s value, read out of `src-tauri/src/lib.rs` at run time with comments stripped
 *  first — never copied here. Anchoring on the *code* is what stops one of that file's many comments
 *  quoting an old URL from satisfying the scan (CLAUDE.md → "Anchor on code, never on prose"). */
function clientCatalogIndexUrlFromRust(): { repo: string; url: string } {
  const src = stripRustComments(readFileSync(join(ROOT, "src-tauri", "src", "lib.rs"), "utf8"));

  const repoAnchor = "const CATALOG_REPO: &str =";
  const repoAt = src.indexOf(repoAnchor);
  expect(repoAt, "src-tauri/src/lib.rs must still declare CATALOG_REPO").toBeGreaterThanOrEqual(0);
  const repo = rustStringLiteralAfter(src, repoAt + repoAnchor.length);

  const fnAnchor = "fn catalog_url()";
  const fnAt = src.indexOf(fnAnchor);
  expect(fnAt, "src-tauri/src/lib.rs must still declare catalog_url()").toBeGreaterThanOrEqual(0);
  // The first literal after the `format!(` inside that fn is the default base. The `unwrap_or_else`
  // above it carries no string literal, so this lands on the URL template.
  const fmtAt = src.indexOf("format!(", fnAt);
  expect(fmtAt, "catalog_url() must still build its default with format!").toBeGreaterThan(fnAt);
  const template = rustStringLiteralAfter(src, fmtAt);

  expect(template, "catalog_url()'s template must interpolate CATALOG_REPO").toContain(
    "{CATALOG_REPO}",
  );
  return { repo, url: `${template.replace("{CATALOG_REPO}", repo)}catalog-index.json` };
}

// ── Execution harness ───────────────────────────────────────────────────────────────────────────

/** bash is REQUIRED, not probed-and-skipped, for the same reason `catalogPublishVersion.test.ts`
 *  requires it: the thing under test IS a shell script, and a silently-green run on a bash-less
 *  machine is the exact "nothing happened, so nothing is wrong" shape this ticket is about. Every
 *  environment that can check this repo out has bash (it ships with Git for Windows; CI's frontend
 *  job is ubuntu-latest). */
function requireBash(): void {
  const probe = spawnSync("bash", ["--version"], { stdio: "ignore" });
  if (probe.error || probe.status !== 0) {
    throw new Error(
      `bash is required to execute ${SCRIPT_REL} — these tests run the real script rather than ` +
        "asserting about it, so a missing bash is a broken environment, not a reason to pass.",
    );
  }
}

/** `jq` is a different case and IS skipped where absent. The script needs it, ubuntu-latest (where
 *  it actually runs, and where CI's frontend job runs these tests) ships it, and Git for Windows
 *  does not. `it.skipIf` marks these SKIPPED in the reporter — visibly not-run — rather than green,
 *  which is the distinction `catalogPublishLoudFailure.test.ts` settled on for the same tools. */
function toolAvailable(tool: string): boolean {
  const probe = spawnSync("bash", ["-c", `command -v ${tool}`], { stdio: "ignore" });
  return !probe.error && probe.status === 0;
}

const hasBash = (() => {
  const probe = spawnSync("bash", ["--version"], { stdio: "ignore" });
  return !probe.error && probe.status === 0;
})();
const hasJq = hasBash && toolAvailable("jq");

/**
 * Every test gated on `jq`, counted at COLLECTION time so `afterAll` can name the exact number that
 * was skipped instead of a hand-kept figure that rots. Loop-generated tests increment once per
 * iteration, which is what a reader needs to know. See the "reporting the skip" block below.
 */
let jqGatedCount = 0;
function itJq(name: string, fn: () => void) {
  jqGatedCount += 1;
  return it.skipIf(!hasJq)(name, fn);
}

/** Kept inside the repo's gitignored `.claude/worktrees/` per house rule, created because a fresh CI
 *  checkout does not have it. Removed in afterAll. */
function scratch(prefix: string): string {
  const holder = join(ROOT, ".claude", "worktrees");
  mkdirSync(holder, { recursive: true });
  return mkdtempSync(join(holder, prefix));
}

/** A gh stub answering `gh api repos/<repo>/releases/latest`, and a curl stub answering the index
 *  URL, both driven by env vars so one PATH can serve every scenario. Extensionless bash scripts
 *  placed FIRST on PATH — `stubsWin()` below confirms they actually win, so a stubbed test can never
 *  quietly hit the real `gh.exe`/`curl.exe` further along PATH. */
const GH_STUB = `#!/usr/bin/env bash
case "\${GH_MODE:-ok}" in
  transport) echo "gh: error connecting to api.github.com" >&2; exit 1 ;;
  garbage) echo "not json at all"; exit 0 ;;
  no_assets_array) echo '{"tag_name":"v9.9.9"}'; exit 0 ;;
  no_index) echo '{"tag_name":"v0.57.69-sidecar","assets":[{"name":"app.msi"},{"name":"latest.json"}]}'; exit 0 ;;
  probe) echo "STUB-GH"; exit 0 ;;
  # Answer with an arbitrary releases-API body, verbatim. Added in #1091 round 2 for the
  # workflow-command-injection cases, which need control of \`tag_name\` itself.
  raw) printf '%s\\n' "\${GH_BODY:-}"; exit 0 ;;
  fail) printf 'gh: the remote said\\n::error::FORGED-VIA-GH-STDERR\\n' >&2; exit 1 ;;
  *) echo '{"tag_name":"v0.57.33","assets":[{"name":"app.msi"},{"name":"catalog-index.json"},{"name":"catalog-index.json.sig"}]}'; exit 0 ;;
esac
`;

const CURL_STUB = `#!/usr/bin/env bash
out=""; prev=""
for a in "\$@"; do
  if [ "\$prev" = "-o" ]; then out="\$a"; fi
  prev="\$a"
done
echo "\$@" >> "\${CURL_ARGV_LOG:-/dev/null}"
case "\${CURL_MODE:-ok}" in
  timeout) echo "curl: (28) Operation timed out after 60000 milliseconds" >&2; printf '000'; exit 28 ;;
  unreachable) echo "curl: (6) Could not resolve host: github.com" >&2; printf '000'; exit 6 ;;
  partial) echo "curl: (18) transfer closed with outstanding read data remaining" >&2; printf '000'; exit 18 ;;
  other) echo "curl: (52) Empty reply from server" >&2; printf '000'; exit 52 ;;
  http404) : > "\$out"; printf '404'; exit 0 ;;
  http500) : > "\$out"; printf '500'; exit 0 ;;
  http418) : > "\$out"; printf '418'; exit 0 ;;
  empty) : > "\$out"; printf '200'; exit 0 ;;
  truncated) printf '{"entries":[{"id":"claude","version":178' > "\$out"; printf '200'; exit 0 ;;
  no_version) printf '{"entries":[]}' > "\$out"; printf '200'; exit 0 ;;
  # Serve an arbitrary index body verbatim. Added in #1091 round 2 so a case can be written as the
  # JSON a hostile or broken publisher would actually serve, rather than as a new stub mode each
  # time — both fail-open bugs that round found were shapes no existing mode could express.
  raw) printf '%s' "\${IDX_BODY:-}" > "\$out"; printf '200'; exit 0 ;;
  *) printf '{"entries":[{"id":"claude","version":%s}]}' "\${PUBLISHED:-1787200000}" > "\$out"; printf '200'; exit 0 ;;
esac
`;

let stubBin = "";
let stubBinPosix = "";
let scratchRoot = "";

/**
 * The path as the shell sees it. Load-bearing on Windows, and it cost an hour to find:
 *
 *  * Prepending the stub dir to `PATH` in node's `env` does NOT win, because MSYS2's bash puts
 *    `/mingw64/bin` and `/usr/bin` in front of the inherited PATH at startup — and `curl.exe` lives
 *    in `/mingw64/bin`. Measured: the "stubbed" run fetched the REAL github.com and every failure
 *    case came back as a genuine 404, all ten of them agreeing for the wrong reason.
 *  * Prepending it INSIDE bash with the `Z:/…` spelling is worse: a Windows path contains a colon,
 *    which is the PATH separator, so `PATH="Z:/x:$PATH"` silently adds two junk entries.
 *
 * So: convert to the POSIX form with `cygpath` where it exists, and prepend inside the shell. On
 * Linux/macOS `cygpath` is absent and the path is already POSIX, so this is a no-op there.
 *
 * WHICH MECHANISM WAS MEASURED WHERE (#1091 round 2). Two bashes on one Windows box do not agree
 * here, so the reader on a third should not conclude either note is wrong:
 *   * the `/mingw64/bin` re-prepend was measured under **MSYS2's** bash (the Git-for-Windows one),
 *     where an `env.PATH` prepend handed to a spawned bash genuinely lost to the real `curl.exe`.
 *   * a reviewer on **cygwin's** `/usr/bin/bash` could NOT reproduce that half — there the
 *     `env.PATH` prepend wins — and reproduced the colon-split half instead.
 * Both hazards are real on some shell here, the fix covers both, and the `beforeAll` probe below
 * is what actually decides: it refuses to run rather than trusting either analysis.
 */
function posixPath(p: string): string {
  const fwd = p.replace(/\\/g, "/");
  const r = spawnSync("cygpath", ["-u", fwd], { encoding: "utf8" });
  if (!r.error && r.status === 0 && r.stdout.trim()) return r.stdout.trim();
  return fwd;
}

interface Run {
  status: number | null;
  stdout: string;
  stderr: string;
  all: string;
}

/** Run the real guard script with the stubs genuinely first on PATH (see `posixPath`). */
function runGuard(args: string[], env: Record<string, string> = {}): Run {
  const r = spawnSync(
    "bash",
    ["-c", `PATH="${stubBinPosix}:$PATH"; export PATH; exec bash "$0" "$@"`, SCRIPT, ...args],
    { encoding: "utf8", cwd: ROOT, env: { ...process.env, ...env } },
  );
  const stdout = r.stdout ?? "";
  const stderr = r.stderr ?? "";
  return { status: r.status, stdout, stderr, all: `${stdout}\n${stderr}` };
}

beforeAll(() => {
  requireBash();
  scratchRoot = scratch("cpe1951-");
  stubBin = join(scratchRoot, "stub-bin");
  mkdirSync(stubBin, { recursive: true });
  for (const [name, body] of [
    ["gh", GH_STUB],
    ["curl", CURL_STUB],
  ] as const) {
    const file = join(stubBin, name);
    writeFileSync(file, body, "utf8");
    chmodSync(file, 0o755);
  }
  stubBinPosix = posixPath(stubBin);

  // Both stubs must actually WIN. A stub that loses does not fail — it quietly runs the real tool
  // against the real network and the test passes for the wrong reason, which is how this file spent
  // an hour reporting ten identical genuine 404s as ten distinct guard verdicts. Checked for BOTH
  // tools, because checking only `gh` is exactly what missed it: `gh` won and `curl` did not.
  for (const tool of ["gh", "curl"]) {
    const probe = spawnSync(
      "bash",
      ["-c", `PATH="${stubBinPosix}:$PATH"; export PATH; command -v ${tool}`],
      { encoding: "utf8" },
    );
    const resolved = (probe.stdout ?? "").trim();
    if (!resolved.startsWith(stubBinPosix)) {
      throw new Error(
        `the ${tool} stub does not win on PATH (resolved to "${resolved}", wanted something under ` +
          `"${stubBinPosix}"). Refusing to run: these tests would silently exercise the real ${tool} ` +
          "against the live network and pass for the wrong reason.",
      );
    }
  }
});

afterAll(() => {
  try {
    if (scratchRoot) rmSync(scratchRoot, { recursive: true, force: true });
  } catch {
    /* gitignored scratch under .claude/worktrees/ */
  }
  // See `jqGatedCount`. `it.skipIf` is honest to a reporter and silent to a human reading a total.
  if (!hasJq && jqGatedCount > 0) {
    // eslint-disable-next-line no-console
    console.warn(
      `\n[catalogPublishLowerBound] jq was NOT found on this machine, so ${jqGatedCount} of this ` +
        `file's tests were SKIPPED, not passed — and they are the entire EXECUTED half: every ` +
        `failure path, both directions, and the whole content-comparison block. What ran was the ` +
        `structural and derived-from-source legs only. Do not quote this file's total as a pass ` +
        `count. Install jq (CI's ubuntu-latest ships it) to run the full set.\n`,
    );
  }
});

// ── 2b. Reporting the skip, not hiding it (CPE-1951, #1091 round 2 finding NEW 1) ───────────────
//
// On a machine without `jq` this file gave 11 passed / 22 skipped. `it.skipIf` is honest to the
// REPORTER — they show as skipped — but a human quoting "33 tests" from a Windows box is quoting 11
// and sounding like 33, which is what happened in round 1's report. Two changes, and deliberately
// NOT removing the skip (a developer without jq should still get the structural half):
//   * `afterAll` prints a loud line naming the exact number gated and why (counted at collection
//     time by `itJq`, so it cannot drift from the real number).
//   * the test below FAILS, rather than skipping, when jq is missing **and** `CI` is set — so CI
//     can never silently run the reduced set and report it as this file passing.

describe("this file cannot report a reduced run as a full one (CPE-1951)", () => {
  it("in CI, jq must be present — the jq-gated tests ARE the executed half", () => {
    if (!process.env.CI) {
      // Locally jq is optional; afterAll says exactly what was skipped.
      expect(hasJq || !process.env.CI).toBe(true);
      return;
    }
    expect(
      hasJq,
      "CI is set but `jq` is not on PATH. Every executed leg of this guard would be SKIPPED and " +
        "the file would report as passing. ubuntu-latest ships jq; if this runner does not, " +
        "install it in the job rather than letting the run be silently partial.",
    ).toBe(true);
  });
});

describe("the guard fetches exactly what a client fetches (CPE-1951)", () => {
  it("the URL the shipped script prints is catalog_url() + catalog-index.json, read from lib.rs", () => {
    const { repo, url } = clientCatalogIndexUrlFromRust();
    const r = spawnSync("bash", ["-c", `source "${SCRIPT}"; catalog_lower_bound_url "${repo}"`], {
      encoding: "utf8",
    });
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout.trim()).toBe(url);
    // Red-proofed 2026-08-28 by editing catalog_url()'s template in src-tauri/src/lib.rs to
    // `releases/download/` — this test failed with the two URLs printed side by side. It re-reads
    // the Rust source on every run; it is not a comment asserting the two agree.
    expect(url).toContain("/releases/latest/download/");
  });

  it("the stubs really answer, for BOTH tools", () => {
    // `beforeAll` already refuses to run if either stub loses on PATH; this is the positive half —
    // the stub is reached AND its output is what the guard reads.
    const gh = spawnSync(
      "bash",
      ["-c", `PATH="${stubBinPosix}:$PATH"; export PATH; gh whatever`],
      { encoding: "utf8", env: { ...process.env, GH_MODE: "probe" } },
    );
    expect(gh.stdout).toContain("STUB-GH");
    const curl = spawnSync(
      "bash",
      ["-c", `PATH="${stubBinPosix}:$PATH"; export PATH; curl -o /dev/null https://example.invalid`],
      { encoding: "utf8", env: { ...process.env, CURL_MODE: "http418" } },
    );
    expect(curl.stdout.trim()).toBe("418");
  });
});

// ── 3. Executed: both directions, and every fail-closed path ────────────────────────────────────

describe("both directions, executed (CPE-1951)", () => {
  const PUBLISHED = "1787200000";

  itJq("a version BELOW the published one is refused (exit 3) and says why", () => {
    const r = runGuard(["1787150000", "owner/repo"], { PUBLISHED });
    expect(r.status).toBe(3);
    expect(r.all).toContain("::error::");
    expect(r.all).toContain("NOT NEWER");
    expect(r.all).toContain("ApplyOutcome::Rollback");
    expect(r.all).toContain(PUBLISHED);
  });

  itJq("a version EQUAL to the published one is refused too", () => {
    // `>=` would let a release publish that reaches no client (AlreadyCurrent). Strictly greater is
    // the boundary the engine actually uses — measured in the Rust sibling.
    const r = runGuard([PUBLISHED, "owner/repo"], { PUBLISHED });
    expect(r.status).toBe(3);
    expect(r.all).toContain("NOT NEWER");
  });

  itJq("a legitimately NEWER version is accepted (exit 0)", () => {
    const r = runGuard(["1787300000", "owner/repo"], { PUBLISHED });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("strictly newer");
    expect(r.all).not.toContain("::error::");
  });

  itJq("a non-integer candidate is refused before anything is fetched (exit 2)", () => {
    const r = runGuard(["not-a-number", "owner/repo"]);
    expect(r.status).toBe(2);
  });
});

// ── 4. The 404 / draft distinction, which is the whole of the design decision ───────────────────

describe("the two different 404s are told apart (CPE-1951)", () => {
  itJq(
    "latest release EXISTS but carries no catalog-index.json -> accepted, loudly, with no lower bound",
    () => {
      // This is the state of the world TODAY (CPE-1953 / issue #1062): /releases/latest/ resolves to
      // v0.57.69-sidecar, only the plain channel runs the catalog job, so the live index URL 404s.
      // Accepted ONLY because the release was found and its assets were enumerated.
      const r = runGuard(["1787300000", "owner/repo"], { GH_MODE: "no_index" });
      expect(r.status).toBe(0);
      expect(r.all).toContain("::warning::");
      expect(r.all).toContain("carries NO catalog-index.json");
      expect(r.all).toContain("v0.57.69-sidecar");
      expect(r.stdout).toContain("no lower bound");
    },
  );

  itJq(
    "a 404 on the index URL while the asset list SAYS it is there is a contradiction, and fatal (exit 10)",
    () => {
      const r = runGuard(["1787300000", "owner/repo"], { CURL_MODE: "http404" });
      expect(r.status).toBe(10);
      expect(r.all).toContain("contradiction");
      // On STDOUT specifically: stdout is the acceptance channel (the bound, or `none`). The
      // refusal message on stderr quotes the phrase "no lower bound" while rejecting it, so
      // asserting over the combined streams would test the wording rather than the verdict.
      expect(r.stdout.trim()).toBe("");
    },
  );

  itJq(
    "a releases-API failure is NOT read as 'nothing is published' — it is fatal (exit 4)",
    () => {
      // The defect class CLAUDE.md names: a wrapper that cannot tell "ran and found nothing" from
      // "did not run". `gh` failing tells us we do not know, which is not the same as knowing there
      // is nothing.
      const r = runGuard(["1787300000", "owner/repo"], { GH_MODE: "transport" });
      expect(r.status).toBe(4);
      expect(r.all).toContain("it is evidence that we do not know");
    },
  );

  itJq("an unreadable releases-API payload is fatal, not a pass (exit 5)", () => {
    for (const mode of ["garbage", "no_assets_array"]) {
      const r = runGuard(["1787300000", "owner/repo"], { GH_MODE: mode });
      expect(r.status, `GH_MODE=${mode}`).toBe(5);
    }
  });

  itJq("a missing tool is 'did not run', and is refused (exit 16)", () => {
    // PATH stripped down to the stub dir only, so `jq` cannot be found. The outer bash keeps the
    // real PATH (node has to be able to spawn it at all, and on Windows bash lives well outside the
    // stub dir); it resolves its own absolute path first and re-execs itself under the stripped one.
    const r = spawnSync(
      "bash",
      ["-c", `B=$(command -v bash); PATH="${stubBinPosix}" exec "$B" "$0" "$@"`, SCRIPT, "1787300000", "owner/repo"],
      { encoding: "utf8" },
    );
    expect(r.status).toBe(16);
    expect(`${r.stdout}${r.stderr}`).toContain("did not run");
  });
});

// ── 5. Red-proofed fetch failures: four causes, four distinct messages ──────────────────────────

describe("every fetch failure is fatal with its own message (CPE-1951)", () => {
  // The acceptance criterion is that a 404, a truncated body, a 500 and a timeout each fail the
  // build with a DISTINCT message. Distinctness is asserted as a property of the whole set rather
  // than by eyeballing four strings, so two of them collapsing into one wording reds here.
  const CASES: Array<{ label: string; env: Record<string, string>; exit: number; says: string }> = [
    { label: "timeout", env: { CURL_MODE: "timeout" }, exit: 6, says: "TIMED OUT" },
    { label: "unreachable", env: { CURL_MODE: "unreachable" }, exit: 7, says: "could NOT REACH" },
    { label: "truncated in transit", env: { CURL_MODE: "partial" }, exit: 8, says: "TRUNCATED in transit" },
    { label: "other transport error", env: { CURL_MODE: "other" }, exit: 9, says: "curl exit 52" },
    { label: "http 404", env: { CURL_MODE: "http404" }, exit: 10, says: "HTTP 404" },
    { label: "http 500", env: { CURL_MODE: "http500" }, exit: 11, says: "SERVER ERROR" },
    { label: "unexpected status", env: { CURL_MODE: "http418" }, exit: 12, says: "unexpected HTTP status 418" },
    { label: "empty body", env: { CURL_MODE: "empty" }, exit: 13, says: "EMPTY body" },
    { label: "unparseable body", env: { CURL_MODE: "truncated" }, exit: 14, says: "NOT PARSEABLE JSON" },
    { label: "no usable version", env: { CURL_MODE: "no_version" }, exit: 15, says: "no usable entries[].version at all" },
    // #1091 round 2. A bound above the u64 a CatalogEntry.version is gets its OWN code rather than
    // sharing 15, because "no numeric version anywhere" and "the biggest one will not fit" are
    // different facts about the index — and because this table asserts one code per cause.
    {
      label: "bound out of u64 range",
      env: { CURL_MODE: "raw", IDX_BODY: '{"entries":[{"version":18446744073709551616}]}' },
      exit: 17,
      says: "outside the range a CatalogEntry.version",
    },
  ];

  for (const c of CASES) {
    itJq(`${c.label} -> fatal, exit ${c.exit}, and never a pass`, () => {
      const r = runGuard(["1787300000", "owner/repo"], c.env);
      expect(r.status, `${c.label} must be fatal`).toBe(c.exit);
      expect(r.all).toContain(c.says);
      // The thing that must never happen: a failed fetch reading as "nothing is published".
      expect(r.stdout).not.toContain("strictly newer");
      expect(r.stdout).not.toContain("no lower bound");
    });
  }

  itJq("no two failure causes share an exit code or a message", () => {
    const codes = new Set(CASES.map((c) => c.exit));
    expect(codes.size, "each cause needs its own exit code").toBe(CASES.length);
    const firstLines = CASES.map((c) => {
      const r = runGuard(["1787300000", "owner/repo"], c.env);
      return r.stderr.trim().split("\n")[0];
    });
    expect(new Set(firstLines).size, `messages collapsed: ${firstLines.join(" | ")}`).toBe(
      CASES.length,
    );
  });
});

// ── 5b. The comparison itself cannot fail OPEN (CPE-1951, #1091 round 2) ────────────────────────
//
// Two reviewers found the same class independently: a comparison that ERRORS is not a comparison
// that is FALSE, and `[`/`jq` both answer "not true" for both. Round 1 shipped two instances, each
// reaching **exit 0 while printing "strictly newer"**. These are NOT rows in `CASES` above: that
// table is fetch failures, and asserts one distinct exit code per cause. These are content
// failures, and several of them legitimately share exit 3 — the guard's *correct* verdict.
//
// Each case below carries the round-1 behaviour it replaces, so reverting the fix reds it.
// RED-PROOFED 2026-08-28 by putting each round-1 line back in the shipped script and re-running
// this file (58 tests, all executed — jq present):
//   * `catalog_lb_num_le "$candidate" "$bound"` -> `[ "$candidate" -le "$bound" ]`
//       3 failed / 55 passed. Named: "a bound of exactly 2^63", "a bound of 2^64-1", and the
//       set-property test "no path anywhere in this file leaves bash's `integer expected` in the
//       log". The 2^63-1 row stayed green, which is the point — it is the neighbour that was fine.
//   * the jq extraction -> `[.entries[]?.version] | max` (no `numbers | select(…)`)
//       5 failed / 53 passed: "one string-typed version…", "an object version…", "a float…",
//       "every version is a string…", "a negative version…".
// A third sabotage covers the log sanitiser and is recorded in the 5c block below.

describe("the comparison cannot fail open on a value it cannot represent (CPE-1951)", () => {
  interface ContentCase {
    label: string;
    /** The index body the server serves, verbatim. */
    index?: string;
    candidate?: string;
    exit: number;
    says?: string;
    /** What round 1 did with this exact input, for the reverting reader. */
    wasRound1: string;
  }

  const PUBLISHED_LOW = '{"entries":[{"id":"claude","version":1787200000}]}';

  const CONTENT: ContentCase[] = [
    {
      // The Security Auditor's first reproduction, verbatim.
      label: "a bound of exactly 2^63 (the first value `[ -le ]` cannot parse)",
      index: '{"entries":[{"version":9223372036854775808}]}',
      exit: 3,
      says: "NOT NEWER",
      wasRound1: "`[: 9223372036854775808: integer expected` on stderr, then exit 0, 'strictly newer'",
    },
    {
      label: "a bound of 2^63-1 — the control that always worked",
      index: '{"entries":[{"version":9223372036854775807}]}',
      exit: 3,
      says: "NOT NEWER",
      wasRound1: "exit 3 (this one was always correct; it is the neighbour that was not)",
    },
    {
      // `CatalogEntry.version` is a u64, so this is a LEGAL published version, not a hostile one.
      label: "a bound of 2^64-1 — the largest a CatalogEntry.version can legally hold",
      index: '{"entries":[{"version":18446744073709551615}]}',
      exit: 3,
      says: "NOT NEWER",
      wasRound1: "exit 0, 'strictly newer' — every value in [2^63, 2^64-1] read as 'we are newer'",
    },
    {
      label: "a bound of 2^64 — beyond the type, so a broken index rather than a big one",
      index: '{"entries":[{"version":18446744073709551616}]}',
      exit: 17,
      says: "outside the range a CatalogEntry.version",
      wasRound1: "exit 0, 'strictly newer'",
    },
    {
      // The Security Auditor's second reproduction, verbatim. jq's total ordering sorts numbers
      // BELOW strings, so `max` over a mixed array returns the string — ONE string-typed entry
      // defeated the check for the whole index.
      label: "one string-typed version alongside a much larger numeric one",
      index: '{"entries":[{"version":1787999999999},{"version":"1"}]}',
      exit: 3,
      says: "1787999999999",
      wasRound1: "bounded against 1 instead of 1787999999999, exit 0, 'strictly newer'",
    },
    {
      label: "a null version alongside a numeric one",
      index: '{"entries":[{"version":1787999999999},{"version":null}]}',
      exit: 3,
      says: "1787999999999",
      wasRound1: "exit 3 — null sorts BELOW numbers, so this one happened to be safe",
    },
    {
      label: "an object version alongside a numeric one",
      index: '{"entries":[{"version":1787999999999},{"version":{"a":1}}]}',
      exit: 3,
      says: "1787999999999",
      wasRound1: "exit 15 — the object won `max` and then failed the digits test",
    },
    {
      label: "a float alongside a numeric one — dropped, never rounded into the bound",
      index: '{"entries":[{"version":1787999999999},{"version":1787999999999.9}]}',
      exit: 3,
      says: "1787999999999",
      wasRound1: "exit 15 — the float won `max` and then failed the digits test",
    },
    {
      label: "every version is a string — nothing usable, and that is fatal, not 'no bound'",
      index: '{"entries":[{"version":"9999999999999"}]}',
      exit: 15,
      says: "no usable entries[].version at all",
      wasRound1: "exit 0, bounded against the string",
    },
    {
      label: "a negative version is discarded, not compared",
      index: '{"entries":[{"version":-5}]}',
      exit: 15,
      says: "no usable entries[].version at all",
      wasRound1: "exit 15 (via the digits test)",
    },
    {
      // 1e20 IS integral and non-negative, so it survives the `numbers` filter — and jq renders it
      // back as `1E+20`, which is neither a plain decimal nor inside u64. It lands on 17, not 15:
      // the fact is "the largest usable version will not fit", not "there were none".
      label: "an exponent-spelled version (jq renders it 1E+20) is refused, not compared",
      index: '{"entries":[{"version":1e20}]}',
      exit: 17,
      says: "outside the range a CatalogEntry.version",
      wasRound1: "exit 15 (via the digits test) — right direction, wrong reason",
    },
    {
      // MEASURED, and not what was assumed when this case was written: jq 1.7 preserves a number's
      // ORIGINAL literal spelling when it is not arithmetically modified, so `1e10` comes back as
      // `1E+10` even though 10^10 fits a u64 comfortably. So 17 is about the spelling as well as
      // the value, and the message says so ("…or not a plain decimal spelling of it").
      //
      // Refusing it is the right trade rather than a gap. Forcing jq to canonicalise (`. + 0`)
      // would round every version above 2^53 through a double — corrupting exactly the large u64s
      // this whole block exists to compare correctly — and our own publisher never emits an
      // exponent: `catalog-sign` serialises `version: u64` through serde as plain digits. An
      // exponent-spelled version in a published index means something other than this repo wrote
      // it, which is a broken index.
      label: "an exponent spelling is refused even when the VALUE fits (jq keeps the literal)",
      index: '{"entries":[{"version":1e10}]}',
      exit: 17,
      says: "not a plain decimal spelling",
      wasRound1: "exit 15 (via the digits test) — right direction, wrong reason",
    },
    {
      label: "a legitimate version of 0 still compares",
      index: '{"entries":[{"version":0}]}',
      candidate: "1787300000",
      exit: 0,
      says: "strictly newer",
      wasRound1: "exit 0 — unchanged; 0 is a plain non-negative integer and must stay usable",
    },
    {
      // The CANDIDATE operand overflows `[ -le ]` identically, and round 1 validated it with the
      // same digits-only regex. It is refused up front now rather than reaching the comparison.
      label: "a candidate above the u64 range is refused before anything is fetched",
      index: PUBLISHED_LOW,
      candidate: "18446744073709551616",
      exit: 2,
      says: "no greater than 18446744073709551615",
      wasRound1: "exit 0 via `[: integer expected` — the candidate side had the same hole",
    },
    {
      label: "a candidate of exactly 2^64-1 is legal and is accepted",
      index: PUBLISHED_LOW,
      candidate: "18446744073709551615",
      exit: 0,
      says: "strictly newer",
      wasRound1: "exit 0 by accident (`[: integer expected`), not by comparison",
    },
    {
      // `[ 010 -eq 8 ]` is FALSE, `[[ 010 -eq 8 ]]` is TRUE. A value whose meaning depends on which
      // comparison spelling someone picked is refused rather than silently octal.
      label: "a candidate with a leading zero is refused (its value differs between [ and [[ )",
      index: PUBLISHED_LOW,
      candidate: "010",
      exit: 2,
      says: "no leading zero",
      wasRound1: "accepted — the digits-only regex had no opinion on leading zeros",
    },
  ];

  for (const c of CONTENT) {
    itJq(`${c.label} -> exit ${c.exit}`, () => {
      const env: Record<string, string> = c.index
        ? { CURL_MODE: "raw", IDX_BODY: c.index }
        : { CURL_MODE: "raw", IDX_BODY: PUBLISHED_LOW };
      const r = runGuard([c.candidate ?? "1787200000", "owner/repo"], env);
      expect(r.status, `${c.label}\nround 1 did: ${c.wasRound1}\ngot:\n${r.all}`).toBe(c.exit);
      if (c.says) expect(r.all).toContain(c.says);
      if (c.exit !== 0) {
        // The failure this whole block exists for: a comparison that could not be made, printed as
        // a comparison that succeeded.
        expect(r.stdout).not.toContain("strictly newer");
        // `[: … integer expected` on stderr means the `test` builtin was handed a value it could
        // not parse — the exact tell for BLOCKER 1, and it must not appear on ANY path now.
        expect(r.stderr).not.toContain("integer expected");
      }
    });
  }

  itJq("no path anywhere in this file leaves bash's `integer expected` in the log", () => {
    // A property over the whole executed set rather than per case: `[ -le ]` erroring is the
    // signature of the bug, and it is asserted absent everywhere, not just where it was found.
    const seen: string[] = [];
    for (const c of CONTENT) {
      const r = runGuard([c.candidate ?? "1787200000", "owner/repo"], {
        CURL_MODE: "raw",
        IDX_BODY: c.index ?? PUBLISHED_LOW,
      });
      if (r.stderr.includes("integer expected")) seen.push(c.label);
    }
    expect(seen, `these inputs still reach a \`test\` builtin that cannot parse them`).toEqual([]);
  });

  itJq("EQUALITY gets the re-run advice, not 're-cut the tag' — the #1062 repair path", () => {
    // If a draft has been published and someone re-runs the catalog job to repair an upload,
    // `latest` IS that release, candidate == published, exit 3. Round 1's single message told them
    // to "re-cut the tag from a commit newer than the one already released", which is wrong advice
    // for exactly the path this repo needs right now.
    const equal = runGuard(["1787200000", "owner/repo"], {
      CURL_MODE: "raw",
      IDX_BODY: PUBLISHED_LOW,
    });
    expect(equal.status).toBe(3);
    expect(equal.all).toContain("RE-RUN");
    expect(equal.all).toContain("#1062");
    expect(equal.all).toContain("do NOT re-cut the tag");

    // …and the genuinely-off-tip case still gets the off-tip advice, so this did not just replace
    // one wrong message with another.
    const below = runGuard(["1787100000", "owner/repo"], {
      CURL_MODE: "raw",
      IDX_BODY: PUBLISHED_LOW,
    });
    expect(below.status).toBe(3);
    expect(below.all).toContain("Re-cut the tag from a commit newer");
    expect(below.all).not.toContain("RE-RUN");
  });

  it("the script's u64 ceiling is READ from CatalogEntry.version's declared Rust type", () => {
    // Derived, not claimed (CLAUDE.md). RED-PROOFED 2026-08-28 by editing catalog.rs to
    // `pub version: u32` and re-running: 1 failed, with
    //   "catalog.rs declares CatalogEntry.version as u32 (max 4294967295), but the guard caps at
    //    18446744073709551615".
    // It re-reads the Rust source on every run; it is not a comment asserting the two agree.
    const rust = stripRustComments(
      readFileSync(join(ROOT, "sidecar", "host", "src", "catalog.rs"), "utf8"),
    );
    const m = /pub\s+version\s*:\s*(u8|u16|u32|u64|u128|i8|i16|i32|i64|i128|usize)\s*,/.exec(rust);
    expect(m, "CatalogEntry.version's declaration was not found in catalog.rs").toBeTruthy();
    const rustType = (m as RegExpExecArray)[1];
    const bits = Number(rustType.replace(/^[ui]/, ""));
    expect(rustType.startsWith("u"), `version is ${rustType}; a signed version changes this guard`).toBe(true);
    const max = (2n ** BigInt(bits) - 1n).toString();
    const shell = readFileSync(SCRIPT, "utf8");
    const decl = /^CATALOG_LB_U64_MAX='(\d+)'$/m.exec(shell);
    expect(decl, "CATALOG_LB_U64_MAX must be a single plain literal in the guard script").toBeTruthy();
    expect(
      (decl as RegExpExecArray)[1],
      `catalog.rs declares CatalogEntry.version as ${rustType} (max ${max}), but the guard caps at ` +
        `${(decl as RegExpExecArray)[1]}`,
    ).toBe(max);
  });
});

// ── 5c. Remote bytes reaching the Actions log cannot become Actions commands ────────────────────
//
// #1091 round 2, MEDIUM. This step echoes REMOTE bytes back into the job log — the API body on
// exits 4 and 5, curl's and jq's stderr, and `$tag` on the exit-0 permissive path. Actions parses
// workflow commands out of a step's stdout/stderr, and `::stop-commands::<token>` DISABLES that
// parsing for the rest of the job — inside the job whose entire purpose (CPE-1953) is to be loud
// when it does not publish. Reproduced before the fix, at exit 0:
//     ::warning::catalog lower-bound: the latest published release of owner/repo is v1
//     ::error::FORGED-ANNOTATION
//     ::stop-commands::deadbeef and it carries NO catalog-index.json …
// A git refname forbids control characters, so this needs a forged API response — but the
// mitigation is a prefix, so it is taken.
//
// RED-PROOFED 2026-08-28: dropping `catalog_lb_log_safe` from the `$tag` interpolation on that one
// `::warning::` line (leaving every other call in place) reds exactly "a forged tag on the exit-0
// permissive path emits no extra workflow command" — 1 failed / 3 passed in this block.

describe("nothing fetched can become a workflow command in the job log (CPE-1951)", () => {
  /** Every line that a runner would read as a workflow command, i.e. `::…` after leading blanks. */
  function commandLines(text: string): string[] {
    return text.split("\n").filter((l) => /^\s*::/.test(l));
  }
  /** The workflow commands this step is ENTITLED to emit. Everything else is smuggled. */
  const OURS = /^::(error|warning|notice)::catalog /;

  const FORGED_TAG =
    '{"tag_name":"v1\\n::error::FORGED-ANNOTATION\\n::stop-commands::deadbeef",' +
    '"assets":[{"name":"other.txt"}]}';

  itJq("a forged tag on the exit-0 permissive path emits no extra workflow command", () => {
    const r = runGuard(["1787200000", "owner/repo"], { GH_MODE: "raw", GH_BODY: FORGED_TAG });
    expect(r.status).toBe(0);
    // The forged text is still SHOWN — defanging is not hiding — but no longer at line start.
    expect(r.all).toContain("FORGED-ANNOTATION");
    expect(r.all).toContain("stop-commands");
    const smuggled = commandLines(r.all).filter((l) => !OURS.test(l));
    expect(smuggled, `these lines would be parsed as workflow commands: ${smuggled.join(" | ")}`).toEqual([]);
  });

  itJq("a forged API body echoed at exit 5 emits no workflow command", () => {
    const r = runGuard(["1787200000", "owner/repo"], {
      GH_MODE: "raw",
      GH_BODY: '{"assets":[{"name":"x"}],"note":"\\n::error::FORGED-5\\n::stop-commands::beef"}',
    });
    expect(r.status).toBe(5);
    expect(r.all).toContain("FORGED-5");
    expect(commandLines(r.all).filter((l) => !OURS.test(l))).toEqual([]);
  });

  itJq("gh's own stderr echoed at exit 4 emits no workflow command", () => {
    const r = runGuard(["1787200000", "owner/repo"], { GH_MODE: "fail" });
    expect(r.status).toBe(4);
    expect(r.all).toContain("FORGED-VIA-GH-STDERR");
    expect(commandLines(r.all).filter((l) => !OURS.test(l))).toEqual([]);
  });

  itJq("the asset count is the array's length, not a line count over the joined names", () => {
    // #1091 round 2, LOW. Round 1 derived `count` from the joined name string, so a release whose
    // assets are all NAMELESS reported "0 asset(s) enumerated" — a count that lies, printed inside
    // the one message that licenses proceeding with no lower bound.
    const r = runGuard(["1787200000", "owner/repo"], {
      GH_MODE: "raw",
      GH_BODY: '{"tag_name":"v1","assets":[{"x":1},{"y":2},{"z":3}]}',
    });
    expect(r.status).toBe(0);
    expect(r.all).toContain("(3 asset(s) enumerated)");
    expect(r.all).not.toContain("(0 asset(s) enumerated)");
  });
});

// ── 6. The whole story, against a real git repository ───────────────────────────────────────────
//
// A purpose-built repo with three commits at controlled committer dates, rather than this
// checkout's own history: CI's frontend job checks out shallow, so `HEAD~1` is not reliably there,
// and a test that silently skipped would guard nothing in the place it matters most.

describe("an off-tip release, end to end on the publish side (CPE-1951)", () => {
  let repo = "";
  let ok = false;
  const V1 = 1_787_100_000;
  const HOTFIX_OFF_OLDER_BASE = 1_787_150_000;
  const V2 = 1_787_200_000;
  const RE_CUT = 1_787_300_000;

  function git(args: string[], dateEpoch?: number) {
    const stamp =
      dateEpoch === undefined
        ? {}
        : { GIT_AUTHOR_DATE: `${dateEpoch} +0000`, GIT_COMMITTER_DATE: `${dateEpoch} +0000` };
    return spawnSync(
      "git",
      ["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main", ...args],
      {
        cwd: repo,
        encoding: "utf8",
        env: {
          ...process.env,
          ...stamp,
          GIT_AUTHOR_NAME: "cpe",
          GIT_AUTHOR_EMAIL: "cpe@example.invalid",
          GIT_COMMITTER_NAME: "cpe",
          GIT_COMMITTER_EMAIL: "cpe@example.invalid",
        },
      },
    );
  }

  function commitTag(tag: string, epoch: number, body: string) {
    writeFileSync(join(repo, "a.txt"), body);
    git(["add", "a.txt"]);
    if (git(["commit", "-q", "-m", tag], epoch).status !== 0) return false;
    return git(["tag", tag]).status === 0;
  }

  beforeAll(() => {
    repo = mkdtempSync(join(scratchRoot, "repo-"));
    if (git(["init", "-q", "-b", "main"]).status !== 0) return;
    if (!commitTag("v1", V1, "one\n")) return;
    // The maintenance branch: cut from v1, committed BEFORE v2 exists, released AFTER it.
    if (git(["checkout", "-q", "-b", "maint"]).status !== 0) return;
    if (!commitTag("hotfix", HOTFIX_OFF_OLDER_BASE, "one + hotfix\n")) return;
    if (git(["checkout", "-q", "main"]).status !== 0) return;
    if (!commitTag("v2", V2, "two\n")) return;
    // The remedy the guard's message tells you to apply: re-cut on top of v2.
    if (!commitTag("hotfix-re-cut", RE_CUT, "two + hotfix\n")) return;
    ok = true;
  });

  /** The REAL CPE-1941 version derivation, run against the fixture. */
  function derive(ref: string): { status: number | null; version: string; err: string } {
    const r = spawnSync("bash", [VERSION_SCRIPT, ref, repo, String(RE_CUT + 3600)], {
      encoding: "utf8",
    });
    return { status: r.status, version: (r.stdout ?? "").trim(), err: r.stderr ?? "" };
  }

  it("the off-tip tag derives a version BELOW the released one, and the derive step is GREEN", () => {
    expect(ok, "fixture repo must have been created").toBe(true);
    const v2 = derive("v2");
    const hotfix = derive("hotfix");
    // Green: the derive step exits 0 for both. The floor and the future-date check see nothing
    // wrong with the off-tip release — that is the entire bug.
    expect(v2.status, v2.err).toBe(0);
    expect(hotfix.status, hotfix.err).toBe(0);
    expect(Number(hotfix.version)).toBeLessThan(Number(v2.version));
    expect(hotfix.version).toBe(String(HOTFIX_OFF_OLDER_BASE));
  });

  itJq("...and the lower-bound guard REFUSES it, using those real derived numbers", () => {
    expect(ok).toBe(true);
    const published = derive("v2").version;
    const candidate = derive("hotfix").version;
    const r = runGuard([candidate, "owner/repo"], { PUBLISHED: published });
    expect(r.status).toBe(3);
    expect(r.all).toContain("NOT NEWER");
  });

  itJq("...while the re-cut tag is accepted, so the fix does not refuse everything", () => {
    expect(ok).toBe(true);
    const published = derive("v2").version;
    const reCut = derive("hotfix-re-cut");
    expect(reCut.status, reCut.err).toBe(0);
    expect(Number(reCut.version)).toBeGreaterThan(Number(published));
    const r = runGuard([reCut.version, "owner/repo"], { PUBLISHED: published });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("strictly newer");
  });
});
