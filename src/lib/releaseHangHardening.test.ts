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
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";

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

/** Verbatim from ciAptGetHardening.test.ts (CPE-1787) -- the full option string every hardened
 *  apt-get invocation in this repo carries. Reused rather than re-derived, per that ticket's
 *  Reviewer round asking future sites to reuse this exact string. */
const HARDENING_FLAGS =
  "-o Acquire::ForceIPv4=true -o Acquire::Retries=3 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20";

/** Verbatim from ciAptGetHardening.test.ts (CPE-1787) -- matches `apt`/`apt-get` as an isolated
 *  COMMAND WORD (not a substring of `apt-transport-https`/`adapter`/etc). That ticket's Reviewer
 *  round widened this from a literal `"apt-get"` substring check specifically so a future site
 *  written with the bare `apt` alias wouldn't sail through; reused verbatim here instead of
 *  re-deriving a third copy. */
const APT_COMMAND_WORD = /(?<![\w-])apt(?:-get)?(?![\w-])/;

/** Strips a shell `#` comment from one line, respecting quotes. A `#` only opens a comment when it
 *  is unquoted AND starts a word (line start, or preceded by whitespace). The quote-awareness is
 *  load-bearing in the SAFE direction: a naive "cut at the first #" would truncate a real curl
 *  invocation whose URL carries a fragment, or whose quoted header value contains a `#`, hiding it
 *  from the scan entirely -- a SILENT false negative, the dangerous direction.
 *
 *  Round 2 dropped only lines whose FIRST character was `#`, so an inline trailing comment such as
 *  `echo hi # curl --retry 3 --max-time 20 ...` was read as code. That direction fails LOUD (a
 *  spurious offender), so it was never a safety hole -- but it is exactly the comment-vs-code
 *  confusion this file's header says the guard exists to avoid. */
function stripShellComment(line: string): string {
  let quote: string | null = null;
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (quote !== null) {
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      continue;
    }
    if (ch === "#" && (i === 0 || /\s/.test(line[i - 1]))) return line.slice(0, i);
  }
  return line;
}

/** Splits a `run` script into LOGICAL shell lines: backslash continuations joined, `#` comments
 *  stripped, before anything looks for a flag.
 *
 *  The join is the important half, and it closes a hole that was fully SILENT. Round 2's scan split
 *  on newlines and required `--retry` and `--max-time` on the SAME physical line, so ordinary shell
 *  formatting evaded it completely:
 *
 *      curl --fail --retry 3 \
 *        --max-time 20 -sS -o /tmp/x https://example.com/x
 *
 *  Neither physical line carries both flags, so the pairing rule never fired and the file reported
 *  a clean pass. That is not a hypothetical style: this repo ALREADY writes curl exactly that way
 *  at ffmpeg-pin-freshness.yml:251 and :409, and apt-get that way at release.yml:56. Any scan of
 *  shell text for a flag COMBINATION has to join continuations first, or it is checking nothing. */
function logicalLines(run: string | undefined): string[] {
  const out: string[] = [];
  let pending = "";
  for (const raw of (run ?? "").split("\n")) {
    const line = stripShellComment(raw).trim();
    if (line.endsWith("\\")) {
      pending += `${line.slice(0, -1).trim()} `;
      continue;
    }
    const joined = (pending + line).trim();
    if (joined) out.push(joined);
    pending = "";
  }
  if (pending.trim()) out.push(pending.trim());
  return out;
}

function aptGetLines(run: string | undefined): string[] {
  return logicalLines(run).filter((line) => APT_COMMAND_WORD.test(line));
}

/** Matches each flag as an isolated FLAG WORD. The `(?![\w-])` tail is what keeps `--retry` from
 *  also matching `--retry-delay`/`--retry-all-errors`/`--retry-connrefused`/`--retry-max-time`,
 *  which are different options entirely -- a line carrying only `--retry-delay` is not retrying. */
const RETRY_FLAG = /(?<![\w-])--retry(?![\w-])/;
const MAX_TIME_FLAG = /(?<![\w-])--max-time(?![\w-])/;
const RETRY_MAX_TIME_FLAG = /(?<![\w-])--retry-max-time(?![\w-])/;

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
        if (!/(?<![\w-])curl(?![\w-])/.test(line)) continue;
        found.push({ where: `${jobName} / ${step.name}`, line });
      }
    }
  }
  return found;
}

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
      // bounds the series. 150 + one in-flight attempt's 180 = 330s, inside the 360s step cap.
      expect(curlLine).toContain("--retry-max-time 150");
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
// Scope note: .github/workflows/ffmpeg-pin-freshness.yml also pairs --retry with --max-time (:251
// and :409). It is deliberately NOT in this list, but do NOT read that as "harmless there" -- it is
// tracked as CPE-1849 and the arithmetic is WORSE there than at the pdfium sites.
//
// What is different: that file's own comment is ACCURATE ("--max-time bounds each attempt"), so the
// specific defect this guard exists to catch -- a false claim in a comment -- is not present, and
// the file is outside CPE-1824's scope. What is worse: one head_check's worst case is
// 4 x 30s + 3 x 2s = 126s, and the "HEAD-check pinned assets" step (ffmpeg-pin-freshness.yml:229)
// makes FIVE head_check calls under a single `timeout-minutes: 5`. A full stall therefore blows the
// 300s cap during the THIRD call and the step dies by opaque runner kill, with no curl diagnostic
// for the two sites never reached. That is real work with a real failure mode, not tidying. Adding
// this file to GUARDED without first fixing those two sites would simply turn the guard red.
describe("no curl retries against a per-attempt-only time bound (CPE-1824)", () => {
  const GUARDED = ["ci.yml", "release.yml", "release-sidecar.yml"] as const;

  for (const fileName of GUARDED) {
    it(`${fileName}: every curl combining --retry with --max-time also carries --retry-max-time`, () => {
      const offenders = curlLines(parseWorkflow(fileName))
        .filter(
          ({ line }) =>
            RETRY_FLAG.test(line) && MAX_TIME_FLAG.test(line) && !RETRY_MAX_TIME_FLAG.test(line),
        )
        .map(({ where, line }) => `${where}: ${line}`);
      expect(offenders).toEqual([]);
    });
  }

  it("the scan is not vacuous -- it really does reach ci.yml's three pdfium curl sites", () => {
    // Without this, deleting every --retry (or renaming the steps, or breaking curlLines) would
    // leave the assertions above trivially green on an empty set and look like a pass.
    const retrying = curlLines(parseWorkflow("ci.yml")).filter(
      ({ line }) => RETRY_FLAG.test(line) && MAX_TIME_FLAG.test(line),
    );
    expect(retrying.length).toBe(3);
    for (const { line } of retrying) {
      expect(line).toContain("--retry-max-time 150");
    }
  });
});
