// CPE-1936 N10: the publish path's expected run title was bound to nothing.
//
// `/run` publishes a DRAFT release only after confirming the run that built it succeeded. For the
// sidecar channel that run cannot be found by tag: `release-sidecar.yml` is `workflow_dispatch`-only,
// so a run's `headBranch` is the dispatched ref (`main`), never the tag. The workflow therefore sets
//
//     run-name: "Release (sidecar) ${{ inputs.tag }}"
//
// purely so the tag reaches `displayTitle`, and `.claude/commands/run.md` looks the run up with an
// EXACT match on that title. The literal `Release (sidecar) ` then lived in THREE places —
// `release-sidecar.yml`, `run.md`'s `-ceq`, and `RELEASING.md`'s prose — with nothing tying them
// together. Editing `run-name:` silently breaks the lookup.
//
// It fails CLOSED (`run.md` throws "no run found -- do not publish" rather than waving an unverified
// draft through), so this is a maintenance hazard rather than a safety hole. But it is exactly the
// provenance-claim shape CLAUDE.md's CPE-1933 rule is about: three copies, one of them a comment,
// and a green suite that vouches for none of them. So this file DERIVES the expected title from the
// workflow at run time instead of restating it.
//
// Both halves are derived, including the placeholder:
//
//   * the prefix comes from `release-sidecar.yml`'s parsed `run-name:` (parsed as YAML, so a `#`
//     comment mentioning `run-name:` cannot be read as the key);
//   * the `<TAG>` placeholder comes from `run.md`'s OWN plain-channel branch, which matches
//     `headBranch -ceq "<TAG>"` — a place the tag, and only the tag, can appear.
//
// Nothing here is transcribed, so changing `run-name:` turns this red rather than turning the publish
// path into a thrower. Red-proofed 2026-08-27 by editing `release-sidecar.yml`'s `run-name:` to
// `"Release (sidecar build) ${{ inputs.tag }}"`: both assertions failed, naming the old and new
// values; reverting restored green.
//
// Anchoring, per CPE-1933 rule 2: the `-ceq` literal is read out of `run.md`'s POWERSHELL FENCED
// BLOCKS with `#` comments stripped first, never out of the whole file. `run.md` is a document that
// talks about this lookup at length — the block being scanned already carries a comment quoting
// `displayTitle "Release (sidecar) v1.2.3-sidecar-decoy"` while explaining the decoy attack, and a
// paragraph outside every fence quotes `"Release (sidecar) <TAG>"` directly.
//
// Being honest about which half of that is load-bearing TODAY, because CPE-1929 says an unreachable
// guard reads as coverage: the FENCE filter is what excludes today's prose. Stripping `#` comments
// is not reached by any string presently in `run.md` — none of its comments spells the full
// `$_.displayTitle -ceq "…"` shape — so it would be exactly the "safe and unverifiable at once"
// pair if it were left asserted-but-untested. It is kept (a comment demonstrating the decoy in the
// obvious way is one edit away) and given its own coverage below, against a synthetic document, with
// the un-stripped reading measured alongside so the test is a literal red-then-green rather than an
// assertion about behaviour nobody checked.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";
import { fencedBlocks, POWERSHELL_LANGS } from "./markdownFences";
import { stripShellComment } from "./shellScriptLines";

const ROOT = process.cwd();
const WORKFLOW = join(".github", "workflows", "release-sidecar.yml");
const RUN_MD = join(".claude", "commands", "run.md");
const RELEASING_MD = "RELEASING.md";

/** The GitHub Actions expression `run-name:` interpolates the dispatched tag with. */
const TAG_EXPRESSION = "${{ inputs.tag }}";

function read(relPath: string): string {
  return readFileSync(join(ROOT, relPath), "utf8");
}

/** `release-sidecar.yml`'s `run-name:`, parsed as YAML so a comment that merely mentions the key
 *  cannot be mistaken for it. */
function workflowRunName(): string {
  const result = parseYaml(read(WORKFLOW));
  if (!result.ok) throw new Error(`${WORKFLOW} did not parse as YAML: ${result.error}`);
  const value = (result.value as Record<string, unknown>)["run-name"];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(
      `${WORKFLOW} has no top-level \`run-name:\`. The sidecar publish path in ${RUN_MD} finds its ` +
        `run by displayTitle and has nothing else to match on, so removing run-name breaks it.`,
    );
  }
  return value;
}

/** Every LIVE PowerShell line in a runbook: the contents of its ```powershell fences, `#` comments
 *  stripped by the repo's shared quote-aware stripper (`shellScriptLines.ts`, the module CLAUDE.md
 *  names for exactly this). Prose outside a fence never reaches here. */
