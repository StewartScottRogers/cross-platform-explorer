// CPE-1865: neither Rust build ever refused a stale lockfile — `cargo build`/`test`/`check`/`clippy`
// silently REWRITE a drifted `Cargo.lock` and exit 0, so the version-drift CLAUDE.md documents (and
// CPE-1853 measured on throwaway crates) has no backstop except the release script's own bump-all-five
// guard. `--locked` converts that into a loud, immediate failure — but only where it is actually
// applied, and "everywhere except the one place a reviewer didn't think to check" is exactly this
// repo's most-repeated defect (CPE-1855's own framing, one ticket up). This is the local ratchet: it
// reads the real workflow YAML (no toolchain, no GitHub Actions run required — the same kind of guard
// `releaseVersionBump.test.ts` and `msrvSync.test.ts` already are) and fails if any real cargo
// build/test/check/clippy/run invocation in a CI or release workflow is missing `--locked`, or if a
// `tauri-action`/`npm run tauri build` step (which has no flag to pass `--locked` through to the
// `cargo build` it drives internally) is not immediately preceded by a `cargo check --locked` preflight.
//
// **CPE-1929 rewrote how it reads the workflows, and the two sabotages that forced the rewrite are the
// point of the ticket.** Until then this guard regexed the *raw `.yml` text* line by line, skipping
// only WHOLE-LINE `#` comments — the same raw-text-rather-than-syntactic-position shape the apt-get
// guards had before CPE-1787, and the one sibling that never migrated to `parseYaml`. Two shapes were
// measured slipping straight through it, both leaving all 6 tests green:
//
//   1. **A trailing comment counted as a real preflight.** Deleting BOTH real
//      `cargo check --locked --all-targets` steps from `gui-smoke.yml` and replacing them with
//      `run: echo skipped   # cargo check --locked --all-targets` left this file at **6 passed / 0
//      failed**. The header comment above `preflightLineNumbers` said this exact false negative had
//      already been found and fixed once ("removing the real step while leaving its explanatory comment
//      above it still passed") — the fix only covered comments on their own line, so the same defect was
//      still live in its trailing form.
//   2. **A backslash continuation hid an unlocked invocation entirely.** Rewriting ci.yml's
//      `run: cargo test --locked` as `cargo \` / `test --all-targets` (no `--locked`) also left this
//      file at **6 passed / 0 failed**: the first physical line does not match `cargo\s+(build|test|…)`
//      and the second contains no `cargo` at all, so the invocation was invisible rather than unlocked.
//
// Both are now closed by reading `step.run` off the **parsed** document (`parseYaml`, the in-repo
// bounded-subset parser, src/lib/preview/yaml.ts) and splitting it with the **shared** shell-line
// splitter `logicalLines` (src/lib/shellScriptLines.ts), whose case table is the one
// `crates/updater-verify/src/workflow_scan.rs` shares via `src/lib/shellScriptLines.cases.json`. That
// is deliberately *not* a fifth hand-rolled comment stripper: this repo wrote four before the fifth was
// caught. `logicalLines` joins continuations, strips trailing `#` comments quote-aware, and skips
// heredoc bodies; reading `run` structurally means a step's `name:` can never be mistaken for a command
// in the first place, so the old `startsWith("name:")` heuristic is gone rather than kept as dead code.
//
// Parity was measured before and after, so the rewrite cannot have quietly narrowed the scan: both the
// old line scanner and the new structural one find exactly **79** cargo invocations across the five
// workflows (66 / 3 / 7 / 2 / 1), with no line found by the old one and missed by the new.
//
// **CPE-1969 widened WHAT it reads, and that was the second half of the same defect.** Until then
// `WORKFLOW_FILES` was a hard-coded five-entry list under a comment claiming the other workflows
// "deliberately" ran no cargo. There are EIGHT workflow files, so a sixth workflow that built Tauri
// without `--locked` was never looked at — and the claim was untestable prose, exactly what CPE-1933
// says not to write. Worse, `.github/workflows/scripts/*.sh` — three files, 109 logical lines,
// invoked BY the workflows — was read by no consumer in the repo at all, so shell moved out of a
// `run:` block into a script silently left every guard's scope. Both lists are now derived at run
// time by `src/lib/workflowShellSources.ts`, which refuses a near-empty enumeration rather than
// scanning nothing and reporting clean. Measured before the widening (2026-08-27): the three
// newly-included workflows and all three scripts contain ZERO cargo invocations and zero Tauri build
// anchors, so this is a pure scope fix with no live defect folded into it — but "zero today" is now
// a measurement the scan re-takes on every run, not a sentence someone wrote once.
import { describe, it, expect, afterEach } from "vitest";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";
import { logicalLines } from "./shellScriptLines";
import {
  MIN_EXPECTED_WORKFLOWS,
  discoverWorkflowScripts,
  discoverWorkflows,
  parseWorkflowFile,
  scriptUnit,
  workflowStepUnits,
  type ShellUnit,
} from "./workflowShellSources";

