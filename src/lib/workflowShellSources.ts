// CPE-1969 — the ONE enumeration of "every piece of shell this repo's CI actually executes".
//
// ## The two gaps this closes
//
// Every guard built on `logicalLines` (src/lib/shellScriptLines.ts) parses its input correctly since
// CPE-1936. None of them was reading all of the input.
//
//   1. `lockfileLockedGuard.test.ts` carried a hard-coded five-entry `WORKFLOW_FILES`. There are
//      **eight** workflow files. A sixth workflow that builds Tauri without `--locked` was simply
//      never looked at — not reported, not skipped-with-a-reason, just absent.
//   2. **Nothing scanned `.github/workflows/scripts/*.sh` at all.** Three scripts (39 / 50 / 20
//      logical lines, measured 2026-08-27), invoked *by* the workflows, read by no consumer: not the
//      hang-hardening scan, not the lockfile guard, not channel purity, not either Rust consumer.
//      Every guard stops at the `run:` block boundary, so shell that has been *moved out* of a `run:`
//      into a file is out of scope by construction.
//
// Gap 2 is the more interesting one, because extracting shell from a `run:` block into a script file
// is **normal, good refactoring** — and here it silently removed that shell from every guard. Nobody
// did anything wrong; the guards' scope just never followed the code. CPE-1941 moved the catalog
// version rule out of `release.yml` into `catalog-version.sh` and the shell went dark; CPE-1796 did
// the same for the ffmpeg anchor rule; CPE-1893 for the catalog freshness arithmetic.
//
// Both are CLAUDE.md's **CPE-1932** — *enumerate, don't recall*: "Any guard over 'all the X in this
// repo' derives its list at run time (`git ls-files`, a tree walk) and **fails loudly when the list
// comes back near-empty** — a hard-coded list of the instances someone remembered is how seventeen
// `Cargo.lock` files became two."
//
// ## Why a tree walk and not `git ls-files`
//
// CLAUDE.md sanctions either. A walk is used here for two reasons. It sees an **untracked** workflow
// or script sitting in the directory — which a CI runner would happily execute if it were committed
// in the next push, and which `git ls-files` reports as absent. And it takes a `root`, so the
// near-empty refusal and the "a sixth workflow is now seen" red-proof can both be run against a real
// fixture directory rather than asserted in prose (`workflowShellSources.test.ts`).
//
// ## What "a step" means for a standalone `.sh`
//
// The `step.run` consumers are built around YAML steps. A script file has no step, so the mapping has
// to be stated rather than assumed: **one `.sh` file is exactly one unit**, and per-step and
// whole-file are the same view of it.
//
// That is not a convenience choice, it is what the file actually is:
//
//   * A YAML step is the unit of ONE shell process, ONE `timeout-minutes` cap, and ONE name to report
//     against. A script is `source`d or `bash`-ed as a single process, has no cap of its own (it
//     inherits the calling step's), and has no name but its path. So the three things a step
//     delimits all coincide with the file.
//   * `logicalLines` is a STATE MACHINE across lines — a heredoc opened on line 3 closes on line 9, a
//     backslash continuation joins across a line break. Chopping a script into pseudo-steps (per
//     function, per blank-line-separated block) would cut that state mid-flight and reintroduce
//     exactly the class of blind window CPE-1936 measured. The whole file is the only split that
//     cannot invent one.
//   * Shell FUNCTIONS are the tempting alternative unit and are the wrong one: a function body is not
//     a separately-executed script, and every consumer matches per LOGICAL LINE anyway, so finer
//     units buy nothing and cost heredoc state.
//
// The one assertion that genuinely cannot be posed against a script is `releaseHangHardening`'s
// per-step ARITHMETIC ("N curl calls under one `timeout-minutes: 5` need N x (retry-max-time +
// retry-delay + max-time) inside the cap"). A script has no cap to divide, so that check stays
// attached to the calling YAML step, where the cap lives. Presence/pairing checks — every apt
// hardened, every retrying curl bounded, every cargo `--locked` — are per-line and apply unchanged.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";

/** Repo-relative directory holding the workflow YAML files. */
export const WORKFLOWS_DIR = ".github/workflows";

/** Repo-relative directory holding shell extracted OUT of `run:` blocks. */
export const WORKFLOW_SCRIPTS_DIR = ".github/workflows/scripts";

