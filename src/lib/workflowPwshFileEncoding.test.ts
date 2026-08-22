// CPE-1842: the Windows code-signing step in release.yml and release-sidecar.yml read and rewrote
// `src-tauri/tauri.conf.json` through `Get-Content -Raw` + `Set-Content -Encoding utf8` under
// `shell: pwsh`. Both halves of that pair depend on the host's default text codec:
//
//   - bare `Get-Content -Raw` decodes with the process default encoding, which is CP1252 on Windows
//     PowerShell 5.1 and BOM-less UTF-8 only from PowerShell 6 onward;
//   - `Set-Content -Encoding utf8` writes a UTF-8 BOM on Windows PowerShell 5.1 (`utf8` means
//     `utf8NoBOM` only from PowerShell 6.2 onward) -- the flag name does not mean "no BOM".
//
// `tauri.conf.json` is BOM-less UTF-8 and carries a real em dash (U+2014) in
// `plugins.cli.description`, so this is not hypothetical content. Measured on 2026-08-21 against a
// scratch copy of the real manifest:
//
//   Windows PowerShell 5.1  E2 80 94  ->  C3 A2 E2 82 AC E2 80 9D   + a leading EF BB BF BOM
//   PowerShell 7.6.5        E2 80 94  ->  E2 80 94                  , no BOM
//
// So the shipped pipeline (GitHub Actions `shell: pwsh` = PowerShell 7) has been surviving on a
// default, not on a guarantee. Switching the step to `shell: powershell`, an older runner image, or
// simply copying the idiom into a new step is enough to turn it into the 5.1 row.
//
// The repo's mojibake guard (src/lib/mojibakeGuard.ts) cannot catch this one: the signing step
// patches the manifest on the runner only and never commits it, so the corrupted bytes never reach
// a tracked file for `scanRepo()` to find. This file is therefore the regression net for this
// specific defect -- it asserts the workflow SOURCE never regrows the host-default-dependent shape,
// which is the only place the corruption is observable from a checkout.
//
// Assertions read `step.run` off the PARSED workflow (via `parseYaml`, the in-repo bounded-subset
// YAML parser) rather than regexing raw file text, for the reason ciAptGetHardening.test.ts records:
// a raw-text regex is satisfiable by a neighbouring COMMENT that merely mentions the cmdlet. That
// bounds the scan to actual `run:` bodies and drops workflow-level YAML comments -- but a `run:`
// block scalar keeps its own SHELL comments verbatim, and the fix this file guards deliberately
// names `Get-Content`/`Set-Content` in one. Confirmed empirically while writing this test: the first
// draft's four "offenders" were all comment lines from the fix's own explanation. So full-line
// PowerShell/shell comments are stripped from each `run:` body before the cmdlet scan
// (`stripFullLineComments`), and the scan still sees every executable line.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";

const WORKFLOWS = join(process.cwd(), ".github", "workflows");

interface WorkflowStep {
  name?: string;
  shell?: string;
  run?: string;
  [key: string]: unknown;
}

interface WorkflowJob {
  steps?: WorkflowStep[];
  [key: string]: unknown;
}

interface WorkflowDoc {
  jobs?: Record<string, WorkflowJob>;
}

function workflowFiles(): string[] {
  return readdirSync(WORKFLOWS)
    .filter((f) => f.endsWith(".yml") || f.endsWith(".yaml"))
    .sort();
}

function parseWorkflow(fileName: string): WorkflowDoc {
  const result = parseYaml(readFileSync(join(WORKFLOWS, fileName), "utf8"));
  if (!result.ok) {
    throw new Error(`${fileName} did not parse as YAML: ${result.error}`);
  }
  return result.value as WorkflowDoc;
}

interface RunStep {
  file: string;
  job: string;
  name: string;
  run: string;
}

function allRunSteps(): RunStep[] {
  const out: RunStep[] = [];
  for (const file of workflowFiles()) {
    const doc = parseWorkflow(file);
    for (const [job, jobDef] of Object.entries(doc.jobs ?? {})) {
      for (const step of jobDef.steps ?? []) {
        if (typeof step.run === "string") {
          out.push({ file, job, name: step.name ?? "(unnamed)", run: step.run });
        }
      }
    }
  }
  return out;
}

