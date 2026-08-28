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
// The synthetic-input test right after the scan is what keeps them from rotting into a vacuous pass.
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
 * Closed in round 4, so NOT on this list any more: transitive assignment (rule 2 above), and an
 * inline `$(tool …)` with no variable at all — the scan below now refuses ANY surviving `$(`/
 * backtick in a `printf … >&2` line, which is what made deleting the sanitiser off
 * `"$(catalog_lb_log_safe "$(cat "$jq_err")")"` red. Before that it was 76 passed, fully green:
 * a live sanitiser call could be deleted with nothing noticing, because `jq_err` comes from
 * `mktemp` (so it is not tainted) and the loop iterates variables, never substitutions.
 *
 * Still blind, each with the sabotage that measures it, none of them present in the script today:
 *   * `read -r name` / `printf -v name` / `mapfile` — a variable that receives remote bytes without
 *     an `=` on its left. Sabotage: replace `tag=$(printf '%s' "$api_out" | jq -r …)` with
 *     `IFS= read -r tag < <(printf '%s' "$api_out" | jq -r …)`. This leg goes green; the executed
 *     leg still reds, because the forged bytes still reach the log.
 *   * A wrapper FUNCTION that prints remote bytes itself, called bare rather than in `$( )` — e.g.
 *     `emit_tag() { printf '%s' "$tag"; }` invoked as a statement. There is no `printf … >&2` here
 *     for the scan to read, so it goes green.
 *   * Any channel that is not a `printf … >&2` at the end of a logical line: `echo … >&2`,
 *     `>&2 printf …`, a heredoc, a `{ …; } >&2` group, or stdout (out of scope by design — see the
 *     `logPrintfs` filter's own comment).
 * All three are caught by the EXECUTED leg instead, which reads the real process's stdout+stderr and
 * cannot be fooled by how the bytes got there — but only for a path some case already drives. That
 * asymmetry is the reason both legs exist.
 */
function taintedVars(lines: string[]): { tainted: Set<string>; sanitised: Set<string> } {
  const tainted = new Set<string>();
  const sanitised = new Set<string>();
  const plain: { name: string; rhs: string }[] = [];
  for (const line of lines) {
    const sub = /(^|[\s!(){};])([A-Za-z_][A-Za-z0-9_]*)=\$\(/.exec(line);
    if (sub) {
      const name = sub[2];
      const rhs = line.slice(sub.index + sub[0].length);
      if (/(^|[\s|(])catalog_lb_log_safe[\s)]/.test(rhs)) sanitised.add(name);
      else if (/(^|[\s|(])(gh|curl|jq|cat)\s/.test(rhs)) tainted.add(name);
      continue;
    }
    // `name=<anything else>` — kept for the transitive pass below.
    const asg = /(^|[\s!(){};])([A-Za-z_][A-Za-z0-9_]*)=(.*)$/.exec(line);
    if (asg) plain.push({ name: asg[2], rhs: asg[3] });
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
 * Every single-quoted span replaced by `''`. In POSIX shell a single-quoted string admits no
 * escapes and no substitution whatsoever, so this is exact — and it is what lets the
 * "no surviving `$(`" rule below run over lines whose FIRST argument is a long single-quoted
 * `printf` format string full of prose, backticks and parentheses. Without it, a message that
 * happened to mention `$(cat …)` in prose would red the scan.
 */
function dropSingleQuoted(s: string): string {
  return s.replace(/'[^']*'/g, "''");
}

/** Blanks out every `$(catalog_lb_log_safe …)` call, matching parens, so what remains is the set of
 *  interpolations that reach the log RAW. */
function blankSanitiserCalls(s: string): string {
  let out = s;
  for (;;) {
    const i = out.indexOf("catalog_lb_log_safe");
    if (i < 0) return out;
    const start = out.lastIndexOf("$(", i);
    if (start < 0) return `${out.slice(0, i)}«safe»${out.slice(i + "catalog_lb_log_safe".length)}`;
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

    // Only `>&2`. The two stdout `printf`s are out of scope and that is stated, not silent: one is
    // `catalog_published_lower_bound`'s return VALUE (captured by its caller, never logged) and one
    // is the success line, which interpolates only validated numbers. The executed leg below scans
    // stdout and stderr TOGETHER, so nothing rides out on the channel this scan does not read.
    const logPrintfs = lines.filter((l) => /^printf\b/.test(l) && />&2\s*$/.test(l));
    expect(logPrintfs.length, "no `printf … >&2` found — the scan is broken").toBeGreaterThan(9);

    const raw = new Map<string, string[]>();
    /** Inline substitutions surviving the sanitiser blanking — no variable to name, so no RAW_OK
     *  key either. Default-DENY: see `rawSubstitutions` below. */
    const subs: string[] = [];
    for (const line of logPrintfs) {
      const stripped = blankSanitiserCalls(line);
      for (const v of tainted) {
        if (sanitised.has(v)) continue;
        if (!new RegExp(`\\$\\{?${v}\\b`).test(stripped)) continue;
        if (!raw.has(v)) raw.set(v, []);
        (raw.get(v) as string[]).push(line.slice(0, 100));
      }
      // A command run INLINE in the argument list has no variable for the loop above to iterate, so
      // round 3's scan could not see it at all: deleting `catalog_lb_log_safe` from
      // `"$(catalog_lb_log_safe "$(cat "$jq_err")")"` left the whole file green at 76 passed. The
      // rule is therefore default-deny rather than a taint judgement — once every
      // `$(catalog_lb_log_safe …)` is blanked, a `printf … >&2` has no business running anything.
      // Backticks too: they are the other substitution spelling, and `dropSingleQuoted` has already
      // removed the ones this script uses in prose inside its format strings.
      const bare = dropSingleQuoted(stripped);
      if (bare.includes("$(") || bare.includes("`")) subs.push(line.slice(0, 120));
    }
    const undeclared = [...raw.keys()].filter((v) => !(v in RAW_OK)).sort();
    const stale = Object.keys(RAW_OK)
      .filter((v) => !raw.has(v))
      .sort();
    expect(
      { undeclared, stale, rawSubstitutions: subs },
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

  it("...and that scan can actually SEE an inline substitution and a transitive alias", () => {
    // The two rules above are the round-4 additions, and both are satisfied by the script today —
    // so on the real source they are indistinguishable from doing nothing. This drives them with
    // synthetic lines instead, so neither can rot into a vacuous pass the way round 3's per-variable
    // loop silently did. `logicalLines` first, so these go through the same stripper the real scan
    // uses rather than a second, kinder one.
    const synthetic = logicalLines(
      [
        "tag=$(jq -r '.tag_name' <<< \"$body\")",
        "rel_name=\"$tag\"",
        "printf 'release %s\\n' \"$rel_name\" >&2",
        "printf 'jq said %s\\n' \"$(cat \"$jq_err\")\" >&2",
        // Must NOT be reported: a sanitiser-wrapped inline call, and prose that merely mentions
        // `$(` and a backtick inside a single-quoted format string.
        "printf 'ok `cmd` $(cat x) %s\\n' \"$(catalog_lb_log_safe \"$(cat \\\"$jq_err\\\")\")\" >&2",
      ].join("\n"),
    );
    const { tainted } = taintedVars(synthetic);
    expect(tainted, "transitive taint (`rel_name=\"$tag\"`) is not being followed").toContain("rel_name");

    const flagged: string[] = [];
    for (const line of synthetic.filter((l) => /^printf\b/.test(l) && />&2\s*$/.test(l))) {
      const stripped = blankSanitiserCalls(line);
      const bare = dropSingleQuoted(stripped);
      if (bare.includes("$(") || bare.includes("`")) flagged.push(line);
      else if ([...tainted].some((v) => new RegExp(`\\$\\{?${v}\\b`).test(stripped))) flagged.push(line);
    }
    expect(flagged.length, `expected exactly the first two printfs to be flagged, got:\n${flagged.join("\n")}`).toBe(2);
    expect(flagged[0]).toContain("rel_name");
    expect(flagged[1]).toContain("jq said");
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
