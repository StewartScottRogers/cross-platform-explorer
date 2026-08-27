// CPE-1908: the channel-purity guard `platforms_with_mismatched_channel` (crates/updater-verify,
// CPE-1894) ran against the PLAIN release manifest only — `release-sidecar.yml`, the workflow that
// builds the channel users actually install (see [[always-install-sidecar-build]]), never invoked
// `verify-release-artifacts` at all. That gap shipped silently: nothing failed, nothing warned, the
// job simply didn't exist. A guard that can go missing without a red build is exactly the shape of
// defect this repo keeps re-discovering (CPE-1872, CPE-1893, CPE-1903), so this ticket's own fix
// must not be exposed to the same failure mode a second time.
//
// This file is the structural ratchet: it reads the REAL vocabulary `--expect-channel` accepts
// straight from `crates/updater-verify/src/lib.rs`'s `Channel` type, and asserts every one of those
// channels has a real, ACTUALLY-WIRED `verify-release-artifacts` invocation in a release workflow.
//
// ROUND 2 (Security Auditor + Reviewer, both independent passes) found the round-1 version of this
// file was itself under-guarded in five distinct ways, all fixed here:
//
//   H2/H3 (Auditor Finding 1 + Reviewer H3) — the round-1 detector matched only `step.run` TEXT. A
//     job that hard-disables the whole guard (`if: ${{ false }}`, or — the Reviewer's addition —
//     DELETING the `if: ${{ !cancelled() }}` line outright, restoring the bare-`needs:` silent-skip
//     shape CPE-1872/CPE-1893 exist to prevent) still showed 5/5 green, because text presence proves
//     nothing about whether the job/step actually RUNS. Every coverage assertion below now goes
//     through `isActuallyWired()`, which checks the job's `if:` is EXACTLY the `!cancelled()` form,
//     the job's `needs:` names the real build job, AND the step's own `if:` is the real secret gate —
//     not merely that some step's `run` text happens to contain the right substring.
//   H1 (Reviewer) — the round-1 regex matched `--expect-channel` even inside a `#` shell COMMENT, so
//     commenting the flag out (a realistic "unblock a red release" edit) still counted as coverage —
//     and without the flag, `verify-release-artifacts` silently falls back to a productName-derived
//     `plain` expectation, so a 100%-plain manifest under a `-sidecar` tag would pass. Fixed by
//     reusing `logicalLines()` (src/lib/preview/shellScriptLines.ts, extracted from
//     `releaseHangHardening.test.ts`'s CPE-1849 comment/continuation-aware splitter) instead of a
//     second hand-rolled stripper, so a commented-out flag is invisible to the match, exactly as it
//     would be to a shell.
//   H4 (Auditor Finding 2 + Reviewer extension) — the round-1 Rust-enum parser only matched a BARE
//     variant (`Ident,`), so `Beta(String),` OR `Beta = 3,` both vanished from the canonical list
//     silently — a channel with genuinely zero guard would have passed clean. Fixed by
//     `parseChannelVariantSegments()`, a depth-aware comma-splitter that extracts every variant's
//     leading identifier regardless of payload/discriminant, and asserts the parsed count matches the
//     enum body's real non-comment/non-attribute segment count — an unrecognisable line is now a loud
//     failure, never a silent drop.
//   The "false RED" trap (Reviewer) — reading the Rust IDENTIFIER'S spelling (as round 1 did) means a
//     pure, harmless rename (`Channel::Sidecar` → `Channel::SidecarBuild`; `FromStr` still accepts
//     the literal `"sidecar"`, nothing breaks) makes this ratchet go red and — worse — recommend
//     `--expect-channel sidecarbuild`, a value the binary actually REJECTS. Fixed at the root: this
//     file now reads the string LITERALS out of `Channel`'s `Display` impl (`write!(f, "sidecar")`),
//     not the Rust identifiers — and `crates/updater-verify/src/lib.rs::Channel::ALL` +
//     `exhaustiveness_guard` + `channel_display_fromstr_round_trip_covers_every_variant` prove, IN
//     RUST, that `Display`'s output for every variant always parses back via `FromStr`, so those
//     literals ARE the real accepted CLI vocabulary, independent of identifier spelling.
//
// Structural assertions go through `parseYaml` (src/lib/preview/yaml.ts, CPE-1617), the same approach
// `catalogPublishFreshnessGuard.test.ts` and `releaseHangHardening.test.ts` use.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";
import { logicalLines } from "./preview/shellScriptLines";

