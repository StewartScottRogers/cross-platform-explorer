// CPE-1802: an override window reintroduces the discipline-based net CPE-1796's mechanical guard
// replaced. When FFMPEG_BUILD_TAG_OVERRIDE_REASON is set, FFMPEG_BUILD_TAG genuinely is a rolling
// daily again (the same ~14-day pruning clock ffmpeg-pin-freshness.yml exists to watch, weekly) --
// but before this ticket, the only mitigation on offer was "someone remembers to workflow_dispatch
// the freshness check by hand". This guard asserts that ci.yml's "ffmpeg pin is a month-end anchor"
// job (`ffmpeg-pin-guard`) instead ARMS the freshness check itself automatically the moment the
// override path is taken -- and that the comments next to the override variable, in both
// release-sidecar.yml and ffmpeg-pin-freshness.yml, were updated in lockstep so neither goes stale
// (exactly the "confident but false comment" class this repo has been bitten by before, e.g.
// CPE-1796's own second item).
//
// Structural assertions (permissions, step ids, `if:`/`run:` fields, the target workflow's own
// triggers) go through `parseYaml`, the in-repo bounded-subset YAML parser (src/lib/preview/yaml.ts,
// CPE-1617) rather than regex/substring scanning -- a Reviewer round on an earlier version of this
// file found three assertions here were satisfied by an unrelated comment or by string presence
// alone, and stayed green through the exact one-line regression each claimed to catch. Parsing the
// real structure closes that gap without adding a new dependency (no yaml/js-yaml package exists in
// this repo -- see package.json).
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
  [key: string]: unknown;
}

interface WorkflowJob {
  permissions?: Record<string, string>;
  steps: WorkflowStep[];
  [key: string]: unknown;
}

interface WorkflowDoc {
  on: Record<string, unknown>;
  jobs: Record<string, WorkflowJob>;
}

/** Parses a workflow file with the same bounded-subset YAML parser the app ships for previewing
 *  .yml files, and fails the test with the parser's own reason if the file falls outside that
 *  subset -- so a future edit that pushes ci.yml/ffmpeg-pin-freshness.yml past what this parser
 *  understands is reported here as a clear parse failure, not a silently-wrong empty result. */
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

/**
 * Comments in these workflows wrap at ~100 chars as a sequence of `# ...` lines forming one flowing
 * sentence. Collapse each run of comment lines into a single space-joined string so a substring
 * check isn't sensitive to exactly where a given edit happened to wrap -- mirrors how a reader
 * actually reads these blocks (a paragraph, not discrete lines).
 */
