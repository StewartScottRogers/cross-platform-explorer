// CPE-1908: the channel-purity guard `platforms_with_mismatched_channel` (crates/updater-verify,
// CPE-1894) ran against the PLAIN release manifest only — `release-sidecar.yml`, the workflow that
// builds the channel users actually install (see [[always-install-sidecar-build]]), never invoked
// `verify-release-artifacts` at all. That gap shipped silently: nothing failed, nothing warned, the
// job simply didn't exist. A guard that can go missing without a red build is exactly the shape of
// defect this repo keeps re-discovering (CPE-1872, CPE-1893, CPE-1903 — see each ticket's own "how did
// this slip through" section), so this ticket's own fix must not be exposed to the same failure mode a
// second time.
//
// This file is the structural ratchet: `crates/updater-verify/src/lib.rs`'s `pub enum Channel { ... }`
// is the SINGLE canonical list of channels the guard logic knows how to check (today: Sidecar, Plain).
// This test reads that enum's variant names DIRECTLY from the Rust source (no re-implementation, no
// hand-maintained duplicate list to drift) and asserts every one of them has a REAL
// `verify-release-artifacts` invocation, in a REAL release workflow, declaring `--expect-channel
// <that channel>` explicitly. If a future PR adds a third `Channel` variant (a new release channel)
// without also wiring a `--expect-channel` invocation for it into some workflow, THIS test goes red —
// the exact "adding a channel without guarding it" failure the ticket asks to make impossible to lose
// silently. Structural assertions go through `parseYaml` (src/lib/preview/yaml.ts, CPE-1617), the same
// approach `catalogPublishFreshnessGuard.test.ts` and `releaseHangHardening.test.ts` use.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseYaml } from "./preview/yaml";

const ROOT = process.cwd();
const LIB_RS = join(ROOT, "crates", "updater-verify", "src", "lib.rs");
const WORKFLOWS = join(ROOT, ".github", "workflows");

interface WorkflowStep {
  name?: string;
  run?: string;
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

/** Every `Channel` enum variant name, read straight from the Rust source rather than duplicated by
 *  hand — this IS the canonical list a future channel would be added to. */
function readCanonicalChannelsFromRustEnum(): string[] {
  const src = readFileSync(LIB_RS, "utf8");
  const enumMatch = src.match(/pub enum Channel \{([\s\S]*?)\n\}/);
  expect(enumMatch, "crates/updater-verify/src/lib.rs must still declare `pub enum Channel { ... }` — the detector may be broken").not.toBeNull();
  const body = enumMatch![1];
  const variants = body
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("//"))
    .map((line) => {
      const m = line.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*,?\s*$/);
      return m ? m[1] : null;
    })
    .filter((v): v is string => v !== null)
    .map((v) => v.toLowerCase());
  return variants;
}

/** Every `--expect-channel <value>` this workflow's steps pass to `verify-release-artifacts`, by
 *  scanning each step's own `run` field (not raw file text/comments — CPE-1908 follows the same
 *  parseYaml-over-regex convention `catalogPublishFreshnessGuard.test.ts`'s header comment explains). */
function channelsGuardedByWorkflow(fileName: string): string[] {
  const doc = parseWorkflow(fileName);
  const found: string[] = [];
  for (const job of Object.values(doc.jobs)) {
    for (const step of job.steps ?? []) {
      const run = step.run ?? "";
      if (!run.includes("verify-release-artifacts")) continue;
      const m = run.match(/--expect-channel\s+(\S+)/);
      if (m) found.push(m[1].toLowerCase());
    }
  }
  return found;
}

describe("every Channel variant the guard logic knows about has a real workflow invocation (CPE-1908)", () => {
  const canonicalChannels = readCanonicalChannelsFromRustEnum();

  it("the Rust enum still names exactly the two channels this repo ships (sanity check on the detector itself)", () => {
    // Not a hard requirement of the mechanism (a third channel is a legitimate future addition), but
    // if this ever silently reads zero or one variant the detector itself is broken and every
    // assertion below would pass vacuously — the "green over zero coverage" trap this repo's other
    // guards explicitly call out (see lockfileLockedGuard.test.ts's identical caution).
    expect(canonicalChannels.length).toBeGreaterThanOrEqual(2);
    expect(canonicalChannels).toContain("plain");
    expect(canonicalChannels).toContain("sidecar");
  });

  it("release.yml guards the plain channel with --expect-channel plain", () => {
    expect(channelsGuardedByWorkflow("release.yml")).toContain("plain");
  });

  it("release-sidecar.yml guards the sidecar channel with --expect-channel sidecar (CPE-1908's own fix)", () => {
    expect(channelsGuardedByWorkflow("release-sidecar.yml")).toContain("sidecar");
  });

  it("the union of every workflow's guarded channels covers EVERY Channel variant the Rust guard knows about", () => {
    const guarded = new Set([
      ...channelsGuardedByWorkflow("release.yml"),
      ...channelsGuardedByWorkflow("release-sidecar.yml"),
    ]);
    const missing = canonicalChannels.filter((c) => !guarded.has(c));
    expect(
      missing,
      `crates/updater-verify/src/lib.rs's Channel enum names ${JSON.stringify(canonicalChannels)}, but no ` +
        `release.yml/release-sidecar.yml step passes --expect-channel for: ${JSON.stringify(missing)}. ` +
        `A Channel variant with no guarding workflow invocation is exactly the CPE-1908 gap re-opening — ` +
        `wire a verify-release-artifacts step for it into the workflow that builds that channel.`,
    ).toEqual([]);
  });

  it("release.yml and release-sidecar.yml each guard a DIFFERENT channel (not both plain, or both sidecar)", () => {
    // Defensive against the specific historical failure mode (CPE-1894/CPE-1908): a channel getting
    // silently left out isn't the only way this could regress — a copy-paste of one workflow's
    // --expect-channel value into the other would leave the union check above green (both channels
    // still appear somewhere) while actually leaving one channel's OWN workflow unguarded again.
    const plainChannels = new Set(channelsGuardedByWorkflow("release.yml"));
    const sidecarChannels = new Set(channelsGuardedByWorkflow("release-sidecar.yml"));
    expect(plainChannels.has("sidecar"), "release.yml (the plain-tag workflow) must not claim to guard the sidecar channel").toBe(false);
    expect(sidecarChannels.has("plain"), "release-sidecar.yml must not claim to guard the plain channel").toBe(false);
  });
});
