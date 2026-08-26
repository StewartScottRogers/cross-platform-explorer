// CPE-1865: neither Rust build ever refused a stale lockfile — `cargo build`/`test`/`check`/`clippy`
// silently REWRITE a drifted `Cargo.lock` and exit 0, so the version-drift CLAUDE.md documents (and
// CPE-1853 measured on throwaway crates) has no backstop except the release script's own bump-all-five
// guard. `--locked` converts that into a loud, immediate failure — but only where it is actually
// applied, and "everywhere except the one place a reviewer didn't think to check" is exactly this
// repo's most-repeated defect (CPE-1855's own framing, one ticket up). This is the local ratchet: it
// reads the real workflow YAML text (no toolchain, no GitHub Actions run required — the same kind of
// guard `releaseVersionBump.test.ts` and `msrvSync.test.ts` already are) and fails if any real cargo
// build/test/check/clippy/run invocation in a CI or release workflow is missing `--locked`, or if a
// `tauri-action`/`npm run tauri build` step (which has no flag to pass `--locked` through to the
// `cargo build` it drives internally) has no preceding `cargo check --locked` preflight.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = process.cwd();

/** Every workflow that runs a real Rust build for this repo's own crates. `ffmpeg-pin-freshness.yml`
 *  and `release-pipeline-watchdog.yml` are deliberately excluded — neither runs `cargo` at all. */
const WORKFLOW_FILES = [
  ".github/workflows/ci.yml",
  ".github/workflows/release.yml",
  ".github/workflows/release-sidecar.yml",
  ".github/workflows/gui-smoke.yml",
  ".github/workflows/model-snapshot.yml",
];

/** A line counts as a real `cargo build|test|check|clippy|run` INVOCATION — not a step's `name:`
 *  label (which often echoes the subcommand in prose, e.g. `name: cargo check`) and not a `#` comment
 *  (which routinely explains what a nearby command does, e.g. "a plain `cargo build` would just..."). */
function isRealCargoInvocationLine(line: string): boolean {
  const trimmed = line.trim();
  if (trimmed.startsWith("#")) return false;
  if (trimmed.startsWith("name:") || trimmed.startsWith("- name:")) return false;
  return /\bcargo\s+(build|test|check|clippy|run)\b/.test(trimmed);
}

/** Anchors where a Tauri CLI build actually runs the real `cargo build` internally, with no flag of
 *  its own to forward `--locked` — `tauri-action` (release.yml, release-sidecar.yml) and
 *  `npm run tauri build` (gui-smoke.yml). Each one needs its own preceding preflight, not a shared one
 *  a refactor could silently orphan. */
const BUILD_ANCHORS = [/uses:\s*tauri-apps\/tauri-action@v0/, /run:\s*npm run tauri build\b/];

describe("every real cargo build/test/check/clippy/run in CI + release is --locked (CPE-1865)", () => {
  for (const file of WORKFLOW_FILES) {
    it(`${file}: every real cargo invocation line carries --locked`, () => {
      const text = readFileSync(join(ROOT, file), "utf8");
      const lines = text.split("\n");
      const invocationLines = lines
        .map((line, i) => ({ line, n: i + 1 }))
        .filter(({ line }) => isRealCargoInvocationLine(line));

      // Sanity check on the detector itself: every one of these files has at least one real
      // invocation. A regex that stopped matching anything would otherwise pass this whole suite
      // vacuously — the exact "green over zero coverage" shape this repo's other guards call out.
      expect(invocationLines.length, `${file}: no real cargo invocation lines were found at all — the detector may be broken`).toBeGreaterThan(0);

      const missing = invocationLines.filter(({ line }) => !line.includes("--locked"));
      expect(
        missing.map(({ n, line }) => `${file}:${n}: ${line.trim()}`),
        "these cargo invocations are missing --locked",
      ).toEqual([]);
    });
  }

  it("every tauri-action / npm run tauri build site has a preceding cargo check --locked preflight", () => {
    const problems: string[] = [];
    for (const file of WORKFLOW_FILES) {
      const text = readFileSync(join(ROOT, file), "utf8");
      const lines = text.split("\n");
      // Real (non-comment) `run:` lines that ARE a `cargo check --locked` preflight, by line number —
      // deliberately reusing `isRealCargoInvocationLine` so a comment merely EXPLAINING the preflight
      // (which necessarily quotes "cargo check --locked" in prose right next to the real step) can
      // never itself satisfy this check. That false-negative was caught by this test's own red-proof:
      // removing the real step while leaving its explanatory comment above it still passed, until this
      // line-level filter replaced a raw substring search over the whole preceding text window.
      const preflightLineNumbers = lines
        .map((line, i) => ({ line, n: i + 1 }))
        .filter(({ line }) => isRealCargoInvocationLine(line) && /cargo check --locked/.test(line))
        .map(({ n }) => n);

      for (const anchor of BUILD_ANCHORS) {
        const re = new RegExp(anchor, "g");
        let m: RegExpExecArray | null;
        while ((m = re.exec(text))) {
          const anchorLine = text.slice(0, m.index).split("\n").length;
          // The preflight lives in the step immediately before this one, well within a 40-line
          // lookback (comfortably covers the longest preamble comment block used anywhere here).
          const hasPreflight = preflightLineNumbers.some((n) => n < anchorLine && anchorLine - n <= 40);
          if (!hasPreflight) {
            problems.push(`${file}:${anchorLine}: real Tauri build with no preceding "cargo check --locked" preflight`);
          }
        }
      }
    }
    expect(problems).toEqual([]);
  });
});
