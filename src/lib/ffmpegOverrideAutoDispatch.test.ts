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
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const WORKFLOWS = join(process.cwd(), ".github", "workflows");

function read(fileName: string): string {
  return readFileSync(join(WORKFLOWS, fileName), "utf8");
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

/** Extracts one top-level (2-space-indented) job block from a workflow's `jobs:` section by name. */
function jobBlock(yamlText: string, jobName: string): string {
  const lines = yamlText.split(/\r?\n/);
  const startIdx = lines.findIndex((l) => new RegExp(`^  ${jobName}:\\s*$`).test(l));
  if (startIdx === -1) {
    throw new Error(`job "${jobName}" not found`);
  }
  let endIdx = lines.length;
  for (let i = startIdx + 1; i < lines.length; i++) {
    if (/^  \S.*:\s*$/.test(lines[i])) {
      endIdx = i;
      break;
    }
  }
  return lines.slice(startIdx, endIdx).join("\n");
}

describe("ffmpeg override window arms its own freshness check (CPE-1802)", () => {
  const ciYml = read("ci.yml");
  const guardJob = jobBlock(ciYml, "ffmpeg-pin-guard");

  it("grants the guard job actions:write, so it can dispatch another workflow", () => {
    // Without this, `gh workflow run` in the dispatch step below fails at runtime with a 403 --
    // silently defeating the whole point of this ticket the first time an override is actually
    // taken, since the guard step itself still passes (it only warns) and nothing else would fail
    // until someone reads the now-red dispatch step's log.
    expect(guardJob).toMatch(/permissions:\s*\n(\s+#.*\n)*\s+contents:\s*read/);
    expect(guardJob).toMatch(/actions:\s*write/);
  });

  it("marks override_active on the override-accepted branch only, never on the anchor-OK branch", () => {
    const anchorOkStart = guardJob.indexOf('is a month-end anchor."');
    const overrideStart = guardJob.indexOf(
      "is a rolling daily, not a month-end anchor -- allowed only because",
    );
    expect(anchorOkStart).toBeGreaterThan(-1);
    expect(overrideStart).toBeGreaterThan(-1);
    expect(overrideStart).toBeGreaterThan(anchorOkStart);

    const anchorOkBranch = guardJob.slice(anchorOkStart, guardJob.indexOf("exit 0", anchorOkStart));
    const overrideBranch = guardJob.slice(overrideStart, guardJob.indexOf("exit 0", overrideStart));

    // A normal push with a real month-end anchor pin must NEVER trigger an extra dispatch -- only
    // the deliberate override path should.
    expect(anchorOkBranch).not.toContain("override_active=true");
    expect(overrideBranch).toContain("override_active=true");
  });

  it("dispatches ffmpeg-pin-freshness.yml, gated on the override having fired AND on push (not PR)", () => {
    const dispatchStepStart = guardJob.indexOf("Fire the freshness check now that the override is live");
    expect(dispatchStepStart).toBeGreaterThan(-1);
    const dispatchStep = guardJob.slice(dispatchStepStart);

    expect(dispatchStep).toContain("steps.guard.outputs.override_active == 'true'");
    expect(dispatchStep).toContain("github.event_name == 'push'");
    expect(dispatchStep).toContain("gh workflow run ffmpeg-pin-freshness.yml");
  });

  it("release-sidecar.yml's override comment documents the automatic dispatch, not just the manual escape hatch", () => {
    const releaseSidecarYml = flattenComments(read("release-sidecar.yml"));
    expect(releaseSidecarYml).toContain(
      "CPE-1802: setting this ALSO arms a second net automatically -- once this merges to main, ci.yml's guard job dispatches ffmpeg-pin-freshness.yml on every push to main for as long as this stays set",
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
