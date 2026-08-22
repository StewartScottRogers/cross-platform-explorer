// CPE-1764: `release-sidecar.yml`'s native-deps download guard (CPE-1762) checked only the HTTP
// status code, so a 200 response carrying an HTML error page, a login/interception redirect, or a
// body cut short by a dropped connection all reached `tar`/`unzip` unexamined -- reproducing the
// exact "xz: File format not recognized" confusion CPE-1762 exists to eliminate. Measured against a
// live 200-serving HTML URL: see this ticket's Work Log.
//
// This guard now checks the downloaded body's magic bytes against the type EXPECTED AT THAT CALL
// SITE (not one hardcoded type), a minimum plausible size, and -- for the two BtbN ffmpeg downloads,
// which publish a real `checksums.sha256` per release -- a full sha256 checksum comparison (catches a
// body with a valid header that's still wrong past the size floor). That checksum layer is SAME-ORIGIN
// (fetched from the same release, same channel, same curl as the artifact) so it buys corruption/
// truncation detection, NOT tamper resistance -- see the in-workflow comment and the dedicated tests
// below. bblanchon (pdfium) does NOT publish a plain checksum file; it publishes something STRONGER --
// a Sigstore attestation bundle with an independent trust anchor -- currently unused here (needs
// `gh attestation verify`/cosign, a heavier follow-up). Reviewer round on this ticket: confirmed a
// Reviewer-authored red-proof that swapped every new `exit 1` for a warning-and-continue left all
// message-content assertions green, so every substantive guard block below is additionally pinned to
// actually abort (see the "aborts, not just diagnoses" test).
//
// Structural assertions go through `parseYaml`, the in-repo bounded-subset YAML parser
// (src/lib/preview/yaml.ts, CPE-1617) -- the same approach ciAptGetHardening.test.ts (CPE-1787) uses,
// adopted after a Reviewer round found a regex-over-raw-text guard there could be satisfied by an
// unrelated neighbouring COMMENT rather than the key it claimed to check. Reading `step.run` off the
// PARSED object means a `# CPE-1764: ...` prose comment sitting above or beside a `fetch` call can
// never be mistaken for the call itself, because YAML comments are stripped before the string value
// is ever produced.
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
 *  subset -- so a future edit that pushes release-sidecar.yml past what this parser understands is
 *  reported here as a clear parse failure, not a silently-wrong empty result. */
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

const doc = parseWorkflow("release-sidecar.yml");
const step = findStep(doc.jobs["release-sidecar"], "Stage native deps — ffmpeg + pdfium (CPE-1258)");
const run = step.run ?? "";
const lines = run.split("\n");

/** Real `fetch` CALL lines (not the `fetch() {` definition, not a comment that merely mentions
 *  `fetch`). Requires all four positional args: a quoted url, a bare output filename, a quoted magic
 *  hex, and a quoted label -- the exact shape CPE-1764 requires at every call site. A line missing
 *  an argument (the pre-CPE-1764 2-arg shape) will NOT match this, so a regression back to a bare
 *  status-only call fails this test by simply not being counted below. */