/** Drops lines whose first non-space character is `#` -- a whole-line shell/PowerShell comment.
 *  Deliberately conservative: it does not try to strip trailing comments, because a `#` mid-line can
 *  be inside a string. A trailing comment naming a risky cmdlet would therefore still be flagged;
 *  that is the safe direction to err (a false positive is visible and fixable, a false negative is
 *  the defect this file exists to prevent). */
function stripFullLineComments(run: string): string[] {
  return run.split(/\r?\n/).map((l) => (/^\s*#/.test(l) ? "" : l));
}

/** The signing steps this ticket fixed, keyed by the step name both workflows share. */
const SIGNING_STEP_NAME = "Set up Windows code signing";

function signingSteps(): RunStep[] {
  return allRunSteps().filter((s) => s.name.startsWith(SIGNING_STEP_NAME));
}

/** PowerShell text I/O cmdlets whose encoding is host-default-dependent. `Get-Content`/`Set-Content`
 *  are the pair CPE-1842 fixed; `Add-Content`/`Out-File` are the same family and CPE-1788 already
 *  documents `Out-File`'s UTF-16LE default on 5.1. */
const RISKY_CMDLETS = /\b(Get-Content|Set-Content|Add-Content|Out-File)\b/;

/** Occurrences that are NOT a repo-file read/write, each with a recorded reason. Matched on the
 *  single `run:` LINE, so an allowlisted line is exempt but a new sibling line is not. */
const ALLOWED_LINES: { file: string; line: string; reason: string }[] = [
  {
    file: "gui-smoke.yml",
    line: "$PWD.Path | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append",
    reason:
      "Writes the runner-managed GITHUB_PATH handoff file, not a file in the repo checkout, and " +
      "the payload is an ASCII filesystem path. The Actions runner parses GITHUB_PATH itself and " +
      "tolerates the BOM; nothing in the repo is read back from it.",
  },
];

describe("workflow PowerShell file encoding (CPE-1842)", () => {
  it("parses every workflow file (so an unparseable one fails loudly instead of scanning nothing)", () => {
    const files = workflowFiles();
    expect(files.length).toBeGreaterThanOrEqual(4);
    for (const f of files) expect(() => parseWorkflow(f)).not.toThrow();
  });

  it("finds the two Windows signing steps (the scan is not silently looking at nothing)", () => {
    const signing = signingSteps();
    expect(signing.map((s) => s.file).sort()).toEqual([
      "release-sidecar.yml",
      "release.yml",
    ]);
    for (const s of signing) expect(s.run).toContain("src-tauri/tauri.conf.json");
  });

  it("no workflow step reads or writes a repo file with a host-default PowerShell codec", () => {
    const offenders: string[] = [];
    for (const step of allRunSteps()) {
      for (const [i, line] of stripFullLineComments(step.run).entries()) {
        if (!RISKY_CMDLETS.test(line)) continue;
        const allowed = ALLOWED_LINES.some(
          (a) => a.file === step.file && line.trim() === a.line,
        );
        if (!allowed) {
          offenders.push(
            `${step.file} [${step.job} / ${step.name}] run line ${i + 1}: ${line.trim()}`,
          );
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it("every ALLOWED_LINES entry still matches a real line (no stale exemptions)", () => {
    const steps = allRunSteps();
    for (const entry of ALLOWED_LINES) {
      const found = steps.some(
        (s) =>
          s.file === entry.file &&
          s.run.split(/\r?\n/).some((l) => l.trim() === entry.line),
      );
      expect(found, `stale ALLOWED_LINES entry for ${entry.file}: ${entry.line}`).toBe(
        true,
      );
    }
  });

  it("both signing steps patch tauri.conf.json through an explicit BOM-less UTF-8 encoder", () => {
    const signing = signingSteps();
    expect(signing).toHaveLength(2);
    for (const step of signing) {
      const where = `${step.file} [${step.name}]`;
      expect(step.run, where).toContain("New-Object System.Text.UTF8Encoding($false)");
      expect(step.run, where).toContain("[System.IO.File]::ReadAllText(");
      expect(step.run, where).toContain("[System.IO.File]::WriteAllText(");
      // Both .NET calls must be handed the BOM-less encoder, not left to a default overload.
      for (const call of ["ReadAllText", "WriteAllText"]) {
        const line = step.run
          .split(/\r?\n/)
          .find((l) => l.includes(`[System.IO.File]::${call}(`));
        expect(line, `${where}: no ${call} line`).toBeDefined();
        expect(line, `${where}: ${call} without $utf8NoBom`).toContain("$utf8NoBom");
      }
    }
  });
});
