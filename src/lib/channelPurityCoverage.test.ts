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
//     reusing `logicalLines()` (src/lib/shellScriptLines.ts, extracted from
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
//     file reads the string LITERALS `Channel`'s definition attaches to each variant, not the Rust
//     identifiers, and `crates/updater-verify/src/lib.rs`'s own
//     `channel_display_fromstr_round_trip_covers_every_variant` proves, IN RUST, that `Display`'s
//     output for every variant always parses back via `FromStr`, so those literals ARE the real
//     accepted CLI vocabulary, independent of identifier spelling.
//
// ROUND 3 (this pass) fixed a Reviewer + Security Auditor second-pass finding this file's own round-2
// coverage was ALSO exposed to, plus followed a round-2-adjacent Rust refactor:
//
//   R2-1 (must-fix — INVERTS the check, doesn't merely disable it) — `guardInvocations()` used to
//     `logicalLines(step.run).join(" ")` and match `--expect-channel` against the WHOLE joined step,
//     not the specific logical line that actually invokes the binary. Since `logicalLines()` already
//     joins backslash continuations, a real `cargo run ... --bin verify-release-artifacts -- ...
//     --expect-channel sidecar` invocation is already ONE logical line — nothing required the flag to
//     be ON that same line for the match to "count". Three shapes kept the ratchet green while the
//     live command had no flag: a quoted `echo "TODO: restore --expect-channel sidecar ..."`, a
//     heredoc body mentioning both strings, and a comment surviving after a backslash-escaped quote
//     (closed by R2-2 below). Without the flag the binary falls back to a productName-derived `plain`
//     expectation, so a 100%-plain manifest under a `-sidecar` tag would pass — the exact CPE-1894
//     contamination, with this ratchet reporting full coverage. Fixed: the regex now runs against the
//     SINGLE logical line that contains `verify-release-artifacts`, and that line must genuinely
//     INVOKE the binary (`cargo run ... --bin verify-release-artifacts --`), not merely mention its
//     name — which also closes a decoy job whose `run:` was just `echo cargo run --bin
//     verify-release-artifacts -- --expect-channel sidecar` (right `if:`/`needs:`, but the step never
//     actually runs the binary).
//   R2-2 (feeds R2-1) — `shellScriptLines.ts` tracked quotes with no escape handling, so
//     `echo "a \" b" # --expect-channel sidecar` misread the backslash-escaped `"` as closing the
//     quote, then treated the line's trailing `"` as opening a NEW unterminated quote that swallowed
//     the real comment — never stripped, so it read as a live invocation. Fixed there (escape
//     handling + a word-boundary rule for when a quote character can open a string at all, so an
//     apostrophe mid-word like "don't" isn't misread as an unterminated quote either) plus added
//     heredoc-body awareness (a heredoc body is DATA fed to a command, never itself a shell statement,
//     so a body line crafted to look like a real invocation must never be scanned as one). See that
//     module's own header comment for the corrected safe-direction analysis.
//   R2-3 (Rust-side, `crates/updater-verify/src/lib.rs`) — `Channel::ALL` used to be a hand-written
//     literal sitting next to a separate `exhaustiveness_guard` match; a variant + `Display` arm +
//     `FromStr` arm + guard arm all compiled clean while `ALL` silently stayed stale, so the Rust
//     round-trip test passed VACUOUSLY for the new variant. Fixed with `define_channel!`, a macro that
//     generates the enum, `ALL`, `Display`, and `FromStr` from ONE invocation — see that file. Because
//     Display is now ALWAYS macro-generated as `write!(f, $token)` (never hand-written, so it can never
//     vary to `f.write_str(...)` or anything else), this file no longer scans the Display impl via
//     regex at all: it reads each variant's `(identifier, token)` pair directly from
//     `define_channel!`'s invocation, which is simpler AND strictly more robust than round 2's
//     separate enum-body parser + Display-arm regex kept in sync by nothing but review discipline.
//
// Structural assertions go through `parseYaml` (src/lib/preview/yaml.ts, CPE-1617), the same approach
// `catalogPublishFreshnessGuard.test.ts` and `releaseHangHardening.test.ts` use.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";
import { logicalLines } from "./shellScriptLines";

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

/** Splits a `define_channel!` invocation body into per-variant segments at TOP-LEVEL commas only
 *  (depth-aware: a comma inside `(...)`/`{...}`/`[...]` doesn't split — not currently exercised by
 *  this repo's variants, but the invocation body is otherwise free-form Rust, so this stays robust to
 *  a doc comment or attribute containing one). */
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