const FETCH_CALL_RE = /^\s*fetch\s+"([^"]*)"\s+(\S+)\s+"([0-9a-f]+)"\s+"([^"]+)"\s*$/;

/** Resolves `${VAR}` (and bare `$VAR`) references in a captured arg by chasing this script's own
 *  `VAR="..."` assignments -- e.g. the ffmpeg call sites pass `"$FFMPEG_WIN_URL"`, itself assigned
 *  `"https://.../${FFMPEG_WIN_ASSET}"`, itself assigned `"ffmpeg-${FFMPEG_BUILD_VER}-win64-lgpl-8.1.zip"`.
 *  A var this script never assigns (`FFMPEG_BUILD_TAG`/`FFMPEG_BUILD_VER`, set in the step's `env:`
 *  block, outside `run:`) is left as a literal `${...}` placeholder -- harmless, since it's the
 *  LITERAL suffix (the file extension) this test cares about, not the fully-resolved URL. Bounded
 *  recursion guards against a self-referential assignment. */
function substituteVars(str: string, depth = 0): string {
  if (depth > 5) return str;
  return str.replace(/\$\{([A-Za-z0-9_]+)\}/g, (whole, name: string) => {
    const assignMatch = new RegExp(`^\\s*${name}="([^"]*)"\\s*$`, "m").exec(run);
    return assignMatch ? substituteVars(assignMatch[1], depth + 1) : whole;
  });
}

function resolveArg(rawArg: string): string {
  const bareVar = /^\$([A-Za-z0-9_]+)$/.exec(rawArg);
  return substituteVars(bareVar ? `\${${bareVar[1]}}` : rawArg);
}

function fetchCalls(): { url: string; out: string; magic: string; label: string }[] {
  return lines
    .map((line) => FETCH_CALL_RE.exec(line))
    .filter((m): m is RegExpExecArray => m !== null)
    .map((m) => ({ url: resolveArg(m[1]), out: m[2], magic: m[3], label: m[4] }));
}

/** Real `verify_btbn_checksum` CALL lines (not its definition, not a comment). */
const CHECKSUM_CALL_RE = /^\s*verify_btbn_checksum\s+\S+\s+\S+\s+\S+\s*$/;

/** Finds the (unique) line containing `marker` inside the run script and returns the line
 *  immediately after it, trimmed. Used to pin `exit 1` directly beneath each guard's error output.
 *
 *  Why this exists (not just `toContain` on the message): a Reviewer round on this ticket red-proofed
 *  the original test suite by swapping every new guard's `exit 1` for
 *  `echo "::warning::continuing anyway"`, leaving every error message intact -- all 10 tests still
 *  passed, because none of them checked that the block actually ABORTS. That is exactly the "fails by
 *  succeeding" class this ticket's Problem section names: a perfect diagnosis printed, then the bad
 *  body handed to tar anyway. Throws (rather than returning something a stray match could satisfy) if
 *  the marker isn't found exactly once, so a future wording change that breaks the pin fails loudly
 *  here instead of silently checking nothing. */
function lineImmediatelyAfter(marker: string): string {
  const matchingIndices = lines.reduce<number[]>((acc, l, i) => (l.includes(marker) ? [...acc, i] : acc), []);
  if (matchingIndices.length !== 1) {
    throw new Error(`expected exactly one line containing ${JSON.stringify(marker)}, found ${matchingIndices.length}`);
  }
  return lines[matchingIndices[0] + 1].trim();
}

describe("release-sidecar.yml's native-deps download guard checks the body, not just the status (CPE-1764)", () => {
  it("the fetch() function itself still gates on HTTP status first (CPE-1762 baseline preserved)", () => {
    expect(run).toContain('if [ "$code" != "200" ]; then');
  });

  it("fetch() checks the body's magic bytes against the call site's expected type", () => {
    expect(run).toContain('if [ "$actual" != "$magic" ]; then');
    // The failure message must name the URL and say what was expected vs. what arrived (rule: a
    // person must be able to act on it) -- not just "download body is wrong".
    expect(run).toContain('download body is not a $label archive: $url');
    expect(run).toContain("expected magic bytes $magic, got");
  });

  it("fetch() also requires a minimum plausible size, as a second independent signal", () => {
    expect(run).toContain('if [ "$size" -lt "$MIN_ARCHIVE_BYTES" ]; then');
    const minMatch = /^\s*MIN_ARCHIVE_BYTES=(\d+)\s*$/m.exec(run);
    expect(minMatch).not.toBeNull();
    const minBytes = Number(minMatch![1]);
    // Every real asset here is multiple MB (smallest pinned pdfium leg ~3.4MB, measured live); the
    // floor must be comfortably below that so a genuine download never trips it, but well above what
    // an HTML error page or an empty/near-empty body would be.
    expect(minBytes).toBeGreaterThanOrEqual(4096);
    expect(minBytes).toBeLessThan(1_000_000);
  });

  it("every substantive guard block actually ABORTS (exit 1), not just prints a diagnosis (Reviewer red-proof: exit 1 -> warning-and-continue)", () => {
    // CPE-1762 baseline: bad status.
    expect(lineImmediatelyAfter("bump FFMPEG_BUILD_TAG/FFMPEG_BUILD_VER to a current release. See CPE-1762.")).toBe(
      "exit 1",
    );
    // CPE-1764: wrong magic bytes.
    expect(
      lineImmediatelyAfter("page, a login/interception redirect, or a placeholder, not the archive. CPE-1764."),
    ).toBe("exit 1");
    // CPE-1764: body too small.
    expect(lineImmediatelyAfter("truncated transfer that never finished. See CPE-1764.")).toBe("exit 1");
    // CPE-1764: couldn't fetch BtbN's checksums.sha256 itself (e.g. a 404 on a bad tag).
    expect(lineImmediatelyAfter("can't verify $url without it. See CPE-1764.")).toBe("exit 1");
    // CPE-1764: checksums.sha256 fetched fine but has no entry for this asset name.
    expect(lineImmediatelyAfter("can't verify $url against a published checksum. See CPE-1764.")).toBe("exit 1");
    // CPE-1764: checksum mismatch.
    expect(
      lineImmediatelyAfter("checksum (this is NOT tamper resistance - see the comment above). CPE-1764."),
    ).toBe("exit 1");
  });

  it("every currently-guarded download site calls fetch() with all four arguments (url, out, magic, label)", () => {
    const calls = fetchCalls();
    // pdfium x3 (windows/linux/macos) + ffmpeg x2 (windows/linux) = 5. The macOS ffmpeg leg builds
    // from source via `git clone` and deliberately does NOT go through fetch() -- see the dedicated
    // test below.
    expect(calls.length).toBe(5);
  });

  it("pdfium call sites (all three OSes) expect a gzip (.tgz) body -- not a single hardcoded type shared with ffmpeg", () => {
    const pdfiumCalls = fetchCalls().filter((c) => c.url.includes("pdfium-binaries"));
    expect(pdfiumCalls.length).toBe(3);
    for (const call of pdfiumCalls) {
      expect(call.magic).toBe("1f8b");
      expect(call.label.toLowerCase()).toContain("gzip");
    }
  });

  it("the Windows ffmpeg call site (a BtbN .zip asset) expects a zip body", () => {
    // Keyed off the URL/asset itself, not the local `out` filename -- a Reviewer round found that
    // keying this off `c.out === "ffmpeg.zip"` lets someone repoint the URL to a different asset type
    // while keeping the same local output name and still pass this suite.
    const winFfmpeg = fetchCalls().find((c) => c.url.includes("BtbN/FFmpeg-Builds") && c.url.endsWith(".zip"));
    expect(winFfmpeg).toBeDefined();
    expect(winFfmpeg!.magic).toBe("504b0304");
    expect(winFfmpeg!.label.toLowerCase()).toContain("zip");
  });

  it("the Linux ffmpeg call site (a BtbN .tar.xz asset) expects an xz body -- proving the type is per-call-site, not hardcoded", () => {
    const linuxFfmpeg = fetchCalls().find((c) => c.url.includes("BtbN/FFmpeg-Builds") && c.url.endsWith(".tar.xz"));
    expect(linuxFfmpeg).toBeDefined();
    expect(linuxFfmpeg!.magic).toBe("fd377a585a00");
    expect(linuxFfmpeg!.label.toLowerCase()).toContain("xz");
    // Different from both the gzip pdfium sites AND the zip Windows ffmpeg site.
    expect(linuxFfmpeg!.magic).not.toBe("1f8b");
    expect(linuxFfmpeg!.magic).not.toBe("504b0304");
  });

  it("both BtbN ffmpeg downloads (Windows zip, Linux tar.xz) are additionally checksum-verified", () => {
    const checksumCalls = lines.filter((line) => CHECKSUM_CALL_RE.test(line) && !line.trim().startsWith("#"));
    expect(checksumCalls.length).toBe(2);
    // Checksum mismatch must also name the URL and say what was expected vs. got, same standard as
    // the magic-byte failure.
    expect(run).toContain("checksum mismatch for $url");
    expect(run).toContain("expected sha256 $expected");
  });

  it("verify_btbn_checksum checks its OWN fetch status separately, so a 404'd checksums.sha256 is reported as a fetch failure, not misdiagnosed as \"no checksum entry\"", () => {
    expect(run).toContain('if [ "$sums_code" != "200" ]; then');
    expect(run).toContain("couldn't fetch BtbN's checksums.sha256 (HTTP $sums_code)");
  });

  it("the checksum-mismatch message does NOT overclaim tamper detection (same-origin, same-channel checksum is not an independent trust anchor)", () => {
    // The old, overclaiming wording this ticket's Reviewer flagged.
    expect(run).not.toContain("tampered in transit. This is exactly what magic-bytes+size can miss");
    // The honest replacement.
    expect(run).toContain("this is NOT tamper resistance");
    expect(run).toContain("substitution that didn't also rewrite the");
  });

  it("documents that the BtbN checksum is same-origin/same-channel with no independent trust anchor, and that release ASSETS are mutable within a pinned tag", () => {
    expect(run).toContain("no independent trust anchor");
    expect(run).toContain("same-origin sha256");
    expect(run).toContain("release");
    expect(run).toContain("ASSETS are mutable within a tag");
  });

  it("frames pdfium's Sigstore attestation as the STRONGER, currently-unused mechanism, and BtbN's same-origin checksum as the WEAKER, wired-up one (not the other way around)", () => {
    expect(run).toContain("Fulcio-issued cert");
    expect(run).toContain("STRONGER mechanism, currently");
    expect(run).toContain("UNUSED here");
    expect(run).toContain("WEAKER of the");
    expect(run).toContain("don't read \"ffmpeg has a checksum, pdfium doesn't\" as \"ffmpeg is");
  });

  it("checksum verification is honest about not being available for pdfium (documented in-workflow, not silently skipped)", () => {
    expect(run).toContain("pdfium-attestation.json");
    expect(run).toContain("gh attestation verify");
  });

  it("the macOS ffmpeg leg (built from source via git clone) is confirmed out of scope in-workflow", () => {
    expect(run).toContain("git clone --depth 1");
    expect(run).toContain("does NOT go through `fetch`");
  });
});