/**
 * Enumeration sanity floor for workflow YAML files. Modelled on
 * `scripts/audit-npm-projects.mjs`'s `MIN_EXPECTED_NPM_PROJECTS` (CPE-1945) and CPE-1904's
 * `MIN_VERSION_PLACES`: a discovery that comes back empty must FAIL, never pass vacuously over
 * nothing. "0 unlocked cargo invocations across 0 workflows" is a zero-enumeration false green, and
 * it is the exact half of CLAUDE.md's rule that keeps getting left off.
 *
 * Set to 8 — today's real count — deliberately, not below it. Workflows are heavyweight, rarely
 * added and never bulk-deleted, so any number under today's means discovery broke (wrong working
 * directory, a renamed directory, a refactor of the walk below), not that CI genuinely shrank. If a
 * workflow is really retired, lower this in the same diff and say which one in the Work Log.
 */
export const MIN_EXPECTED_WORKFLOWS = 8;

/**
 * The same floor for extracted shell scripts. 3 today
 * (`catalog-freshness-check.sh`, `catalog-version.sh`, `ffmpeg-anchor-check.sh`).
 *
 * A floor of 3 rather than 1 on purpose: the failure this guards against is not only "the directory
 * vanished" but "the walk stopped classifying `.sh` as shell", which shows up as a partial result.
 * One surviving script would otherwise read as a working scan.
 */
export const MIN_EXPECTED_WORKFLOW_SCRIPTS = 3;

/** A workflow file: a `.yml`/`.yaml` sitting directly in `.github/workflows/`. */
const WORKFLOW_EXT = /\.ya?ml$/i;

/** Recognised as shell by NAME. */
const SHELL_EXT = /\.(sh|bash)$/i;

/** Recognised as documentation, deliberately not scanned. */
const DOC_EXT = /\.(md|txt)$/i;

/** Recognised as shell by SHEBANG, for an extensionless script. */
const SHELL_SHEBANG = /^#!.*\b(?:ba)?sh\b/;

function fileNames(dir: string): string[] {
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return []; // a missing directory is a near-empty enumeration, reported by the caller's floor
  }
  return entries.filter((name) => {
    try {
      return statSync(join(dir, name)).isFile();
    } catch {
      return false;
    }
  });
}

function refuseNearEmpty(kind: string, dir: string, found: string[], floor: number): void {
  if (found.length >= floor) return;
  throw new Error(
    `${kind} enumeration came back near-empty: found ${found.length} in ${dir}, floor is ${floor}. ` +
      `Either discovery is broken (wrong working directory, renamed directory, a bug in the walk) ` +
      `or CI genuinely lost files — lower the floor in src/lib/workflowShellSources.ts in the same ` +
      `diff and say which. A guard that scans nothing reports clean, which is the failure this ` +
      `refusal exists to stop (CPE-1932/CPE-1969). Found: ${found.join(", ") || "(nothing)"}`,
  );
}

/**
 * Every GitHub Actions workflow in the repo, as repo-relative paths, sorted. Derived by walking the
 * directory at run time, never a list someone remembered. Throws when the result is near-empty.
 *
 * `.github/workflows/scripts/` is a SUBdirectory and is not descended into: its contents are shell,
 * not workflows, and are enumerated by `discoverWorkflowScripts` instead.
 */
export function discoverWorkflows(root: string = process.cwd()): string[] {
  const dir = join(root, WORKFLOWS_DIR);
  const found = fileNames(dir)
    .filter((name) => WORKFLOW_EXT.test(name))
    .sort()
    .map((name) => `${WORKFLOWS_DIR}/${name}`);
  refuseNearEmpty("workflow", dir, found, MIN_EXPECTED_WORKFLOWS);
  return found;
}

/**
 * Every shell script the workflows invoke, as repo-relative paths, sorted. Throws when the result is
 * near-empty.
 *
 * A file in this directory that is neither recognisable shell nor documentation is a LOUD failure
 * rather than a silent skip. That is the whole lesson of gap 2: a file nobody classified is a file
 * nobody scans, and it took a directory sweep rather than a code review to notice. A future
 * `helper.py` here must force a decision about how it is guarded, not disappear.
 */
