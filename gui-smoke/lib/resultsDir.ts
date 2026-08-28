// CPE-1910 — the ONE definition of "read the WebdriverIO JSON-reporter chunks out of `.results/`".
//
// Lifted verbatim out of `scripts/run-ratchet.ts#loadCaseResults`, which was its only caller until this
// ticket. `scripts/run-suite.ts` (the retry driver) now has to answer the same question the ratchet's
// `incomplete` clause answers — "how many of this shard's spec files actually reported anything?" —
// BEFORE the ratchet runs, in order to tell a session that died before asserting from a run that
// asserted and failed. Two readers, two `readdirSync`/`JSON.parse`/manifest-filter implementations, one
// of them gating and one of them deciding whether to re-run: that is precisely the divergence this repo
// keeps paying for, so the duplication is REMOVED rather than derived (CPE-1950's preferred answer of
// the three).
//
// Living in `lib/` rather than `scripts/` is load-bearing for a second reason: `package.json`'s
// `test:unit` glob is `lib/*.test.ts`, so this logic is unit-tested (`resultsDir.test.ts`) for the first
// time. Inside `run-ratchet.ts` it never could be — the same gap CPE-1680 found in `toCaseStatus`.
import fs from "node:fs";
import path from "node:path";
import { type RawResultChunk } from "./ratchet.js";
import { SHARD_MANIFEST_PREFIX } from "./shard.js";

/** What `readResultChunks` found. `undefined` chunks means the DIRECTORY ITSELF was absent — a distinct
 *  fact from "the directory is there and empty", and the caller must be able to tell them apart. The
 *  ratchet prints a different note for each; the retry driver treats a missing directory as a suite that
 *  never got far enough to write anything, which is a retry candidate, whereas it treats an unreadable
 *  file as a hard error (see `run-suite.ts`). Never collapse the two into "0 results, carry on" — that
 *  is the "the tool said nothing, so nothing is wrong" family this repo has found nine instances of. */
export interface ResultChunkRead {
  /** `undefined` when `resultsDir` does not exist at all. Otherwise every parsed chunk, in `readdir`
   *  order. */
  chunks: RawResultChunk[] | undefined;
  /** The `*.json` file names that were read (manifests already excluded). Empty when the directory is
   *  missing OR present-but-empty — use `chunks === undefined` to tell those apart. */
  files: string[];
}

/**
 * Reads every non-manifest `*.json` file in `resultsDir` and `JSON.parse`s each into a `RawResultChunk`.
 *
 * A missing directory returns `{ chunks: undefined, files: [] }` rather than throwing — a run cancelled
 * before the suite wrote even one spec's output is a real, expected state that both callers report in
 * their own words (CPE-1728). A directory that exists but holds a file this cannot parse THROWS, naming
 * the file: a truncated or half-written artifact must never be silently counted as "that spec reported
 * nothing", because that reads as a clean-but-small run to every clause downstream.
 *
 * CPE-1753: shard manifests share this directory and the `.json` suffix. They are excluded by NAME
 * rather than by shape — a manifest happens to have a `specs` array and no `suites`, so
 * `reduceResultChunks` would contribute nothing from it, but that is harmless by accident rather than by
 * intent, and "harmless by accident" is what stops being true the next time either shape changes.
 */
export function readResultChunks(resultsDir: string): ResultChunkRead {
  if (!fs.existsSync(resultsDir)) return { chunks: undefined, files: [] };

  const files = fs
    .readdirSync(resultsDir)
    .filter((f) => f.endsWith(".json") && !f.startsWith(SHARD_MANIFEST_PREFIX));
  const chunks: RawResultChunk[] = [];
  for (const file of files) {
    const raw = fs.readFileSync(path.join(resultsDir, file), "utf-8");
    try {
      chunks.push(JSON.parse(raw) as RawResultChunk);
    } catch (err) {
      throw new Error(
        `[gui-smoke] failed to parse ${file} as JSON: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }
  return { chunks, files };
}
