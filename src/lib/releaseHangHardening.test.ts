// CPE-1824: the release pipeline (`release.yml`, `release-sidecar.yml`) and ci.yml's non-apt-get
// package fetches (`brew`, `choco`, the pdfium `curl` sites) carried the same unhardened-fetch hang
// class CPE-1787 fixed for ci.yml's five `apt-get` sites -- a stalled mirror/CDN with none of
// ForceIPv4/retry/timeout hardening (or, for `curl`, no `--max-time`/`--connect-timeout` bounding a
// STALLED transfer, since `--retry` only helps once a transfer reaches a terminal state) rides to
// the job's 360-minute default instead of failing fast. A hang in the release pipeline is worse than
// one in CI: nobody is watching a release build the way someone waits on a PR, so the first symptom
// is a draft release with no installer assets.
//
// Structural assertions go through `parseYaml`, the in-repo bounded-subset YAML parser
// (src/lib/preview/yaml.ts, CPE-1617) -- the same approach ciAptGetHardening.test.ts (CPE-1787) and
// releaseSidecarDownloadBodyGuard.test.ts (CPE-1764) use, adopted after a Reviewer round on CPE-1787
// found a regex-over-raw-text guard there could be satisfied by an unrelated neighbouring COMMENT
// rather than the key it claimed to check. Reading `step.run`/`step['timeout-minutes']` off the
// PARSED object means a workflow comment sitting above or beside a step can never be mistaken for
// the step's real fields.
import { describe, it, expect, afterEach } from "vitest";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";
import { logicalLines } from "./shellScriptLines";
import { HARDENING_FLAGS, APT_COMMAND_WORD } from "./aptGetHardening";
import {
  MIN_EXPECTED_WORKFLOWS,
  allShellUnits,
  discoverWorkflowScripts,
  discoverWorkflows,
} from "./workflowShellSources";

const WORKFLOWS = join(process.cwd(), ".github", "workflows");

function read(fileName: string): string {
  return readFileSync(join(WORKFLOWS, fileName), "utf8");
}