export function discoverWorkflowScripts(root: string = process.cwd()): string[] {
  const dir = join(root, WORKFLOW_SCRIPTS_DIR);
  const found: string[] = [];
  const unclassified: string[] = [];
  for (const name of fileNames(dir).sort()) {
    if (DOC_EXT.test(name)) continue;
    const first = readFileSync(join(dir, name), "utf8").split("\n", 1)[0] ?? "";
    if (SHELL_EXT.test(name) || SHELL_SHEBANG.test(first)) {
      found.push(`${WORKFLOW_SCRIPTS_DIR}/${name}`);
      continue;
    }
    unclassified.push(name);
  }
  if (unclassified.length > 0) {
    throw new Error(
      `${WORKFLOW_SCRIPTS_DIR} holds ${unclassified.length} file(s) that are neither recognisable ` +
        `shell (a .sh/.bash name or a sh/bash shebang) nor documentation (.md/.txt): ` +
        `${unclassified.join(", ")}. Classify them in src/lib/workflowShellSources.ts rather than ` +
        `leaving them unscanned — an unclassified file is one no guard reads (CPE-1969 gap 2).`,
    );
  }
  refuseNearEmpty("workflow script", dir, found, MIN_EXPECTED_WORKFLOW_SCRIPTS);
  return found;
}

/**
 * One executable piece of shell, uniform across both sources.
 *
 * `kind: "step"` is one workflow step's `run:` script; `kind: "script"` is one whole `.sh` file — see
 * this module's header for why the whole file is exactly one unit and not, say, one per function.
 */
export interface ShellUnit {
  kind: "step" | "script";
  /** Repo-relative path of the file this shell lives in. */
  file: string;
  /** Job name for a step; `undefined` for a script, which has no job. */
  job?: string;
  /** Step name for a step; `undefined` for a script, which has no step. */
  step?: string;
  /** A human label for failure messages: `file [job / step]`, or `file (whole script)`. */
  where: string;
  /** The raw shell text, ready for `logicalLines`. */
  run: string;
}

interface RawStep {
  name?: string;
  run?: unknown;
  [key: string]: unknown;
}
interface RawJob {
  steps?: RawStep[];
  [key: string]: unknown;
}
interface RawDoc {
  jobs?: Record<string, RawJob>;
}

/**
 * Parses a workflow with the same bounded-subset YAML parser the app ships for previewing `.yml`
 * files, and throws the parser's own reason if the file falls outside that subset — so a future edit
 * that pushes a workflow past what this parser understands surfaces as a clear parse failure, never
 * as a silently-empty (and therefore vacuously green) result.
 */
export function parseWorkflowFile(file: string, root: string = process.cwd()): RawDoc {
  const result = parseYaml(readFileSync(join(root, file), "utf8"));
  if (!result.ok) throw new Error(`${file} did not parse as YAML: ${result.error}`);
  return result.value as RawDoc;
}

/** Every `run:` step of one workflow, as units. */
export function workflowStepUnits(file: string, root: string = process.cwd()): ShellUnit[] {
  const out: ShellUnit[] = [];
  const doc = parseWorkflowFile(file, root);
  for (const [job, jobDoc] of Object.entries(doc.jobs ?? {})) {
    for (const step of jobDoc.steps ?? []) {
      if (typeof step.run !== "string") continue;
      const name = step.name ?? "(unnamed)";
      out.push({ kind: "step", file, job, step: name, where: `${file} [${job} / ${name}]`, run: step.run });
    }
  }
  return out;
}

/** One whole `.sh` file, as a single unit. See this module's header for the mapping. */
export function scriptUnit(file: string, root: string = process.cwd()): ShellUnit {
  return {
    kind: "script",
    file,
    where: `${file} (whole script)`,
    run: readFileSync(join(root, file), "utf8"),
  };
}

/**
 * Every piece of shell CI executes: one unit per workflow `run:` step, plus one per extracted script.
 * This is the list any "no X is left unhardened / unlocked / unguarded anywhere" scan should walk.
 */
export function allShellUnits(root: string = process.cwd()): ShellUnit[] {
  const out: ShellUnit[] = [];
  for (const file of discoverWorkflows(root)) out.push(...workflowStepUnits(file, root));
  for (const file of discoverWorkflowScripts(root)) out.push(scriptUnit(file, root));
  return out;
}