const ROOT = process.cwd();
const LIB_RS = join(ROOT, "crates", "updater-verify", "src", "lib.rs");
const WORKFLOWS = join(ROOT, ".github", "workflows");

interface WorkflowStep {
  name?: string;
  run?: string;
  if?: string;
  [key: string]: unknown;
}
interface WorkflowJob {
  needs?: string | string[];
  if?: string;
  steps?: WorkflowStep[];
  [key: string]: unknown;
}
interface WorkflowDoc {
  jobs: Record<string, WorkflowJob>;
  [key: string]: unknown;
}

function parseWorkflow(fileName: string): WorkflowDoc {
  const text = readFileSync(join(WORKFLOWS, fileName), "utf8");
  const result = parseYaml(text);
  if (!result.ok) {
    throw new Error(`${fileName} did not parse as YAML: ${result.error}`);
  }
  return result.value as WorkflowDoc;
}

function normalizeNeeds(needs: string | string[] | undefined): string[] {
  if (needs === undefined) return [];
  return Array.isArray(needs) ? needs : [needs];
}

// --- Rust-side canonical channel vocabulary --------------------------------------------------------

/** Splits an enum body into per-variant segments at TOP-LEVEL commas only (depth-aware: a comma
 *  inside `(...)`/`{...}`/`[...]` — a tuple/struct variant's payload — does not split). Handles
 *  `Ident,`, `Ident(Payload),`, `Ident { field: T },`, `Ident = N,` uniformly by construction, since
 *  none of those forms are distinguished until AFTER splitting. */