/** Every `(identifier, token)` pair `define_channel!` is invoked with (CPE-1908 round 3, R2-3) — the
 *  ONE place `crates/updater-verify/src/lib.rs`'s `Channel` enum, `Channel::ALL`, its `Display` impl,
 *  and its `FromStr` impl are all generated from, in lockstep, by that macro. Reading the invocation
 *  directly is simpler AND strictly more robust than round 2's approach (a separate enum-body parser
 *  plus a separate Display-arm regex, kept in sync by nothing but review discipline): there is no
 *  Display impl left to scan by regex at all — it's macro-generated as `write!(f, $token)` uniformly,
 *  so it can never vary to `f.write_str(...)` or anything else a hand-written regex could miss.
 *
 *  Still reads the string LITERAL each variant is attached to, not the Rust identifier alone (CPE-1908
 *  round 2, Reviewer's "false RED" trap): a pure identifier rename doesn't change the token, so it
 *  doesn't change this list either, matching the fact (proved in Rust by
 *  `channel_display_fromstr_round_trip_covers_every_variant`) that the string tokens — not identifier
 *  spelling — are the real, load-bearing CLI vocabulary. Strips whole-line `//`/`///` doc comments
 *  first, then splits on top-level commas and asserts the parsed segment count matches the body's real
 *  non-comment line count, and that every segment parses as `Ident => "token"` — an unrecognisable
 *  line is a LOUD failure here, never a silent drop, because dropping is exactly how a real guard gap
 *  disappeared from view in round 1. */
function readChannelDefinition(): { identifier: string; token: string }[] {
  const src = readFileSync(LIB_RS, "utf8");
  const invocationMatch = src.match(/^define_channel!\s*\{([\s\S]*?)\n\}/m);
  expect(
    invocationMatch,
    "crates/updater-verify/src/lib.rs must still have a top-level `define_channel! { ... }` invocation — the detector may be broken",
  ).not.toBeNull();
  const rawBody = invocationMatch![1];

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
    `define_channel!'s invocation body has ${codeLines.length} non-comment line(s) but only ` +
      `${segments.length} variant segment(s) were split out of it — a variant spanning something this ` +
      `detector doesn't understand would show up here as a mismatch rather than a silent drop.`,
  ).toBe(codeLines.length);

  return segments.map((segment) => {
    const m = segment.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*=>\s*"([^"]*)"$/);
    expect(
      m,
      `define_channel! has a variant segment this detector can't parse as \`Ident => "token"\`: ${JSON.stringify(segment)}. ` +
        `A variant this detector can't read is a variant this whole file can silently forget to guard.`,
    ).not.toBeNull();
    return { identifier: m![1], token: m![2].toLowerCase() };
  });
}

/** The REAL CLI vocabulary `--expect-channel` accepts — see `readChannelDefinition()`'s own comment
 *  for why these are read as string literals, not derived from the Rust identifier spelling. */
