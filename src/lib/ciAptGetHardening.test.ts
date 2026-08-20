// CPE-1787: ci.yml had five `apt-get update && apt-get install` sites with none of the
// ForceIPv4/retry/timeout hardening gui-smoke.yml already carries for the same class of hang
// (CPE-1772: a GitHub-hosted-runner IPv6-connectivity stall can hang `apt-get update` with zero
// output before it ever falls back to IPv4). Site 3 (ffmpeg install) is the sharpest case: it hung
// with zero output for 1.5+ hours on PR #935's own CI, because `continue-on-error: true` only
// helps once a step reaches a terminal state and there was no `timeout-minutes` to make it reach
// one.
//
// Structural assertions go through `parseYaml`, the in-repo bounded-subset YAML parser
// (src/lib/preview/yaml.ts, CPE-1617), the same approach ffmpegOverrideAutoDispatch.test.ts uses
// for ci.yml -- a Reviewer round on that ticket found regex-over-raw-text assertions here can be
// satisfied by an unrelated neighbouring COMMENT (`/actions:\s*write/` matched a comment that
// merely mentioned the key). Reading `step.run`/`step['timeout-minutes']` off the PARSED object
// avoids that: a workflow comment sitting above or beside a step never becomes part of the
// parsed `run:` string or any other field value.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";

const WORKFLOWS = join(process.cwd(), ".github", "workflows");

function read(fileName: string): string {
  return readFileSync(join(WORKFLOWS, fileName), "utf8");
}

interface WorkflowStep {
  id?: string;
  name?: string;
  uses?: string;
  if?: string;
  run?: string;
  "continue-on-error"?: boolean;
  "timeout-minutes"?: number;
  [key: string]: unknown;
}

interface WorkflowJob {
  steps: WorkflowStep[];
  [key: string]: unknown;
}

interface WorkflowDoc {
  jobs: Record<string, WorkflowJob>;
}

/** Parses a workflow file with the same bounded-subset YAML parser the app ships for previewing
 *  .yml files, and fails the test with the parser's own reason if the file falls outside that
 *  subset -- so a future edit that pushes ci.yml past what this parser understands is reported
 *  here as a clear parse failure, not a silently-wrong empty result. */
function parseWorkflow(fileName: string): WorkflowDoc {
  const result = parseYaml(read(fileName));
  if (!result.ok) {
    throw new Error(`${fileName} did not parse as YAML: ${result.error}`);
  }
  return result.value as WorkflowDoc;
}

function findStep(job: WorkflowJob, name: string): WorkflowStep {
  const step = job.steps.find((s) => s.name === name);
  if (!step) {
    throw new Error(`step "${name}" not found`);
  }
  return step;
}

/** The full option string every hardened apt-get invocation in this repo carries, verbatim --
 *  matches gui-smoke.yml's two already-hardened sites. */
const HARDENING_FLAGS =
  "-o Acquire::ForceIPv4=true -o Acquire::Retries=3 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20";

/** Every `sudo apt-get ...` invocation line inside a step's `run:` script, so each can be checked
 *  individually rather than treating the whole multi-line script as one blob (a hardened `update`
 *  sitting next to an unhardened `install` would otherwise pass a whole-script substring check). */
function aptGetLines(run: string | undefined): string[] {
  return (run ?? "").split("\n").filter((line) => line.includes("apt-get"));
}

describe("ci.yml apt-get sites carry the ForceIPv4/retry/timeout hardening (CPE-1787)", () => {
  const doc = parseWorkflow("ci.yml");

  it("site 1 -- backend job's Linux system deps step is hardened on both update and install, with a timeout", () => {
    const step = findStep(doc.jobs.backend, "Install Linux system dependencies");
    const lines = aptGetLines(step.run);
    expect(lines.length).toBe(2);
    for (const line of lines) {
      expect(line).toContain(HARDENING_FLAGS);
    }
    expect(step["timeout-minutes"]).toBe(8);
  });

  it("site 2 -- crates job's attr/getfattr install step is hardened, with a timeout", () => {
    const step = findStep(doc.jobs.crates, "Install attr (getfattr) for xattr interop test");
    const lines = aptGetLines(step.run);
    expect(lines.length).toBe(2);
    for (const line of lines) {
      expect(line).toContain(HARDENING_FLAGS);
    }
    expect(step["timeout-minutes"]).toBe(5);
  });

  it("site 3 -- crates job's ffmpeg install step is hardened AND has a timeout-minutes so continue-on-error can act on a hang", () => {
    // AC2: this is the site that actually hung for 1.5+ hours on PR #935 -- continue-on-error
    // alone never saved it because the step never reached a terminal state without a cap.
    const step = findStep(doc.jobs.crates, "Install ffmpeg (video-thumb real-render test, Linux)");
    const lines = aptGetLines(step.run);
    expect(lines.length).toBe(2);
    for (const line of lines) {
      expect(line).toContain(HARDENING_FLAGS);
    }
    expect(step["continue-on-error"]).toBe(true);
    expect(typeof step["timeout-minutes"]).toBe("number");
    expect(step["timeout-minutes"]).toBeGreaterThan(0);
  });

  it("site 4 -- sidecar job's libdbus install step is hardened, with a timeout", () => {
    const step = findStep(doc.jobs.sidecar, "Install Linux system dependencies (libdbus for the keyring)");
    const lines = aptGetLines(step.run);
    expect(lines.length).toBe(2);
    for (const line of lines) {
      expect(line).toContain(HARDENING_FLAGS);
    }
    expect(step["timeout-minutes"]).toBe(8);
  });

  it("site 5 -- sidecar job's gnome-keyring install (inside the keychain round-trip step) is hardened, with a timeout", () => {
    const step = findStep(doc.jobs.sidecar, "host — real keychain round-trip (CPE-322)");
    const lines = aptGetLines(step.run).filter((line) => line.includes("gnome-keyring"));
    expect(lines.length).toBe(1);
    expect(lines[0]).toContain(HARDENING_FLAGS);
    // Unlike sites 1/2/3/4, this step was already `continue-on-error: true` before this ticket --
    // the bug was the missing timeout, not a missing continue-on-error, so assert both still hold.
    expect(step["continue-on-error"]).toBe(true);
    expect(typeof step["timeout-minutes"]).toBe("number");
    expect(step["timeout-minutes"]).toBeGreaterThan(0);
  });

  it("no apt-get invocation anywhere in ci.yml is left unhardened (regression guard against a future sixth site)", () => {
    const unhardened: string[] = [];
    for (const [jobName, job] of Object.entries(doc.jobs)) {
      for (const step of job.steps) {
        for (const line of aptGetLines(step.run)) {
          if (!line.includes(HARDENING_FLAGS)) {
            unhardened.push(`${jobName} / ${step.name}: ${line.trim()}`);
          }
        }
      }
    }
    expect(unhardened).toEqual([]);
  });
});
