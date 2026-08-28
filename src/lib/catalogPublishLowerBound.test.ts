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
# Forged stderr, injectable on EVERY mode. #1091 round 3: the 5c block drives one case per exit
# code, and the transport codes (6/7/8/9) echo curl's stderr — which quotes the host and, on some
# failures, the server's own text. Without this the stub could only emit its own fixed strings, so
# those paths would have been "covered" by a case that carried nothing forgeable.
[ -n "\${CURL_ERR:-}" ] && printf '%s\\n' "\${CURL_ERR}" >&2
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
/** bash's own absolute path, resolved once — see `runGuardWithoutJq`. */
let bashPosix = "";

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

/**
 * The same run with PATH set to the stub dir and NOTHING else, so `jq` is genuinely absent and the
 * missing-tool refusal (exit 16) is reachable from a test. The stub dir holds `gh` and `curl`, so
 * this isolates exactly one tool. Everything the script does before that refusal is a bash builtin.
 *
 * The inner shell is invoked by ABSOLUTE path: with PATH stripped, `exec bash` cannot find bash
 * itself and the run dies 127 before it ever reaches the tools check — measured while writing this.
 */
function runGuardWithoutJq(args: string[], env: Record<string, string> = {}): Run {
  const r = spawnSync(
    "bash",
    ["-c", `PATH="${stubBinPosix}"; export PATH; exec "${bashPosix}" "$0" "$@"`, SCRIPT, ...args],
    { encoding: "utf8", cwd: ROOT, env: { ...process.env, ...env } },
  );
  const stdout = r.stdout ?? "";
  const stderr = r.stderr ?? "";
  return { status: r.status, stdout, stderr, all: `${stdout}\n${stderr}` };
}