interface WorkflowStep {
  name?: string;
  run?: string;
  "continue-on-error"?: boolean;
  "timeout-minutes"?: number | string;
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
 *  subset -- so a future edit that pushes a workflow past what this parser understands is reported
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

// CPE-1950: HARDENING_FLAGS and APT_COMMAND_WORD are IMPORTED from `src/lib/aptGetHardening.ts` (at
// the top of this file) instead of being re-declared here.
//
// They used to be local copies under the comment "Verbatim from ciAptGetHardening.test.ts … reused
// rather than re-derived" -- and that claim was ALREADY FALSE when CPE-1950 read it. CPE-1916 widened
// the command-word lookbehind in ciAptGetHardening.test.ts from `(?<![\w-])` to `(?<![\w\-/])`, so a
// path segment like `/etc/apt/sources.list.d/` stopped counting as an apt invocation THERE and kept
// counting HERE. Two suites, both green, both claiming to hold the same regex, holding two. Nothing
// could have reddened: "verbatim" was prose. One declaration, imported by both, is the fix.
//
// SIDE EFFECT of unifying on the CPE-1916 (wider) lookbehind, stated rather than discovered later:
// this file now also excludes `/` before the command word, so an apt invocation written with an
// ABSOLUTE PATH -- `/usr/bin/apt-get update` -- would stop being counted as a site here, exactly as it
// already did in ciAptGetHardening.test.ts. No workflow this file reads contains such a line today
// (checked across release.yml and release-sidecar.yml), and the narrower alternative reintroduces the
// `/etc/apt/sources.list.d/` false positive CPE-1916 fixed. If a path-qualified apt invocation is ever
// added, this filter will miss it -- widen the shared regex in aptGetHardening.ts, not one copy.

// stripShellComment()/logicalLines() now live in src/lib/shellScriptLines.ts (CPE-1908
// round 2) so channelPurityCoverage.test.ts can reuse the exact same comment/continuation
// handling instead of a second hand-rolled stripper. Imported at the top of this file.

function aptGetLines(run: string | undefined): string[] {
  return logicalLines(run).filter((line) => APT_COMMAND_WORD.test(line));
}

/** Matches each flag as an isolated FLAG WORD. The `(?![\w-])` tail is what keeps `--retry` from
 *  also matching `--retry-delay`/`--retry-all-errors`/`--retry-connrefused`/`--retry-max-time`,
 *  which are different options entirely -- a line carrying only `--retry-delay` is not retrying. */
const RETRY_FLAG = /(?<![\w-])--retry(?![\w-])/;
/** `curl` as an isolated command word, hoisted out of `curlLines` (CPE-1969) so the derived
 *  whole-repo scan below and the per-workflow helper match on exactly one regex rather than two. */
const CURL_COMMAND_WORD = /(?<![\w-])curl(?![\w-])/;
const MAX_TIME_FLAG = /(?<![\w-])--max-time(?![\w-])/;
const RETRY_MAX_TIME_FLAG = /(?<![\w-])--retry-max-time(?![\w-])/;

/** Builds a matcher for a flag set to an EXACT value. The `(?![\d.])` tail is the whole point and
 *  is not decoration: `"--retry-max-time 200".includes("--retry-max-time 20")` is **true**, so the
 *  substring spot checks these tests used to rely on pinned a PREFIX, not a value. Demonstrated
 *  under CPE-1849 -- setting the head_check site to `--retry-max-time 200` (5 x 230s against a
 *  300s cap, comfortably broken) left every test green. Any assertion that a numeric flag holds a
 *  particular value must go through this, never through `toContain`. */
function flagValue(flag: string, value: number): RegExp {
  return new RegExp(`(?<![\\w-])${flag}\\s+${value}(?![\\d.])`);
}

/** Reads a numeric flag's value off a joined shell line, or null if the flag is absent. Used by the
 *  arithmetic assertion below, which needs the NUMBERS, not merely the flags' presence. */
function flagNumber(line: string, flag: string): number | null {
  const m = line.match(new RegExp(`(?<![\\w-])${flag}\\s+(\\d+(?:\\.\\d+)?)(?![\\d.])`));
  return m ? Number(m[1]) : null;
}

/** Every real curl invocation in a parsed workflow, tagged with where it lives, read as LOGICAL
 *  lines (continuations joined, comments stripped) so that neither line-wrapping nor an
 *  explanatory comment can move a flag out of the scan's view. The `run` blocks in these workflows
 *  carry long comments that NAME these very flags, so letting one count as an invocation would let
 *  a comment satisfy or falsely trip the assertions below -- the same comment-vs-key confusion
 *  CPE-1787's Reviewer round found in a regex-over-raw-text guard. */
function curlLines(doc: WorkflowDoc): { where: string; line: string }[] {
  const found: { where: string; line: string }[] = [];
  for (const [jobName, job] of Object.entries(doc.jobs)) {
    for (const step of job.steps) {
      for (const line of logicalLines(step.run)) {
        if (!CURL_COMMAND_WORD.test(line)) continue;
        found.push({ where: `${jobName} / ${step.name}`, line });
      }
    }
  }
  return found;
}

// CPE-1849. Every other assertion in this file reads the REAL workflows, which means a helper is
// only ever exercised on the inputs those workflows happen to contain today. stripShellComment()'s
// quote-awareness was exercised by none of them -- replacing its body with a naive
// `line.slice(0, line.indexOf("#"))` left the whole suite green -- so the property the comment on
// logicalLines() calls "load-bearing in the SAFE direction" was pure prose.
//
// These tests supply the inputs the workflows do not. The first is the one that matters: a `#`
// inside a quoted URL fragment must NOT truncate the line, because that direction fails SILENTLY --
// the curl vanishes from the scan and the pairing rule reports a clean pass on an unbounded site.
// The others cover the loud direction, which is a correctness bug but never a safety hole.
describe("logicalLines() handles shell comments and continuations (CPE-1849)", () => {
  it("does not truncate a curl at a `#` inside a quoted URL fragment -- the SILENT failure direction", () => {
    const run = `curl --fail --retry 3 --max-time 20 -sS -o /tmp/x "https://example.com/a#frag"`;
    const [line] = logicalLines(run);
    expect(line).toContain("https://example.com/a#frag");
    // The point of not truncating: the flags must still be visible to the pairing scan.
    expect(RETRY_FLAG.test(line)).toBe(true);
    expect(MAX_TIME_FLAG.test(line)).toBe(true);
    expect(RETRY_MAX_TIME_FLAG.test(line)).toBe(false);
  });

  it("still strips a real trailing comment that follows code", () => {
    const [line] = logicalLines(`echo hi # curl --fail --retry 3 --max-time 20 https://x`);
    expect(line).toBe("echo hi");
    expect(RETRY_FLAG.test(line)).toBe(false);
  });

  it("drops a whole-line comment", () => {
    expect(logicalLines(`# curl --retry 3 --max-time 20 https://x`)).toEqual([]);
  });

  it("does not treat a `#` mid-word as opening a comment", () => {
    const [line] = logicalLines(`curl -sS https://example.com/a#frag --retry 3 --max-time 20`);
    expect(line).toContain("a#frag");
    expect(MAX_TIME_FLAG.test(line)).toBe(true);
  });

  it("joins a backslash continuation before matching, including across three physical lines", () => {
    const run = ["curl --fail --retry 3 \\", "  --max-time 20 \\", "  -sS https://example.com/x"].join("\n");
    const lines = logicalLines(run);
    expect(lines.length).toBe(1);
    expect(RETRY_FLAG.test(lines[0]) && MAX_TIME_FLAG.test(lines[0])).toBe(true);
  });
});

describe("release.yml apt-get sites carry the ForceIPv4/retry/timeout hardening (CPE-1824)", () => {
  const doc = parseWorkflow("release.yml");

  it("release job's Linux system deps step is hardened on both update and install, with a timeout", () => {
    const step = findStep(doc.jobs.release, "Install Linux system dependencies");
    const lines = aptGetLines(step.run);
    expect(lines.length).toBe(2);
    for (const line of lines) {
      expect(line).toContain(HARDENING_FLAGS);
    }
    expect(step["timeout-minutes"]).toBe(8);
    // This step has no continue-on-error: a cap here converts a silent hang into a hard, fast
    // failure of the release job (there was nothing for continue-on-error to swallow before, and
    // still isn't -- the change is failing in 8 minutes instead of riding to 360).
    expect(step["continue-on-error"]).toBeUndefined();
  });

  it("catalog job's libdbus install step is hardened, with a timeout", () => {
    const step = findStep(doc.jobs.catalog, "Install libdbus (host crate's Linux keyring dep)");
    const lines = aptGetLines(step.run);
    expect(lines.length).toBe(2);
    for (const line of lines) {
      expect(line).toContain(HARDENING_FLAGS);
    }
    expect(step["timeout-minutes"]).toBe(5);
    expect(step["continue-on-error"]).toBeUndefined();
  });

  it("no apt/apt-get invocation anywhere in release.yml is left unhardened", () => {
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

describe("release-sidecar.yml apt-get + curl sites carry hang hardening (CPE-1824)", () => {
  const doc = parseWorkflow("release-sidecar.yml");

  it("release-sidecar job's Linux system deps step is hardened on both update and install, with a timeout", () => {
    const step = findStep(doc.jobs["release-sidecar"], "Install Linux system dependencies");
    const lines = aptGetLines(step.run);
    expect(lines.length).toBe(2);
    for (const line of lines) {
      expect(line).toContain(HARDENING_FLAGS);
    }
    expect(step["timeout-minutes"]).toBe(8);
    // No continue-on-error here either -- same "cap converts a silent hang into a hard failure"
    // shape as release.yml's site above (this step builds a required native dep for the bundle).
    expect(step["continue-on-error"]).toBeUndefined();
  });

  it("no apt/apt-get invocation anywhere in release-sidecar.yml is left unhardened", () => {
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

  it("'Stage native deps' step carries a per-matrix timeout-minutes (none existed before CPE-1824)", () => {
    const step = findStep(doc.jobs["release-sidecar"], "Stage native deps — ffmpeg + pdfium (CPE-1258)");
    expect(step["timeout-minutes"]).toBe("${{ matrix.platform == 'macos-latest' && 35 || 12 }}");
    // This step is not continue-on-error: it stages required build inputs, so the cap must fail
    // the job outright rather than being swallowed.
    expect(step["continue-on-error"]).toBeUndefined();
  });

  it("the fetch() helper's curl call bounds both the connect phase and the whole transfer", () => {
    const step = findStep(doc.jobs["release-sidecar"], "Stage native deps — ffmpeg + pdfium (CPE-1258)");
    const run = step.run ?? "";
    const fetchLine = run.split("\n").find((l) => l.trim().startsWith("code=$(curl"));
    expect(fetchLine).toBeDefined();
    expect(fetchLine).toContain("--connect-timeout 15");
    expect(fetchLine).toContain("--max-time 240");
  });

  it("the verify_btbn_checksum() helper's curl call bounds both the connect phase and the whole transfer", () => {
    const step = findStep(doc.jobs["release-sidecar"], "Stage native deps — ffmpeg + pdfium (CPE-1258)");
    const run = step.run ?? "";
    const sumsLine = run.split("\n").find((l) => l.trim().startsWith("sums_code=$(curl"));
    expect(sumsLine).toBeDefined();
    expect(sumsLine).toContain("--connect-timeout 15");
    expect(sumsLine).toContain("--max-time 60");
  });
});

/** curl's own worst case for ONE invocation that retries, in seconds.
 *
 *  `--retry-max-time` is checked BEFORE each new retry is started; a retry permitted at an elapsed
 *  time just under the limit then sleeps `--retry-delay` and runs a further attempt which is
 *  allowed to finish, so the supremum is `retry-max-time + retry-delay + max-time`.
 *
 *  THE `retry-delay` TERM IS NOT PEDANTRY -- omitting it makes the formula WRONG, not merely
 *  loose, and both CPE-1824 and CPE-1849's first round omitted it. Measured on curl 8.21.0 against
 *  a server that accepts and then sends nothing:
 *
 *      --max-time 2 --retry 10 --retry-delay 3 --retry-max-time 4  ->  7,928 ms   (rmt+mt = 6,000: EXCEEDED)
 *      --max-time 2 --retry 10 --retry-delay 2 --retry-max-time 4  ->  7,039 ms   (rmt+mt = 6,000: EXCEEDED)
 *      --max-time 2 --retry 10 --retry-delay 1 --retry-max-time 4  ->  5,581 ms   (inside 6,000)
 *      --max-time 6 --retry  5 --retry-delay 1 --retry-max-time 10 -> 14,379 ms   (inside 16,000)
 *
 *  The last two are why the omission survived review twice: a 1s delay is too small a term to push
 *  the total past `rmt + mt`, and both earlier scaled experiments happened to use `--retry-delay 1`.
 *  A confirming experiment chosen from inside the wrong model confirms the wrong model.
 *
 *  ALL THREE FLAGS ARE REQUIRED; an absent one returns null rather than a default. An earlier cut
 *  defaulted a missing `--retry-delay` to 1 "as a floor", which is the wrong shape for this file
 *  because of the DIRECTION the error runs: curl's default backoff is *exponential*, so the sleep
 *  before the final permitted retry can be 8s or 16s, not 1s. Substituting 1 therefore UNDER-states
 *  the worst case, and an under-stated worst case makes the assertion below PASS a site that is
 *  genuinely broken -- a guard that fails open. Returning null makes the assertion fail loudly with
 *  the offending line instead, which is the only safe direction for a check whose whole job is to
 *  refuse a bound that does not hold.
 *
 *  Dead code today -- every retrying curl across all six workflows carries an explicit
 *  `--retry-delay` (the two here at 2, ci.yml's three at 3) -- but the shape matters more than
 *  whether it currently fires. */
function curlWorstCaseSeconds(line: string): number | null {
  const maxTime = flagNumber(line, "--max-time");
  const retryMaxTime = flagNumber(line, "--retry-max-time");
  const retryDelay = flagNumber(line, "--retry-delay");
  if (maxTime === null || retryMaxTime === null || retryDelay === null) return null;
  return retryMaxTime + retryDelay + maxTime;
}

// CPE-1849. Round 1 of this ticket pinned only the INPUTS to the sizing (the cap, the call count)
// and left the arithmetic itself to prose, repeating CPE-1824's documented "enforces PAIRING, never
// SIZING" gap one ticket later. That gap was then shown to be live on BOTH terms of the product:
// raising `--max-time` from 30 to 300 left every test green, and so did `--retry-max-time 200`
// (which `toContain("--retry-max-time 20")` matches as a prefix -- see flagValue()). A file that
// has now had two tickets about arithmetic nobody was checking should check the arithmetic.
//
// So this block computes it. For each site: calls x (retry-max-time + retry-delay + max-time) must
// fit inside the step's `timeout-minutes`, with margin. The margin is the substantive requirement,
// not a rounding allowance -- it is what makes curl lose on its OWN terms (a real exit code, and an
// http_code of 000 the step classifies as inconclusive) instead of being killed opaquely by the
// runner partway through, which is the exact failure this ticket fixed.
//
// This assertion would have caught finding 2 (the missing retry-delay term) by construction, and
// it is deliberately written to fail on the numbers rather than to encode the specific value 20 --
// a future edit may raise the cap, drop a call, or change --max-time, and any of those is fine so
// long as the product still fits.
//
// WHY ci.yml's THREE pdfium SITES ARE NOT INCLUDED, since the helper is general enough to cover
// them and the honest answer is not "scope". Run the corrected formula over them and they come to
// 150 + 3 + 180 = 333s against a `timeout-minutes: 6` (360s) cap: inside it, so NOT broken, but
// only 27s / 7.5% of margin -- under the 10% this block requires, so folding them in would turn the
// guard red on work this ticket did not do. (CPE-1824 computed 330s there, omitting the same
// retry-delay term; the corrected figure is 333s and ci.yml's comment now says so at its source.)
//
// Three reasons, in order of weight, for leaving them out rather than folding them in:
//   1. They are NOT BROKEN -- 333 < 360. An assertion that reddens CI over a non-defect is a false
//      alarm, and the first thing anyone would do with it is turn it off.
//   2. Both alternatives are worse INSIDE THIS PR. Loosening MIN_MARGIN_FRACTION to 0.075 would
//      turn the threshold into a DESCRIPTION OF THE STATUS QUO rather than a requirement; tightening
//      ci.yml's 150 to 140 would be an unmeasured behaviour change to the release-critical fetch
//      path, made from inside a ticket about a different workflow.
//   3. The reason is recorded HERE, in code, where the next person to touch this guard reads it --
//      the same standard CPE-1824's deleted exclusion note was held to.
// Whether 27s is enough margin for a step that also untars and copies is a real question with a
// real answer, and it belongs to whoever picks that up (CPE-1860).
describe("ffmpeg-pin-freshness.yml's HEAD-check sizing is arithmetically sound (CPE-1849)", () => {
  const doc = parseWorkflow("ffmpeg-pin-freshness.yml");

  /** Fraction of the cap that must remain unused. 10% of 300s is 30s -- comfortably more than the
   *  step's own non-curl work (echo/case/printf, milliseconds) while still refusing a value that
   *  merely grazes the cap.
   *
   *  An INTERIM value, argued rather than measured, and deliberately not measured before merge. The
   *  strongest evidence it is not tuned to flatter its own subject: the two sites it guards sit at
   *  260/300 = 13.3% margin, comfortably clear rather than grazing. A threshold picked to pass its
   *  own work would sit just under 13.3%. It also errs STRICT -- a wrong interim value produces a
   *  loud red, never a silent pass. (CPE-1860 also notes it is a pure fraction with no absolute
   *  floor, so on a site with a small cap 10% could be only a few seconds.) */
  const MIN_MARGIN_FRACTION = 0.1;

  for (const [stepName, calls] of [
    ["HEAD-check pinned assets", 5],
    ["Validate the recommendation before publishing it", 2],
  ] as const) {
    it(`${stepName}: ${calls} x curl worst case fits inside the step's timeout-minutes`, () => {
      const step = findStep(doc.jobs["check-pins"], stepName);
      const capSeconds = Number(step["timeout-minutes"]) * 60;
      expect(Number.isFinite(capSeconds)).toBe(true);

      const retrying = logicalLines(step.run).filter(
        (l) => /(?<![\w-])curl(?![\w-])/.test(l) && RETRY_FLAG.test(l),
      );
      // One shared curl line per step -- five head_check CALLS, but one curl; two loop iterations,
      // but one curl. The multiplier is the call count, asserted separately below.
      expect(retrying.length).toBe(1);

      const worst = curlWorstCaseSeconds(retrying[0]);
      // Fails loudly on an absent flag rather than defaulting one -- see curlWorstCaseSeconds().
      expect(
        worst,
        `need --max-time, --retry-max-time AND --retry-delay to compute a worst case; ` +
          `one is missing from: ${retrying[0]}`,
      ).not.toBeNull();

      const total = calls * (worst as number);
      // Reported as a message so a failure states the real numbers rather than "expected true".
      expect(
        total,
        `${stepName}: ${calls} calls x ${worst}s = ${total}s against a ${capSeconds}s cap ` +
          `(needs < ${capSeconds * (1 - MIN_MARGIN_FRACTION)}s to keep ${MIN_MARGIN_FRACTION * 100}% margin)`,
      ).toBeLessThan(capSeconds * (1 - MIN_MARGIN_FRACTION));
    });
  }
});

// The companion to the arithmetic above: it multiplies by a call count, and nothing in a curl line
// reveals that count. These two assertions pin it, plus the cap the arithmetic divides into. Add a
// sixth head_check or a third loop entry and the product changes without any flag changing.
describe("ffmpeg-pin-freshness.yml's HEAD-check sizing inputs are pinned (CPE-1849)", () => {
  const doc = parseWorkflow("ffmpeg-pin-freshness.yml");

  it("'HEAD-check pinned assets' still makes exactly five head_check calls under a 5-minute cap", () => {
    const step = findStep(doc.jobs["check-pins"], "HEAD-check pinned assets");
    expect(step["timeout-minutes"]).toBe(5);
    // Count CALLS, not the definition: `head_check() {` declares it, `head_check "label" "$URL"`
    // invokes it. Only the invocations multiply the worst case.
    const calls = logicalLines(step.run).filter((l) => /^head_check\s+"/.test(l));
    expect(calls.length).toBe(5);
    // No continue-on-error: this step's failure is the workflow's whole signal, so the cap must
    // stay a hard failure rather than something swallowed into a green run.
    expect(step["continue-on-error"]).toBeUndefined();
  });

  it("'Validate the recommendation before publishing it' still HEAD-checks two URLs under a 5-minute cap", () => {
    const step = findStep(doc.jobs["check-pins"], "Validate the recommendation before publishing it");
    expect(step["timeout-minutes"]).toBe(5);
    // Here the multiplier is the `for entry in ...` list, not repeated call lines: one curl inside
    // a two-element loop. Assert the loop is still two-element, since that is the N in the sizing.
    const loop = logicalLines(step.run).find((l) => l.startsWith("for entry in "));
    expect(loop).toBe('for entry in "win64 ${win64_url}" "linux64 ${linux64_url}"; do');
    expect(step["continue-on-error"]).toBeUndefined();
  });
});

describe("ci.yml brew/choco/curl (pdfium) sites carry hang hardening (CPE-1824)", () => {
  const doc = parseWorkflow("ci.yml");

  it("crates job's brew ffmpeg install step has a step-level timeout (brew has no CLI timeout flag of its own)", () => {
    const step = findStep(doc.jobs.crates, "Install ffmpeg (video-thumb real-render test, macOS)");
    expect(step.run).toContain("brew install ffmpeg");
    expect(step["timeout-minutes"]).toBe(10);
    // Pre-existing continue-on-error must still hold -- the cap should fail the STEP fast, and
    // continue-on-error is what then swallows that into a non-fatal job outcome.
    expect(step["continue-on-error"]).toBe(true);
  });

  it("crates job's choco ffmpeg install step carries choco's own --execution-timeout AND a step-level backstop", () => {
    const step = findStep(doc.jobs.crates, "Install ffmpeg (video-thumb real-render test, Windows)");
    expect(step.run).toContain("--execution-timeout=480");
    expect(step["timeout-minutes"]).toBe(10);
    expect(step["continue-on-error"]).toBe(true);
  });

  it("crates job's pdfium curl sites (Linux/macOS/Windows) all bound connect + total transfer time, with a step timeout", () => {
    for (const [stepName, urlFragment] of [
      ["Install pdfium prebuilt (pdf-thumb real-render test, Linux)", "pdfium-linux-x64.tgz"],
      ["Install pdfium prebuilt (pdf-thumb real-render test, macOS)", "pdfium-mac-arm64.tgz"],
      ["Install pdfium prebuilt (pdf-thumb real-render test, Windows)", "pdfium-win-x64.tgz"],
    ] as const) {
      const step = findStep(doc.jobs.crates, stepName);
      const run = step.run ?? "";
      const curlLine = run.split("\n").find((l) => l.includes("curl") && l.includes(urlFragment));
      expect(curlLine, `curl line for ${stepName}`).toBeDefined();
      expect(curlLine).toContain("--connect-timeout 15");
      expect(curlLine).toContain("--max-time 180");
      // --retry was already present pre-CPE-1824 -- assert it's still there, since --retry and
      // --max-time cover DIFFERENT failure modes (retryable terminal errors vs. an open stall) and
      // neither should be dropped in favour of the other.
      expect(curlLine).toContain("--retry 5");
      // ...and because --retry RESETS the --max-time counter, --retry-max-time is what actually
      // bounds the series. 150 + a --retry-delay 3 + one in-flight attempt's 180 = 333s, inside
      // the 360s step cap. (CPE-1824 wrote 330 here, omitting the delay term -- see CPE-1849's
      // correction of the formula at the arithmetic assertion below. Its conclusion survives.)
      // flagValue(), not toContain: "--retry-max-time 1500" contains "--retry-max-time 150".
      expect(curlLine).toMatch(flagValue("--retry-max-time", 150));
      expect(step["timeout-minutes"]).toBe(6);
      expect(step["continue-on-error"]).toBe(true);
    }
  });
});

// CPE-1824 round 2. The first cut of this PR asserted --retry and --max-time were both present and
// described --max-time in a code comment as bounding "the ENTIRE curl invocation including all
// --retry attempts". That is backwards. curl's own docs for --max-time say: "If you enable retrying
// the transfer (--retry) then the maximum time counter is reset each time the transfer is retried.
// You can use --retry-max-time to limit the retry time." So `--retry 5 --max-time 180` has a curl-
// level worst case near 5x180s plus delays, not 180s -- the step-level `timeout-minutes` was doing
// all the real bounding while the comment credited curl.
//
// Confirmed by measurement afterwards, not only by reading the docs: run against a server that
// accepts the connection and then sends nothing, the ci.yml flag set WITHOUT --retry-max-time took
// 1101s (18.35 min) and printed six separate "Operation timed out after 1800xx ms" -- one full 180s
// cycle for the initial attempt plus each of the 5 retries: PLAIN --retry already retries a timeout
// (curl's retry.md lists "a timeout" FIRST among the transient errors it covers) and every retry
// then gets a FRESH max-time clock. With --retry-max-time 150 the same test ends at 182s, exit 28,
// one timeout message.
//
// --retry-all-errors is NOT the cause, and this file must not imply it is. Measured counterexample:
// `--retry 2 --retry-delay 1 --max-time 8` with NO --retry-all-errors still burned three full
// attempts (~29s, three timeout messages). Dropping --retry-all-errors would fix nothing. The
// hazard is --retry + --max-time, full stop -- which is exactly what the rule below keys on.
//
// The control matters just as much: release-sidecar.yml's curl calls, which pass NO --retry, each
// died on their first and only attempt at their own --max-time -- so --max-time really does bound
// the whole invocation there. (Observed times run ~10-20ms over nominal, e.g. 60016ms for the 60s
// call; the property is "one attempt, dies at --max-time", not a sub-millisecond figure.) The
// original claim was wrong ONLY where --retry appears; nothing here should over-correct into
// asserting --max-time is never a whole-invocation bound.
//
// WHAT THIS FILE CAN AND CANNOT DO. Every assertion here is STRUCTURAL -- it parses YAML and reads
// flags off the text; it never executes curl. That is precisely why the first round of this guard
// passed while the behaviour its comments described was wrong: a semantic interaction between two
// flags is invisible to a structural check. The assertion below is the structural PROXY for that
// semantic property -- "if you retry, you must also bound the retry series" is a flag-pairing rule a
// parser CAN enforce, standing in for a timing behaviour it cannot observe. Treat it as a tripwire
// against the known mistake, not as proof the timing is right; that part came from measurement.
//
// Two specific limits, so nobody over-trusts a green run:
//   1. It enforces PAIRING, never SIZING. `--retry 3 --retry-max-time 9999 --max-time 20` passes
//      here (verified by injecting exactly that into release.yml -- 15 passed). Nothing checks the
//      arithmetic against the step's timeout-minutes. The three pdfium sites are protected from a
//      nonsense value only because the spot check hard-codes the literal "--retry-max-time 150" --
//      a string match, not an arithmetic one. A new site gets pairing enforced and sizing trusted.
//   2. It reads shell TEXT. Round 2's version split on physical newlines, so a backslash
//      continuation split the pair across two lines and evaded it silently; logicalLines() now
//      joins continuations and strips comments first, but the scan is still lexical, not a shell
//      parser -- a flag assembled from a variable ($CURL_OPTS) would not be seen.
//
// Attacks this guard was verified to CATCH, for the next reader's calibration: a brand-new
// `curl --fail --retry 3 --max-time 20` inserted into release.yml; the same split across a
// backslash continuation; and adding `--retry 5` to release-sidecar.yml's fetch() call (so that
// file's assertion is demonstrably not vacuous). Verified NOT to false-positive: offending flags
// sitting in a `#` comment, whether the comment starts the line or trails real code.
//
// This is the assertion that stops that reasoning error recurring: it is a generic scan of every
// curl line in these workflows rather than a spot check on the three known sites, so a NEW curl
// added anywhere in them with --retry + --max-time and no --retry-max-time fails here.
//
// CPE-1849 folded .github/workflows/ffmpeg-pin-freshness.yml into GUARDED. CPE-1824 had left it out
// with a note saying it was deliberate rather than missed; that exclusion is now spent and the note
// is gone with it, because a stale "deliberately out of scope" comment is itself the kind of
// once-true-now-false claim this file exists to stop.
//
// Its two sites (the `head_check()` call in "HEAD-check pinned assets", and the candidate-URL call
// in "Validate the recommendation before publishing it") both gained `--retry-max-time 20` first.
// The order mattered: adding the file here before fixing them turns the guard red -- which is
// exactly what CPE-1849 did on purpose, as its pre-fix check, and got both sites reported.
//
// Why 20 and not 150 like the pdfium sites: the arithmetic is per-step, never a house number. The
// "HEAD-check pinned assets" step makes FIVE head_check calls under ONE `timeout-minutes: 5`, so
// the worst case that has to fit inside 300s is 5 x (retry-max-time + retry-delay + max-time), not
// one of them. That gives 5 x 52 = 260s, 40s of margin. Measured, not just derived: without
// --retry-max-time one call took 126.7s against a stalling server (4 x 30s + 3 x 2s, four timeout
// messages), so five calls is ~634s -- the step used to die by opaque runner kill during the THIRD
// call, leaving assets 4 and 5 unchecked despite that step's deliberate `set -uo pipefail` (not -e)
// "check every asset even if an earlier one is bad" design. With the flag it is 31.1s per call.
//
// That is the general lesson for anyone adding the next site: a step that makes N curl calls under
// one cap needs N x (retry-max-time + retry-delay + max-time) inside that cap, and copying a
// sibling's number without counting the calls is how you get a value that passes a PAIRING check
// and still gets killed by the runner. Limit 1 above ("enforces PAIRING, never SIZING") is now
// narrower than it was: the ffmpeg-pin-freshness.yml sites DO get their arithmetic computed, by the
// describe block above this one. It still holds for every other file here, ci.yml's pdfium sites
// included -- see the note in that block about why they were not folded in.
//
// **CPE-1969 replaced `GUARDED` with a derived enumeration.** It was a hard-coded four-file list, and
// this repo has eight workflows plus three extracted `.sh` scripts that no consumer read at all —
// the same "enumerate, don't recall" defect (CPE-1932) the lockfile guard carried, on the guard whose
// own comment above boasts it is "a generic scan of every curl line in these workflows rather than a
// spot check". It was generic within four files someone remembered. It now walks
// `allShellUnits()` — every `run:` step of every workflow, plus every script as one unit.
//
// Measured over the newly-included files before widening (2026-08-27): `catalog-freshness.yml`'s
// live-catalog fetch and `model-snapshot.yml`'s reseller fetch are the only new curl sites, and
// neither is an offender (the first carries `--retry-max-time 20`; the second passes no `--retry` at
// all, so the pairing rule does not apply). The three scripts contain no `curl`. No live defect was
// folded into this scope fix — but "no offender out there" is now re-measured every run instead of
// being a file list nobody revisited.
describe("no curl retries against a per-attempt-only time bound (CPE-1824)", () => {
  // One case per FILE, as before — the units within a file are collected into one report so a
  // failure names every offending site at once rather than one per run.
  for (const file of [...discoverWorkflows(), ...discoverWorkflowScripts()]) {
    it(`${file}: every curl combining --retry with --max-time also carries --retry-max-time`, () => {
      const offenders = allShellUnits()
        .filter((u) => u.file === file)
        .flatMap((unit) =>
          logicalLines(unit.run)
            .filter((line) => CURL_COMMAND_WORD.test(line))
            .filter(
              (line) =>
                RETRY_FLAG.test(line) &&
                MAX_TIME_FLAG.test(line) &&
                !RETRY_MAX_TIME_FLAG.test(line),
            )
            .map((line) => `${unit.where}: ${line}`),
        );
      expect(offenders).toEqual([]);
    });
  }

  it("the widened scan is not vacuous — it reaches every workflow AND all three scripts", () => {
    // The failure this catches is the enumeration silently shrinking back: a scan of four files that
    // believes it covers eleven reports clean over seven it never opened, which is precisely the
    // state this ticket found. `allShellUnits()` already refuses a near-empty result; this asserts
    // the units really span every file, not just that there were enough of them.
    const files = new Set(allShellUnits().map((u) => u.file));
    for (const f of [...discoverWorkflows(), ...discoverWorkflowScripts()]) {
      expect(files.has(f), `${f} contributed no shell unit to the hang-hardening scan`).toBe(true);
    }
  });

  it("the scan is not vacuous -- it really does reach ci.yml's three pdfium curl sites", () => {
    // Without this, deleting every --retry (or renaming the steps, or breaking curlLines) would
    // leave the assertions above trivially green on an empty set and look like a pass.
    const retrying = curlLines(parseWorkflow("ci.yml")).filter(
      ({ line }) => RETRY_FLAG.test(line) && MAX_TIME_FLAG.test(line),
    );
    expect(retrying.length).toBe(3);
    for (const { line } of retrying) {
      expect(line).toMatch(flagValue("--retry-max-time", 150));
    }
  });

  it("the scan is not vacuous for ffmpeg-pin-freshness.yml either -- both HEAD-check sites are reached (CPE-1849)", () => {
    // Same non-vacuity role as the ci.yml case above, and needed for the same reason: the pairing
    // scan goes green both when a site is correctly bounded AND when it stopped retrying at all
    // (or when curlLines()/logicalLines() quietly stopped reaching it). Only a count can tell the
    // two apart. 2, not 5 -- "HEAD-check pinned assets" calls one shared head_check() helper five
    // times, so there is exactly ONE curl line there, plus one in "Validate the recommendation".
    const retrying = curlLines(parseWorkflow("ffmpeg-pin-freshness.yml")).filter(
      ({ line }) => RETRY_FLAG.test(line) && MAX_TIME_FLAG.test(line),
    );
    expect(retrying.length).toBe(2);
    for (const { line } of retrying) {
      expect(line).toMatch(flagValue("--retry-max-time", 20));
    }
  });
});

// CPE-1969 gap 1, apt half. Before this, the "no apt/apt-get invocation left unhardened" scan existed
// three times over three REMEMBERED files: ci.yml (ciAptGetHardening.test.ts) and release.yml +
// release-sidecar.yml (above). `gui-smoke.yml` runs FOUR apt-get invocations across two jobs and was
// in none of them; `catalog-freshness.yml`, `ffmpeg-pin-freshness.yml`, `model-snapshot.yml`,
// `release-pipeline-watchdog.yml` and the three extracted scripts were in none of them either.
//
// Measured before widening (2026-08-27): gui-smoke.yml's four sites are all correctly hardened, and no
// other newly-included file invokes apt at all — so this closes a scope gap without a live defect
// behind it. The per-file describe blocks above and in ciAptGetHardening.test.ts keep their site-
// specific assertions (timeout-minutes, continue-on-error, exact step names); this is the generic
// backstop that no longer needs anyone to remember to extend it.
describe("no apt/apt-get invocation anywhere in CI is left unhardened (CPE-1969)", () => {
  it("every apt invocation in every workflow and every extracted script carries the hardening flags", () => {
    const unhardened: string[] = [];
    for (const unit of allShellUnits()) {
      for (const line of aptGetLines(unit.run)) {
        if (!line.includes(HARDENING_FLAGS)) unhardened.push(`${unit.where}: ${line}`);
      }
    }
    expect(unhardened).toEqual([]);
  });

  it("the scan is not vacuous — it reaches gui-smoke.yml, which no apt guard used to read", () => {
    // A count, not `> 0`: the pairing above goes green both when every site is hardened AND when
    // aptGetLines()/the enumeration quietly stopped reaching them.
    const guiSmoke = allShellUnits().filter((u) => u.file.endsWith("gui-smoke.yml"));
    const sites = guiSmoke.flatMap((u) => aptGetLines(u.run));
    expect(sites.length).toBe(4);
    for (const line of sites) expect(line).toContain(HARDENING_FLAGS);
  });
});

// CPE-1969 red-proofs for the widened scope. Each builds a real fixture tree under `.claude/tmp/`,
// runs the SAME scan predicates over it, and is removed afterwards — so "the widened scan catches a
// newcomer" is a measurement rather than a claim next to a green test.
describe("the widened hang-hardening scope really catches a newcomer (CPE-1969)", () => {
  const scratch: string[] = [];
  afterEach(() => {
    while (scratch.length > 0) rmSync(scratch.pop()!, { recursive: true, force: true });
  });

  function fixtureRoot(files: Record<string, string>): string {
    const base = join(process.cwd(), ".claude", "tmp");
    mkdirSync(base, { recursive: true });
    const root = mkdtempSync(join(base, "cpe1969-hang-"));
    scratch.push(root);
    mkdirSync(join(root, ".github/workflows/scripts"), { recursive: true });
    for (let i = 0; i < MIN_EXPECTED_WORKFLOWS; i += 1) {
      writeFileSync(
        join(root, `.github/workflows/pad${i}.yml`),
        "jobs:\n  j:\n    steps:\n      - name: noop\n        run: echo hi\n",
        "utf8",
      );
    }
    for (const [rel, body] of Object.entries(files)) writeFileSync(join(root, rel), body, "utf8");
    return root;
  }

  it("a curl in a FOURTH script that retries against a per-attempt bound is reported", () => {
    const root = fixtureRoot({
      ".github/workflows/scripts/a.sh": "echo hi\n",
      ".github/workflows/scripts/b.sh": "echo hi\n",
      ".github/workflows/scripts/c.sh": "echo hi\n",
      ".github/workflows/scripts/newcomer.sh":
        '#!/usr/bin/env bash\ncurl --fail --retry 3 \\\n  --max-time 20 "https://example.com/x"\n',
    });
    const offenders = allShellUnits(root).flatMap((unit) =>
      logicalLines(unit.run)
        .filter((line) => CURL_COMMAND_WORD.test(line))
        .filter(
          (line) =>
            RETRY_FLAG.test(line) && MAX_TIME_FLAG.test(line) && !RETRY_MAX_TIME_FLAG.test(line),
        )
        .map((line) => `${unit.where}: ${line}`),
    );
    // Also proves the continuation is joined across the fixture's two physical lines: without that,
    // --max-time would sit on a line with no `curl` and the offender would vanish.
    expect(offenders).toEqual([
      ".github/workflows/scripts/newcomer.sh (whole script): " +
        `curl --fail --retry 3 --max-time 20 "https://example.com/x"`,
    ]);
  });

  it("an unhardened apt-get in a SIXTH workflow is reported", () => {
    const root = fixtureRoot({
      ".github/workflows/scripts/a.sh": "echo hi\n",
      ".github/workflows/scripts/b.sh": "echo hi\n",
      ".github/workflows/scripts/c.sh": "echo hi\n",
      ".github/workflows/newcomer.yml":
        "jobs:\n  j:\n    steps:\n      - name: deps\n        run: sudo apt-get install -y foo\n",
    });
    const unhardened = allShellUnits(root)
      .flatMap((unit) => aptGetLines(unit.run).map((line) => ({ where: unit.where, line })))
      .filter(({ line }) => !line.includes(HARDENING_FLAGS));
    expect(unhardened.map((u) => u.line)).toEqual(["sudo apt-get install -y foo"]);
    expect(unhardened[0].where).toContain("newcomer.yml");
  });

  it("gui-smoke.yml's apt-LOCK wait message is prose, not a fifth unhardened site (CPE-1969)", () => {
    // The false positive the widening exposed, and the reason APT_COMMAND_WORD excludes `/` in its
    // LOOKAHEAD. Before that fix this exact line read as an unhardened apt invocation and the
    // widened scan false-failed on its first run. Red-proof: revert the lookahead to `(?![\w-])`
    // and this case fails while the real invocations below keep passing.
    expect(APT_COMMAND_WORD.test('echo "waiting for background apt/dpkg lock (attempt $i/24)..."')).toBe(
      false,
    );
    expect(APT_COMMAND_WORD.test("sudo apt-get update")).toBe(true);
    expect(APT_COMMAND_WORD.test("sudo apt install -y foo")).toBe(true);
    expect(APT_COMMAND_WORD.test("sudo rm -f /etc/apt/sources.list.d/x.list")).toBe(false);
  });

  it("an ABSOLUTE-PATH apt invocation is seen — the lookbehind used to swallow it (CPE-1969 N4)", () => {
    // The mirror image of the case above, and the dangerous direction: from CPE-1916 until now the
    // lookbehind excluded `/`, so `sudo /usr/bin/apt-get update` matched NEITHER the pre-CPE-1969
    // regex NOR round 1's — a real, entirely unhardened apt invocation that every guard reported as
    // absent. Silent, unlike the `echo` false positive, which is why it is folded in here rather
    // than filed. Red-proof: put `/` back in the lookbehind (`(?<![\w\-/])`) and all five of these
    // fail while every case in the test above keeps passing.
    expect(APT_COMMAND_WORD.test("sudo /usr/bin/apt-get update")).toBe(true);
    expect(APT_COMMAND_WORD.test("/usr/bin/apt install -y foo")).toBe(true);
    expect(APT_COMMAND_WORD.test("exec /usr/bin/apt-get -o Acquire::Retries=3 update")).toBe(true);
    expect(APT_COMMAND_WORD.test("  /usr/bin/apt-get update")).toBe(true); // path-prefixed, inside a script
    expect(
      APT_COMMAND_WORD.test("bash .github/workflows/scripts/x.sh && /usr/local/bin/apt-get -y clean"),
    ).toBe(true);

    // …and dropping the lookbehind exclusion must not re-open the path-SEGMENT direction. `.` joined
    // `/` in the lookahead for these two: they are the only cells the 26-shape old/current/new sweep
    // moved that were not intended. Red-proof: drop `.` from the lookahead and both fail.
    expect(APT_COMMAND_WORD.test("cat /etc/apt/apt.conf.d/99custom")).toBe(false);
    expect(APT_COMMAND_WORD.test("cat /etc/apt.conf")).toBe(false);
    expect(APT_COMMAND_WORD.test("sudo rm -f /etc/apt/preferences.d/nosnap.pref")).toBe(false);
  });

  it("the scan REFUSES rather than reporting clean when the enumeration comes back empty", () => {
    const root = mkdtempSync(join(process.cwd(), ".claude", "tmp", "cpe1969-empty-"));
    scratch.push(root);
    expect(() => allShellUnits(root)).toThrow(/near-empty/);
  });
});