function readCanonicalChannelTokens(): string[] {
  return readChannelDefinition().map((d) => d.token);
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
 *  behind (`Detect updater signing key`), on both release.yml and release-sidecar.yml.
 *
 *  CPE-1923 ("verify-release-artifacts passes at exit 0 on three hostile manifests, including a
 *  signed downgrade") is expected to change how/where this step reports its result — if that lands as
 *  a rename of the `sig` step id, a change to its output name, or a different comparison value, this
 *  constant must be updated in the SAME change, or every workflow's guard invocation reads as
 *  not-actually-wired and this ratchet goes red for a reason unrelated to CPE-1923's own fix. */
const SIGNING_KEY_STEP_IF = "steps.sig.outputs.has == 'true'";

/** The job that actually BUILDS this channel's installers — the guarding job's `needs:` must include
 *  it, or the guard could run completely detached from the matrix it's supposed to gate. */
const BUILD_JOB_FOR_WORKFLOW: Record<string, string> = {
  "release.yml": "release",
  "release-sidecar.yml": "release-sidecar",
};

/** True only if `line` — one LOGICAL shell line, comments already stripped and continuations already
 *  joined by `logicalLines()` — actually INVOKES `verify-release-artifacts`, rather than merely
 *  mentioning its name somewhere on the line (CPE-1908 round 3, R2-1). A real invocation always
 *  starts the line with `cargo run` and passes the binary name after `--bin`, followed by cargo's
 *  `--` separator before the binary's own flags — the exact shape every real site in this repo uses
 *  (see `release.yml`/`release-sidecar.yml`'s `Verify the published manifest` steps). Anchoring on
 *  `^cargo\s+run\b` is what rejects the Reviewer's decoy job: a step whose `run:` is just
 *  `echo cargo run --bin verify-release-artifacts -- --expect-channel sidecar` contains every
 *  substring a naive scan would look for, but the line actually starts with `echo`, so it never runs
 *  the binary at all. */
function isRealInvocationLine(line: string): boolean {
  return /^cargo\s+run\b.*--bin\s+verify-release-artifacts\b.*--(?:\s|$)/.test(line);
}

/** Every `--expect-channel` value a single step's `run:` text genuinely declares — one entry per
 *  logical line that `isRealInvocationLine()` confirms actually invokes the binary AND carries the
 *  flag. Factored out of `guardInvocations()` so it can be exercised directly against synthetic
 *  `run:` text in this file's own red-proof tests, without needing a full workflow YAML fixture. */
function channelsDeclaredByStepRun(run: string | undefined): string[] {
  const channels: string[] = [];
  for (const line of logicalLines(run)) {
    if (!isRealInvocationLine(line)) continue;
    const m = line.match(/--expect-channel\s+(\S+)/);
    if (m) channels.push(m[1].toLowerCase());
  }
  return channels;
}

/** Every `--expect-channel <value>` a workflow's steps pass to `verify-release-artifacts`, found by
 *  scanning each step's LOGICAL shell lines individually (CPE-1908 round 2 H1, tightened round 3
 *  R2-1) — comments stripped and continuations joined via `logicalLines()` (so `logicalLines()` never
 *  needs to be told the flag matters to a particular consumer; it just returns real shell lines), and
 *  the `--expect-channel` match runs ONLY against the single logical line that
 *  `isRealInvocationLine()` confirms is a genuine invocation.
 *
 *  Round 2's version joined EVERY logical line of a step into one string before matching either
 *  pattern — since a real `cargo run ...` invocation is already one logical line after continuations
 *  are joined, nothing required the flag to be ON that line for the match to "count". Three shapes
 *  kept round 2's ratchet green while the live command had no flag: a quoted
 *  `echo "TODO: restore --expect-channel sidecar after the hotfix"` on a DIFFERENT logical line of the
 *  same step, a heredoc body mentioning both strings (now impossible: `logicalLines()` skips heredoc
 *  bodies entirely, CPE-1908 round 2/3 R2-2), and a comment surviving after a backslash-escaped quote
 *  (closed by R2-2's escape handling). Matching per-line, on only the line
 *  `isRealInvocationLine()` accepts, closes all three: a flag mentioned anywhere else in the step no
 *  longer "covers" an invocation that doesn't actually carry it. */
function guardInvocations(fileName: string): GuardInvocation[] {
  const doc = parseWorkflow(fileName);
  const found: GuardInvocation[] = [];
  for (const [jobName, job] of Object.entries(doc.jobs)) {
    for (const step of job.steps ?? []) {
      for (const channel of channelsDeclaredByStepRun(step.run)) {
        found.push({
          channel,
          jobName,
          jobIf: typeof job.if === "string" ? job.if : undefined,
          jobNeeds: normalizeNeeds(job.needs),
          stepIf: typeof step.if === "string" ? step.if : undefined,
        });
      }
    }
  }
  return found;
}

/** Collapses whitespace strictly INSIDE every `${{ ... }}` GitHub Actions expression in `value` to a
 *  single space each, trimmed — so `${{ !cancelled()  }}` (one accidental extra space before the
 *  closing braces) compares equal to the canonical `${{ !cancelled() }}` (CPE-1908 round 3, smaller
 *  fix). `CANCELLED_GUARD_IF`/`SIGNING_KEY_STEP_IF` were exact string compares before this, so that
 *  harmless whitespace variant — a real edit someone could make without thinking twice — failed
 *  `isActuallyWired` even though GitHub Actions evaluates the two identically. Only touches text
 *  between the delimiters; text outside `${{ }}` (a bare expression like `SIGNING_KEY_STEP_IF`'s, a
 *  quoted `if:` form, a trailing YAML comment — already stripped by `parseYaml` before this ever runs)
 *  is untouched. */
function normalizeExpressionWhitespace(value: string): string {
  return value.replace(/\$\{\{([\s\S]*?)\}\}/g, (_match, inner: string) => `\${{ ${inner.trim().replace(/\s+/g, " ")} }}`);
}

/** A guard invocation only actually PROTECTS a release if it can't be silently switched off: the job
 *  runs regardless of the build matrix's pass/fail outcome (`if:` is EXACTLY the `!cancelled()` form
 *  — never absent, never `always()`, never a hard `false`), the job is genuinely WIRED to the build
 *  it's supposed to gate (`needs:` names that workflow's real build job), and the verification step
 *  itself still carries the same secret gate every real invocation in this repo uses. CPE-1908 round 2
 *  (Security Auditor + Reviewer): round 1 only ever checked `step.run` text, so hard-disabling the
 *  whole job (`if: ${{ false }}`), DELETING its `if:` line outright, or neutering the step's own `if:`
 *  all still showed 5/5 green — text presence proves nothing about whether the guard actually RUNS.
 *  Every coverage assertion in this file now goes through this predicate. Both `if:` comparisons go
 *  through `normalizeExpressionWhitespace()` (CPE-1908 round 3) so incidental whitespace inside a
 *  `${{ }}` expression can't cause a false RED. */
function isActuallyWired(fileName: string, inv: GuardInvocation): boolean {
  const buildJob = BUILD_JOB_FOR_WORKFLOW[fileName];
  return (
    inv.jobIf !== undefined &&
    normalizeExpressionWhitespace(inv.jobIf) === CANCELLED_GUARD_IF &&
    buildJob !== undefined &&
    inv.jobNeeds.includes(buildJob) &&
    inv.stepIf !== undefined &&
    normalizeExpressionWhitespace(inv.stepIf) === SIGNING_KEY_STEP_IF
  );
}

function channelsActuallyGuardedByWorkflow(fileName: string): string[] {
  return guardInvocations(fileName)
    .filter((inv) => isActuallyWired(fileName, inv))
    .map((inv) => inv.channel);
}

describe("every Channel token the guard logic knows about has a real, ACTUALLY-WIRED workflow invocation (CPE-1908)", () => {
  const canonicalChannels = readCanonicalChannelTokens();

  it("the Rust define_channel! invocation still names exactly the two channels this repo ships (sanity check on the detector itself)", () => {
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

  it("the union of every workflow's ACTUALLY-WIRED channels covers EVERY channel token define_channel! knows about", () => {
    const guarded = new Set([
      ...channelsActuallyGuardedByWorkflow("release.yml"),
      ...channelsActuallyGuardedByWorkflow("release-sidecar.yml"),
    ]);
    const missing = canonicalChannels.filter((c) => !guarded.has(c));
    expect(
      missing,
      `define_channel! names ${JSON.stringify(canonicalChannels)}, but no ACTUALLY-WIRED ` +
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

/** Reproduces round 2's exact combined bug so the tests below can prove the three decoy shapes really
 *  did read as coverage under the OLD approach before asserting the round-3 fix rejects them — a
 *  literal red-then-green demonstration, not just an assertion about current behaviour. Round 2 had
 *  TWO independent bugs stacked: `guardInvocations()` joined every logical line of a step into one
 *  string before matching (fixed by R2-1), AND `stripShellComment()` had no escape handling / heredoc
 *  awareness (fixed by R2-2) — the heredoc and escaped-quote-comment decoys below only fool round 2
 *  because BOTH bugs were present simultaneously, so this reproduces both, byte-for-byte from before
 *  either fix landed. Kept local to this test file, not production code. */
function legacyStripShellComment(line: string): string {
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
function legacyLogicalLines(run: string | undefined): string[] {
  const out: string[] = [];
  let pending = "";
  for (const raw of (run ?? "").split("\n")) {
    const line = legacyStripShellComment(raw).trim();
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
function round2StyleChannelsFromRun(run: string | undefined): string[] {
  const joined = legacyLogicalLines(run).join(" ");
  if (!joined.includes("verify-release-artifacts")) return [];
  const m = joined.match(/--expect-channel\s+(\S+)/);
  return m ? [m[1].toLowerCase()] : [];
}

const REAL_INVOCATION_LINE =
  'cargo run --locked --manifest-path crates/updater-verify/Cargo.toml --release --bin verify-release-artifacts -- \\\n' +
  '  --conf src-tauri/tauri.conf.json \\\n' +
  '  --manifest release-assets/latest.json';

describe("CPE-1908 round 3, R2-1: guardInvocations() only credits a channel to the LINE that actually invokes the binary", () => {
  it("sanity: a genuine invocation carrying the flag on its own logical line is still recognised", () => {
    const run = `${REAL_INVOCATION_LINE} \\\n  --expect-channel sidecar`;
    expect(round2StyleChannelsFromRun(run)).toEqual(["sidecar"]);
    expect(channelsDeclaredByStepRun(run)).toEqual(["sidecar"]);
  });

  it("RED (round 2 shape) / GREEN (round 3 fix): a quoted TODO on a different logical line no longer counts as coverage", () => {
    // The Reviewer's exact scenario: the real flag was disabled and replaced with a reminder comment,
    // on a SEPARATE line from the (now flag-less) real invocation.
    const run = `${REAL_INVOCATION_LINE}\necho "TODO: restore --expect-channel sidecar after the hotfix"`;
    expect(round2StyleChannelsFromRun(run), "RED: round 2's joined-string match wrongly finds coverage").toEqual(["sidecar"]);
    expect(channelsDeclaredByStepRun(run), "GREEN: round 3's per-line match correctly finds none").toEqual([]);
  });

  it("RED (round 2 shape) / GREEN (round 3 fix): a heredoc body mentioning both strings no longer counts as coverage", () => {
    const run = [REAL_INVOCATION_LINE, "cat <<'EOF'", "cargo run --bin verify-release-artifacts -- --expect-channel sidecar", "EOF"].join("\n");
    expect(round2StyleChannelsFromRun(run), "RED: round 2's joined-string match wrongly finds coverage").toEqual(["sidecar"]);
    expect(channelsDeclaredByStepRun(run), "GREEN: round 3's per-line match + heredoc-aware logicalLines() correctly finds none").toEqual([]);
  });

  it("RED (round 2 shape) / GREEN (round 3 fix): a comment surviving after a backslash-escaped quote no longer counts as coverage", () => {
    const run = `${REAL_INVOCATION_LINE}\necho "a \\" b"   # --expect-channel sidecar`;
    expect(round2StyleChannelsFromRun(run), "RED: round 2's joined-string match wrongly finds coverage").toEqual(["sidecar"]);
    expect(channelsDeclaredByStepRun(run), "GREEN: round 3's per-line match + R2-2's escape-aware stripShellComment correctly finds none").toEqual([]);
  });

  it("a decoy job whose step only ECHOES the invocation text is never credited, even with every substring present", () => {
    const decoyRun = "echo cargo run --bin verify-release-artifacts -- --expect-channel sidecar";
    // The decoy contains every substring a naive scan looks for -- prove that, then prove it's still
    // rejected because the line doesn't actually START with `cargo run`.
    expect(decoyRun.includes("verify-release-artifacts")).toBe(true);
    expect(decoyRun.includes("--expect-channel sidecar")).toBe(true);
    expect(isRealInvocationLine(decoyRun)).toBe(false);
    expect(channelsDeclaredByStepRun(decoyRun)).toEqual([]);
  });
});

describe("CPE-1908 round 3, smaller fix: isActuallyWired()'s if: comparisons tolerate incidental whitespace", () => {
  const baseInvocation: GuardInvocation = {
    channel: "sidecar",
    jobName: "verify-published-manifest-sidecar",
    jobIf: CANCELLED_GUARD_IF,
    jobNeeds: ["create-release", "release-sidecar"],
    stepIf: SIGNING_KEY_STEP_IF,
  };

  it("sanity: the canonical, exact-match form is wired", () => {
    expect(isActuallyWired("release-sidecar.yml", baseInvocation)).toBe(true);
  });

  it("RED (pre-fix behaviour) / GREEN (fixed): one extra space before the closing braces no longer reads as unwired", () => {
    const withExtraSpace: GuardInvocation = { ...baseInvocation, jobIf: "${{ !cancelled()  }}" };
    // Reproduce the OLD exact-string-compare directly to prove it really did reject this.
    expect(withExtraSpace.jobIf === CANCELLED_GUARD_IF, "RED: an exact compare rejects the harmless whitespace variant").toBe(false);
    expect(isActuallyWired("release-sidecar.yml", withExtraSpace), "GREEN: the fixed predicate accepts it").toBe(true);
  });

  it("a genuinely different if: condition (e.g. always()) is still correctly rejected", () => {
    const wrongCondition: GuardInvocation = { ...baseInvocation, jobIf: "${{ always() }}" };
    expect(isActuallyWired("release-sidecar.yml", wrongCondition)).toBe(false);
  });

  it("a hard-disabled job (if: false) is still correctly rejected", () => {
    const disabled: GuardInvocation = { ...baseInvocation, jobIf: "${{ false }}" };
    expect(isActuallyWired("release-sidecar.yml", disabled)).toBe(false);
  });

  it("an absent if: (the bare-needs: silent-skip shape) is still correctly rejected", () => {
    const absent: GuardInvocation = { ...baseInvocation, jobIf: undefined };
    expect(isActuallyWired("release-sidecar.yml", absent)).toBe(false);
  });
});