beforeAll(() => {
  requireBash();
  const whichBash = spawnSync("bash", ["-c", "command -v bash"], { encoding: "utf8" });
  bashPosix = (whichBash.stdout ?? "").trim();
  if (!bashPosix) throw new Error("could not resolve bash's own path; see runGuardWithoutJq");
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

// ── 4b. The tools list is DERIVED from the script, not remembered (CPE-1951, #1091 round 4) ─────
//
// `catalog_lower_bound_tools` refuses a missing tool with exit 16, on the argument that "did not
// run" must never read as "found nothing". Its list was `gh curl jq` — and the script also ran
// `grep -Fxq 'catalog-index.json'` on the asset enumeration, which nothing checked for.
//
// `grep` missing therefore FAILED OPEN, and did so through the one branch that licenses proceeding:
// `if ! grep …` is TRUE on a 127, so the script took the "no catalog-index.json among the assets"
// path. Measured on round 4 with `grep` shadowed by a stub exiting 127, against a release whose
// assets DO list the index:
//     ::warning::… carries NO catalog-index.json (2 asset(s) enumerated) … Proceeding with no
//     lower bound.
//     none
//     exit=0
// A message that contradicts itself in its own second clause, and precisely the defect
// `catalog_lower_bound_tools`'s own header refuses, emitted from inside the function that refuses
// it.
//
// TWO fixes, and the second is the one that matters. `grep` is gone — the match is pure bash now,
// so the dependency is removed rather than declared, which is strictly better than a fourth entry
// in a list someone has to remember. But removing one instance does not close the class: the next
// `awk`/`sed`/`tr` would repeat it exactly. So this test DERIVES the external commands the script
// actually invokes, from the script's own text, and requires each to be either checked for by
// `catalog_lower_bound_tools` or declared here with the reason its absence cannot be read as a
// result (CLAUDE.md, "enumerate, don't recall").
//
// RED-PROOFED 2026-08-28 (bash 5.3.15 cygwin, no jq — this leg needs neither jq nor an execution,
// so all four are 1 failed / 17 passed / 60 skipped):
//   * re-insert a `grep -Fxq 'catalog-index.json' <<< "$assets"` line -> `unchecked: ['grep']`,
//     i.e. the exact defect this section exists for reds on the day it is re-typed.
//   * remove `jq` from the script's own `for t in gh curl jq` -> `unchecked: ['jq']`. The checked
//     list is read out of the script, so it cannot drift from the script's real one.
//   * delete `mktemp` from `TOOLS_NOT_CHECKED` while it is still used, and add a bogus `awk` key,
//     in one run -> `unchecked: ['mktemp']` AND `staleExemptions: ['awk']`, so an exemption can
//     neither be omitted nor outlive its site.
//
// WHAT IT CANNOT SEE, since a scanner over shell text is never complete: a tool named through a
// variable (`"$TOOL" …`), one reached via `eval`, and one inside a heredoc body (`logicalLines`
// skips those as data by design). Also `command -v "$t"` is read as a probe, not an invocation,
// which is what makes `catalog_lower_bound_tools`'s own loop not report every tool as a use.

/**
 * External commands this script runs that `catalog_lower_bound_tools` deliberately does NOT gate on,
 * each with the reason its absence cannot be mistaken for a result. Same shape and same argument as
 * `RAW_OK` below: growth is never silent, because it costs a named key with a prose reason in the
 * diff, and an entry the scan no longer finds is reported as stale.
 *
 * Not registered as a ratchet (docs/design/RATCHETS.md), for the reason `RAW_OK` gives and
 * `ICON_ROLES` gave before it: the assertion is an exact match in BOTH directions, so a fourth key
 * is a reviewable artefact — a name plus a prose justification in the diff — which is exactly what a
 * ratchet row would be. Saying so here rather than relying on `ratchetBaselines.test.ts` not having
 * noticed: that test's auto-detector is what makes the silence uninformative.
 */
const TOOLS_NOT_CHECKED: Record<string, string> = {
  mktemp:
    "absence is 127, and every `x=$(mktemp)` is either `|| return 9` or feeds a redirect that then " +
    "fails the surrounding command — fail-CLOSED, never a verdict",
  rm: "only removes temp files; its status is never read and it can affect no verdict",
  cat:
    "diagnostic text only, at three sites, and each absorbs its own 127: `gh_err=$(cat …) || " +
    "gh_err=\"\"` (which then falls back to `$api_out`), the same shape for `curl_err`, and one " +
    "interpolation inside the exit-14 message. Absence empties a message; it cannot change an exit " +
    "code, so it is not a 'did not run' that could read as 'found nothing'",
};

/** Shell keywords and builtins: present on every POSIX shell, so their absence is not a thing that
 *  can happen. `[` and `[[` included — `[` is a bash builtin, not `/usr/bin/[`, here. */
const SHELL_WORDS = new Set([
  "[", "[[", "]]", ":", ".", "alias", "bg", "bind", "break", "builtin", "caller", "case", "cd",
  "command", "compgen", "complete", "compopt", "continue", "declare", "dirs", "disown", "do",
  "done", "echo", "elif", "else", "enable", "esac", "eval", "exec", "exit", "export", "false",
  "fc", "fg", "fi", "for", "function", "getopts", "hash", "help", "history", "if", "in", "jobs",
  "kill", "let", "local", "logout", "mapfile", "popd", "printf", "pushd", "pwd", "read",
  "readarray", "readonly", "return", "select", "set", "shift", "shopt", "source", "suspend",
  "test", "then", "time", "times", "trap", "true", "type", "typeset", "ulimit", "umask", "unalias",
  "unset", "until", "wait", "while",
]);
/** Words that PREFIX a command in the same fragment — strip and look again. */
const PREFIXES = new Set(["if", "while", "until", "!", "then", "else", "elif", "do", "time", "{", "("]);
/** Words after which the rest of the fragment is a NAME list, a word list or a pattern — never a
 *  command. Without `local` here the scan reports this script's own locals (`line`, `out`, `sep`, …)
 *  as external tools, which is the false-POSITIVE direction and would make the list unreadable. */
const NOT_A_COMMAND = new Set([
  "for", "case", "select", "in", "esac", "done", "fi", "]]",
  "local", "declare", "typeset", "export", "readonly", "unset",
]);

/**
 * The command words of a shell script, best-effort but anchored on code. Two passes before the split,
 * each of which was added because leaving it out produced FALSE POSITIVES on this very script:
 *   1. single-quoted spans removed — its `printf` format strings and its jq filters are full of `|`,
 *      `(` and prose that a naive split reads as commands (`numbers`, `select`);
 *   2. `${…}` and `$((…))` expansions collapsed — otherwise splitting on `{`/`}`/`(` turns
 *      `out="${out}${sep}  |${line}"` into the "commands" `out`, `sep` and `line`, and `i=$((i + 1))`
 *      into `i`. Measured: 7 of the 13 words the first draft reported were this.
 * Then each logical line is cut at every point where a new command can begin.
 */
function collapseExpansions(s: string): string {
  let out = s.replace(/\$\(\([^()]*\)\)/g, "$V");
  for (;;) {
    const next = out.replace(/\$\{[^{}]*\}/g, "$V");
    if (next === out) return next;
    out = next;
  }
}
function commandWords(lines: string[]): Set<string> {
  const found = new Set<string>();
  for (const line of lines) {
    for (const frag of collapseExpansions(dropSingleQuoted(line)).split(
      /\$\(|`|\|\||&&|[;|(){}]|\bdo\b(?=\s)/,
    )) {
      let words = frag.trim().split(/\s+/).filter(Boolean);
      // Strip leading `NAME=value` assignment prefixes (`IFS= read -r x`), then command prefixes.
      for (;;) {
        if (words.length === 0) break;
        if (/^[A-Za-z_][A-Za-z0-9_]*=/.test(words[0]) || PREFIXES.has(words[0])) {
          words = words.slice(1);
          continue;
        }
        break;
      }
      const w = words[0];
      if (!w || NOT_A_COMMAND.has(w)) continue;
      if (!/^[A-Za-z_][A-Za-z0-9_.+-]*$/.test(w)) continue;
      found.add(w);
    }
  }
  return found;
}

describe("every external tool the guard runs is one it refuses to run without (CPE-1951)", () => {
  it("no command is invoked that is neither checked for nor declared unable to affect a verdict", () => {
    const text = readFileSync(SCRIPT, "utf8");
    const lines = logicalLines(text);
    expect(lines.length, "the guard script parsed to almost nothing").toBeGreaterThan(80);

    // The list the script itself checks, read out of the script — not restated here (CLAUDE.md:
    // derive provenance, don't claim it). Anchored on the `for t in …; do` inside the function.
    const forLine = lines.find((l) => /^for t in .*; do$/.test(l));
    expect(forLine, "catalog_lower_bound_tools's `for t in …; do` is gone or was reshaped").toBeTruthy();
    const checked = new Set((forLine as string).replace(/^for t in /, "").replace(/; do$/, "").split(/\s+/));
    expect(checked.size, "the tools loop parsed to nothing").toBeGreaterThan(1);

    // The script's own functions are not external commands.
    const own = new Set(
      lines.flatMap((l) => {
        const m = /^([A-Za-z_][A-Za-z0-9_]*)\(\)\s*\{/.exec(l);
        return m ? [m[1]] : [];
      }),
    );
    expect(own.size, "no function definitions found — the scan, not the script, is broken").toBeGreaterThan(3);

    const invoked = commandWords(lines);
    // Parser self-check, NOT the property: if the command scan stops finding the tools the script
    // visibly runs, every assertion below goes vacuous.
    expect(
      [...invoked].sort(),
      "the command scan found none of the tools this script visibly runs — the scan is broken",
    ).toEqual(expect.arrayContaining(["curl", "gh", "jq", "mktemp"]));

    const external = [...invoked].filter((w) => !SHELL_WORDS.has(w) && !own.has(w)).sort();
    const unchecked = external.filter((w) => !checked.has(w) && !(w in TOOLS_NOT_CHECKED));
    const staleExemptions = Object.keys(TOOLS_NOT_CHECKED)
      .filter((w) => !external.includes(w))
      .sort();
    expect(
      { unchecked, staleExemptions },
      "`unchecked` are external commands this script runs that `catalog_lower_bound_tools` does not " +
        "refuse to run without. That is a FAIL-OPEN: a missing tool's 127 is read as its answer — " +
        "which is how `grep`'s absence made the guard announce 'carries NO catalog-index.json' " +
        "about a release that carries one, at exit 0. Add it to the `for t in …` list, remove the " +
        "dependency, or declare it in TOOLS_NOT_CHECKED with the reason its absence cannot become a " +
        `verdict. \`staleExemptions\` are declared but no longer used. All commands found: ${external.join(", ")}`,
    ).toEqual({ unchecked: [], staleExemptions: [] });
  });

  it("...and that command scan reads code, not the prose in a log message", () => {
    // ROUND 7, and it is column 3 of the enumeration above. `commandWords` was answered for only by
    // the real script, where every command it finds is already in the tools loop — the same gap
    // round 5 left on `flagPrintf`'s consumer, one function over. Sabotage says the coverage was
    // PARTIAL and accidental rather than absent, which is why this test asserts an exact set rather
    // than only the absences: dropping its `dropSingleQuoted` reds the real-script caller too (that
    // script's prose is wordy enough to yield ~30 phantom commands), but disabling the
    // `NAME=value`/`PREFIXES` strip reds NOTHING there — the caller merely stops seeing `read`, and
    // `SHELL_WORDS` excuses it. **2 failed / 82 passed** and **1 failed / 83 passed** respectively.
    // Synthetic lines, none of them in the script: three tool names appear ONLY inside a
    // single-quoted `printf` format string (`awk` in backticks, a bare `xmlstarlet`, a `$(perl …)`)
    // and must not be reported, while the real invocations behind an assignment prefix, a `while`
    // body, a pipe, a process substitution and an `if !` must be.
    const lines = logicalLines(
      [
        "printf 'run `awk` or xmlstarlet by hand, then $(perl -pe s/x/y/) it\\n' >&2",
        "IFS= read -r line < <(jq -r .a x)",
        'while :; do curl -sS "$url" | tee out; done',
        "if ! command -v gh >/dev/null; then unzip -o f; fi",
      ].join("\n"),
    );
    expect(lines.length, "the synthetic lines did not survive `logicalLines`").toBe(4);
    expect(
      [...commandWords(lines)].sort(),
      "the command scan is reading prose out of a single-quoted `printf` format string as commands, " +
        "or is missing a real invocation behind a prefix, a pipe or a substitution — an over-report " +
        "here is a phantom `unchecked` tool, an under-report is the FAIL-OPEN the caller above exists " +
        "to catch",
    ).toEqual(["command", "curl", "jq", "printf", "read", "tee", "unzip"]);
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
// #1091 round 2, MEDIUM. This step echoes REMOTE bytes back into the job log. Actions parses
// workflow commands out of a step's stdout/stderr, and `::stop-commands::<token>` DISABLES that
// parsing for the rest of the job — inside the job whose entire purpose (CPE-1953) is to be loud
// when it does not publish. Reproduced before the fix, at exit 0:
//     ::warning::catalog lower-bound: the latest published release of owner/repo is v1
//     ::error::FORGED-ANNOTATION
//     ::stop-commands::deadbeef and it carries NO catalog-index.json …
// A git refname forbids control characters, so this needs a forged API response — but the
// mitigation is a prefix, so it is taken.
//
// ### WHY THIS BLOCK WAS RESTRUCTURED IN ROUND 3 — read before adding a case to it
//
// Round 2 shipped this describe's title as a UNIVERSAL — "nothing fetched can become a workflow
// command" — standing on THREE enumerated paths: the API body at exits 4/5, gh/curl/jq stderr, and
// `$tag` on the exit-0 permissive path. The sanitiser was applied to exactly those three. The
// FOURTH site, `$tag` on the exit-10 contradiction path, went out unsanitised and green. Round 3
// reproduced it twice, the second time with only `gh` stubbed and the REAL curl hitting the REAL
// index URL — which 404s today (#1062) — so it needs no control over curl at all:
//     catalog lower-bound check: https://…/catalog-index.json returned HTTP 404, but the latest
//     release (v0.57.69-sidecar
//     ::error::FORGED-ANNOTATION
//     ::stop-commands::deadbeef) DOES list catalog-index.json among its assets. …   exit=10
// Both forged lines at column 0, and exit 10 is a FAILURE path — the worst of the four to miss,
// because `::stop-commands::` there silences the annotations this job exists to emit.
//
// A universal claim standing on a remembered list is what let the fourth through, so the list is
// gone. Two legs now, neither of which anyone has to remember to extend:
//
//   * STRUCTURAL — `taintedVars()` derives, from the script's own text, every variable holding
//     remote bytes, then requires each `printf … >&2` to route each of them through
//     `catalog_lb_log_safe` AND to contain no surviving substitution once the sanitiser calls are
//     blanked out. This is the leg that catches a new echo site the DAY it is written, with no case
//     to add — for the shapes it can see. What it cannot see is enumerated at `taintedVars`, each
//     with the sabotage result that measured it, rather than left for the next reader to discover.
//   * EXECUTED — `EXIT_CODE_CASES` ranges over every exit code DERIVED from the script's own
//     `return N` statements, not over a chosen subset, and the coverage assertion reds when the two
//     sets differ. Add a `return 18` to the script and this file fails until a case drives it. One
//     case per exit CODE, which is not one per BRANCH — see that array's own docblock.
//
// ### WHAT ROUND 4 FOUND, and why the sentence above now reads "for the shapes it can see"
//
// Round 3 wrote that the structural leg "catches a new echo site the DAY it is written, with no case
// to add", full stop. That was measurably false for two shapes, and both were green sabotages on the
// guard in the round whose whole subject was unowned claims (CLAUDE.md, CPE-1929):
//   * TRANSITIVE ASSIGNMENT. Round 3's `taintedVars` matched only `name=$(tool …)`. Inserting
//     `local rel_name; rel_name="$tag"` on the exit-13 branch and printing `"$rel_name"` left the
//     structural leg GREEN (16 passed / 60 skipped, no jq) with remote bytes going raw into a
//     `printf … >&2`; only the executed leg reddened (1 failed / 75 passed, with jq). Closed by a
//     fixed-point pass over plain assignments — re-measured with the same sabotage:
//     **1 failed / 16 passed / 60 skipped, `undeclared: ['rel_name']`**.
//   * INLINE `$(tool …)` WITH NO VARIABLE. Deleting the sanitiser from
//     `"$(catalog_lb_log_safe "$(cat "$jq_err")")"` at the exit-14 site left the WHOLE FILE green,
//     76 passed — a live sanitiser call deletable from a live site with nothing noticing, because
//     `jq_err` comes from `mktemp` (untainted) and the loop iterated variables, never substitutions.
//     Closed by default-deny on any surviving `$(`/backtick in a `printf … >&2` — re-measured with
//     the same sabotage: **1 failed / 16 passed / 60 skipped**, `rawSubstitutions` naming the
//     exit-14 line.
// Both rules are satisfied by the script as it stands, so on the real source they look like nothing.
//
// ### WHAT ROUND 5 FOUND, and it is the sentence that used to stand here
//
// Round 4 wrote: "The synthetic-input test right after the scan is what keeps them from rotting into
// a vacuous pass." It did not. That test RE-IMPLEMENTED both predicates instead of calling the scan's,
// so the two copies could disagree in silence. The reviewer ran the CPE-1929 pair:
//   1. `if (false && (bare.includes("$(") || bare.includes("`")))` in the real scan, and
//   2. the sanitiser dropped from the live exit-14 site: `"$url" "$(cat "$jq_err")" >&2`.
// Result: structural leg **19 passed / 60 skipped**, whole file **79 passed / 0 failed** — a live
// sanitiser deleted from a live remote-bytes log site, the rule that catches it disabled, and the
// test that exists to prevent exactly that, green. Closed by extracting `flagPrintf`, which both the
// real scan and the synthetic driver now call (CLAUDE.md, CPE-1950: remove the duplication rather
// than derive it), and by asserting the two predicates SEPARATELY there instead of counting flags.
// Re-measured with the reviewer's same pair applied: **1 failed / 22 passed / 60 skipped** — the
// synthetic test reds on the substitution predicate. Note WHICH test reds and which does not: the
// real scan stays silent, because `jq_err` comes from `mktemp` and is not tainted, so the deleted
// sanitiser at the exit-14 site is visible to the shared predicate and to nothing else. That is
// precisely why re-implementing the predicate in the synthetic test made the pair green.
//
// A claim that a rule "keeps another rule from rotting" is a provenance claim about code, so it is
// subject to CLAUDE.md's rule 3 like any other: sabotage the rule and watch the guard red, or do not
// write the sentence.
//
// RED-PROOFED 2026-08-28, five ways, bash 5.3.15 cygwin — results here rather than only in the PR
// body (CLAUDE.md rule 3). The first three were run under jq 1.7.1 in round 3; the two round-4 ones
// were run on a machine WITHOUT jq, so their numbers are the no-jq figures (the executed leg was
// skipped, which is the point — each reds on the STRUCTURAL leg alone):
//   * restore the round-3 bug (drop `catalog_lb_log_safe` from the `$tag` interpolation on the
//     exit-10 line ONLY, leaving all three other calls in place) -> **2 failed / 74 passed**. The
//     structural leg reds with `undeclared: ['tag']`; the executed leg reds on the exit-10 case
//     with `['::error::FORGED-404', '::stop-commands::deadbeef404']`. Both, independently.
//   * append `catalog_lb_redproof_18() { return 18; }` to the script -> the coverage leg reds with
//     `missing: [18]`, i.e. a new exit code cannot be added without a case that drives it.
//   * add a `tag:` entry to `RAW_OK` while the site IS sanitised -> reds with `stale: ['tag']`, so
//     an exemption cannot outlive the site it was written for.
//   * the two round-4 sabotages above, with their measured counts.
//
// ROUND 5's four, same machine, WITH real jq 1.7.1 on PATH unless noted — the un-run half is where
// the last regression was, so the baseline is 83 passed / 0 skipped rather than a jq-less subset:
//   * delete `assets="${assets//$'\r'/}"` from the script -> **46 failed / 37 passed**. One of those
//     46 is the pure-bash `the asset enumeration matches on jq's real line endings` block, which
//     reds `"CRLF, index first": "0"` with no jq involved at all, so it reds on LF-only Linux CI
//     too; the other 45 are the executed leg discovering the same thing the expensive way.
//   * the reviewer's CPE-1929 pair, both halves at once — `substitution: false && (…)` in
//     `flagPrintf`, and the sanitiser dropped from the live exit-14 site — which at round-4 head
//     left the whole file green at 79 passed. Now **1 failed / 22 passed / 60 skipped** (no jq),
//     the synthetic-input test naming the predicate: `expected [false,false,false,false] to deeply
//     equal [false,true,false,true]`. The real scan still says nothing, which is the point: `jq_err`
//     comes from `mktemp`, so only the shared predicate can see that site.
//   * revert the taint pass to first-match-only (`m = null`), drop `+=` from `ASSIGN`, and kill
//     `FILLED` -> the ordinary-shapes test reds naming four of its six cases: `append`, `append onto
//     a SANITISED name re-opens it`, `two assignments on one line`, and `` `read` with no `=` ``.
//   * append a line with an unterminated `'` to the script -> the fail-OPEN-branch test reds,
//     quoting the offending logical line back.
//   * `blankSanitiserCalls`'s two fixes reverted ONE AT A TIME -> the synthetic-input test reds
//     each time, on a different case: the anchor alone for the double-quoted mention, the
//     `dropSingleQuoted`-first ordering alone for the unclosed `$(catalog_lb_log_safe` in prose.
//     Neither fix is redundant, which is not what it looked like before the pair was run.
//
// ── EVERY FUNCTION IN THIS FILE THAT SCANS SHELL TEXT, and the same three questions ─────────────
//
// Three rounds running, the SAME defect landed in a SIBLING of the thing that round fixed: round 5
// widened `ASSIGN` for two names on a line and left `FILLED` taking one; round 5 extracted
// `flagPrintf` and left its consuming loop unguarded; round 6 applied `dropSingleQuoted` inside
// `flagPrintf` and not inside `taintedVars` one function away, where its absence was a live
// fail-OPEN. Each fix was correct. None PROPAGATED. So the enumeration is written down rather than
// re-derived from memory next round.
//
// THE MEMBERSHIP RULE, because round 7 answered a COUNT instead. Round 7's table had three rows and
// closed with "adding a fourth line-scanning function means adding a fourth row" — and two functions
// that qualified, `collapseExpansions` and `blankSanitiserCalls`, were already in the file and in
// neither the rows nor the excused pair. A count cannot notice what was never counted. The rule is a
// predicate, and it is APPLIED rather than remembered:
//
//     a row is owed by every function DECLARED IN THIS FILE that takes shell TEXT — one logical
//     line, a list of them, or the script's source — and answers a question about the shell in it.
//
//         grep -nE '^[ ]*function [A-Za-z]' src/lib/catalogPublishLowerBound.test.ts
//
// `…every shell scanner in this file is a row in the table above…` below runs that grep against the
// file's own source and reds unless every name it finds is either a row or named in `NOT_SHELL_TEXT`
// with the reason it is not one — so the NEXT scanner fails the day it lands rather than the round
// after, and calling something "not a scanner" costs a reviewable diff. The reason is now asserted
// too, not merely stored: round 9 added `reviewerProbeScanner: ""` beside a new function and this
// test went green, so an empty excuse bought the same pass as an argued one. Excused, with reasons:
//   * `dropSingleQuoted` / `endsInsideSingleQuote` — the stripper itself and its self-check, so
//     question 1 is what they ARE rather than something they do.
//   * `guardLogicalLines` — the SOURCE of the shell text (a file read plus imported `logicalLines`),
//     not a scanner of it; a narrowing here is caught by the parser self-checks its callers carry.
//   * `commandLines` — scans the runner's OUTPUT for `::…` workflow commands, not shell.
//   * `enumerate` — RUNS a slice of the script under bash; it answers nothing by reading.
//   * `shapeCorpus` / `bashBinds` — one GENERATES shell lines from bash's option grammar and the
//     other executes them; neither reads shell text to answer a question about it.
//   * the harness and fixture drivers (`parseWorkflow`, `runGuard`, `git`, `derive`, …), which take
//     YAML, Rust, argv or a temp path.
//
//   fn                  | 1. strips single-quoted prose first?   | 2. every name/word its construct binds?
//   --------------------|----------------------------------------|-------------------------------------------
//   collapseExpansions  | n/a — it runs INSIDE `commandWords`,   | n/a: it collapses `${…}`/`$((…))` so the
//                       | after that function's strip            | split does not read them as commands.
//   commandWords        | YES, since round 4 —                   | n/a: it reports command WORDS, not
//                       | `dropSingleQuoted(line)` at the head   | bindings. The analogous widening is its
//                       | of its own loop                        | `NAME=value` / `PREFIXES` / `$( )` walk.
//   expansionEnd        | n/a — it runs INSIDE `shellTokens`,    | n/a: it reports one INDEX, the end of a
//                       | on already-stripped text               | `${…}`/`$(…)`/`$((…))` span. Its whole
//                       |                                        | content is "where does this expansion
//                       |                                        | close", and -1 when it does not.
//   shellTokens         | n/a — it runs INSIDE `filledTargets`,  | n/a: it reports WORDS. Its widenings are
//                       | after `taintedVars`' strip             | the metacharacter token (round 8) and the
//                       |                                        | expansion span + escape (round 9).
//   tokensRound7        | n/a — same position, but nothing in    | n/a: the tokenizer round 7 shipped, kept
//   tokensRound8        | the live path calls either. They are   | ONLY so the generated-corpus test can
//                       | scoring baselines for the corpus test  | show it still SEES the round-7/round-8
//                       |                                        | trade in both directions.
//   filledTargets       | n/a — same, it is handed already-      | NO, and stated as rows rather than as a
//                       | stripped text                          | universal — see below.
//   taintedVars         | YES as of round 7 — round 6's gap,     | NO. Round 7 wrote "YES … every name/word
//                       | and it was fail-OPEN, not merely       | its construct binds" here, in the round
//                       | noisy                                  | whose own framing is that a universal
//                       |                                        | written at this pass gets falsified by an
//                       |                                        | ordinary shape within a day — and it was,
//                       |                                        | by six of them, the same day. The honest
//                       |                                        | answer: `filledTargets` walks bash's
//                       |                                        | documented option grammar for `read` /
//                       |                                        | `mapfile` / `printf -v` and NOTHING else,
//                       |                                        | it is scored against the table of shapes
//                       |                                        | below in BOTH directions, and its own
//                       |                                        | docblock lists what it over- and
//                       |                                        | under-reports. `local -n`, `getopts` and
//                       |                                        | `eval` are not bound at all.
//   blankSanitiserCalls | NO, and correctly — its CALLER         | n/a: it blanks sanitiser calls, it binds
//                       | `flagPrintf` strips first, and that    | nothing. Its widening was round 4's
//                       | ORDERING is itself asserted (case 5)   | anchor fix, red-proofed at its docblock.
//   flagPrintf          | YES, since round 5, and its ordering   | n/a: it matches names against an
//   (+ scanLogSites)    | vs `blankSanitiserCalls` is asserted   | already-derived taint set.
//   enumerationLines    | NO — it filters on `$(`/backtick and   | n/a: it slices the script's own
//                       | never strips. Fail-open in the         | enumeration loop out to run it.
//                       | direction its own docblock states.     |
//
//   fn                  | 3. its CONSUMER covered by a test that would red?
//   --------------------|-----------------------------------------------------------------------------
//   collapseExpansions  | PARTLY, and by ACCIDENT — the same answer round 7 gave `commandWords` and
//                       | did not record for this one. Measured, not reasoned: `if (s) return s;`
//                       | reds **1 failed / 84 passed**, and the red is the REAL-SCRIPT caller only,
//                       | `unchecked: i, line, missing, out, repo, sep, t`. The synthetic
//                       | `…reads code, not the prose…` test does not exercise it at all — none of
//                       | its four lines carries a `${…}` next to a split character.
//   commandWords        | PARTLY, and by ACCIDENT, until round 7 — measured, not reasoned: its only
//                       | caller was the real script, where every command found is already legal.
//                       | Dropping its `dropSingleQuoted` DOES red there (this script's prose is wordy
//                       | enough to produce ~30 phantom "commands"), but disabling the prefix strip
//                       | reds NOTHING — the caller just loses `read`, which `SHELL_WORDS` excuses.
//                       | `…that command scan reads code, not the prose…` below closes that.
//   expansionEnd        | YES, by the corpus AND by rows — `return -1;` at the head of it (i.e. no
//                       | expansion is ever a span) reds the generated-corpus test with 69 fail-open
//                       | lines AND the ordinary-shapes table's eight `${…}`/`$((…))` rows, 2 failed
//                       | / 24 passed. Measured, not reasoned: round 9's red-proofs, below.
//   shellTokens         | YES — the metacharacter rows of the ordinary-shapes table AND the generated
//                       | corpus. Disabling the metacharacter token reds THREE of the five round-8 rows
//                       | (measured; the other two survive on `bound` lifting the leading identifier
//                       | off a whole operand); reverting `filledTargets` to round 7 entire reds all
//                       | five plus `read -ar arr`; and either sabotage now also reds the corpus test,
//                       | which is the point of adding it — a row list only covers what someone wrote.
//   tokensRound7        | YES, and INVERTED: these two exist to FAIL. The corpus test asserts each
//   tokensRound8        | fails open where the other does not, so making either equal `shellTokens`
//                       | reds it with "no longer reproduces the round-7/round-8 TRADE".
//   filledTargets       | YES — nineteen rows of that same table, plus the whole generated corpus.
//   taintedVars         | YES — `…the taint pass follows the ordinary shell shapes…` below, now
//                       | twenty-two rows, each a `printf … >&2` that was classified wrongly.
//   blankSanitiserCalls | YES — `…that scan can actually SEE…`, synthetic cases (3)/(4)/(5), each
//                       | fix reverted one at a time.
//   flagPrintf          | YES — `…that scan can actually SEE…` drives `flagPrintf` AND `scanLogSites`
//   (+ scanLogSites)    | with six synthetic lines; round 5 covered only the former and the mutant moved.
//   enumerationLines    | PARTLY, and its docblock says which half: the executed leg catches a
//                       | transformation that breaks the enumeration (33 failed / 50 passed), the
//                       | pure-bash probe does not.
//
// A "no" in column 3 is the shape that has bitten twice; a "no" in column 2 is what round 7 wrote as
// a "yes". Note what column 3 costs to answer honestly: round 7's first draft wrote a flat YES for
// `commandWords` and a red-proof that had not been run, and running it produced a different number
// and a different conclusion; round 8's `collapseExpansions` answer came out the same way. Answer
// these three by sabotage, never by reading.
//
// ROUND 9's red-proofs live at `filledTargets`' docblock, where the clauses they cover live — seven
// of them, each run. Two belong here because they are about THIS table's own machinery:
//   * `reviewerProbeScanner: ""` added to `NOT_SHELL_TEXT` beside a real new `function
//     reviewerProbeScanner(line: string)` -> reds with `['reviewerProbeScanner']`, naming the entry
//     that excuses a scanner without saying why. With the new `expect` disabled (`false && why.trim()
//     …`) and BOTH the probe function and the empty entry still in place, the test goes **green** —
//     the pair, run rather than reasoned (CPE-1929), so this assertion is reachable and is not
//     shadowed by the `stale` check that would have caught an entry naming no function at all.
//   * the two round-9 additions to the row list, `expansionEnd` and the `tokensRound7`/`tokensRound8`
//     pair, arrived as `unclassified` from this same test before their rows were written — which is
//     the membership rule doing its job on the diff that added it a fourth and fifth scanner.
//
// ROUND 8's red-proofs, same machine, jq 1.7.1 on PATH, baseline **85 passed / 0 skipped** (round
// 7's 84 plus the new membership test). All of them run, numbers as the runner printed them:
//   * `filledTargets` reverted BODILY to round 7's — whole-word regex, `cut` operand, no `array`
//     flag -> **1 failed / 84 passed**, the ordinary-shapes test naming exactly SIX rows: the five
//     metacharacter-adjacent ones and `read -ar arr`. That is the round-7 trade, priced.
//   * `else if (false && SHELL_META.test(ch))` in `shellTokens`, everything else left alone ->
//     **1 failed / 84 passed**, and **three** rows, not five — see `filledTargets`' docblock for
//     which two survive it and why. The first draft of this line said five, reasoned off the row
//     labels; running it said three.
//   * the `array` flag never set (`array = false` where the `-a` clause sets it) -> **1 failed /
//     84 passed**, only the `read -ar arr` row, `["r","arr"]` against `["r"]`.
//   * `collapseExpansions` neutered (`if (s) return s;`) -> **1 failed / 84 passed**, and the red is
//     the REAL-SCRIPT command scan (`i, line, missing, out, repo, sep, t` arrive as `unchecked`
//     commands), not the synthetic one. That is the table's column 3 for it, run rather than read.
//   * `NOT_SHELL_TEXT` shortened by one entry -> the membership test reds with
//     `unclassified: ['posixPath']`. Renaming a TABLE row instead reds with both `unclassified` and
//     `stale` populated, so the table and the lists cannot drift apart in either direction.
//
// ROUND 7's red-proofs, re-quoted because they are still the coverage for those clauses, against
// its own **84 passed** baseline:
//   * remove `dropSingleQuoted` from `taintedVars`' loop (`const line = raw`) -> **1 failed /
//     83 passed**, the ordinary-shapes test naming the two round-7 prose rows and NOTHING else:
//     `"a remedy MESSAGE naming the sanitiser marks a tainted name clean": []` (want `["safe"]`)
//     and `"prose inside a single-quoted printf is not a `read`": ["release"]` (want `[]`).
//   * revert `filledTargets` to round 6's `FILLED` regex, `dropSingleQuoted` left in place so this
//     isolates the grammar fix from the prose fix -> **1 failed / 83 passed**, the same test naming
//     exactly the four shapes — `read --`, `read -p`, bare `read` (REPLY), `mapfile -C` — with the
//     `read -a` row staying GREEN, which is the measurement behind that row's "only by luck" note.
//   * `if (false && cmd === "read" && w[k] === "a" …)` -> **1 failed / 83 passed**, only the
//     `read -a` row, `[]` against `["arr"]`.
//   * `commandWords` sabotaged TWO ways, and the pair is the informative part (CPE-1929). Removing
//     its `dropSingleQuoted` reds **2 failed / 82 passed** — the real-script caller reds too, with
//     ~30 English words in `unchecked` — so that stripper was already covered, accidentally, by this
//     script's prose being wordy. Disabling the `NAME=value` / `PREFIXES` strip instead reds **only
//     the new test**, **1 failed / 83 passed**: the real caller merely loses `read` from `invoked`,
//     `read` is in `SHELL_WORDS`, and nothing notices. The SECOND is the boundary that test actually
//     closes. The draft of that note claimed the first sabotage was green before round 7; it is not,
//     and it was corrected by running it rather than by rereading it.

/** Every line that a runner would read as a workflow command, i.e. `::…` after leading blanks. */
function commandLines(text: string): string[] {
  return text.split("\n").filter((l) => /^\s*::/.test(l));
}
/** The workflow commands this step is ENTITLED to emit. Everything else is smuggled. */
const OURS = /^::(error|warning|notice)::catalog /;

/** Comment-stripped, continuation-joined logical shell lines of the guard script. `logicalLines`
 *  rather than a hand-rolled stripper (CLAUDE.md: anchor on code, never on prose) — this file's own
 *  script is dense with comments quoting the very strings these scans look for. */
function guardLogicalLines(): string[] {
  return logicalLines(readFileSync(SCRIPT, "utf8"));
}

/**
 * The variables holding REMOTE bytes, derived rather than listed. Two ways in:
 *
 *   1. DIRECT — assigned from a command substitution whose text invokes `gh`, `curl`, `jq` or `cat`.
 *      That covers `api_out`/`gh_err` (gh, cat), `tag`/`assets`/`count`/`bound` (jq),
 *      `http`/`curl_err` (curl, cat), and correctly leaves out `url` (built from a workflow input)
 *      and the four `mktemp` paths.
 *   2. TRANSITIVE — assigned from an expression that merely MENTIONS an already-tainted variable
 *      (`rel_name="$tag"`, `msg="[$tag]"`), to a fixed point. Added in #1091 round 4 because the
 *      round-3 rule matched only `name=$(tool …)`, so the most ordinary refactor there is walked
 *      straight through it: inserting `local rel_name; rel_name="$tag"` and printing `"$rel_name"`
 *      on the exit-13 `printf … >&2` left THIS leg green (measured: 16 passed / 60 skipped, no jq;
 *      the reviewer measured 1 failed / 75 passed with jq, i.e. only the EXECUTED leg reddened).
 *      Flow-insensitive and deliberately one-way — a later `tag=""` does not untaint `tag` — which
 *      is the fail-closed direction.
 *
 * `sanitised` is the subset re-assigned through `catalog_lb_log_safe`, which may then be
 * interpolated bare; a sanitised name is never re-tainted by rule 2.
 *
 * ### WHAT THIS DERIVATION STILL CANNOT SEE — stated, because a green sabotage on a guard has to be
 * ### expected rather than alarming (CLAUDE.md, CPE-1929)
 *
 * Closed in round 4: transitive assignment (rule 2 above), and an inline `$(tool …)` with no
 * variable at all — the scan below refuses ANY surviving `$(`/backtick in a `printf … >&2` line,
 * which is what made deleting the sanitiser off `"$(catalog_lb_log_safe "$(cat "$jq_err")")"` red.
 * Before that it was 76 passed, fully green: a live sanitiser call could be deleted with nothing
 * noticing, because `jq_err` comes from `mktemp` (so it is not tainted) and the loop iterated
 * variables, never substitutions. Closed in round 5: several assignments on one line, `+=`, a
 * substitution running a non-tool command, and `read`/`printf -v`/`mapfile` targets — see the
 * `ASSIGN` and `filledTargets` comments in the body. Closed in round 6: the SECOND and later targets
 * of one `read`, which round 5's `FILLED` dropped while widening `ASSIGN` for the identical
 * several-names-on-one-line shape in the same diff. Closed in round 7, and both are the SAME defect
 * a third time — a fix applied at one instance of a pattern while a sibling instance keeps it:
 * this pass reading PROSE (`dropSingleQuoted` ran in `flagPrintf` and not here, which was fail-OPEN
 * in one direction — a REMEDY MESSAGE naming `catalog_lb_log_safe` marked a genuinely tainted
 * variable `sanitised` — and fail-closed noise in the other, four English words out of one message
 * standing in the live taint set), and `read`/`mapfile` option parsing, now bash's documented
 * grammar in `filledTargets` rather than a regex that mis-bound four shapes bash accepts.
 * Closed in round 8, and it is the FOURTH instance of the same shape, this time INSIDE round 7's own
 * fix: replacing that regex with a word walk also dropped the regex's anchor, so the six shapes that
 * put the builtin hard against a shell metacharacter (`…|read -r x`, `read -r a;read -r b`,
 * `(read …)`, `jq f|mapfile -t a`, `{ read …; }`) went from bound to `[]` in the diff that fixed
 * four others. Round 9 re-measured those six against bash and only TWO of them bind in the enclosing
 * shell at all — see `filledTargets`' OVER-reported bullet.
 * Closed in round 9, and it is the FIFTH, this time inside round 8's fix and one function from the
 * cure: making `{`/`}`/`(`/`)` metacharacter tokens broke the operand walk on any `${…}` / `$((…))`
 * in an option argument, losing ten more shapes, while `collapseExpansions` had been doing exactly
 * that job for `commandWords` since round 4. A widening is a TRADE until both directions are scored,
 * and three consecutive rounds have now scored one direction by enumerating rows. The rows are kept,
 * but the coverage is a GENERATED corpus scored against bash itself — see `…scored against BASH
 * ITSELF over a GENERATED corpus…`. The live taint set is 12 either way, at every one of these
 * rounds — re-measured, not assumed: `IFS, api_out, asset_name, assets, bound, count, curl_err,
 * gh_err, http, line, out, tag`, identical to round 7's, because this script never writes those
 * shapes today. That is a LATENT regression, not a shipped fail-open, and saying which is part of
 * the report.
 *
 * ### READ THE FORM OF THIS SECTION BEFORE ADDING TO IT
 *
 * Round 3 wrote "there is nothing to add"; round 4 replaced it with a three-item list ending "none
 * of them present in the script today" — and `while IFS= read -r asset_name`, receiving remote
 * asset names, was on the FIRST item of that list, added by round 4 itself, in the same diff. The
 * defect is not which shapes got listed. It is that a closed list of what a scan misses is a claim
 * of exactly the same kind as the claim it qualifies, and it is unowned in exactly the same way.
 * So: the shapes below are AT LEAST these, never all of them; nothing here asserts what the script
 * does or does not contain (that is measured, not recalled — the derivation is the measurement, and
 * where a shape could be followed instead of listed it was, above); and the honest summary is that
 * this scan reads assignments and `printf … >&2` lines, so anything that is neither is invisible to
 * it and reaches the executed leg or nothing.
 *
 * At least these, each with the sabotage that measures it:
 *   * A wrapper FUNCTION that prints remote bytes itself, called bare rather than in `$( )` — e.g.
 *     `emit_tag() { printf '%s' "$tag"; }` invoked as a statement. There is no `printf … >&2` here
 *     for the scan to read, so it goes green.
 *   * Any channel that is not a `printf … >&2` at the end of a logical line: `echo … >&2`,
 *     `>&2 printf …`, a heredoc, a `{ …; } >&2` group, or stdout (out of scope by design — see the
 *     `logSites` filter's own comment inside `scanLogSites`; round 6 renamed `logPrintfs` and moved
 *     that comment, and this line was the one reference left pointing at the old name).
 *   * Indirection that never names the variable: `printf … "${!ref}" >&2`, `eval`, `${arr[@]}`.
 * These are caught by the EXECUTED leg instead, which reads the real process's stdout+stderr and
 * cannot be fooled by how the bytes got there — but only for a path some case already drives. That
 * asymmetry is the reason both legs exist.
 */
/**
 * Every name one logical shell line BINDS without an `=` — `read`, `mapfile`/`readarray`,
 * `printf -v` — per bash's documented option grammar rather than a regex approximation of it.
 *
 * A WIDENING IS A TRADE UNTIL BOTH DIRECTIONS ARE SCORED, and rounds 7, 8 and 9 have each been one.
 * Round 7 replaced round 6's regex with a word walk, verified the four shapes the regex mis-bound,
 * and never ran the regex's OWN cases back through the walk. Round 6 anchored on
 * `(?:^|[\s!(){};|])`, so it saw the builtin after a metacharacter with no space; round 7 took whole
 * whitespace-delimited words, so it did not, and six shapes went from caught to `[]` in the same
 * diff that fixed four — measured by the round-8 reviewer, reproduced here, each now a row below:
 *     printf … "$(jq …)"|read -r leaked   -> []      -- pipe, no space
 *     read -r a;read -r b < <(jq …)        -> ["a"]   (bash: a AND b)    -- `;`, no space
 *     (read -r x < <(jq -r .n f))          -> []      -- subshell parens
 *     jq -r .n f|mapfile -t arr2           -> []      -- pipe into mapfile
 *     { read -r p;read -r q; }             -> ["p"]   (bash: p AND q)    -- brace group
 *     v=$(read -r x)                       -> []      -- command substitution
 * ROUND 9 CORRECTION: round 8 wrote "(bash: leaked=TT)", "(bash: x=NN)", "(bash: arr2=(NN))" on
 * three of those and said all five were "measured, not read off the man page". Re-measured with
 * `declare -p` in the ENCLOSING shell, THOSE THREE BIND NOTHING THERE — a pipeline's RHS and an
 * explicit `( … )` each run in their own subshell, `lastpipe` is off, and the guard script never
 * sets it. Only the `;` and the `{` rows are genuine catches, so round 8's widening bought TWO, not
 * five, and moved three into the OVER-reported bullet below. `v=$(read -r x)` was always in that
 * bullet and the docblock reasons about it correctly, four lines away — the pipe and the parens
 * simply did not get the same treatment, which is what makes this a claim defect and not a slip.
 *
 * `shellTokens` answers all three directions and no single anchor did: metacharacters are their own
 * tokens (round 6's property), a double-quoted span stays whole (round 7's), and an expansion or a
 * backslash-escape stays whole too (round 9's). The rule round 8 left behind — when you replace a
 * matcher, run the PREVIOUS matcher's cases through the new one and say what moved, in both
 * directions — is right, and round 8 then broke it in its own diff by running the previous rounds'
 * ENUMERATED ROWS instead of a sweep. Ten more shapes went from caught to the `REPLY`/`MAPFILE`
 * fallback, all of them an ordinary `${…}` / `$((…))` / `\;` in an option argument, because round
 * 8 made `{`, `}`, `(` and `)` metacharacter tokens and the operand walk breaks on any of them.
 * Third round, same trade, this time inside the round that named it. So the rows are no longer the
 * coverage: `…scored against BASH ITSELF over a GENERATED corpus…` sweeps every option × argument
 * spelling × shell context and asks bash, which is the leg that does not depend on anyone having
 * thought of the case.
 *
 * WHY A WORD WALK AND NOT A REGEX. Round 6's regex allowed `-x` and `-x arg` option runs and then
 * took the trailing run of identifiers. That is not the grammar: WHICH short options consume the
 * following word is a fixed, documented, per-builtin list, and without it four shapes bash accepts
 * came out wrong while the comment at the regex said "EVERY name after the options … UNCONDITIONALLY".
 * Measured at round 6's head, and each is also a row in the ordinary-shapes table below:
 *     read -- name                -> []        (bash: name=hello)   -- end-of-options unhandled
 *     read -p "Enter: " x         -> []        (bash: x=hello)      -- the option argument has a
 *                                                                      space, so `\S+` stopped
 *     read / read -r              -> []        (bash: REPLY=hi)     -- the implicit target
 *     mapfile -C mycb -c 2 arr    -> ["mycb"]  (bash: arr=a b)      -- the CALLBACK, not the array
 * and `read -r -- name` answered `["name"]` only by luck: the regex read `-r` as an option whose
 * argument was `--`. Dropping the `-r` broke it.
 *
 * The grammar (bash(1), `read` and `mapfile`), which is the whole content of this function:
 *   read [-ers] [-a aname] [-d delim] [-i text] [-n n] [-N n] [-p prompt] [-t sec] [-u fd] [name…]
 *   mapfile [-d delim] [-n count] [-O origin] [-s count] [-t] [-u fd] [-C callback] [-c n] [array]
 * so `adinNptu` / `dnOsuCc` are the argument-taking options, `read` binds a LIST of names and
 * `mapfile` exactly one array, `-a aname` is the one option ARGUMENT that is itself a target, and
 * with no name at all the defaults are `REPLY` / `MAPFILE`. Short options bundle (`-ra`), `--` ends
 * them, and the command ends at the first shell metacharacter so `read -r a b; then` binds `a` and
 * `b` rather than `then`, and `mapfile -t lines < <(jq . f)` does not bind `f`.
 *
 * AT LEAST these, and the list is split by whether a wider walk could catch it (CLAUDE.md's round-9
 * rule: never a count, and say which half each shape is in). Every entry below was RUN through the
 * live path before being written down — round 7's version of this list asserted a mechanism for the
 * array-subscript entry that running it falsified, in both of its halves:
 *   * OVER-reported, i.e. fail-CLOSED noise. `read -ar arr` is `-a` with the inline argument `r`,
 *     so bash binds `r` and IGNORES `arr` (measured); the `array` flag is that fix and the
 *     `` `read -ar` `` row is its red-proof. `read -- -x` is an ERROR in bash (`-x': not a valid
 *     identifier`, status 1) binding nothing, and is still reported as `["REPLY"]` — kept rather
 *     than fixed, because the direction is fail-closed and the row below says so out loud.
 *     JOINING THEM IN ROUND 9, the three round 8 filed as catches: a `read`/`mapfile` on the RHS of
 *     a PIPE and one inside an explicit `( … )`. Both run in a subshell, so the enclosing shell —
 *     the only one whose variables can reach a `printf … >&2` — binds nothing, and reporting the
 *     name is noise. Measured, `declare -p` in the enclosing shell, bash 5.3.15:
 *         printf "%s\n" "$(jq -r .t x)"|read -r leaked   ->  leaked: not found
 *         (read -r x < <(jq -r .n f))                    ->  x: not found
 *         jq -r .n f|mapfile -t arr2                     ->  arr2: not found
 *         read -r a;read -r b < <(jq -r .n x)            ->  b=$'NN'     <- genuine
 *         { read -r p;read -r q; } < <(printf "1\n2\n")  ->  q="2"       <- genuine
 *     Their rows stay, at the same `want`: over-reporting costs a false positive and pinning it is
 *     how the direction stays visible. Their LABELS now say which half they are in. The generated
 *     corpus makes this structural rather than remembered — its second assertion is that every
 *     over-report sits in a context bash cannot bind in, and 150 of its 395 lines do.
 *     Also over-reported: an option whose argument runs into a metacharacter (`read -r -d ; x`) is
 *     a line bash REJECTS, and the walk answers `["REPLY"]` rather than `["x"]` — the operand after
 *     it belongs to the next command, so reporting it would be reporting a name that cannot exist.
 *   * CAUGHT, contrary to round 7's note here. `read -r arr[0]` reports `arr`, because the `bound`
 *     mapping below takes the leading identifier of each operand. Round 7 wrote that the operand
 *     "is taken whole, so `arr[$i]` is reported verbatim and matches no `$arr` use"; both halves are
 *     wrong. Written single-quoted as `read -r 'arr[$i]'` — the only spelling that survives an
 *     unset `$i` — `taintedVars` has already run `dropSingleQuoted`, so the operand arrives as `''`
 *     and the walk reports the `REPLY` fallback, not `arr[$i]`.
 *   * NOT CAUGHT TODAY, catchable — `local`/`declare`/`typeset -n` nameref binding; `getopts
 *     optstring name`.
 *   * CANNOT be caught by any walk of this line — the builtin reached through a variable or an
 *     alias (`$RD -r x`), `eval "read $names"`, and a name computed at run time. `read -r ${nm}`
 *     is the ordinary spelling of that last one, and it is why the generated corpus varies the
 *     OPTION ARGUMENT's spelling and never the operand's: no walk can know what `${nm}` expands to.
 *     Those are the executed leg's job. A `read` inside a `$( … )` is not caught either and does not
 *     need to be — same subshell argument as the pipe and the parens above (measured: `v=$(read -r
 *     x)` leaves `x` UNSET), and round 9's expansion span restores the `[]` that round 8's tokenizer
 *     had turned back into `["x"]`.
 * Round 9 CLOSED, rather than listing, the one shape the round-8 list did not name at all: an
 * unterminated double quote. Round 7's regex and round 8's loop both swallowed to end of line, which
 * hides a live `read` from a default-DENY scan — the same fail-open the `fail-OPEN branch` test
 * measures away for `dropSingleQuoted`'s unterminated `'`. It is now an ordinary character, and the
 * `an unterminated `"` does not swallow the rest of the line` row is the red-proof.
 * ROUND 9's red-proofs. Every red here is in the derived-from-source leg, which `jq` does not gate,
 * so they were run against the cheap **26 passed / 60 skipped** no-jq baseline and the numbers are
 * quoted that way rather than converted. That jq-independence is measured, not assumed: the FIRST
 * one below was re-run with jq 1.7.1 on PATH and gave **2 failed / 84 passed / 0 skipped** — the
 * same two tests and the same 69 lines, against the with-jq baseline of **86 passed / 0 skipped**:
 *   * `return -1;` at the head of `expansionEnd` (no expansion is ever a span) -> **2 failed /
 *     24 passed**: the generated-corpus test with **69** fail-open lines, and the ordinary-shapes
 *     table naming exactly the EIGHT `${…}`/`$((…))` rows. That is round 8's trade, priced.
 *   * `if (false && ch === "\\" …)` in `shellTokens` -> **2 failed / 24 passed**: 6 corpus lines
 *     and the two `\;` rows. A backslash-escaped metacharacter IS an ordinary character.
 *   * `bound` reading the operand without `.replace(/"/g, "")` -> **1 failed / 25 passed**, only
 *     the `read -r "quoted"` row. The corpus stays GREEN, and that is worth knowing: it varies the
 *     spelling of option ARGUMENTS, never of operands, so a quoted operand is a shape it cannot
 *     see. Every generated space has an edge; say where this one's is.
 *   * `if (false && (arg === undefined || SHELL_META.test(arg)))` -> **2 failed / 24 passed**, and
 *     it is the corpus's OVER-report assertion that reds, with 12 lines in binding contexts, plus
 *     the `an option whose argument ran into a metacharacter` row. So that assertion is load-
 *     bearing rather than decorative.
 *   * `if (true || j < line.length)` in the `"` branch (swallow to end of line, as rounds 7 and 8
 *     did) -> **1 failed / 25 passed**, the unterminated-`"` row. Run BEFORE the row existed too:
 *     the file was fully green, which is why the row exists rather than a note.
 *   * `tokensRound8` returning `shellTokens(line)` -> **1 failed / 25 passed**, the corpus's own
 *     sensitivity check, `r8LosesWhatR7Catches: false`.
 *   * the GENERATOR narrowed instead: `braced`, `arith` and `escaped` all rendered bare, keeping the
 *     line count at 395 so the `> 300` self-check cannot answer first (CPE-1929 — do not leave a
 *     guard shadowed) -> **1 failed**, the same sensitivity check, `round 8 on 0`. Neutering only
 *     ONE of the three leaves it GREEN, which is the correct semantics: the check asks whether the
 *     corpus can still see the trade at all, not whether it covers every spelling.
 * Round 8's red-proofs, still the coverage for those clauses, against its own **85 passed /
 * 0 skipped** baseline (jq 1.7.1 on PATH):
 *   * reverting this function BODILY to round 7's — the whole-word regex, the `cut` operand, no
 *     `array` flag -> **1 failed / 84 passed**, reding exactly SIX rows: the five
 *     metacharacter-adjacent ones and `` `read -ar arr` ``. That is the trade round 7 made, priced.
 *   * the metacharacter token alone (`else if (false && SHELL_META.test(ch))`, everything else this
 *     round left in place) -> **1 failed / 84 passed**, and it reds **three** rows, not five: the
 *     pipe-`read`, the subshell and the pipe-`mapfile`. The `;` and `{` rows survive it, and the
 *     reason is worth keeping — without the token the operand `a;read` is taken whole and `bound`
 *     still lifts `a` off the front, so the walk carries on and reaches `b`. It was round 7's `cut`,
 *     not its word regex, that lost those two. Written down because the first draft of this note
 *     said "five", reasoned from the row labels, and running it said three.
 *   * `if (false && cmd === "read" && w[k] === "a" …)` -> **1 failed / 84 passed**, exactly the two
 *     `-a` rows: `read -r -a arr` goes `["arr"]` -> `[]` and `read -ar arr` goes `["r"]` -> `["arr"]`.
 *   * `array = false` where the `-a` clause sets it -> **1 failed / 84 passed**, only the
 *     `` `read -ar arr` `` row, `["r","arr"]` against `["r"]`.
 *   * round 6's `FILLED` regex in place of this walk (round 7's measurement, re-quoted) -> **1 failed
 *     / 83 passed** at that baseline, ONLY the four shapes the regex mis-bound.
 */
/**
 * One shell metacharacter. It both ENDS the command before it and BEGINS the one after it, with or
 * without a space on either side, which is the whole reason `shellTokens` emits it as its own token.
 */
const SHELL_META = /^[;|&(){}<>`]$/;
/**
 * The end index (inclusive) of the balanced `${…}` / `$(…)` / `$((…))` span that starts at
 * `line[i] === "$"`, or -1 when there is no expansion there OR it never closes on this line.
 *
 * Returning -1 for the UNCLOSED case rather than "to end of line" is the whole point of the
 * function's shape: swallowing to the end is what the `"` branch used to do, and for a default-deny
 * detector hidden text is fail-OPEN — the same reasoning the `fail-OPEN branch` test applies to
 * `dropSingleQuoted`'s unterminated `'`. An unbalanced `${` therefore degrades to "the `$` is an
 * ordinary character", which loses no token.
 */
function expansionEnd(line: string, i: number): number {
  const open = line[i + 1];
  if (open !== "{" && open !== "(") return -1;
  const close = open === "{" ? "}" : ")";
  let depth = 0;
  for (let j = i + 1; j < line.length; j += 1) {
    const c = line[j];
    if (c === "\\") {
      j += 1;
      continue;
    }
    if (c === open) depth += 1;
    else if (c === close) {
      depth -= 1;
      if (depth === 0) return j;
    }
  }
  return -1;
}
/**
 * One logical line's words, with the properties `filledTargets` needs and which no single regex gave
 * it at once. Each was added because leaving it out was MEASURED to lose bindings bash makes:
 *   * a double-quoted span is held TOGETHER, so `-p "Enter: "` is ONE option argument and a `read`
 *     inside a double-quoted message is not a word at all (round 7's gain, kept);
 *   * every shell metacharacter is emitted as its OWN token, so `jq f|mapfile -t a` is two commands
 *     with no space between them, and the operand walk stops at one instead of pattern-matching for
 *     it inside a word (round 6's anchor, recovered in round 8);
 *   * an EXPANSION — `${…}`, `$(…)`, `$((…))` — is held together the same way a quoted span is
 *     (round 9). Without it `{`, `}`, `(` and `)` are metacharacter tokens INSIDE an ordinary option
 *     argument, the operand walk breaks on the first of them, and `read -p ${msg} p9` came back as
 *     the `REPLY` fallback: ten shapes bash binds, lost by round 8's own widening, in the round that
 *     wrote the rule against unscored trades. `collapseExpansions` had done exactly this job for
 *     `commandWords` since round 4, one function away — the sibling-propagation defect a fifth time;
 *   * a BACKSLASH escapes the next character, so `mapfile -d \; -t arr9` is one command, not two.
 * The `"` branch, like `expansionEnd`, treats an UNTERMINATED quote as an ordinary character rather
 * than swallowing the rest of the line. Round 7's regex and round 8's loop both swallowed, which
 * hides text from a default-deny scan; nothing in the script trips it today (the live taint set is
 * 12 either way), and it is a hole rather than a feature, so it is closed rather than listed.
 */
function shellTokens(line: string): string[] {
  const out: string[] = [];
  let cur = "";
  const flush = () => {
    if (cur !== "") out.push(cur);
    cur = "";
  };
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (ch === "\\" && i + 1 < line.length) {
      cur += line.slice(i, i + 2);
      i += 1;
      continue;
    }
    if (ch === '"') {
      let j = i + 1;
      for (; j < line.length; j += 1) {
        if (line[j] === "\\") j += 1;
        else if (line[j] === '"') break;
      }
      if (j < line.length) {
        cur += line.slice(i, j + 1);
        i = j;
        continue;
      }
      cur += ch; // unterminated: an ordinary character, NOT the rest of the line
      continue;
    }
    if (ch === "$") {
      const end = expansionEnd(line, i);
      if (end !== -1) {
        cur += line.slice(i, end + 1);
        i = end;
        continue;
      }
    }
    if (/\s/.test(ch)) flush();
    else if (SHELL_META.test(ch)) {
      flush();
      out.push(ch);
    } else cur += ch;
  }
  flush();
  return out;
}
/**
 * The tokenizer round 7 shipped: whole whitespace-delimited words with a double-quoted span held
 * together, no metacharacter tokens, no expansion spans. Kept ONLY as a scoring baseline for the
 * generated-corpus test below — nothing in the live path calls it. It is here because that corpus's
 * own self-check is whether it can still SEE the round-7/round-8 trade in BOTH directions; a corpus
 * that cannot is a corpus that measures nothing, and `failOpen: []` over it would be vacuous.
 */
function tokensRound7(line: string): string[] {
  return line.match(/(?:"(?:\\.|[^"])*"|[^\s"])+/g) ?? [];
}
/** The tokenizer round 8 shipped: metacharacter tokens and quoted spans, no expansion spans and no
 *  escape handling. The other half of that self-check, and the thing round 9 replaces. */
function tokensRound8(line: string): string[] {
  const out: string[] = [];
  let cur = "";
  const flush = () => {
    if (cur !== "") out.push(cur);
    cur = "";
  };
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (ch === '"') {
      let j = i + 1;
      for (; j < line.length; j += 1) {
        if (line[j] === "\\") j += 1;
        else if (line[j] === '"') break;
      }
      cur += line.slice(i, j + 1);
      i = j;
      continue;
    }
    if (/\s/.test(ch)) flush();
    else if (SHELL_META.test(ch)) {
      flush();
      out.push(ch);
    } else cur += ch;
  }
  flush();
  return out;
}
function filledTargets(line: string, tokens: (l: string) => string[] = shellTokens): string[] {
  const out: string[] = [];
  // `printf -v var` binds exactly one name — bash's `printf` has no other name-binding option, so
  // this branch is single-capture on purpose rather than by omission.
  for (const m of line.matchAll(/(?:^|[\s!(){};|&])printf\s+-v\s*([A-Za-z_][A-Za-z0-9_]*)/g)) out.push(m[1]);
  const words = tokens(line);
  for (let i = 0; i < words.length; i += 1) {
    const cmd = words[i] === "readarray" ? "mapfile" : words[i];
    if (cmd !== "read" && cmd !== "mapfile") continue;
    const takesArg = cmd === "read" ? "adinNptu" : "dnOsuCc";
    const names: string[] = [];
    let operands = false; // set by `--`
    let array = false; // set by `-a aname`, after which bash IGNORES every trailing name
    let j = i + 1;
    for (; j < words.length; j += 1) {
      const w = words[j];
      if (SHELL_META.test(w)) break; // the command ends at a metacharacter
      if (!operands && w === "--") {
        operands = true;
        continue;
      }
      if (!operands && w.length > 1 && w.startsWith("-")) {
        for (let k = 1; k < w.length; k += 1) {
          if (!takesArg.includes(w[k])) continue;
          // The argument is the rest of the bundle if there is one (`-dX`), else the next word.
          const inline = w.slice(k + 1);
          const arg = inline !== "" ? inline : words[(j += 1)];
          // The argument ran into a metacharacter or off the end of the line, so there ISN'T one:
          // `read -r -d ; x` is `read -r -d` (which bash rejects, "option requires an argument")
          // followed by the command `x`. Reporting `x` would be reporting a name bash never binds.
          if (arg === undefined || SHELL_META.test(arg)) {
            j = words.length;
            break;
          }
          // `read -a aname` is the one option argument that IS the target.
          if (cmd === "read" && w[k] === "a") {
            names.push(arg);
            array = true;
          }
          break;
        }
        continue;
      }
      // An operand. `shellTokens` has already cut it at the metacharacter that ends the command.
      if (array) break; // `read -ar arr` binds `r` and IGNORES `arr` — measured against bash 5.3.15
      names.push(w);
      if (cmd === "mapfile") break; // mapfile binds ONE array name
    }
    // Quotes around an operand are removed before the name is read out of it: `read -r "quoted"`
    // binds `quoted` in bash (measured), and taking the leading identifier of the token WITH its
    // quote finds none and falls back to `REPLY`. Round 7 and round 8 both lost this one.
    const bound = names
      .map((n) => /^[A-Za-z_][A-Za-z0-9_]*/.exec(n.replace(/"/g, ""))?.[0])
      .filter((n): n is string => Boolean(n));
    out.push(...(bound.length > 0 ? bound : [cmd === "read" ? "REPLY" : "MAPFILE"]));
    i = j;
  }
  return out;
}

function taintedVars(lines: string[]): { tainted: Set<string>; sanitised: Set<string> } {
  const tainted = new Set<string>();
  const sanitised = new Set<string>();
  const plain: { name: string; rhs: string }[] = [];
  // EVERY assignment on the line, `g` and a loop — not `.exec` once. Round 4 took the first match
  // and, on the `=$(` branch, `continue`d past the rest of the line entirely; three ordinary shell
  // shapes walked through it, none of them exotic and none on its stated blind-spot list:
  //     local a="$tag" b="$tag"      -> `b` never tainted, `printf … "$b" >&2` not flagged
  //     local a="$tag" b=$(date)     -> `a` never seen at all (the `=$(` branch consumed the line)
  //     msg+="$tag"                  -> `+=` matched no regex at all
  // `\+?=` picks up append, and every match is classified from the text that FOLLOWS it, so a
  // second assignment on the same line is judged on its own right-hand side.
  const ASSIGN = /(?:^|[\s!(){};])([A-Za-z_][A-Za-z0-9_]*)(\+?)=/g;
  // A variable filled WITHOUT an `=` on its left — `read`, `mapfile`/`readarray`, `printf -v`.
  // Round 4 listed these as "still blind … none of them present in the script today" — and
  // `while IFS= read -r asset_name` was added by that very round, one line from a live hole,
  // receiving remote asset names. So they are followed rather than listed, in `filledTargets`
  // below, which walks bash's DOCUMENTED option grammar for those three builtins instead of
  // approximating it with a regex. Every name it reports is tainted with no right-hand side
  // consulted: unlike an `=` there is nothing to judge, and fail-closed is the only answer
  // available. On this script that taints `asset_name` (remote, correct) and
  // `catalog_lb_log_safe`'s own `line` (which never reaches a `printf … >&2`).
  //
  // ROUND 7 replaced the regex, and the sentence that stood here, together — see `filledTargets`
  // for the four shapes bash accepts that the regex bound wrongly or not at all, each now a row in
  // the ordinary-shapes table below. Round 5's `FILLED` captured one target per `read`; round 6
  // widened it to "EVERY name after the options" and wrote that as a universal; four shapes
  // (`read -- name`, `read -p "…" x`, bare `read`, `mapfile -C cb -c 2 arr`) falsified it the same
  // day. A regex cannot express "this option consumes the next word", which is the whole grammar.
  for (const raw of lines) {
    // PROSE FIRST, exactly as `flagPrintf` does — round 6 applied `dropSingleQuoted` there and not
    // here, one function away, and the omission ran in BOTH directions. Over-taint, visible:
    // `catalog-lower-bound.sh`'s "Refusing to read an unenumerable release as …" is a single-quoted
    // `printf` format string, and `read an unenumerable release as` fed FOUR English words to the
    // name list, taking the live taint set to 16. Fail-OPEN, the one that matters, reproduced by
    // the round-7 reviewer:
    //     printf 'remedy: wrap it as safe=$(catalog_lb_log_safe "$VAR") before logging\n' >&2
    // put `safe` into `sanitised` from PROSE, so the real `safe="$tag"` two lines later and its
    // `printf … "$safe" >&2` came back `flagged: []` — a variable carrying remote bytes reported
    // clean because a REMEDY MESSAGE named the sanitiser. That is the same bypass-by-rewording that
    // synthetic cases (3)/(4) record round 4 as closing for `blankSanitiserCalls`, still standing
    // in the sibling scan. Stripping is exact, not merely safer: the shell expands nothing inside
    // `'…'`, so `msg='$tag'` is a literal and NOT tainting it is the correct answer, not a
    // loosening. The one input on which this hides text — an unterminated `'` — is measured away
    // for the real script by the `fail-OPEN branch` test below, which now covers this scan too.
    const line = dropSingleQuoted(raw);
    for (const target of filledTargets(line)) tainted.add(target);
    ASSIGN.lastIndex = 0;
    for (let m = ASSIGN.exec(line); m; m = ASSIGN.exec(line)) {
      const name = m[1];
      const append = m[2] === "+";
      const rest = line.slice(m.index + m[0].length);
      // `x+=` on a sanitised name re-opens it: appending unsanitised bytes to a sanitised string
      // makes the whole string unsanitised, so the name loses its exemption rather than keeping it.
      if (append) sanitised.delete(name);
      if (rest.startsWith("$(")) {
        const call = rest.slice(2);
        if (/(^|[\s|(])catalog_lb_log_safe[\s)]/.test(call)) {
          if (!append) sanitised.add(name);
          continue;
        }
        if (/(^|[\s|(])(gh|curl|jq|cat)\s/.test(call)) {
          tainted.add(name);
          continue;
        }
        // A substitution running something else — `x=$(printf '%s' "$tag")`. Round 4 `continue`d
        // here, so a tainted variable laundered through any other command came out clean; it falls
        // through to the transitive pass instead.
        plain.push({ name, rhs: call });
        continue;
      }
      plain.push({ name, rhs: rest });
    }
  }
  // Fixed point: `a="$tainted"` taints `a`, `b="$a"` then taints `b`, and so on.
  for (;;) {
    let grew = false;
    for (const { name, rhs } of plain) {
      if (tainted.has(name) || sanitised.has(name)) continue;
      if (![...tainted].some((v) => new RegExp(`\\$\\{?${v}\\b`).test(rhs))) continue;
      tainted.add(name);
      grew = true;
    }
    if (!grew) break;
  }
  return { tainted, sanitised };
}

/**
 * Every single-quoted span replaced by `''`. In POSIX shell a single-quoted string admits no escapes
 * and no substitution whatsoever, so removing them is exact — and it is what lets the
 * "no surviving `$(`" rule below run over lines whose first argument is a long single-quoted `printf`
 * format string full of prose, backticks and parentheses. Without it, a message that merely MENTIONS
 * `$(cat …)` or a `` `command` `` in prose reds the scan.
 *
 * A STATE MACHINE, not `/'[^']*'/g`, and the difference is not stylistic. This script writes an
 * apostrophe inside a single-quoted string the only way sh allows — `'…on %s'"'"'s latest release…'`
 * — and the regex pairs those quotes WRONG: it consumes `'"'` as a span and then treats the real
 * prose that follows as unquoted code. Measured on the exit-3 message's shape, whose prose contains
 * `` `git tag` ``:
 *     naive  -> printf ''"''s latest release. (ApplyOutcome::Rollback) `git tag` on a non-tip … >&2
 * i.e. a backtick surviving out of a comment-like string, which the rule below would report as a raw
 * substitution. Whether that false positive fires at all depends on the parity of apostrophes in the
 * message — exactly the kind of accidental green CLAUDE.md's "anchor on code, never on prose" is
 * about. Inside `"…"` a `'` is a literal character, and this scanner knows that.
 */
function dropSingleQuoted(s: string): string {
  let out = "";
  let i = 0;
  let dq = false; // inside a double-quoted span, where `'` is an ordinary character
  // `$(` opens a FRESH quoting context — bash re-lexes the substitution body from scratch, so a `'`
  // inside `"$( … )"` starts a real single-quoted span even though the `"` is still open. Round 4's
  // machine did not know that and read `"$(awk '{print}' f)"` as three literal characters, which
  // fed `{print}` to `commandWords` as if it were a command name. Stack of enclosing `dq` states
  // rather than a boolean, popped on the `)` that closes the substitution (and only while not
  // inside a nested `"…"`, so a literal `)` in `$(printf "(%s)" x)` does not close it early).
  const dqStack: boolean[] = [];
  while (i < s.length) {
    const ch = s[i];
    if (ch === "\\" && i + 1 < s.length) {
      out += s.slice(i, i + 2);
      i += 2;
      continue;
    }
    if (ch === '"') {
      dq = !dq;
      out += ch;
      i += 1;
      continue;
    }
    if (ch === "$" && s[i + 1] === "(") {
      dqStack.push(dq);
      dq = false;
      out += "$(";
      i += 2;
      continue;
    }
    if (ch === ")" && !dq && dqStack.length > 0) {
      dq = dqStack.pop() as boolean;
      out += ch;
      i += 1;
      continue;
    }
    if (ch === "'" && !dq) {
      const end = s.indexOf("'", i + 1);
      out += "''";
      // An unterminated span runs to end of line, which is what the shell does — but note the
      // DIRECTION, because round 4's comment here had it exactly backwards and called it
      // "fail-CLOSED … it can only hide code, never invent it". Hiding code is precisely
      // fail-OPEN for the default-deny rule downstream: a `$(` swallowed by a phantom span is a
      // substitution that never gets flagged. It is kept because it matches the shell, and the
      // exposure is bounded by an assertion rather than by this sentence — `everyLogicalLine
      // closes its single quotes` below re-lexes every logical line of the real script and reds if
      // any of them ever reaches this branch, so on this source the fail-open path is unreachable
      // rather than merely believed to be unused.
      i = end < 0 ? s.length : end + 1;
      continue;
    }
    out += ch;
    i += 1;
  }
  return out;
}

/** True when `s` ends inside an unterminated single-quoted span — the one input on which
 *  `dropSingleQuoted` hides text rather than merely blanking a quoted literal. Same walk, so it
 *  cannot drift from the stripper it describes. */
function endsInsideSingleQuote(s: string): boolean {
  let i = 0;
  let dq = false;
  const dqStack: boolean[] = [];
  while (i < s.length) {
    const ch = s[i];
    if (ch === "\\" && i + 1 < s.length) {
      i += 2;
      continue;
    }
    if (ch === '"') {
      dq = !dq;
      i += 1;
      continue;
    }
    if (ch === "$" && s[i + 1] === "(") {
      dqStack.push(dq);
      dq = false;
      i += 2;
      continue;
    }
    if (ch === ")" && !dq && dqStack.length > 0) {
      dq = dqStack.pop() as boolean;
      i += 1;
      continue;
    }
    if (ch === "'" && !dq) {
      const end = s.indexOf("'", i + 1);
      if (end < 0) return true;
      i = end + 1;
      continue;
    }
    i += 1;
  }
  return false;
}

/**
 * Blanks out every `$(catalog_lb_log_safe …)` call, matching parens, so what remains is the set of
 * interpolations that reach the log RAW.
 *
 * ANCHORED ON `$(catalog_lb_log_safe`, not on the bare name, and that is a fix rather than a
 * tidy-up. Round 4 searched for the name ANYWHERE in the line and then blanked backwards from
 * `lastIndexOf("$(", i)`, so a message that merely NAMES the sanitiser in prose ate whatever
 * substitution came before it:
 *     printf '%s' "$(cat /etc/passwd)" 'per catalog_lb_log_safe policy' >&2
 *       -> raw=[]  rawSubstitutions=[]      (green)
 * A bypass of the headline default-deny rule achievable by REWORDING a log message, in a script
 * whose messages are long prose. Two changes close it: the anchor above (a bare mention no longer
 * names a span to blank, so nothing is removed and the line is judged on what it actually runs),
 * and `flagPrintf` running `dropSingleQuoted` FIRST so single-quoted prose is gone before this
 * ever looks.
 *
 * BOTH, and they are not redundant — measured, one at a time, against synthetic cases (3)/(4)/(5)
 * in the test below. Reverting only the anchor reds case (4), a mention in a DOUBLE-quoted argument
 * that `dropSingleQuoted` cannot reach. Reverting only the ordering reds case (5), a prose mention
 * that opens `$(catalog_lb_log_safe` and never closes it, so the paren matcher runs off the end of
 * the line and blanks the real substitution along with everything else. Case (3), the shape the
 * reviewer found, is closed by either.
 */
function blankSanitiserCalls(s: string): string {
  let out = s;
  const OPEN = /\$\(\s*catalog_lb_log_safe\b/;
  for (;;) {
    const m = OPEN.exec(out);
    if (!m) return out;
    const start = m.index;
    let depth = 0;
    let j = start + 1;
    for (; j < out.length; j += 1) {
      if (out[j] === "(") depth += 1;
      else if (out[j] === ")") {
        depth -= 1;
        if (depth === 0) break;
      }
    }
    out = `${out.slice(0, start)}«safe»${out.slice(j + 1)}`;
  }
}

/** How ONE `printf … >&2` logical line is classified: which tainted variables reach the log raw,
 *  and whether any command substitution survives the sanitiser blanking.
 *
 *  ONE implementation, called by BOTH the scan over the real script and the synthetic-input test
 *  below (CLAUDE.md, CPE-1950: where the duplication is removable, remove it). Round 4's synthetic
 *  test COPIED this body instead, which is why the docblock claiming it "keeps them from rotting
 *  into a vacuous pass" was false: the reviewer disabled the substitution predicate in the real
 *  scan AND deleted a live sanitiser from the exit-14 remote-bytes log site, and the whole file
 *  stayed green at 79 passed — the synthetic copy answered for a rule that was no longer wired up.
 *  With the predicates shared, that same sabotage reds the synthetic test directly.
 *
 *  Order matters and is the fix for a second hole: `dropSingleQuoted` runs BEFORE
 *  `blankSanitiserCalls`, so prose inside a single-quoted `printf` format string is gone before
 *  anything searches it. Removing single-quoted spans cannot lose a real interpolation — the shell
 *  performs no expansion of any kind inside `'…'` — so this direction is exact, not merely safer. */
function flagPrintf(
  line: string,
  tainted: Set<string>,
  sanitised: Set<string>,
): { rawVars: string[]; substitution: boolean } {
  const stripped = blankSanitiserCalls(dropSingleQuoted(line));
  const rawVars = [...tainted]
    .filter((v) => !sanitised.has(v) && new RegExp(`\\$\\{?${v}\\b`).test(stripped))
    .sort();
  return { rawVars, substitution: stripped.includes("$(") || stripped.includes("`") };
}

/**
 * The WHOLE scan, from a set of shell logical lines to the verdict triple the assertion compares —
 * taint pass, `printf … >&2` selection, the loop over `flagPrintf`, and the `undeclared`/`stale`
 * bookkeeping against an exemption list.
 *
 * EXTRACTED ONE LEVEL OUT FROM `flagPrintf`, and the reason is the interesting part (CPE-1929).
 * Round 4's surviving mutant was the substitution PREDICATE: the synthetic test carried a copy of
 * it, so `if (false && …)` on the real scan changed nothing anyone could see. Round 5 closed that
 * by sharing `flagPrintf` — and the mutant did not die, it MOVED: the predicate became covered and
 * its CONSUMER, the `if (substitution) subs.push(…)` line in the real scan's own loop, became the
 * new uncovered boundary. Measured at round 5's head, re-run here rather than quoted:
 * `if (false && substitution)` there, plus deleting the live sanitiser off the exit-14
 * `"$(catalog_lb_log_safe "$(cat "$jq_err")")"`, gave **23 passed / 0 failed / 60 skipped** —
 * green.
 *
 * That is the general shape, worth naming rather than patching: **extracting a shared helper closes
 * the mutant at the helper and opens one at the new boundary.** The same move one level out closes
 * it — the loop, the filter and the bookkeeping now live here, and the synthetic test below drives
 * THIS function, not just `flagPrintf`. RE-MEASURED with that identical pair applied to this
 * function (`if (false && substitution) rawSubstitutions.push(…)` on the line below, plus the same
 * exit-14 sanitiser deletion): **1 failed / 22 passed / 60 skipped**, the failure being
 *
 *     AssertionError: the scan's own loop — selection, accumulation and exemption bookkeeping — is
 *     not carrying `flagPrintf`'s verdicts through to the triple the real assertion compares:
 *     expected { logSites: 6, …(3) } to deeply equal { logSites: 6, …(3) }
 *     - "subs": 4   + "subs": 0
 *
 * so the mutant is dead rather than moved again. One level further out still are `guardLogicalLines()`
 * and `RAW_OK`, supplied by the caller — and round 6 left those described as simply open, which
 * overstates it in both cases. NEITHER is a silent boundary, and round 7 says which, because
 * "still open" and "covered by a different mechanism" are not the same report:
 *   * `guardLogicalLines()` NARROWING is caught by the parser self-check in the assertion below
 *     (`expect(tainted).toEqual(arrayContaining(["api_out", "assets", "bound", "gh_err", "tag"]))`
 *     plus `lines.length > 80`), which exists precisely so a parse that stops finding what the
 *     script visibly assigns reds instead of going vacuous. What it does NOT catch is a parse that
 *     returns those five and drops some sixth line.
 *   * `RAW_OK` cannot quietly GROW: `stale` reports every entry the scan no longer finds and the
 *     assertion is exact-match, so an exemption added without a matching raw site reds on the spot.
 *     What it does not do is stop a diff adding an entry ALONGSIDE the raw site it excuses — which
 *     is the reviewable-diff trade `RAW_OK`'s own docblock already states, not a hidden gap.
 * The genuinely uncovered boundary is the mutant one level out from BOTH: the assertion body itself.
 * That is the natural terminus — the reviewer's round-7 note, kept because it is the answer to the
 * next person who runs the sabotage chain — since mutating an expected literal there is just
 * deleting the assertion, which no test can be asked to notice on its own behalf.
 *
 * `rawOk` is a parameter rather than a reach for the module-level `RAW_OK` so the synthetic test can
 * drive the bookkeeping with its own (empty) exemption list; the real scan passes `RAW_OK`.
 */
function scanLogSites(
  lines: string[],
  rawOk: Record<string, string>,
): {
  logSites: string[];
  raw: Map<string, string[]>;
  undeclared: string[];
  stale: string[];
  rawSubstitutions: string[];
} {
  const { tainted, sanitised } = taintedVars(lines);
  // Only `>&2`. The two stdout `printf`s are out of scope and that is stated, not silent: one is
  // `catalog_published_lower_bound`'s return VALUE (captured by its caller, never logged) and one
  // is the success line, which interpolates only validated numbers. The executed leg scans stdout
  // and stderr TOGETHER, so nothing rides out on the channel this scan does not read.
  const logSites = lines.filter((l) => /^printf\b/.test(l) && />&2\s*$/.test(l));
  const raw = new Map<string, string[]>();
  /** Inline substitutions surviving the sanitiser blanking — no variable to name, so no `rawOk`
   *  key either. Default-DENY: a command run INLINE in the argument list has no variable for a
   *  per-variable loop to iterate, so round 3's scan could not see it at all — deleting
   *  `catalog_lb_log_safe` from `"$(catalog_lb_log_safe "$(cat "$jq_err")")"` left the whole file
   *  green at 76 passed. Once every `$(catalog_lb_log_safe …)` is blanked, a `printf … >&2` has no
   *  business running anything. Backticks too: they are the other substitution spelling. */
  const rawSubstitutions: string[] = [];
  for (const line of logSites) {
    const { rawVars, substitution } = flagPrintf(line, tainted, sanitised);
    for (const v of rawVars) {
      if (!raw.has(v)) raw.set(v, []);
      (raw.get(v) as string[]).push(line.slice(0, 100));
    }
    if (substitution) rawSubstitutions.push(line.slice(0, 120));
  }
  return {
    logSites,
    raw,
    undeclared: [...raw.keys()].filter((v) => !(v in rawOk)).sort(),
    stale: Object.keys(rawOk)
      .filter((v) => !raw.has(v))
      .sort(),
    rawSubstitutions,
  };
}

/**
 * Tainted variables that may legally reach the job log RAW, each with the reason it cannot carry
 * `::`. Modelled on `app.css.accent-text-contrast.test.ts`'s `ICON_ROLES` (CLAUDE.md): the scan
 * below reds on any raw interpolation NOT declared here — so a new echo site fails the day it
 * lands, and claiming it is safe costs a reviewable diff with a reason — and equally on any entry
 * here the scan no longer finds, so an excuse written for one site cannot be inherited by another.
 *
 * Two limitations, stated rather than left for the next reader to discover:
 *   * The scan is flow-INSENSITIVE — an entry exempts the variable everywhere, not at one line.
 *     That is why the executed leg exists: it drives all three of these variables' paths with
 *     forged bytes in flight regardless of what is written here.
 *   * This is an exemption list, so a diff COULD add a fourth entry alongside the raw site it
 *     excuses. It is not registered as a ratchet (docs/design/RATCHETS.md) for the same reason
 *     `ICON_ROLES` is not: the exact-match assertion means growth is never silent — it is a named
 *     key with a prose reason in the diff, which is the reviewable artefact a ratchet row would be.
 */
const RAW_OK: Record<string, string> = {
  bound: "printed only after `catalog_lb_plain_u64` accepted it: digits only, no leading zero, <= 2^64-1",
  count: "clamped by `case \"$count\" in '' | *[!0-9]*) count='an unreportable number of'`",
  http: "curl's own `-w '%{http_code}'` formatting, never response bytes",
};

describe("no remote-influenced variable reaches the job log unsanitised (CPE-1951)", () => {
  it("every `printf … >&2` routes every tainted variable through catalog_lb_log_safe", () => {
    const lines = guardLogicalLines();
    // A parse that came back near-empty must fail loudly, not vacuously pass (CLAUDE.md:
    // "enumerate, don't recall" — and fail on a near-empty enumeration).
    expect(lines.length, "the guard script parsed to almost nothing").toBeGreaterThan(80);
    const { tainted, sanitised } = taintedVars(lines);
    // Parser self-check, NOT the property: if the taint derivation stops finding the variables the
    // script visibly assigns from gh/curl/jq, every assertion below goes vacuous.
    expect(
      [...tainted].sort(),
      "the taint derivation found no remote-assigned variables — the scan, not the script, is broken",
    ).toEqual(expect.arrayContaining(["api_out", "assets", "bound", "gh_err", "tag"]));
    expect([...sanitised]).toContain("curl_err");

    // NOTHING is re-implemented here. The taint pass, the `printf … >&2` selection, the loop over
    // `flagPrintf` and the exemption bookkeeping all live in `scanLogSites`, which the synthetic-
    // input test below drives with its own lines — so every step between "logical lines" and "the
    // triple this asserts" is covered by something other than the source it is pointed at. Round 4
    // had this loop and that test carrying two copies of the rule; round 5 shared the predicate and
    // left the loop unshared, which just moved the surviving mutant one line down. See
    // `scanLogSites`' docblock for the measurement.
    const { logSites, raw, undeclared, stale, rawSubstitutions } = scanLogSites(lines, RAW_OK);
    expect(logSites.length, "no `printf … >&2` found — the scan is broken").toBeGreaterThan(9);
    expect(
      { undeclared, stale, rawSubstitutions },
      "`undeclared` interpolate REMOTE bytes straight into the job log, where a forged " +
        "`\\n::stop-commands::` silences every annotation for the rest of the job — wrap each in " +
        '`"$(catalog_lb_log_safe "$VAR")"`, or add it to RAW_OK with the reason it cannot carry ' +
        "`::`. `stale` are RAW_OK entries the scan no longer finds: delete them, or the next raw " +
        "site inherits an excuse written for a different one. `rawSubstitutions` run a command " +
        "inline in a log line without the sanitiser wrapping the RESULT — wrap the whole thing: " +
        '`"$(catalog_lb_log_safe "$(cmd …)")"`. Sites found:\n' +
        [...raw].map(([v, ls]) => `  $${v}\n    ${ls.join("\n    ")}`).join("\n"),
    ).toEqual({ undeclared: [], stale: [], rawSubstitutions: [] });
  });

  it("...and that scan can actually SEE an inline substitution, a transitive alias, and prose", () => {
    // The rules above are satisfied by the script today — so on the real source they are
    // indistinguishable from doing nothing. This drives them with synthetic lines instead, through
    // `flagPrintf` AND through `scanLogSites`, the same two functions the scan above calls. Round 4
    // re-implemented the predicates here, so `if false && …` on the real scan left this test green
    // and the file passed at 79 while a live sanitiser was deleted from the exit-14 site.
    //
    // PASTED FROM THE RUNNER, not summarised. Round 5 recorded this red as
    // `expected [false,false,false,false] to deeply equal [false,true,false,true]`; the synthetic
    // set is SIX lines, not four, and vitest elides rather than prints them all. Re-measured at
    // round 6's head with the substitution predicate inside `flagPrintf` disabled
    // (`substitution: false && (…)`) and the live sanitiser deleted off the exit-14
    // `"$(catalog_lb_log_safe "$(cat "$jq_err")")"`, `npx vitest run` says, verbatim:
    //
    //     AssertionError: the default-deny `$(`/backtick predicate is not classifying the
    //     synthetic lines: expected [ false, false, false, false, …(2) ] to deeply equal
    //     [ Array(6) ]
    //
    // and with real jq on PATH it is the ONLY failure in the file: **1 failed / 82 passed**.
    // `logicalLines` first, so these go through the same stripper the real scan uses rather than a
    // second, kinder one.
    const synthetic = logicalLines(
      [
        "tag=$(jq -r '.tag_name' <<< \"$body\")",
        "rel_name=\"$tag\"",
        // (0) a tainted variable, no substitution -> rawVars only.
        "printf 'release %s\\n' \"$rel_name\" >&2",
        // (1) an inline substitution, no tainted variable -> substitution only.
        "printf 'jq said %s\\n' \"$(cat \"$jq_err\")\" >&2",
        // (2) must NOT be reported: a sanitiser-wrapped inline call, plus prose mentioning `$(` and
        // a backtick inside a single-quoted format string.
        "printf 'ok `cmd` $(cat x) %s\\n' \"$(catalog_lb_log_safe \"$(cat \\\"$jq_err\\\")\")\" >&2",
        // (3) must BE reported. Round 4 read this as clean: `blankSanitiserCalls` searched for the
        // bare name ANYWHERE in the line and blanked back to the nearest preceding `$(`, so naming
        // the sanitiser in a trailing prose argument swallowed a real `$(cat …)` in front of it —
        // a bypass of the headline rule achievable by rewording a message.
        "printf '%s\\n' \"$(cat /etc/passwd)\" 'per catalog_lb_log_safe policy' >&2",
        // (4) the same mention in a DOUBLE-quoted argument, where `dropSingleQuoted` cannot reach
        // it. Closed by the `$(catalog_lb_log_safe` anchor alone — with the round-4 bare-name
        // search this reds and case (3) does not, which is how the two fixes are told apart.
        "printf '%s\\n' \"$(cat /etc/passwd)\" \"per catalog_lb_log_safe policy\" >&2",
        // (5) a mention that opens `$(catalog_lb_log_safe` inside prose and never closes it: the
        // paren matcher runs off the end of the line and blanks everything after it, real
        // substitution included. Closed by running `dropSingleQuoted` FIRST alone — the anchor
        // matches this one, so ordering is what saves it.
        "printf 'see $(catalog_lb_log_safe %s\\n' \"$(cat /etc/passwd)\" >&2",
      ].join("\n"),
    );
    const { tainted, sanitised } = taintedVars(synthetic);
    expect(tainted, "transitive taint (`rel_name=\"$tag\"`) is not being followed").toContain("rel_name");

    const printfs = synthetic.filter((l) => /^printf\b/.test(l) && />&2\s*$/.test(l));
    expect(printfs.length, "the synthetic lines did not survive `logicalLines`").toBe(6);
    const verdicts = printfs.map((l) => flagPrintf(l, tainted, sanitised));
    // Asserted SEPARATELY per predicate, not as one `flagged` count: a count cannot tell which of
    // the two rules answered, so a count is green when one rule dies and the other happens to fire.
    expect(
      verdicts.map((v) => v.rawVars),
      "the tainted-variable predicate is not classifying the synthetic lines",
    ).toEqual([["rel_name"], [], [], [], [], []]);
    expect(
      verdicts.map((v) => v.substitution),
      "the default-deny `$(`/backtick predicate is not classifying the synthetic lines",
    ).toEqual([false, true, false, true, true, true]);

    // ...and the SAME lines through `scanLogSites`, which is what the real scan actually calls.
    // Driving `flagPrintf` alone covers the predicates and leaves their CONSUMER — the selection
    // filter, the loop, `raw`/`undeclared`/`stale`/`rawSubstitutions` — answered for only by the
    // source it is pointed at, where every value is already legal. That is exactly where round 5's
    // surviving mutant went (`scanLogSites`' docblock has the numbers). Asserted as the triple the
    // real scan compares, with an EMPTY exemption list so `undeclared` is not silently excused:
    // `rel_name` is the tainted variable reaching the log raw, four of the six lines run a
    // substitution, and `stale` is empty because nothing is exempted.
    const scan = scanLogSites(synthetic, {});
    expect(
      {
        logSites: scan.logSites.length,
        undeclared: scan.undeclared,
        stale: scan.stale,
        subs: scan.rawSubstitutions.length,
      },
      "the scan's own loop — selection, accumulation and exemption bookkeeping — is not carrying " +
        "`flagPrintf`'s verdicts through to the triple the real assertion compares",
    ).toEqual({ logSites: 6, undeclared: ["rel_name"], stale: [], subs: 4 });
    // ...and `stale` really does report an exemption nothing matches, rather than being structurally
    // empty: an entry for a variable this input never logs raw must come back in `stale`.
    expect(
      scanLogSites(synthetic, { bound: "not present in these synthetic lines" }).stale,
      "`stale` is not reporting an exemption the scan no longer finds",
    ).toEqual(["bound"]);
  });

  it("...and the taint pass follows the ordinary shell shapes, not just the first `=` on a line", () => {
    // Ordinary shell shapes the pass walked straight through, all of them plain shell and none of
    // them on its stated blind-spot list. Each is a `printf … >&2` carrying remote bytes that the
    // scan reported as clean. Driven synthetically because the script contains none of them today —
    // which is the whole reason they were invisible.
    //
    // MEASURED, not counted from memory. Round 5's comment here said "four shapes the round-4 pass
    // walked straight through", which understated its own result: under a faithful round-4-
    // equivalent revert of the loop in `taintedVars` (first `=$(` match only, `continue` past the
    // rest of the line, no `FILLED`, no `+=`, no non-tool-substitution fallthrough) ALL SIX of the
    // shapes that existed then red at once —
    //     expected { …(6) } to deeply equal { …(6) }
    //     received every one of the six as `Array []`
    // i.e. "laundered through a non-tool substitution" and "append onto a SANITISED name" were
    // walked through too. The `IFS=: read with TWO targets` case is round 6's, and reds against
    // round 5's own `FILLED`, which captured one target per `read`. The rows from
    // `a remedy MESSAGE naming the sanitiser` down are round 7's: the first two are the two
    // directions of `taintedVars` not stripping single-quoted prose (one fail-OPEN, one a false
    // positive), and the rest are the shapes bash's `read`/`mapfile` grammar accepts that round 6's
    // regex bound wrongly or not at all. They are rows and not a widened sentence on purpose —
    // three rounds running, a universal written at this pass was falsified by an ordinary shape
    // within a day, so the next widening gets scored against a table rather than asserted in prose.
    // Which is exactly what happened to round 7: the last seven rows are round 8's, and five of them
    // are shapes round SIX bound and round 7 lost while widening — the trade nobody scored, found by
    // running the old matcher's cases through the new one. Every row's `want` was checked against
    // real bash (5.3.15) before it was written down, over-reports included and labelled as such.
    const cases: { label: string; lines: string[]; want: string[] }[] = [
      {
        label: "two assignments on one line, second one ignored",
        lines: ["tag=$(jq -r .t x)", 'local a="$tag" b="$tag"', 'printf \'%s\\n\' "$b" >&2'],
        want: ["b"],
      },
      {
        label: "a plain assignment sharing a line with a `=$(` one",
        lines: ["tag=$(jq -r .t x)", 'local a="$tag" b=$(date)', 'printf \'%s\\n\' "$a" >&2'],
        want: ["a"],
      },
      {
        label: "append",
        lines: ["tag=$(jq -r .t x)", 'msg+="$tag"', "printf '%s\\n' \"$msg\" >&2"],
        want: ["msg"],
      },
      {
        label: "`read` with no `=` on its left",
        lines: ['while IFS= read -r asset_name; do :; done <<< "$assets"', "printf '%s\\n' \"$asset_name\" >&2"],
        want: ["asset_name"],
      },
      {
        // Round 6. `read` splits its input across a LIST of names; round 5's `FILLED` captured the
        // first and stopped, so the SECOND target of a live `read` was clean while the comment at
        // the regex said every target is tainted "UNCONDITIONALLY".
        label: "IFS=: read with TWO targets",
        lines: [
          'IFS=: read -r name size <<< "$assets_line"',
          "assets_line=$(jq -r .a x)",
          "printf '%s\\n' \"$size\" >&2",
        ],
        want: ["size"],
      },
      {
        label: "laundered through a non-tool substitution",
        lines: ["tag=$(jq -r .t x)", 'washed=$(printf \'%s\' "$tag")', 'printf \'%s\\n\' "$washed" >&2'],
        want: ["washed"],
      },
      {
        label: "append onto a SANITISED name re-opens it",
        lines: [
          "tag=$(jq -r .t x)",
          'safe=$(catalog_lb_log_safe "$tag")',
          'safe+="$tag"',
          "printf '%s\\n' \"$safe\" >&2",
        ],
        want: ["safe"],
      },
      {
        // Round 7, and the reason the round exists: `flagPrintf` ran `dropSingleQuoted` and
        // `taintedVars` did not, one function away. The sanitiser is named in PROSE, inside a
        // single-quoted `printf` format string offering the remedy — and that put `safe` into
        // `sanitised`, so the real `safe="$tag"` below it and its log site came back clean. The
        // same bypass-by-rewording that cases (3)/(4) above record round 4 as closing for
        // `blankSanitiserCalls`. Reproduced by the round-7 reviewer, verbatim.
        label: "a remedy MESSAGE naming the sanitiser marks a tainted name clean",
        lines: [
          "tag=$(jq -r .t x)",
          "printf 'remedy: wrap it as safe=$(catalog_lb_log_safe \"$VAR\") before logging\\n' >&2",
          'safe="$tag"',
          "printf '%s\\n' \"$safe\" >&2",
        ],
        want: ["safe"],
      },
      {
        // The other direction of the same omission — a FALSE POSITIVE, so it reds as a name the
        // scan reports rather than one it misses. `catalog-lower-bound.sh:383` says "Refusing to
        // read an unenumerable release as …" in a single-quoted format string, and round 6's
        // widened name list read `an unenumerable release as` as four `read` targets. Measured on
        // the real script: the live taint set was 12 real names at round 5 and at round 7, and 14
        // / 16 in between (round 5 spuriously held `an` and `as`; round 6 added `release` and
        // `unenumerable`). A local literal reported as carrying remote bytes.
        label: "prose inside a single-quoted printf is not a `read`",
        lines: [
          "printf 'Refusing to read an unenumerable release as \"publishes no catalog\"\\n' >&2",
          "release=v1.2.3",
          "printf 'built %s\\n' \"$release\" >&2",
        ],
        want: [],
      },
      // ── Round 7: the four shapes bash accepts that round 6's `FILLED` regex bound wrongly or not
      // at all, while the comment at it said "EVERY name after the options … UNCONDITIONALLY". They
      // are rows rather than a widened sentence so the NEXT widening is scored rather than asserted.
      {
        label: "`read --` ends the options",
        lines: ["read -- name < <(jq -r .n x)", "printf '%s\\n' \"$name\" >&2"],
        want: ["name"],
      },
      {
        label: "`read -p` takes a QUOTED prompt, so the target is two words along",
        lines: ['read -p "Enter release: " x', "printf '%s\\n' \"$x\" >&2"],
        want: ["x"],
      },
      {
        label: "a `read` with no name at all binds the implicit REPLY",
        lines: ['read -r <<< "$(jq -r .n x)"', "printf '%s\\n' \"$REPLY\" >&2"],
        want: ["REPLY"],
      },
      {
        // Round 6 reported `["mycb"]` here — the CALLBACK, not the array. Both halves wrong: the
        // real target untainted, a function name tainted instead.
        label: "`mapfile -C cb -c n arr` binds the array, not the callback",
        lines: ["mapfile -C mycb -c 2 arr < f", "printf '%s\\n' \"${arr[0]}\" >&2"],
        want: ["arr"],
      },
      {
        // Not a round-6 miss — it answered `["arr"]`, but only because it read `-a` as an option
        // and `arr` as a trailing name. `-a aname` is the one option ARGUMENT that is itself a
        // target, so `filledTargets` says so explicitly; this row is that clause's red-proof.
        label: "`read -a` takes the array as its option ARGUMENT",
        lines: ['read -r -a arr <<< "$(jq -r .n x)"', "printf '%s\\n' \"${arr[0]}\" >&2"],
        want: ["arr"],
      },
      // ── Round 8: the OTHER direction of round 7's widening. Round 6 anchored `FILLED` on
      // `(?:^|[\s!(){};|])`, so it saw the builtin after a metacharacter with no space; round 7's
      // whole-word walk did not, and these five went from caught to `[]` in the diff that fixed the
      // four above. They are rows because a widening is a trade until both directions are scored,
      // and this half went unscored for a round.
      //
      // ROUND 9 CORRECTION, and it is a claim defect of the class rounds 4–8 have been closing.
      // Round 8 wrote "every one BINDS in bash 5.3.15 (measured, not read off the man page)" over
      // all five. Re-measured with `declare -p` in the ENCLOSING shell, THREE of them bind nothing
      // there — the RHS of a pipeline and an explicit `( … )` each run in their own subshell, and
      // `lastpipe` is off (the guard script never sets it):
      //     printf … "$(jq …)"|read -r leaked   -> `leaked: not found`
      //     (read -r x < <(jq …))               -> `x: not found`
      //     jq … |mapfile -t arr2               -> `arr2: not found`
      // So the widening buys TWO genuine catches, the `;` and the `{` rows, and those three belong
      // beside `read -- -x` in the OVER-reported / fail-CLOSED bullet of `filledTargets`' docblock,
      // not among shapes round 7 "lost". The rows STAY, at the same `want`, because reporting a
      // name a subshell binds is fail-closed noise and pinning it is how the next reader learns the
      // direction — but the labels now say which half each is in. What makes this the same defect:
      // `filledTargets`' own docblock reasons correctly about identical scoping a few lines away
      // (`v=$(read -r x)` "binds x only inside the substitution's own subshell"); the pipe and the
      // parens simply did not get that treatment.
      {
        label: "a `read` on the far side of a PIPE, no space — OVER-reported, the subshell eats it",
        lines: ["printf '%s\\n' \"$(jq -r .t x)\"|read -r leaked", "printf '%s\\n' \"$leaked\" >&2"],
        want: ["leaked"],
      },
      {
        label: "the SECOND `read` of two on one line, separated by `;` with no space — BINDS",
        lines: ["read -r a;read -r b < <(jq -r .n x)", "printf '%s\\n' \"$b\" >&2"],
        want: ["b"],
      },
      {
        label: "a `read` opening a SUBSHELL, `(` hard against it — OVER-reported, nothing survives it",
        lines: ["(read -r x < <(jq -r .n f))", "printf '%s\\n' \"$x\" >&2"],
        want: ["x"],
      },
      {
        label: "`mapfile` on the far side of a pipe, no space — OVER-reported, the subshell eats it",
        lines: ["jq -r .n f|mapfile -t arr2", "printf '%s\\n' \"${arr2[0]}\" >&2"],
        want: ["arr2"],
      },
      {
        label: "two `read`s in a BRACE GROUP, `{` and `;` hard against them — BINDS",
        lines: ["{ read -r p;read -r q; }", "printf '%s\\n' \"$q\" >&2"],
        want: ["q"],
      },
      // ── Round 8, the two over-reports the round-7 `-a` clause introduced or left. Both are
      // fail-CLOSED, so they are recorded rather than tolerated silently: the first is FIXED here
      // and this row is that fix's red-proof, the second is deliberately kept.
      {
        // `-ar` is `-a` with the INLINE argument `r`, so bash binds the array `r` and ignores the
        // trailing `arr` entirely (measured: `r=<a>`, `arr=UNSET`). Round 7 answered `["r","arr"]`.
        // Both names are logged, and only `r` may come back.
        label: "`read -ar arr` binds `r`, and `arr` is NOT a second target",
        lines: [
          'read -ar arr <<< "$(jq -r .n x)"',
          "printf '%s\\n' \"${r[0]}\" >&2",
          "printf '%s\\n' \"${arr[0]}\" >&2",
        ],
        want: ["r"],
      },
      {
        // bash refuses this outright — `read: `-x': not a valid identifier`, status 1, nothing
        // bound — and the walk still reports the `REPLY` fallback. Kept, because over-reporting a
        // name that cannot carry remote bytes costs a false positive at worst, while teaching this
        // walk bash's identifier validation costs a second grammar. The row pins the direction so
        // the next reader does not mistake it for a claim that bash accepts the line.
        label: "a `read` bash REJECTS still reports REPLY — fail-closed, on purpose",
        lines: ["read -- -x < <(jq -r .n f)", "printf '%s\\n' \"$REPLY\" >&2"],
        want: ["REPLY"],
      },
      // ── Round 9: the trade round 8 made and did not score, which is the THIRD consecutive round
      // to trade one class of shape for another and the first to do it in the round that wrote the
      // rule against it. Round 8 added `{`, `}`, `(`, `)` to `SHELL_META` and made the operand walk
      // break on any of them — so an unquoted `${…}` / `$((…))` ANYWHERE in the command before the
      // target truncates the walk and the `REPLY`/`MAPFILE` fallback comes back instead. Round 7's
      // whole-word regex bound every one of these. Each was run through bash 5.3.15 with
      // `declare -p` first (all eleven bind in the enclosing shell) and then through both matchers:
      //     line                              r7          r8(HEAD)
      //     mapfile -d \; -t arr9 < f         ["arr9"]    ["MAPFILE"]
      //     read -d \; -r d9                  ["d9"]      ["REPLY"]
      //     read -p ${msg} p9                 ["p9"]      ["REPLY"]
      //     read -n $((n)) n9                 ["n9"]      ["REPLY"]
      //     read -t ${tmo} t9                 ["t9"]      ["REPLY"]
      //     mapfile -n ${cnt} -t m9 < f       ["m9"]      ["MAPFILE"]
      //     read -u ${fd} u9                  ["u9"]      ["REPLY"]
      //     read -N ${k} k9                   ["k9"]      ["REPLY"]
      //     read -d ${sep} s9                 ["s9"]      ["REPLY"]
      //     read -N${k} i4                    ["i4"]      ["REPLY"]
      // Mechanism, taking the `-p` row: `shellTokens` emitted `["read","-p","$","{","msg","}","p9"]`,
      // `-p` consumed the `$`, and the walk hit `{` and broke before ever reaching `p9`.
      //
      // These ten are rows for the same reason the previous ten are, but the ROWS ARE NO LONGER THE
      // COVERAGE — an enumeration is a list of what someone thought to write down, and the trade
      // keeps hiding in what nobody did. `…scored against BASH ITSELF over a GENERATED corpus…`
      // below is the leg that does not depend on anyone having written the case.
      {
        label: "`mapfile -d \\;` — an ESCAPED metacharacter is not a metacharacter",
        lines: ["mapfile -d \\; -t arr9 < <(jq -r .n f)", "printf '%s\\n' \"${arr9[0]}\" >&2"],
        want: ["arr9"],
      },
      {
        label: "`read -d \\;` — same, with the option before the `-r` flag",
        lines: ['read -d \\; -r d9 <<< "$(jq -r .n x)"', "printf '%s\\n' \"$d9\" >&2"],
        want: ["d9"],
      },
      {
        label: "`read -p ${msg}` — a BRACED expansion as the option argument",
        lines: ['read -p ${msg} p9 <<< "$(jq -r .n x)"', "printf '%s\\n' \"$p9\" >&2"],
        want: ["p9"],
      },
      {
        label: "`read -n $((n))` — an ARITHMETIC expansion as the option argument",
        lines: ['read -n $((n)) n9 <<< "$(jq -r .n x)"', "printf '%s\\n' \"$n9\" >&2"],
        want: ["n9"],
      },
      {
        label: "`read -t ${tmo}` — a braced expansion on the timeout",
        lines: ['read -t ${tmo} t9 <<< "$(jq -r .n x)"', "printf '%s\\n' \"$t9\" >&2"],
        want: ["t9"],
      },
      {
        label: "`mapfile -n ${cnt}` — a braced expansion, and the array is two words further on",
        lines: ["mapfile -n ${cnt} -t m9 < <(jq -r .n f)", "printf '%s\\n' \"${m9[0]}\" >&2"],
        want: ["m9"],
      },
      {
        label: "`read -u ${fd}` — a braced expansion on the fd",
        lines: ["read -u ${fd} u9 < <(jq -r .n f)", "printf '%s\\n' \"$u9\" >&2"],
        want: ["u9"],
      },
      {
        label: "`read -N ${k}` — a braced expansion on the exact-character count",
        lines: ['read -N ${k} k9 <<< "$(jq -r .n x)"', "printf '%s\\n' \"$k9\" >&2"],
        want: ["k9"],
      },
      {
        label: "`read -d ${sep}` — a braced expansion on the delimiter",
        lines: ['read -d ${sep} s9 <<< "$(jq -r .n x)"', "printf '%s\\n' \"$s9\" >&2"],
        want: ["s9"],
      },
      {
        label: "`read -N${k}` — a braced expansion BUNDLED onto the option, no space",
        lines: ['read -N${k} i4 <<< "$(jq -r .n x)"', "printf '%s\\n' \"$i4\" >&2"],
        want: ["i4"],
      },
      {
        // Not a round-8 regression — round 7 answered `["REPLY"]` here too, and neither round's
        // blind-spot list named it. bash binds `quoted` (measured); the walk found no leading
        // identifier in the token `"quoted"` because the quote is still on the front of it.
        label: "a QUOTED operand is still a name — `read -r \"quoted\"`",
        lines: ['read -r "quoted" <<< "$(jq -r .n x)"', "printf '%s\\n' \"$quoted\" >&2"],
        want: ["quoted"],
      },
      {
        // The other half of round 9's `filledTargets` change, and the reason it is not merely
        // cosmetic: `-d` here has NO argument (the `;` is a metacharacter that ends the command),
        // so bash rejects the line — `read: -d: option requires an argument` — and binds nothing.
        // Reporting `x` would be reporting a name that cannot exist. `REPLY` is the fail-closed
        // fallback, exactly as for `read -- -x`.
        label: "an option whose argument ran into a metacharacter binds no operand",
        lines: ["read -r -d ; x < <(jq -r .n f)", "printf '%s\\n' \"$x\" >&2"],
        want: [],
      },
      {
        // The DOUBLE-quote twin of the `fail-OPEN branch` test's unterminated `'`, and the same
        // argument: round 7's regex and round 8's loop both swallowed to end of line, so everything
        // after a stray `"` — including a live `read` — was hidden from a default-DENY scan. That is
        // fail-open, not "merely safer". `logicalLines` joins `\`-continuations but NOT a
        // double-quoted string spanning physical lines, so a logical line ending inside `"` is a
        // shape a real script can produce. Treating the quote as an ordinary character over-reports
        // in that case (fail-CLOSED) and hides nothing, which is the direction to be wrong in.
        // The blind-spot list at `taintedVars` named only the unterminated SINGLE quote.
        label: "an unterminated `\"` does not swallow the rest of the line",
        lines: ['msg=" ; read -r sneaky < <(jq -r .n f)', "printf '%s\\n' \"$sneaky\" >&2"],
        want: ["sneaky"],
      },
    ];
    const got = cases.map(({ lines }) => {
      const parsed = logicalLines(lines.join("\n"));
      const { tainted, sanitised } = taintedVars(parsed);
      const printfs = parsed.filter((l) => /^printf\b/.test(l) && />&2\s*$/.test(l));
      return printfs.flatMap((l) => flagPrintf(l, tainted, sanitised).rawVars);
    });
    expect(
      Object.fromEntries(cases.map((c, i) => [c.label, got[i]])),
      "a `printf … >&2` carrying remote bytes was classified clean — the taint pass is narrower " +
        "than the sentence describing it",
    ).toEqual(Object.fromEntries(cases.map((c) => [c.label, c.want])));
  });

  /**
   * THE GENERATED CORPUS, and why the rows above stopped being the coverage.
   *
   * Rounds 6→7 and 7→8 each replaced this walk's tokenizer, each verified the shapes it now got
   * right, and each SILENTLY LOST a class it used to get right. Round 8 wrote the rule — "when you
   * replace a matcher, run the PREVIOUS matcher's cases through the new one and say what moved, in
   * both directions" — and then broke it in the same diff, because it ran the previous rounds'
   * ENUMERATED ROWS rather than a sweep, and `${…}` in an option argument was on nobody's list.
   * Three rounds, the same trade, the third inside the round that named it.
   *
   * A row is a thing someone thought to write down. So the coverage is a corpus GENERATED from the
   * grammar instead: every argument-taking option of `read` and of `mapfile`, its argument spelled
   * bare / "quoted" / `${…}` / `$((…))` / `\escaped` / bundled-inline, in each of five shell
   * contexts (plain, after `;`, in a `{ … }` group, on the far side of a pipe, inside `( … )`).
   *
   * The ORACLE IS BASH, not a hand-written expectation column — the shared-case-file trap
   * (CLAUDE.md, CPE-1950) is exactly that a case nobody thought of is absent from the file and both
   * sides answer it the same wrong way. Every generated line is run, and `declare -p` in the
   * ENCLOSING shell says whether the name is bound there. That is also what caught round 8's other
   * claim: the pipeline and `( … )` contexts bind NOTHING in the enclosing shell, so a matcher that
   * reports them is over-reporting, not catching.
   *
   * Two assertions, both empty-list, neither a stored count:
   *   1. FAIL-OPEN is `[]` — bash bound a name and the walk missed it. This is the direction that
   *      matters, and it is what all three rounds' trades landed in.
   *   2. Every OVER-report sits in a context bash cannot bind in at all. Over-reporting is
   *      fail-closed noise and is tolerated, but only where the subshell explains it.
   * Plus the corpus's own self-check, which is not `length > N`: the round-7 and round-8 tokenizers
   * are kept above and scored over the same corpus, and each must fail open on lines the OTHER
   * handles. A corpus that cannot still see that trade is measuring nothing, and `failOpen: []`
   * over it would be vacuous.
   *
   * Measured on this machine, bash 5.3.15, 395 generated lines (295 of which bash binds):
   *     tokenizer        fail-open   over-report
   *     round 7             75            0
   *     round 8 (HEAD)      75          100
   *     round 9 (this)       0          150
   * with 50 lines round 7 loses that round 8 catches and 50 the other way — the trade, symmetric,
   * and reproduced by the harness rather than recalled. Round 9's 150 over-reports are all pipeline
   * or `( … )` contexts, i.e. assertion 2.
   */
  interface ShapeCase {
    id: number;
    line: string;
    target: string;
    varName: string;
    varValue: string;
    /** Whether the ENCLOSING shell can see a binding at all — false for a pipeline RHS and `( … )`.
     *  Declared by construction here and CONFIRMED by the oracle, never trusted on its own. */
    binds: boolean;
  }
  /** Every argument-taking option of each builtin, with a value bash accepts and whether that value
   *  is an arithmetic expression (so the `$((…))` spelling is legal for it). From bash(1)'s grammar,
   *  the same source `filledTargets` walks — a divergence between the two is a bug in one of them. */
  const CORPUS_OPTS: Record<string, [string, string, boolean][]> = {
    read: [
      ["d", ";", false],
      ["i", "txt", false],
      ["n", "3", true],
      ["N", "2", true],
      ["p", "P:", false],
      ["t", "5", true],
      ["u", "0", true],
    ],
    mapfile: [
      ["d", ";", false],
      ["n", "1", true],
      ["O", "0", true],
      ["s", "0", true],
      ["u", "0", true],
      ["C", "cb", false],
      ["c", "1", true],
    ],
  };
  const CORPUS_CONTEXTS: { id: string; binds: boolean; piped: boolean; wrap: (c: string) => string }[] = [
    { id: "plain", binds: true, piped: false, wrap: (c) => c },
    { id: "semi", binds: true, piped: false, wrap: (c) => `:;${c}` },
    { id: "brace", binds: true, piped: false, wrap: (c) => `{ ${c}; }` },
    { id: "pipe", binds: false, piped: true, wrap: (c) => `printf 'hello world\\n'|${c}` },
    { id: "subshell", binds: false, piped: false, wrap: (c) => `(${c})` },
  ];
  function shapeCorpus(): ShapeCase[] {
    const cases: ShapeCase[] = [];
    for (const cmd of ["read", "mapfile"]) {
      const flag = cmd === "read" ? "-r" : "-t";
      for (const [opt, val, numeric] of CORPUS_OPTS[cmd]) {
        for (const spelling of ["bare", "quoted", "braced", "arith", "escaped", "inline"]) {
          if (spelling === "arith" && !numeric) continue;
          for (const ctx of CORPUS_CONTEXTS) {
            const id = cases.length;
            const target = `t${id}`;
            const varName = `v${id}`;
            const arg =
              spelling === "bare"
                ? `-${opt} ${val}`
                : spelling === "quoted"
                  ? `-${opt} "${val}"`
                  : spelling === "braced"
                    ? `-${opt} \${${varName}}`
                    : spelling === "arith"
                      ? `-${opt} $((${val}))`
                      : spelling === "escaped"
                        ? `-${opt} \\${val}`
                        : `-${opt}${val}`;
            // A piped command reads the pipe; everything else needs its own input or `read` blocks.
            const input = ctx.piped ? "" : cmd === "read" ? ' <<< "hello world"' : ' < "$CORPUS_FILE"';
            cases.push({
              id,
              line: ctx.wrap(`${cmd} ${flag} ${arg} ${target}${input}`),
              target,
              varName,
              varValue: val,
              binds: ctx.binds,
            });
          }
        }
      }
    }
    return cases;
  }
  /** bash's own answer, one process for the whole corpus: is `target` bound in the ENCLOSING shell
   *  after the line runs? Each case unsets its own uniquely-named target first, so nothing carries.
   *  This EXECUTES; it answers nothing by reading. */
  function bashBinds(cases: ShapeCase[]): Map<number, boolean> {
    const script = [
      "cb() { :; }",
      'CORPUS_FILE="$(mktemp)"',
      "printf 'a;b;c\\nd\\n' > \"$CORPUS_FILE\"",
      ...cases.map((c) => `${c.varName}='${c.varValue}'`),
      ...cases.map(
        (c) =>
          `{ unset ${c.target}; ${c.line}; } >/dev/null 2>&1\n` +
          `if declare -p ${c.target} >/dev/null 2>&1; then printf '${c.id}=1\\n'; else printf '${c.id}=0\\n'; fi`,
      ),
      'rm -f "$CORPUS_FILE"',
      "",
    ].join("\n");
    const r = spawnSync("bash", [], { input: script, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
    if (r.status !== 0 || !r.stdout) {
      throw new Error(`the bash oracle did not run (status ${r.status}): ${r.stderr ?? r.error}`);
    }
    const answered = new Map<number, boolean>();
    for (const l of r.stdout.split("\n")) {
      const m = /^(\d+)=([01])$/.exec(l.trim());
      if (m) answered.set(Number(m[1]), m[2] === "1");
    }
    return answered;
  }

  it("...and that walk is scored against BASH ITSELF over a GENERATED corpus, both directions", () => {
    requireBash();
    const cases = shapeCorpus();
    expect(cases.length, `the generator produced ${cases.length} lines — the generator is broken`).toBeGreaterThan(300);
    const oracle = bashBinds(cases);
    expect(
      oracle.size,
      `the oracle answered for ${oracle.size} of ${cases.length} lines — a partial run is "did not run"`,
    ).toBe(cases.length);
    const boundCount = [...oracle.values()].filter(Boolean).length;
    expect(
      boundCount,
      `bash bound a name on only ${boundCount} of ${cases.length} generated lines. An oracle that ` +
        "says 'unbound' to everything makes every fail-open assertion below vacuous.",
    ).toBeGreaterThan(cases.length / 2);

    const score = (tokens: (l: string) => string[]) => {
      const failOpen: ShapeCase[] = [];
      const overReport: ShapeCase[] = [];
      for (const c of cases) {
        const reported = filledTargets(c.line, tokens).includes(c.target);
        if (oracle.get(c.id) && !reported) failOpen.push(c);
        else if (!oracle.get(c.id) && reported) overReport.push(c);
      }
      return { failOpen, overReport };
    };
    const now = score(shellTokens);
    const r7 = score(tokensRound7);
    const r8 = score(tokensRound8);

    // The corpus's own sensitivity check, and it is the reason the two old tokenizers are kept:
    // each must fail open on lines the other handles. If this ever goes one-sided the corpus has
    // stopped covering the defect class, and the two empty-list assertions below mean nothing.
    const r8Ids = new Set(r8.failOpen.map((c) => c.id));
    const r7Ids = new Set(r7.failOpen.map((c) => c.id));
    const onlyR7 = r7.failOpen.filter((c) => !r8Ids.has(c.id));
    const onlyR8 = r8.failOpen.filter((c) => !r7Ids.has(c.id));
    expect(
      { r7LosesWhatR8Catches: onlyR7.length > 0, r8LosesWhatR7Catches: onlyR8.length > 0 },
      "this corpus no longer reproduces the round-7/round-8 TRADE in both directions " +
        `(round 7 fails open on ${r7.failOpen.length} lines, round 8 on ${r8.failOpen.length}, ` +
        `${onlyR7.length} / ${onlyR8.length} of them exclusive). It was generated to cover exactly ` +
        "that class of defect, so a one-sided answer means the generator narrowed and the two " +
        "assertions below have gone vacuous — widen the generator, do not relax this.",
    ).toEqual({ r7LosesWhatR8Catches: true, r8LosesWhatR7Catches: true });

    // 1. The direction that matters. Every previous round's trade landed here.
    expect(
      now.failOpen.map((c) => c.line).sort(),
      "bash BINDS a name on these generated lines and `filledTargets` did not report it. That is " +
        "fail-OPEN: a variable filled with remote bytes is never added to the taint set, so a " +
        `\`printf … >&2\` interpolating it is reported clean. Scored over ${cases.length} lines; ` +
        `for comparison the round-7 tokenizer fails open on ${r7.failOpen.length} of them and the ` +
        `round-8 one on ${r8.failOpen.length}. Fix the tokenizer or the option grammar — do not ` +
        "add the shape to a row list, which is what let this recur three rounds running.",
    ).toEqual([]);

    // 2. The other direction, scored rather than waved at. Over-reporting is fail-CLOSED and is
    //    tolerated, but only where a subshell explains it.
    expect(
      now.overReport.filter((c) => c.binds).map((c) => c.line).sort(),
      "`filledTargets` reported a target on these lines and bash bound nothing — in a context where " +
        "bash CAN bind, so a subshell does not explain it. Over-reporting is fail-closed and cheap, " +
        `but an unexplained one means the grammar walk is wrong (${now.overReport.length} of ` +
        `${cases.length} lines over-report in total, all of them pipeline or \`( … )\` contexts).`,
    ).toEqual([]);
  });

  it("...and no logical line of the script ever reaches the stripper's fail-OPEN branch", () => {
    // `dropSingleQuoted` consumes an unterminated `'` to end of line, matching the shell. Round 4's
    // comment called that "the fail-CLOSED direction … it can only hide code, never invent it" —
    // backwards. For a default-DENY detector, hidden text is fail-OPEN: a `$(` swallowed by a
    // phantom span is a substitution that is never flagged. Rather than reason about whether that
    // can happen here, measure it — every logical line of the real script is re-lexed and none may
    // end inside a single-quoted span. `logicalLines` has already joined `\`-continuations, so a
    // quote legitimately spanning physical lines is one logical line and closes within it.
    const lines = guardLogicalLines();
    expect(lines.length, "the guard script parsed to almost nothing").toBeGreaterThan(80);
    expect(
      lines.filter(endsInsideSingleQuote),
      "these logical lines end inside an unterminated single quote, so `dropSingleQuoted` deletes " +
        "the rest of the line — anything after the quote, including a real `$(`, is hidden from " +
        "the default-deny rule rather than judged by it",
    ).toEqual([]);
  });

  it("...and its quote stripper survives the `'\"'\"'` apostrophe idiom this script uses", () => {
    // The shape at the exit-3 message: an apostrophe inside a single-quoted format string, spelled
    // the only way sh allows. `/'[^']*'/g` pairs these WRONG and lets the prose after them — which
    // here contains a backtick — out into the scanned text, where the rule above reads it as a
    // substitution. Nothing in the script trips that today, which is exactly why it is pinned here
    // rather than left to the parity of apostrophes in a message someone may reword.
    const q = "'";
    const line =
      `printf ${q}::error::published on %s${q}"${q}"${q}s latest release. ` +
      `(ApplyOutcome::Rollback) \`git tag\` on a non-tip commit. %s\\n${q} "$a" "$b" >&2`;
    const stripped = dropSingleQuoted(line);
    expect(stripped, "prose leaked out of the single-quoted format string").not.toContain("`");
    expect(stripped).not.toContain("ApplyOutcome");
    // The double-quoted arguments must survive — they are what the taint scan reads.
    expect(stripped).toContain('"$a"');
    expect(stripped).toContain(">&2");
    // Sanity: a REAL inline substitution in a double-quoted argument is not stripped.
    expect(dropSingleQuoted(`printf ${q}%s${q} "$(cat f)" >&2`)).toContain("$(cat f)");
  });

  it("...and every shell scanner in this file is a row in the table above", () => {
    // Round 8, MINOR 4, and the reviewer's own note that the ask was half the defect: round 7 was
    // asked for three rows and filled in three, while `collapseExpansions` and `blankSanitiserCalls`
    // were already in the file and already qualified. **A count cannot notice what was never
    // counted**, so the table's admission rule is a PREDICATE applied here — by the same grep the
    // table names, against this file's own source — rather than a closing sentence promising that
    // the next person will remember. Both halves are derived: the rows come out of the table's own
    // text, the candidates out of the file's own `function` declarations, and neither is restated.
    const self = readFileSync(join(__dirname, "catalogPublishLowerBound.test.ts"), "utf8");

    // The table's first column, read out of the table (CLAUDE.md: derive provenance, don't claim
    // it). `(+ scanLogSites)` is a continuation row and counts as its own name.
    const from = /^\/\/ {3}fn +\| 1\. strips/m.exec(self);
    const to = /^\/\/ {3}fn +\| 3\. its CONSUMER/m.exec(self);
    expect(from && to, "the scanner table's two headers are gone or were reworded").toBeTruthy();
    const tableText = self.slice((from as RegExpExecArray).index, (to as RegExpExecArray).index);
    const rows = [...tableText.matchAll(/^\/\/ {3}(?:\(\+ )?([A-Za-z][A-Za-z0-9]*)\)? +\|/gm)]
      .map((m) => m[1])
      .filter((n) => n !== "fn");
    expect(rows.length, `the table parsed to ${rows.length} rows — the parse, not the table, is broken`).toBeGreaterThan(6);

    // The two functions question 1 is ABOUT rather than asked of.
    const STRIPPER: Record<string, string> = {
      dropSingleQuoted: "the stripper itself — question 1 is what it IS",
      endsInsideSingleQuote: "the stripper's self-check, over the same walk",
    };
    // Everything else this file declares, each with the reason it owes no row. An entry here is the
    // reviewable diff that says "this one does not read shell"; leaving a new function out of BOTH
    // lists reds below rather than passing quietly.
    const NOT_SHELL_TEXT: Record<string, string> = {
      parseWorkflow: "parses a workflow file — YAML",
      catalogSteps: "selects steps out of parsed YAML",
      stepIndexRunning:
        "locates a workflow STEP by a substring of its `run:`; the shell question is delegated whole " +
        "to the shared `logicalLines`, and it decides nothing about the shell's grammar itself",
      clientCatalogIndexUrlFromRust: "reads Rust source via the shared `rustSource` helpers",
      requireBash: "probes the environment for bash",
      toolAvailable: "probes the environment for a tool",
      itJq: "a test registrar",
      scratch: "makes a temp directory",
      posixPath: "converts a path spelling",
      runGuard: "EXECUTES the script; it reads no source",
      runGuardWithoutJq: "EXECUTES the script with jq removed from PATH",
      commandLines: "scans the RUNNER'S OUTPUT for `::…` workflow commands, not shell",
      guardLogicalLines:
        "the SOURCE of the shell text — a file read plus the imported `logicalLines`; a narrowing " +
        "here is caught by the parser self-checks its callers carry, not by a row",
      enumerate: "RUNS a slice of the script under bash; it answers nothing by reading",
      shapeCorpus:
        "GENERATES shell lines from bash's option grammar; it reads no shell text and answers no " +
        "question about any. What it produces is then answered by bash itself",
      bashBinds: "EXECUTES every generated line under bash; like `enumerate`, it reads nothing",
      git: "drives a git fixture",
      commitTag: "drives a git fixture",
      derive: "EXECUTES catalog-version.sh",
    };
    // Round 9, MINOR: this file's text says these lists carry "the reason it owes no row", and until
    // now only `Object.keys` was ever read — so `reviewerProbeScanner: ""` alongside a new function
    // went GREEN, which is a membership rule that does not ask the question it advertises. One
    // `expect` makes the sentence true.
    const reasonless = [...Object.entries(STRIPPER), ...Object.entries(NOT_SHELL_TEXT)]
      .filter(([, why]) => why.trim().length < 12)
      .map(([n]) => n)
      .sort();
    expect(
      reasonless,
      "these entries excuse a function from the scanner table with no reason given. The excuse IS " +
        "the reviewable diff — an empty string buys the same green as an argument, which is how a " +
        "scanner gets waved past instead of classified.",
    ).toEqual([]);

    const declared = [...self.matchAll(/^[ ]*function ([A-Za-z][A-Za-z0-9_]*)\s*\(/gm)].map((m) => m[1]);
    expect(
      declared.length,
      `only ${declared.length} function declarations found — the grep, not the file, is broken`,
    ).toBeGreaterThan(20);
    const known = new Set([...rows, ...Object.keys(STRIPPER), ...Object.keys(NOT_SHELL_TEXT)]);
    const unclassified = declared.filter((n) => !known.has(n)).sort();
    const stale = [...Object.keys(STRIPPER), ...Object.keys(NOT_SHELL_TEXT), ...rows]
      .filter((n) => !declared.includes(n))
      .sort();
    expect(
      { unclassified, stale },
      "`unclassified` are functions declared in this file that the scanner table above neither has a " +
        "row for nor excuses. Decide which: if it takes shell TEXT — one logical line, a list of " +
        "them, or the script's source — and answers a question about the shell in it, it owes a row " +
        "and all THREE columns, answered by sabotage rather than by reading. If it does not, add it " +
        "to NOT_SHELL_TEXT with the reason. Round 7 closed this table with a COUNT (\"a fourth " +
        "line-scanning function means a fourth row\") and two qualifying functions were already " +
        "sitting in the file uncounted. `stale` are names in the table or the two lists that this " +
        `file no longer declares. All ${declared.length} declarations found: ${declared.join(", ")}`,
    ).toEqual({ unclassified: [], stale: [] });
  });

  it("exit 1 is a shell predicate's boolean, never a code this script returns", () => {
    // Justifies the `N >= 2` cutoff the executed leg's derivation uses, instead of asserting it.
    // A function's status escapes the script only through a propagating call (`f || return $?`);
    // every function that can `return 1` is called exclusively in a CONDITION, so its 1 is consumed.
    const lines = guardLogicalLines();
    const fnsReturningOne = new Set<string>();
    let current = "";
    for (const line of lines) {
      const open = /^([A-Za-z_][A-Za-z0-9_]*)\(\)\s*\{/.exec(line);
      if (open) current = open[1];
      else if (line === "}") current = "";
      // `return 1` ANYWHERE in the logical line, not anchored at its start: both instances live in
      // `case` arms (`'' | *[!0-9]*) return 1 ;;`), and a start-anchored scan found neither —
      // measured while writing this, which is why the size floor below is not decoration.
      else if (current && /(^|[\s;)])return\s+1\s*(;|$)/.test(line)) fnsReturningOne.add(current);
    }
    expect(fnsReturningOne.size, "no `return 1` found — the function scan is broken").toBeGreaterThan(0);
    const propagated = [...fnsReturningOne].filter((fn) =>
      lines.some((l) => new RegExp(`\\b${fn}\\b[^|]*\\|\\|\\s*return`).test(l)),
    );
    expect(
      propagated,
      `these functions can return 1 AND have their status propagated out of the script, so exit 1 is ` +
        `reachable and the executed leg's "N >= 2" derivation is wrong: ${propagated.join(", ")}`,
    ).toEqual([]);
  });
});

// ── The asset enumeration, run on the line endings jq actually produces (CPE-1951, round 5) ──────
//
// Round 4 replaced `grep -Fxq 'catalog-index.json'` with `while IFS= read -r` + `[ = ]` and wrote
// "EQUIVALENCE MEASURED, not assumed, executing this script with gh/curl/jq shimmed". The shim was
// a shell `printf`, which emits LF. Real jq writes stdout in TEXT mode on Windows, so the `\n`
// inside `join("\n")` leaves the process as `\r\n`; cygwin `grep -Fx` tolerates the trailing `\r`
// and `[ "$asset_name" = 'catalog-index.json' ]` does not. Measured with jq 1.7.1, bash 5.3.15
// cygwin, at that head:
//     $assets bytes: c a t a l o g - i n d e x . j s o n \r \n x
//     grep -Fxq          -> MATCH
//     while read + [ = ] -> have_index=0
// i.e. the guard took the "no index among the assets" branch on a release that lists it, and printed
// `::warning::… carries NO catalog-index.json (2 asset(s) enumerated) … Proceeding with no lower
// bound.` at exit 0 — byte-for-byte the self-contradicting fail-open the change was made to remove.
// `catalogPublishLowerBound.test.ts` on Windows: round 3 = 76 passed / 0 failed; round 4 head =
// 34 passed / **45 failed**. CI runs vitest on ubuntu-latest only, where jq emits LF, so CI stayed
// green throughout. A SHIM MEASURES YOUR SHIM.
//
// This block is the platform-independent pin, and it is deliberately NOT another end-to-end run: on
// Linux real jq emits LF and there is nothing to catch, and a CRLF-emitting jq stub would be a
// third configuration that exists on no real machine (it would also make `count` non-numeric, which
// Windows does not do — `$( )` there strips a trailing `\r\n` as a unit, so the single-line `tag`,
// `count` and `bound` captures come back clean; swept, all four jq sites, same run). Instead it
// lifts the script's OWN transformation-and-enumeration lines out of the file and drives them with
// the byte sequence jq hands them. Nothing here asserts that a CR strip exists — the behaviour is
// the assertion, so deleting the strip reds this rather than reding a line-is-present check.
describe("the asset enumeration matches on jq's real line endings (CPE-1951)", () => {
  /**
   * The script's own lines between "the assets fetch has succeeded" and "the enumeration is done",
   * minus anything that runs a command. Anchored on code: the `fi` closing the `if ! assets=$(…)`
   * block, and `done <<< "$assets"`.
   *
   * THE FILTER DROPS BY SPELLING, NOT BY FETCH-VS-TRANSFORM, and the docblock that used to sit here
   * said the opposite — *"a new line that TRANSFORMS `$assets` is picked up automatically; a new
   * line that FETCHES something is dropped"*. The predicate is
   * `!l.includes("$(") && !l.includes("`")`: **any line containing a command substitution is
   * dropped, transformation or not.** Measured — inserting
   * `assets=$(printf '%s' "${assets%%$'\n'*}")` after the CR strip truncates the real asset list to
   * its first line, a live fail-open on a release that lists the index SECOND, and this block stays
   * fully green (2 passed) because that line never reaches the probe.
   *
   * So the direction is only safe for transformations this enumeration does not need: a fetch has
   * no `$api_out` to give here and must go, and today's one dropped line (`count=$(… jq …)`, which
   * touches `count` and not `$assets`) is exactly that. **A transformation that BREAKS the
   * enumeration is invisible to this block** — say it rather than imply coverage. It is not a
   * coverage hole in the file: the executed leg drives the real script and caught that same
   * insertion at **33 failed / 50 passed** with real jq. Slice as it stands: 10 logical lines, 9
   * kept, 1 dropped.
   */
  function enumerationLines(): string[] {
    const lines = guardLogicalLines();
    const fetch = lines.findIndex((l) => /\bassets=\$\(/.test(l));
    expect(fetch, "no `assets=$(…)` line found — the slice is broken, not the script").toBeGreaterThan(-1);
    const fi = lines.findIndex((l, i) => i > fetch && l === "fi");
    const done = lines.findIndex((l, i) => i > fi && /^done\s+<<<\s+"\$assets"$/.test(l));
    expect(fi, "no `fi` after the assets fetch").toBeGreaterThan(-1);
    expect(done, 'no `done <<< "$assets"` after it').toBeGreaterThan(fi);
    const slice = lines.slice(fi + 1, done + 1).filter((l) => !l.includes("$(") && !l.includes("`"));
    // Fail loudly on a near-empty enumeration rather than pass vacuously (CLAUDE.md).
    expect(slice.length, `the enumeration slice came back as ${slice.length} lines:\n${slice.join("\n")}`).toBeGreaterThan(4);
    return slice;
  }

  /** `have_index` after running those lines with `$assets` set to `input`. */
  function enumerate(input: string): string {
    const body = enumerationLines()
      .map((l) => `  ${l}`)
      .join("\n");
    const probe = `probe() {\n  local assets="$CPE_ASSETS"\n${body}\n  printf '%s' "$have_index"\n}\nprobe\n`;
    const r = spawnSync("bash", ["-c", probe], {
      encoding: "utf8",
      env: { ...process.env, CPE_ASSETS: input },
    });
    if (r.status !== 0) throw new Error(`probe exited ${r.status}: ${r.stderr}\n---\n${probe}`);
    return (r.stdout ?? "").trim();
  }

  it("finds the index whether jq joined the names with LF or with CRLF", () => {
    const cases: Record<string, string> = {
      // What jq hands it on Linux, and on Windows. Both must match.
      "LF, index first": "catalog-index.json\napp.msi",
      "CRLF, index first": "catalog-index.json\r\napp.msi",
      "CRLF, index last": "app.msi\r\ncatalog-index.json",
      "CRLF, sole asset": "catalog-index.json",
    };
    expect(
      Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, enumerate(v)])),
      "the asset enumeration did not find `catalog-index.json` in an asset list that contains it. " +
        "That is not a near-miss: it takes the exit-0 `carries NO catalog-index.json … Proceeding " +
        "with no lower bound` branch, which is the fail-open this whole guard exists to remove.",
    ).toEqual(Object.fromEntries(Object.keys(cases).map((k) => [k, "1"])));
  });

  it("...and still refuses the substring and the `.sig` trap on both line endings", () => {
    // `-F` alone (substring, no `-x`) would match all four of these. The whole-line comparison the
    // `grep -Fxq` spelling meant must survive the CR strip, or the fix trades a fail-open for a
    // different one: a release that publishes only a signature would read as publishing an index.
    const cases: Record<string, string> = {
      "sig only, CRLF": "catalog-index.json.sig\r\napp.msi",
      "sig only, LF": "catalog-index.json.sig\napp.msi",
      "prefixed name": "old-catalog-index.json\r\napp.msi",
      "no assets at all": "",
    };
    expect(
      Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, enumerate(v)])),
      "the enumeration matched something that is NOT `catalog-index.json` — the whole-line " +
        "comparison `-Fx` stood for has been weakened",
    ).toEqual(Object.fromEntries(Object.keys(cases).map((k) => [k, "0"])));
  });
});

describe("nothing fetched can become a workflow command in the job log (CPE-1951)", () => {
  /** A forged payload per site, so a failure names WHICH input leaked. Embedded in JSON strings, so
   *  `\\n` here is a real newline once jq parses it. */
  const forged = (site: string) =>
    `\\n::error::FORGED-${site}\\n::stop-commands::deadbeef${site.toLowerCase()}`;
  /** The same bytes as plain text, for the stubs' own stderr (no JSON decoding in between). */
  const forgedRaw = (site: string) =>
    `\n::error::FORGED-${site}\n::stop-commands::deadbeef${site.toLowerCase()}`;

  const tagWithIndex = (site: string) =>
    `{"tag_name":"v9${forged(site)}","assets":[{"name":"catalog-index.json"},{"name":"app.msi"}]}`;
  const tagNoIndex = (site: string) =>
    `{"tag_name":"v9${forged(site)}","assets":[{"name":"other.txt"}]}`;
  const PUBLISHED_IDX = '{"entries":[{"id":"claude","version":1787200000}]}';

  interface Case {
    code: number;
    label: string;
    run: () => Run;
    /** A marker that must appear SOMEWHERE in the output — defanging is not hiding, and it also
     *  proves the forged bytes actually travelled the path rather than being dropped upstream. */
    shows?: string;
  }

  /**
   * ONE CASE PER EXIT CODE, and the set is checked against the script's own `return N` statements
   * below — so this is an enumeration of what the script can do, not of what someone remembered.
   * Every case forges `::`-bearing bytes into every remote-influenced input that path reads.
   *
   * PER CODE IS NOT PER BRANCH, and the difference is not academic (#1091 round 4). Exit 5 has TWO
   * branches — the missing-`tag_name` one and the `assets is not an array` one — and the case below
   * supplies a body with no `tag_name`, so it drives only the first. The second branch, which is the
   * one that echoes BOTH `$tag` and `$assets`, is driven by no executed case at all; what covers it
   * is the structural leg. So "the cases cover every exit code the script can produce" means exactly
   * that and no more: adding a second branch under an EXISTING code adds no case here and reds
   * nothing. A new branch that echoes something new is caught, if at all, by `taintedVars`'s scan —
   * which is why that scan's blind spots are enumerated at its docblock rather than assumed empty.
   */
  const EXIT_CODE_CASES: Case[] = [
    {
      code: 0,
      label: "exit 0, permissive `none` branch — forged tag in the ::warning::",
      run: () => runGuard(["1787200000", "owner/repo"], { GH_MODE: "raw", GH_BODY: tagNoIndex("NONE") }),
      shows: "FORGED-NONE",
    },
    {
      code: 0,
      label: "exit 0, strictly-newer branch — forged tag fetched but not echoed",
      run: () =>
        runGuard(["1787300000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("NEWER"),
          CURL_MODE: "raw",
          IDX_BODY: PUBLISHED_IDX,
          CURL_ERR: forgedRaw("NEWERCURL"),
        }),
    },
    {
      code: 2,
      label: "exit 2, invalid candidate — refused before any fetch is read",
      // The candidate is a WORKFLOW input (`VERSION`), not a fetched byte, so it is passed plain;
      // the forged fetchable inputs are supplied and must never be reached, let alone echoed.
      run: () =>
        runGuard(["not-a-number", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("PREFETCH"),
          CURL_MODE: "raw",
          IDX_BODY: PUBLISHED_IDX,
        }),
    },
    {
      code: 3,
      label: "exit 3, NOT NEWER — the refusal's own ::error:: plus a forged tag on the same run",
      run: () =>
        runGuard(["1787100000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("NOTNEWER"),
          CURL_MODE: "raw",
          IDX_BODY: PUBLISHED_IDX,
        }),
    },
    {
      code: 4,
      label: "exit 4, gh's own stderr echoed",
      run: () => runGuard(["1787200000", "owner/repo"], { GH_MODE: "fail" }),
      shows: "FORGED-VIA-GH-STDERR",
    },
    {
      code: 5,
      label: "exit 5, the whole API body echoed",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: `{"assets":[{"name":"x"}],"note":"${forged("BODY5")}"}`,
        }),
      shows: "FORGED-BODY5",
    },
    {
      code: 6,
      label: "exit 6, timeout — curl's stderr echoed",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T6"),
          CURL_MODE: "timeout",
          CURL_ERR: forgedRaw("CURL6"),
        }),
      shows: "FORGED-CURL6",
    },
    {
      code: 7,
      label: "exit 7, unreachable host — curl's stderr echoed",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T7"),
          CURL_MODE: "unreachable",
          CURL_ERR: forgedRaw("CURL7"),
        }),
      shows: "FORGED-CURL7",
    },
    {
      code: 8,
      label: "exit 8, truncated transfer — curl's stderr echoed",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T8"),
          CURL_MODE: "partial",
          CURL_ERR: forgedRaw("CURL8"),
        }),
      shows: "FORGED-CURL8",
    },
    {
      code: 9,
      label: "exit 9, other transport failure — curl's stderr echoed",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T9"),
          CURL_MODE: "other",
          CURL_ERR: forgedRaw("CURL9"),
        }),
      shows: "FORGED-CURL9",
    },
    {
      code: 10,
      // #1091 round 3's finding. The asset list says catalog-index.json is there and the fetch
      // 404s, so the forged tag is echoed on a FAILURE path — where `::stop-commands::` silences
      // the rest of the job's annotations.
      label: "exit 10, listed-but-not-served contradiction — forged tag echoed on a FAILURE path",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("404"),
          CURL_MODE: "http404",
        }),
      shows: "FORGED-404",
    },
    {
      code: 11,
      label: "exit 11, HTTP 5xx",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T11"),
          CURL_MODE: "http500",
          CURL_ERR: forgedRaw("CURL11"),
        }),
    },
    {
      code: 12,
      label: "exit 12, unexpected HTTP status",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T12"),
          CURL_MODE: "http418",
          CURL_ERR: forgedRaw("CURL12"),
        }),
    },
    {
      code: 13,
      label: "exit 13, HTTP 200 with an empty body",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T13"),
          CURL_MODE: "empty",
          CURL_ERR: forgedRaw("CURL13"),
        }),
    },
    {
      code: 14,
      // jq 1.7.1's own parse/runtime messages were MEASURED here not to echo the body — they quote
      // the temp-file path and a type name. So the sanitiser on `jq_err` is defence in depth, and
      // what this case proves is that the exit-14 path stays clean while forged bytes are in flight
      // on every other input, not that jq leaks today.
      label: "exit 14, unparseable body — jq's stderr echoed",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T14"),
          CURL_MODE: "raw",
          IDX_BODY: `{"entries": ::error::FORGED-JQ14\n::stop-commands::beefjq }`,
        }),
    },
    {
      code: 15,
      label: "exit 15, parsed but no usable entries[].version",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T15"),
          CURL_MODE: "no_version",
        }),
    },
    {
      code: 16,
      label: "exit 16, a required tool is missing",
      run: () =>
        runGuardWithoutJq(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T16"),
        }),
    },
    {
      code: 17,
      label: "exit 17, bound outside the u64 a CatalogEntry.version can hold",
      run: () =>
        runGuard(["1787200000", "owner/repo"], {
          GH_MODE: "raw",
          GH_BODY: tagWithIndex("T17"),
          CURL_MODE: "raw",
          IDX_BODY: '{"entries":[{"version":18446744073709551616}]}',
        }),
    },
  ];

  it("the cases cover every exit code the script can produce, derived from its `return N`s", () => {
    // THE point of round 3's restructure. Not a list of the paths someone thought of: the script's
    // own `return` statements. `N >= 2` because 0 and 1 are the boolean protocol of the shell
    // predicates in this file — 0 is covered explicitly below anyway, and the sibling describe
    // proves 1 never escapes.
    const lines = guardLogicalLines();
    const derived = new Set<number>([0]);
    for (const line of lines) {
      // Unanchored, for the same reason as the `return 1` scan above: two of this script's returns
      // sit inside `case` arms, and a start-anchored regex silently misses them.
      for (const m of line.matchAll(/(?:^|[\s;)])return\s+(\d+)\s*(?:;|$)/g)) {
        if (Number(m[1]) >= 2) derived.add(Number(m[1]));
      }
    }
    expect(derived.size, "the `return N` scan came back near-empty").toBeGreaterThan(10);
    const covered = new Set(EXIT_CODE_CASES.map((c) => c.code));
    const missing = [...derived].filter((c) => !covered.has(c)).sort((a, b) => a - b);
    const extra = [...covered].filter((c) => !derived.has(c)).sort((a, b) => a - b);
    expect(
      { missing, extra },
      "EXIT_CODE_CASES must range over every exit code the guard can produce. A new `return N` in " +
        "catalog-lower-bound.sh needs a case here that drives it with forged `::` bytes — that is " +
        "what round 3's finding (the exit-10 site) cost when the block enumerated three chosen paths.",
    ).toEqual({ missing: [], extra: [] });
  });

  for (const c of EXIT_CODE_CASES) {
    itJq(`${c.label} (exit ${c.code}) emits no unowned workflow command`, () => {
      const r = c.run();
      expect(r.status, `wanted exit ${c.code}, got ${r.status}:\n${r.all}`).toBe(c.code);
      if (c.shows) {
        // Defanging is not hiding: the forged text must still be VISIBLE in the log.
        expect(r.all, "the forged bytes never reached the log, so this case proves nothing").toContain(c.shows);
      }
      const smuggled = commandLines(r.all).filter((l) => !OURS.test(l));
      expect(
        smuggled,
        `these lines would be parsed as workflow commands by the runner: ${smuggled.join(" | ")}`,
      ).toEqual([]);
    });
  }

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