function splitTopLevelVariantSegments(body: string): string[] {
  const segments: string[] = [];
  let depth = 0;
  let current = "";
  for (const ch of body) {
    if (ch === "(" || ch === "{" || ch === "[") depth += 1;
    if (ch === ")" || ch === "}" || ch === "]") depth -= 1;
    if (ch === "," && depth === 0) {
      segments.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  if (current.trim().length > 0) segments.push(current);
  return segments.map((s) => s.trim()).filter((s) => s.length > 0);
}

/** Every `Channel` variant IDENTIFIER, parsed robustly (CPE-1908 round 2, Auditor Finding 2 + its
 *  Reviewer extension): strips whole-line `//`/`///` comments and `#[...]` attribute lines first,
 *  then splits on top-level commas (so `Beta(String)` and `Beta = 3` are both recognised, not just a
 *  bare `Beta`), and asserts the number of variant segments equals the number of non-comment,
 *  non-attribute lines in the body — a variant this parser can't read is a LOUD failure, never a
 *  silent drop, because dropping is exactly how a real guard gap disappeared from view in round 1. */
function readChannelVariantIdentifiers(): string[] {
  const src = readFileSync(LIB_RS, "utf8");
  const enumMatch = src.match(/pub enum Channel \{([\s\S]*?)\n\}/);
  expect(enumMatch, "crates/updater-verify/src/lib.rs must still declare `pub enum Channel { ... }` — the detector may be broken").not.toBeNull();
  const rawBody = enumMatch![1];

  const codeLines = rawBody.split("\n").filter((line) => {
    const t = line.trim();
    if (t.length === 0) return false;
    if (t.startsWith("//")) return false; // covers both `//` and `///`
    if (t.startsWith("#[")) return false;
    return true;
  });
  const codeOnly = codeLines.join("\n");

  const segments = splitTopLevelVariantSegments(codeOnly);

  expect(
    segments.length,
    `pub enum Channel's body has ${codeLines.length} non-comment/non-attribute line(s) but only ` +
      `${segments.length} variant segment(s) were split out of it — a variant spanning something this ` +
      `detector doesn't understand would show up here as a mismatch rather than a silent drop.`,
  ).toBe(codeLines.length);

  const identifiers = segments.map((segment) => {
    const m = segment.match(/^([A-Za-z_][A-Za-z0-9_]*)/);
    return m ? m[1] : null;
  });
  const unparseable = segments.filter((_, i) => identifiers[i] === null);
  expect(
    unparseable,
    `pub enum Channel has variant segment(s) with no recognisable leading identifier: ${JSON.stringify(unparseable)}. ` +
      `A variant this detector can't read is a variant this whole file can silently forget to guard.`,
  ).toEqual([]);

  return identifiers as string[];
}

/** The REAL CLI vocabulary `--expect-channel` accepts — read from `Channel`'s `Display` impl's
 *  string LITERALS, not from the Rust identifiers (CPE-1908 round 2, Reviewer's "false RED" trap):
 *  a pure identifier rename doesn't change what `Display` emits, so it doesn't change what this list
 *  contains either, matching the fact (proved in Rust by the `Display`/`FromStr` round-trip test)
 *  that the string tokens — not the identifier spelling — are the real, load-bearing vocabulary. */
function readCanonicalChannelTokens(): string[] {
  const identifiers = readChannelVariantIdentifiers(); // also runs the count/unparseable red-proofs
  const src = readFileSync(LIB_RS, "utf8");
  const displayMatch = src.match(/impl std::fmt::Display for Channel \{[\s\S]*?\n\}/);
  expect(displayMatch, "crates/updater-verify/src/lib.rs must still declare `impl std::fmt::Display for Channel { ... }` — the detector may be broken").not.toBeNull();
  const displayBody = displayMatch![0];

  const arms = new Map<string, string>();
  // Tolerates a tuple/struct-variant binding pattern between the identifier and `=>` (e.g.
  // `Channel::Beta(_) => write!(f, "beta")`), not just a bare unit-variant arm — found by this
  // file's own red-proofing (CPE-1908 round 2 self-check): an earlier version of this regex
  // required `Channel::Ident =>` with nothing in between, so a CORRECTLY-written Display arm for a
  // hypothetical payload-carrying variant still read as "no arm found", which would have blocked a
  // legitimate PR with a misleading message even though it failed safe (loudly, not silently).
  const armRe = /Channel::(\w+)(?:\([^)]*\)|\{[^}]*\})?\s*=>\s*write!\(f,\s*"([^"]*)"\)/g;
  let m: RegExpExecArray | null;
  while ((m = armRe.exec(displayBody)) !== null) {
    arms.set(m[1], m[2].toLowerCase());
  }

  const missingArms = identifiers.filter((id) => !arms.has(id));
  expect(
    missingArms,
    `Channel's Display impl has no 'Channel::<Ident> => write!(f, "...")' arm for: ${JSON.stringify(missingArms)}. ` +
      `Every enum variant must be represented in Display for this detector to know its real CLI token.`,
  ).toEqual([]);
  expect(
    arms.size,
    `Channel's Display impl has ${arms.size} arm(s) but the enum declares ${identifiers.length} variant(s) — ` +
      `an extra/stale Display arm (e.g. for a since-removed variant) would otherwise inflate the canonical list.`,
  ).toBe(identifiers.length);

  return identifiers.map((id) => arms.get(id)!);
}

// --- Workflow-side guard wiring ---------------------------------------------------------------------

interface GuardInvocation {
  channel: string;
  jobName: string;
  jobIf: string | undefined;
  jobNeeds: string[];
  stepIf: string | undefined;
}