const ROOT = process.cwd();

/** The per-file invocation counts, pinned as **floors** (CPE-1929 review), for the files that carry
 *  cargo today. A workflow or script absent from this record is still SCANNED — CPE-1969: the scan
 *  walks the derived enumeration, and this record only says which files have a floor worth pinning.
 *  A file here that no longer exists is reported (see the staleness check below) rather than skipped.
 *
 *  `> 0` alone would not
 *  catch a PARTIAL narrowing: if `parseYaml` ever silently dropped a job or a step, ci.yml could
 *  fall from 66 real invocations to 3 and still read as "the detector works". These are the
 *  numbers the old raw-text line scanner and the new structural one BOTH produced when the rewrite
 *  was measured for parity — 66/3/7/2/1, 79 in total, with no line found by the old and missed by
 *  the new — so the parity claim is an assertion here rather than a sentence in a comment.
 *
 *  A floor, not an equality: adding a genuine new cargo step must not fail this. Lowering one is
 *  legitimate when a workflow really loses a step — edit the number, and only then, because the
 *  failure message says which file moved and by how much. */
const MIN_CARGO_INVOCATIONS: Record<string, number> = {
  ".github/workflows/ci.yml": 66,
  ".github/workflows/release.yml": 3,
  ".github/workflows/release-sidecar.yml": 7,
  ".github/workflows/gui-smoke.yml": 2,
  ".github/workflows/model-snapshot.yml": 1,
};

/** The cargo subcommands that read and can silently REWRITE a `Cargo.lock`. `cargo install`,
 *  `cargo fmt` and friends are deliberately absent: they do not build this repo's own lockfiles. */
const CARGO_INVOCATION = /\bcargo\s+(build|test|check|clippy|run)\b/;

interface WorkflowStep {
  name?: string;
  run?: unknown;
  uses?: unknown;
  [key: string]: unknown;
}
interface WorkflowJob {
  steps?: WorkflowStep[];
  [key: string]: unknown;
}
interface WorkflowDoc {
  jobs: Record<string, WorkflowJob>;
}

/** Parses a workflow with the same bounded-subset YAML parser the app ships for previewing `.yml`
 *  files, and throws the parser's own reason if the file falls outside that subset — so a future edit
 *  that pushes a workflow past what this parser understands surfaces here as a clear parse failure,
 *  never as a silently-empty (and therefore vacuously green) result. */
function parseWorkflow(file: string): WorkflowDoc {
  return parseWorkflowFile(file) as WorkflowDoc;
}

/** Every real, live cargo invocation in ONE unit of shell — a workflow step's `run:` script, or a
 *  whole extracted `.sh` file (CPE-1969; see `workflowShellSources.ts` for why a script is exactly
 *  one unit). Identical matching either side: `logicalLines` strips comments, joins continuations
 *  and skips heredoc bodies before the regex is tried, so a script gets the same treatment a `run:`
 *  block always got. */
function cargoInvocationsIn(unit: ShellUnit): { where: string; line: string }[] {
  return logicalLines(unit.run)
    .filter((line) => CARGO_INVOCATION.test(line))
    .map((line) => ({ where: unit.where, line }));
}

interface Invocation {
  job: string;
  step: string;
  line: string;
}

/** Every real, live cargo invocation in a workflow: a logical shell line of a step's `run:` script.
 *  A `name:` label that echoes a subcommand in prose (`- name: cargo check`) is structurally not a
 *  `run`, so it cannot reach here; a `#` comment (leading OR trailing) and a heredoc body are removed
 *  by `logicalLines` before the match is tried. */
export function cargoInvocations(doc: WorkflowDoc): Invocation[] {
  const out: Invocation[] = [];
  for (const [job, jobDoc] of Object.entries(doc.jobs ?? {})) {
    for (const step of jobDoc.steps ?? []) {
      if (typeof step.run !== "string") continue;
      for (const line of logicalLines(step.run)) {
        if (CARGO_INVOCATION.test(line)) out.push({ job, step: step.name ?? "(unnamed)", line });
      }
    }
  }
  return out;
}