function flattenComments(text: string): string {
  return text.replace(/\r?\n[ \t]*#[ \t]?/g, " ").replace(/[ \t]+/g, " ");
}

describe("ffmpeg override window arms its own freshness check (CPE-1802)", () => {
  it("grants the guard job actions:write (and contents:read), so it can dispatch another workflow", () => {
    // Without this, `gh workflow run` in the dispatch step below fails at runtime with a 403 --
    // silently defeating the whole point of this ticket the first time an override is actually
    // taken, since the guard step itself still passes (it only warns) and nothing else would fail
    // until someone reads the now-red dispatch step's log. Read from the PARSED `permissions:`
    // mapping, not a regex over the raw text -- an earlier version of this assertion
    // (`/actions:\s*write/`) matched the explanatory comment sitting right next to the key
    // ("# CPE-1802: actions:write lets the step below..."), so deleting the actual `actions: write`
    // line while leaving that comment in place stayed green.
    const guardJob = parseWorkflow("ci.yml").jobs["ffmpeg-pin-guard"];
    expect(guardJob.permissions?.actions).toBe("write");
    expect(guardJob.permissions?.contents).toBe("read");
  });

  it("pins id: guard on the assertion step, so the dispatch step's steps.guard.outputs reference resolves", () => {
    // If `id: guard` is ever removed, `steps.guard.outputs.override_active` in the dispatch step's
    // `if:` refers to a step id that no longer exists -- GitHub Actions evaluates that as empty, so
    // the condition is permanently false and the net silently never arms again. Nothing else in this
    // job would go red; it would just quietly stop firing, invisible until someone actually needed
    // it during a real override.
    const guardJob = parseWorkflow("ci.yml").jobs["ffmpeg-pin-guard"];
    const assertStep = findStep(
      guardJob,
      "Assert FFMPEG_BUILD_TAG is a month-end anchor, not a rolling daily",
    );
    expect(assertStep.id).toBe("guard");
  });

  it("marks override_active on the override-accepted branch only, never on the anchor-OK branch", () => {
    const guardJob = parseWorkflow("ci.yml").jobs["ffmpeg-pin-guard"];
    const assertStep = findStep(
      guardJob,
      "Assert FFMPEG_BUILD_TAG is a month-end anchor, not a rolling daily",
    );
    const script = assertStep.run ?? "";

    const anchorOkStart = script.indexOf('is a month-end anchor."');
    const overrideStart = script.indexOf(
      "is a rolling daily, not a month-end anchor -- allowed only because",
    );
    expect(anchorOkStart).toBeGreaterThan(-1);
    expect(overrideStart).toBeGreaterThan(-1);
    expect(overrideStart).toBeGreaterThan(anchorOkStart);

    const anchorOkBranch = script.slice(anchorOkStart, script.indexOf("exit 0", anchorOkStart));
    const overrideBranch = script.slice(overrideStart, script.indexOf("exit 0", overrideStart));

    // A normal push with a real month-end anchor pin must NEVER trigger an extra dispatch -- only
    // the deliberate override path should.
    expect(anchorOkBranch).not.toContain("override_active=true");
    expect(overrideBranch).toContain("override_active=true");
  });

  it("dispatches ffmpeg-pin-freshness.yml, gated on the override having fired AND on push (not PR)", () => {
    const guardJob = parseWorkflow("ci.yml").jobs["ffmpeg-pin-guard"];
    const dispatchStep = findStep(guardJob, "Fire the freshness check now that the override is live");

    expect(dispatchStep.if).toContain("steps.guard.outputs.override_active == 'true'");
    expect(dispatchStep.if).toContain("github.event_name == 'push'");
    expect(dispatchStep.run).toContain("gh workflow run ffmpeg-pin-freshness.yml");
  });

  it("the target workflow still declares workflow_dispatch, or `gh workflow run` above hard-fails", () => {
    // `gh workflow run <file>` requires the TARGET workflow, on the ref it's dispatched against, to
    // declare a `workflow_dispatch:` trigger -- without one the call errors out, and because the
    // dispatch step runs under `set -euo pipefail` that turns the guard job red on every push while
    // an override is active. This is also the cheapest available proxy for the live dispatch itself,
    // which this ticket could not verify end-to-end (would require a real override merged to `main`
    // and a real Actions run against upstream).
    const freshness = parseWorkflow("ffmpeg-pin-freshness.yml");
    expect(Object.prototype.hasOwnProperty.call(freshness.on, "workflow_dispatch")).toBe(true);
  });

  // The two tests below pin comment PROSE verbatim -- they encode the ticket's "if it stays manual,
  // say so explicitly" acceptance criterion (documenting the override variable), not the dispatch
  // mechanism itself. Mechanism coverage is entirely the five tests above.

  it("release-sidecar.yml's override comment documents the automatic dispatch, not just the manual escape hatch", () => {
    const releaseSidecarYml = flattenComments(read("release-sidecar.yml"));
    expect(releaseSidecarYml).toContain(
      "CPE-1802: setting this ALSO arms a second net automatically -- once this merges to main, ci.yml's guard job dispatches ffmpeg-pin-freshness.yml on every push to main that reaches that job, for as long as this stays set",
    );
  });

  it("ffmpeg-pin-freshness.yml's cadence comment no longer tells a human to dispatch it by hand during an override", () => {
    const freshnessYml = flattenComments(read("ffmpeg-pin-freshness.yml"));
    // The old guidance this ticket replaces -- must be gone, or the comment is now false (CPE-1796
    // already burned this repo once on a stale-but-confident comment; don't repeat it here).
    expect(freshnessYml).not.toContain(
      "is rare and self-documenting; run this workflow manually (workflow_dispatch) during that window rather than tightening the schedule for everyone to cover it.",
    );
    expect(freshnessYml).toContain(
      "no longer relies on someone remembering to workflow_dispatch this workflow by hand during that window",
    );
    expect(freshnessYml).toContain("CPE-1802");
  });
});