/** The exact `if:` GitHub-Actions-documents for "run this job regardless of a needed job's pass/fail
 *  outcome, unless the run itself was cancelled" — the CPE-1872-round-3 pattern this repo already
 *  established for `verify-published-manifest`/`catalog`. Absent, `always()`, or a hard `false` are
 *  all wrong and must all fail `isActuallyWired`. */
const CANCELLED_GUARD_IF = "${{ !cancelled() }}";

/** The secret-detection gate every real `verify-release-artifacts` invocation in this repo runs
 *  behind (`Detect updater signing key`), on both release.yml and release-sidecar.yml. */
const SIGNING_KEY_STEP_IF = "steps.sig.outputs.has == 'true'";

/** The job that actually BUILDS this channel's installers — the guarding job's `needs:` must include
 *  it, or the guard could run completely detached from the matrix it's supposed to gate. */
const BUILD_JOB_FOR_WORKFLOW: Record<string, string> = {
  "release.yml": "release",
  "release-sidecar.yml": "release-sidecar",
};

/** Every `--expect-channel <value>` a workflow's steps pass to `verify-release-artifacts`, found by
 *  scanning each step's LOGICAL shell lines (CPE-1908 round 2, Reviewer H1) — comments stripped and
 *  backslash continuations joined via `logicalLines()` — rather than raw `step.run` text, so a
 *  commented-out flag (`# TODO: re-enable --expect-channel sidecar ...`) is correctly invisible, the
 *  same way it would be to the shell that actually runs this step. */
function guardInvocations(fileName: string): GuardInvocation[] {
  const doc = parseWorkflow(fileName);
  const found: GuardInvocation[] = [];
  for (const [jobName, job] of Object.entries(doc.jobs)) {
    for (const step of job.steps ?? []) {
      const joined = logicalLines(step.run).join(" ");
      if (!joined.includes("verify-release-artifacts")) continue;
      const m = joined.match(/--expect-channel\s+(\S+)/);
      if (!m) continue; // flag absent or only present inside a stripped comment -- not a real invocation
      found.push({
        channel: m[1].toLowerCase(),
        jobName,
        jobIf: typeof job.if === "string" ? job.if : undefined,
        jobNeeds: normalizeNeeds(job.needs),
        stepIf: typeof step.if === "string" ? step.if : undefined,
      });
    }
  }
  return found;
}

/** A guard invocation only actually PROTECTS a release if it can't be silently switched off: the job
 *  runs regardless of the build matrix's pass/fail outcome (`if:` is EXACTLY the `!cancelled()` form
 *  — never absent, never `always()`, never a hard `false`), the job is genuinely WIRED to the build
 *  it's supposed to gate (`needs:` names that workflow's real build job), and the verification step
 *  itself still carries the same secret gate every real invocation in this repo uses. CPE-1908 round 2
 *  (Security Auditor + Reviewer): round 1 only ever checked `step.run` text, so hard-disabling the
 *  whole job (`if: ${{ false }}`), DELETING its `if:` line outright, or neutering the step's own `if:`
 *  all still showed 5/5 green — text presence proves nothing about whether the guard actually RUNS.
 *  Every coverage assertion in this file now goes through this predicate. */
function isActuallyWired(fileName: string, inv: GuardInvocation): boolean {
  const buildJob = BUILD_JOB_FOR_WORKFLOW[fileName];
  return (
    inv.jobIf === CANCELLED_GUARD_IF &&
    buildJob !== undefined &&
    inv.jobNeeds.includes(buildJob) &&
    inv.stepIf === SIGNING_KEY_STEP_IF
  );
}

function channelsActuallyGuardedByWorkflow(fileName: string): string[] {
  return guardInvocations(fileName)
    .filter((inv) => isActuallyWired(fileName, inv))
    .map((inv) => inv.channel);
}