/** A step that drives the Tauri CLI's internal `cargo build`, which has no flag of its own to forward
 *  `--locked`: `tauri-action` (release.yml, release-sidecar.yml) and `npm run tauri build`
 *  (gui-smoke.yml). Each needs its OWN preflight, not a shared one a refactor could silently orphan. */
function isTauriBuildAnchor(step: WorkflowStep): boolean {
  if (typeof step.uses === "string" && /^tauri-apps\/tauri-action@/.test(step.uses)) return true;
  return (
    typeof step.run === "string" &&
    logicalLines(step.run).some((l) => /\bnpm run tauri build\b/.test(l))
  );
}

function isLockedPreflight(step: WorkflowStep): boolean {
  if (typeof step.run !== "string") return false;
  return logicalLines(step.run).some((l) => /\bcargo\s+check\b/.test(l) && l.includes("--locked"));
}

describe("every real cargo build/test/check/clippy/run in CI + release is --locked (CPE-1865)", () => {
  // CPE-1969: the file list is DERIVED here, not typed out. `discoverWorkflows()` walks
  // `.github/workflows/` and refuses a near-empty result, so a sixth workflow is scanned the day it
  // lands and a broken enumeration reds instead of reporting clean over nothing.
  for (const file of discoverWorkflows()) {
    it(`${file}: every real cargo invocation carries --locked`, () => {
      const invocations = workflowStepUnits(file).flatMap(cargoInvocationsIn);

      // Sanity check on the detector itself. A detector that stopped matching anything would pass
      // this whole suite vacuously — the exact "green over zero coverage" shape this repo's other
      // guards call out — and one that stopped matching MOST things would too, which is why this
      // is a per-file floor rather than `> 0`. See `MIN_CARGO_INVOCATIONS`. A file with no pinned
      // floor (one of the three CPE-1969 folded in, none of which runs cargo today) has floor 0:
      // it is scanned, but there is no count to protect yet.
      const floor = MIN_CARGO_INVOCATIONS[file] ?? 0;
      expect(
        invocations.length,
        `${file}: found ${invocations.length} real cargo invocations, below the pinned floor of ` +
          `${floor}. Either a workflow genuinely lost a cargo step (lower the ` +
          `floor in MIN_CARGO_INVOCATIONS and say why), or the detector has silently narrowed and ` +
          `is no longer seeing steps it used to — which is the failure this floor exists to catch.`,
      ).toBeGreaterThanOrEqual(floor);

      const missing = invocations.filter(({ line }) => !line.includes("--locked"));
      expect(
        missing.map(({ where, line }) => `${where}: ${line}`),
        "these cargo invocations are missing --locked",
      ).toEqual([]);
    });
  }

  // CPE-1969 gap 2. Shell moved OUT of a `run:` block into `.github/workflows/scripts/*.sh` used to
  // leave every guard's scope — normal refactoring, silent loss of coverage. A script that runs
  // `cargo build` is exactly as capable of rewriting a `Cargo.lock` as a `run:` block is.
  for (const file of discoverWorkflowScripts()) {
    it(`${file}: every real cargo invocation carries --locked`, () => {
      const missing = cargoInvocationsIn(scriptUnit(file)).filter(
        ({ line }) => !line.includes("--locked"),
      );
      expect(
        missing.map(({ where, line }) => `${where}: ${line}`),
        "these cargo invocations are missing --locked",
      ).toEqual([]);
    });
  }

  it("MIN_CARGO_INVOCATIONS names no file that no longer exists", () => {
    // The floors are the one hand-maintained thing left here. A renamed or deleted workflow would
    // otherwise leave a floor sitting there protecting nothing, which reads as coverage.
    const known = new Set([...discoverWorkflows(), ...discoverWorkflowScripts()]);
    expect(Object.keys(MIN_CARGO_INVOCATIONS).filter((f) => !known.has(f))).toEqual([]);
  });

  it("every tauri-action / npm run tauri build step is immediately preceded by a cargo check --locked preflight", () => {
    const problems: string[] = [];
    let anchorsSeen = 0;
    for (const file of discoverWorkflows()) {
      const doc = parseWorkflow(file);
      for (const [job, jobDoc] of Object.entries(doc.jobs ?? {})) {
        const steps = jobDoc.steps ?? [];
        steps.forEach((step, i) => {
          if (!isTauriBuildAnchor(step)) return;
          anchorsSeen += 1;
          // **Immediately** preceding, not "somewhere in the last 40 lines". Measured on CPE-1929: all
          // four anchors in the tree have the preflight as the step directly above them, which is what
          // their own comments promise ("run immediately before it"). Pinning the exact adjacency means
          // an unrelated step wedged between them is reported rather than tolerated.
          const previous = i > 0 ? steps[i - 1] : undefined;
          if (!previous || !isLockedPreflight(previous)) {
            problems.push(
              `${file} [${job}]: step "${step.name ?? "(unnamed)"}" runs a real Tauri build with no ` +
                `"cargo check --locked" preflight immediately before it`,
            );
          }
        });
      }
    }
    expect(problems).toEqual([]);
    // Vacuity: if the anchor matcher stopped recognising `tauri-action` / `npm run tauri build`, the
    // loop above would find nothing to check and pass in silence.
    expect(anchorsSeen, "no Tauri build anchor was found at all — the anchor matcher may be broken").toBe(4);
  });

  it("no extracted script drives a Tauri build, where the preflight rule cannot be expressed", () => {
    // CPE-1969 gap 2, and the one place where "a script is one step" has a consequence worth
    // stating: the preflight rule is about the step IMMEDIATELY BEFORE the anchor, and a `.sh` file
    // has no preceding step to inspect — its caller's step does. So a Tauri build inside a script is
    // not something this guard can check the preflight of; it is something the guard has to refuse.
    // None exists today (measured 2026-08-27), and this keeps it that way rather than letting one
    // land in the one place the rule goes quiet.
    const anchored = discoverWorkflowScripts()
      .map((f) => scriptUnit(f))
      .filter((u) => logicalLines(u.run).some((l) => /\bnpm run tauri build\b/.test(l)))
      .map((u) => u.where);
    expect(
      anchored,
      "a Tauri build inside an extracted script has no preceding step to carry the " +
        "`cargo check --locked` preflight — keep it in a workflow step, or teach this guard how to " +
        "find the script's caller",
    ).toEqual([]);
  });
});

