// CPE-1753 — the ONE definition of "which spec files does this suite consist of", shared by everything
// that has to agree on it: `wdio.conf.ts` (which files to run), `scripts/write-shard-manifest.ts` (what a
// shard declares it owns), and `scripts/run-ratchet.ts` (how many should have reported).
//
// Before this ticket the answer was written twice — `wdio.conf.ts`'s `specs: ["./specs/**/*.smoke.ts"]`
// glob and `run-ratchet.ts`'s `readdirSync(specsDir).filter(endsWith(".smoke.ts"))`. They agreed only
// because every spec file happens to sit flat directly under `specs/`. Under sharding they MUST agree
// exactly: the ratchet's expectation and the shard partition are computed from this list, so any
// divergence would either expect a file nobody was assigned (a permanent red) or, far worse, assign a
// file to nobody while nothing expects it (a permanent, invisible coverage hole).
//
// Not in `lib/shard.ts` because that module is deliberately pure/no-I/O so it can be unit-tested without
// a fixture tree; this one reads a directory and is exercised by the real CI path.
import fs from "node:fs";
import path from "node:path";
import { compareSpecNames } from "./shard.js";

/** The suffix that makes a file a spec. Matches the `specs/*.smoke.ts` convention documented in
 *  `gui-smoke/README.md` and used by `known-failing.json`'s `spec` field. */
export const SPEC_SUFFIX = ".smoke.ts";

/**
 * Every spec file in `specsDir`, as BASENAMES (`"samples.smoke.ts"`), sorted with the same
 * locale-independent comparison the shard partition uses.
 *
 * Basenames, not paths, because that is the identity `known-failing.json`, `lib/ratchet.ts#caseKey` and
 * `reduceResultChunks`'s `specBasename` all already speak. Callers that need a runnable path prepend the
 * directory themselves.
 *
 * NON-RECURSIVE, and it THROWS if it finds a subdirectory. The old wdio glob was `./specs/**\/*.smoke.ts`
 * while the old ratchet count was a flat `readdirSync`, so a nested spec would have RUN but never been
 * EXPECTED — harmless-looking then, and a silent hole now: under sharding a nested spec would be assigned
 * to no shard, run nowhere, be expected by nothing, and every job would stay green. Rather than pick one
 * of the two behaviours quietly, this refuses to run at all until the tree is flat again, which is a
 * ten-second fix and cannot be mistaken for a passing suite.
 */
export function listSpecFiles(specsDir: string): string[] {
  if (!fs.existsSync(specsDir)) {
    throw new Error(`[gui-smoke] specs directory not found: ${specsDir}`);
  }
  const entries = fs.readdirSync(specsDir, { withFileTypes: true });
  const nested = entries.filter((e) => e.isDirectory()).map((e) => e.name);
  if (nested.length > 0) {
    throw new Error(
      `[gui-smoke] specs/ contains subdirector${nested.length === 1 ? "y" : "ies"} (${nested.join(", ")}), ` +
        `but the sharded runner only supports a FLAT specs/ directory. A nested spec file would be assigned ` +
        `to no shard, run nowhere, and be expected by nothing — a silent coverage hole behind a green gate ` +
        `(CPE-1753). Move the spec files back up to specs/, or teach lib/specFiles.ts, lib/shard.ts and ` +
        `lib/ratchet.ts#specBasename about nesting together, in one change.`,
    );
  }
  return entries
    .filter((e) => e.isFile() && e.name.endsWith(SPEC_SUFFIX))
    .map((e) => e.name)
    .sort(compareSpecNames);
}

/** The path form `wdio.conf.ts`'s `specs` option wants — relative to the config file's own directory,
 *  matching the `./specs/…` shape that was there before sharding. */
export function specRunPath(basename: string): string {
  return `./${path.posix.join("specs", basename)}`;
}