describe("every Channel token the guard logic knows about has a real, ACTUALLY-WIRED workflow invocation (CPE-1908)", () => {
  const canonicalChannels = readCanonicalChannelTokens();

  it("the Rust Display impl still names exactly the two channels this repo ships (sanity check on the detector itself)", () => {
    // Not a hard requirement of the mechanism (a third channel is a legitimate future addition), but
    // if this ever silently reads zero or one token the detector itself is broken and every
    // assertion below would pass vacuously — the "green over zero coverage" trap this repo's other
    // guards explicitly call out (see lockfileLockedGuard.test.ts's identical caution).
    expect(canonicalChannels.length).toBeGreaterThanOrEqual(2);
    expect(canonicalChannels).toContain("plain");
    expect(canonicalChannels).toContain("sidecar");
  });

  it("the build-job config itself names a real job in each workflow (sanity check)", () => {
    for (const [file, buildJob] of Object.entries(BUILD_JOB_FOR_WORKFLOW)) {
      const doc = parseWorkflow(file);
      expect(Object.keys(doc.jobs), `${file} has no job named "${buildJob}" — BUILD_JOB_FOR_WORKFLOW is stale`).toContain(buildJob);
    }
  });

  it("release.yml's plain-channel guard is actually wired (correct if:, needs: the build job, correct secret gate)", () => {
    const invocations = guardInvocations("release.yml").filter((inv) => inv.channel === "plain");
    expect(invocations.length, "no --expect-channel plain invocation found in release.yml at all").toBeGreaterThan(0);
    expect(invocations.some((inv) => isActuallyWired("release.yml", inv))).toBe(true);
  });

  it("release-sidecar.yml's sidecar-channel guard is actually wired (CPE-1908's own fix)", () => {
    const invocations = guardInvocations("release-sidecar.yml").filter((inv) => inv.channel === "sidecar");
    expect(invocations.length, "no --expect-channel sidecar invocation found in release-sidecar.yml at all").toBeGreaterThan(0);
    expect(invocations.some((inv) => isActuallyWired("release-sidecar.yml", inv))).toBe(true);
  });

  it("the union of every workflow's ACTUALLY-WIRED channels covers EVERY channel token Channel::Display knows about", () => {
    const guarded = new Set([
      ...channelsActuallyGuardedByWorkflow("release.yml"),
      ...channelsActuallyGuardedByWorkflow("release-sidecar.yml"),
    ]);
    const missing = canonicalChannels.filter((c) => !guarded.has(c));
    expect(
      missing,
      `Channel's Display impl names ${JSON.stringify(canonicalChannels)}, but no ACTUALLY-WIRED ` +
        `release.yml/release-sidecar.yml step guards: ${JSON.stringify(missing)}. A channel with no ` +
        `guarding workflow invocation — or one that LOOKS wired but isn't (wrong if:/needs:/secret ` +
        `gate) — is exactly the CPE-1908 gap re-opening.`,
    ).toEqual([]);
  });

  it("release.yml and release-sidecar.yml each guard a DIFFERENT channel (not both plain, or both sidecar)", () => {
    // Defensive against the specific historical failure mode (CPE-1894/CPE-1908): a channel getting
    // silently left out isn't the only way this could regress — a copy-paste of one workflow's
    // --expect-channel value into the other would leave the union check above green (both channels
    // still appear somewhere) while actually leaving one channel's OWN workflow unguarded again.
    const plainChannels = new Set(channelsActuallyGuardedByWorkflow("release.yml"));
    const sidecarChannels = new Set(channelsActuallyGuardedByWorkflow("release-sidecar.yml"));
    expect(plainChannels.has("sidecar"), "release.yml (the plain-tag workflow) must not claim to guard the sidecar channel").toBe(false);
    expect(sidecarChannels.has("plain"), "release-sidecar.yml must not claim to guard the plain channel").toBe(false);
  });
});