// The two shapes measured slipping through the old raw-text scanner, pinned as unit tests so the
// rewrite cannot regress into them. These are the red-proof for the rewrite itself: run them against
// the pre-CPE-1929 line scanner and both fail.
describe("the detector is not fooled by the shapes that defeated the raw-text scanner (CPE-1929)", () => {
  function doc(run: string): WorkflowDoc {
    const parsed = parseYaml(run);
    if (!parsed.ok) throw new Error(parsed.error);
    return parsed.value as WorkflowDoc;
  }

  it("a backslash continuation cannot hide an unlocked invocation", () => {
    const found = cargoInvocations(
      doc(
        [
          "jobs:",
          "  demo:",
          "    steps:",
          "      - name: split",
          "        run: |",
          "          cargo \\",
          "            test --all-targets",
          "",
        ].join("\n"),
      ),
    );
    expect(found.map((f) => f.line)).toEqual(["cargo test --all-targets"]);
    expect(found.every((f) => f.line.includes("--locked"))).toBe(false);
  });

  it("a trailing # comment is not a real invocation and cannot satisfy the preflight", () => {
    const found = cargoInvocations(
      doc(
        [
          "jobs:",
          "  demo:",
          "    steps:",
          "      - name: fake",
          "        run: echo skipped   # cargo check --locked --all-targets",
          "",
        ].join("\n"),
      ),
    );
    expect(found).toEqual([]);
  });

  it("a step name that echoes a subcommand in prose is not an invocation", () => {
    const found = cargoInvocations(
      doc(["jobs:", "  demo:", "    steps:", "      - name: cargo test", "        run: echo hi", ""].join("\n")),
    );
    expect(found).toEqual([]);
  });

  it("a heredoc body that looks like a build is not an invocation", () => {
    const found = cargoInvocations(
      doc(
        [
          "jobs:",
          "  demo:",
          "    steps:",
          "      - name: doc",
          "        run: |",
          "          cat <<'EOF' > notes.txt",
          "          cargo build --release",
          "          EOF",
          "",
        ].join("\n"),
      ),
    );
    expect(found).toEqual([]);
  });
});