function powershellLinesOf(md: string, label: string, strip = true): string[] {
  const blocks = fencedBlocks(md).filter((b) => POWERSHELL_LANGS.has(b.lang));
  expect(blocks.length, `${label} has no \`\`\`powershell blocks — the scan would be vacuous`).toBeGreaterThan(0);
  return blocks
    .flatMap((b) => b.lines.map((l) => (strip ? stripShellComment(l) : l)))
    .filter((l) => l.trim() !== "");
}

function powershellLines(relPath: string): string[] {
  return powershellLinesOf(read(relPath), relPath);
}

/** The double-quoted literal a `$_.<property> -ceq "…"` comparison in `run.md` is pinned to. Exactly
 *  one such comparison must exist per property — two would mean the publish path grew a second,
 *  unchecked copy, which is the defect this file exists to prevent. */
function ceqCandidates(lines: string[], property: string): string[] {
  const pattern = new RegExp(`\\$_\\.${property}\\s+-ceq\\s+"([^"]*)"`);
  return lines.flatMap((l) => {
    const m = pattern.exec(l);
    return m ? [m[1]] : [];
  });
}

function ceqLiteral(lines: string[], property: string): string {
  const hits = ceqCandidates(lines, property);
  expect(
    hits,
    `${RUN_MD} must contain exactly one live \`$_.${property} -ceq "…"\` comparison; found ${hits.length}`,
  ).toHaveLength(1);
  return hits[0];
}

describe("the sidecar publish path's expected run title is derived, not transcribed (CPE-1936 N10)", () => {
  it("release-sidecar.yml's run-name ends with the dispatched tag, so an exact-match lookup is possible at all", () => {
    // If the tag moved to the front or the middle, `"<prefix><TAG>"` would not be the right shape and
    // the lookup below would silently match nothing — the fail-closed thrower, forever.
    expect(workflowRunName().endsWith(TAG_EXPRESSION)).toBe(true);
  });

  it("run.md's displayTitle match equals the workflow's run-name with the tag substituted", () => {
    const lines = powershellLines(RUN_MD);
    // The placeholder is read out of run.md's OWN plain-channel branch rather than written here:
    // `headBranch` on a tag-triggered `release.yml` run IS the tag, so whatever that branch compares
    // against is, by construction, run.md's spelling of "the tag".
    const tagPlaceholder = ceqLiteral(lines, "headBranch");
    const expected = workflowRunName().replace(TAG_EXPRESSION, tagPlaceholder);
    expect(
      ceqLiteral(lines, "displayTitle"),
      `${RUN_MD} looks the sidecar run up by an exact displayTitle match. ${WORKFLOW}'s run-name is ` +
        `now ${JSON.stringify(workflowRunName())}, so the match must be ${JSON.stringify(expected)}. ` +
        `They disagree, which means /run would find no run for any tag and refuse to publish every ` +
        `sidecar draft.`,
    ).toBe(expected);
  });

  // The anchoring's own coverage. `run.md` cannot exercise this today (see the header), so it is
  // exercised against a synthetic document shaped like the one edit that would matter: a commented-out
  // older comparison sitting above the live one, in the same block.
  describe("the anchor: a `-ceq` inside a comment is not read as the live comparison", () => {
    const decoyDoc = [
      "```powershell",
      '  # was: $_.displayTitle -ceq "Release (sidecar) DECOY"',
      '  $runId = ($runs | Where-Object { $_.displayTitle -ceq "Release (sidecar) <TAG>" } |',
      "    Select-Object -First 1).databaseId",
      "```",
    ].join("\n");

    it("RED (fences only, comments left in): the decoy is a second candidate", () => {
      expect(ceqCandidates(powershellLinesOf(decoyDoc, "<synthetic>", false), "displayTitle")).toEqual([
        "Release (sidecar) DECOY",
        "Release (sidecar) <TAG>",
      ]);
    });

    it("GREEN (comments stripped): only the live comparison survives", () => {
      expect(ceqLiteral(powershellLinesOf(decoyDoc, "<synthetic>"), "displayTitle")).toBe(
        "Release (sidecar) <TAG>",
      );
    });

    it("prose outside every fence is not scanned at all", () => {
      const prose = ['Look the run up with `$_.displayTitle -ceq "Release (sidecar) PROSE"`.', "", decoyDoc].join("\n");
      expect(ceqLiteral(powershellLinesOf(prose, "<synthetic>"), "displayTitle")).toBe("Release (sidecar) <TAG>");
    });
  });

  it("RELEASING.md quotes the workflow's run-name verbatim", () => {
    const quoted = `run-name: "${workflowRunName()}"`;
    expect(
      read(RELEASING_MD).includes(quoted),
      `RELEASING.md explains the sidecar lookup by quoting ${WORKFLOW}'s run-name. It must quote the ` +
        `real one: ${JSON.stringify(quoted)}.`,
    ).toBe(true);
  });
});
