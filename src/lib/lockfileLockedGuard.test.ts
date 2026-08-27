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
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";
import { logicalLines } from "./shellScriptLines";

const ROOT = process.cwd();

/** Every workflow that runs a real Rust build for this repo's own crates. `ffmpeg-pin-freshness.yml`
 *  and `release-pipeline-watchdog.yml` are deliberately excluded — neither runs `cargo` at all. */
const WORKFLOW_FILES = [
  ".github/workflows/ci.yml",
  ".github/workflows/release.yml",
  ".github/workflows/release-sidecar.yml",
  ".github/workflows/gui-smoke.yml",
  ".github/workflows/model-snapshot.yml",
];

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
  const result = parseYaml(readFileSync(join(ROOT, file), "utf8"));
  if (!result.ok) throw new Error(`${file} did not parse as YAML: ${result.error}`);
  return result.value as WorkflowDoc;
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
  for (const file of WORKFLOW_FILES) {
    it(`${file}: every real cargo invocation carries --locked`, () => {
      const invocations = cargoInvocations(parseWorkflow(file));

      // Sanity check on the detector itself: every one of these files has at least one real
      // invocation. A detector that stopped matching anything would otherwise pass this whole suite
      // vacuously — the exact "green over zero coverage" shape this repo's other guards call out.
      expect(
        invocations.length,
        `${file}: no real cargo invocation was found at all — the detector may be broken`,
      ).toBeGreaterThan(0);

      const missing = invocations.filter(({ line }) => !line.includes("--locked"));
      expect(
        missing.map(({ job, step, line }) => `${file} [${job} / ${step}]: ${line}`),
        "these cargo invocations are missing --locked",
      ).toEqual([]);
    });
  }

  it("every tauri-action / npm run tauri build step is immediately preceded by a cargo check --locked preflight", () => {
    const problems: string[] = [];
    let anchorsSeen = 0;
    for (const file of WORKFLOW_FILES) {
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