// CPE-1969's own red-proofs: the widened scope must actually CATCH the thing it was widened for.
// A derived list that finds the new files and then does nothing with them is the same blindness with
// a longer file listing. Each fixture is a real directory on disk, built under `.claude/tmp/` and
// removed afterwards, so these are measurements rather than claims about what the code would do.
describe("the widened scope really catches what the five-file list could not (CPE-1969)", () => {
  const scratch: string[] = [];
  afterEach(() => {
    while (scratch.length > 0) rmSync(scratch.pop()!, { recursive: true, force: true });
  });

  function fixtureRoot(files: Record<string, string>): string {
    const base = join(ROOT, ".claude", "tmp");
    mkdirSync(base, { recursive: true });
    const root = mkdtempSync(join(base, "cpe1969-lock-"));
    scratch.push(root);
    mkdirSync(join(root, ".github/workflows/scripts"), { recursive: true });
    for (const [rel, body] of Object.entries(files)) writeFileSync(join(root, rel), body, "utf8");
    return root;
  }

  function padWorkflows(files: Record<string, string>, n: number): Record<string, string> {
    for (let i = 0; i < n; i += 1) {
      files[`.github/workflows/pad${i}.yml`] =
        "jobs:\n  j:\n    steps:\n      - name: noop\n        run: echo hi\n";
    }
    return files;
  }

  it("a SIXTH workflow building without --locked is reported — the exact case the old list missed", () => {
    const root = fixtureRoot(
      padWorkflows(
        {
          ".github/workflows/newcomer.yml":
            "jobs:\n  build:\n    steps:\n      - name: build it\n        run: cargo build --release\n",
          ".github/workflows/scripts/a.sh": "echo hi\n",
          ".github/workflows/scripts/b.sh": "echo hi\n",
          ".github/workflows/scripts/c.sh": "echo hi\n",
        },
        MIN_EXPECTED_WORKFLOWS - 1,
      ),
    );
    const unlocked = discoverWorkflows(root)
      .flatMap((f) => workflowStepUnits(f, root))
      .flatMap(cargoInvocationsIn)
      .filter(({ line }) => !line.includes("--locked"));
    expect(unlocked.map((u) => u.line)).toEqual(["cargo build --release"]);
    expect(unlocked[0].where).toContain("newcomer.yml");
  });

  it("a FOURTH script building without --locked is reported — gap 2's exact case", () => {
    const root = fixtureRoot(
      padWorkflows(
        {
          ".github/workflows/scripts/a.sh": "echo hi\n",
          ".github/workflows/scripts/b.sh": "echo hi\n",
          ".github/workflows/scripts/c.sh": "echo hi\n",
          ".github/workflows/scripts/newcomer.sh":
            "#!/usr/bin/env bash\n# cargo test --locked   <- a comment must not count\nset -euo pipefail\ncargo \\\n  clippy --all-targets\n",
        },
        MIN_EXPECTED_WORKFLOWS,
      ),
    );
    const unlocked = discoverWorkflowScripts(root)
      .map((f) => scriptUnit(f, root))
      .flatMap(cargoInvocationsIn)
      .filter(({ line }) => !line.includes("--locked"));
    // The continuation is joined and the comment is stripped, exactly as in a `run:` block — the
    // two shapes CPE-1929 measured defeating the old raw-text scanner, now proven over a `.sh`.
    expect(unlocked.map((u) => u.line)).toEqual(["cargo clippy --all-targets"]);
    expect(unlocked[0].where).toBe(".github/workflows/scripts/newcomer.sh (whole script)");
  });

  it("a script's heredoc body is still inert data, not an unlocked invocation", () => {
    const root = fixtureRoot(
      padWorkflows(
        {
          ".github/workflows/scripts/a.sh": "echo hi\n",
          ".github/workflows/scripts/b.sh": "echo hi\n",
          ".github/workflows/scripts/heredoc.sh":
            "#!/usr/bin/env bash\ncat <<'EOF' > notes.txt\ncargo build --release\nEOF\ncargo test --locked\n",
        },
        MIN_EXPECTED_WORKFLOWS,
      ),
    );
    const found = discoverWorkflowScripts(root)
      .map((f) => scriptUnit(f, root))
      .flatMap(cargoInvocationsIn)
      .map((u) => u.line);
    expect(found).toEqual(["cargo test --locked"]);
  });

  it("the scan REFUSES rather than reporting clean when the enumeration comes back empty", () => {
    const root = fixtureRoot({});
    expect(() => discoverWorkflows(root)).toThrow(/near-empty/);
    expect(() => discoverWorkflowScripts(root)).toThrow(/near-empty/);
  });
});
