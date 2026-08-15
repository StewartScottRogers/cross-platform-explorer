// CPE-1753 — deterministic spec-file sharding for the gui-smoke Linux leg. Pure, no I/O (same "pure lib
// module + thin CLI wrapper" split as `ratchet.ts`/`scripts/run-ratchet.ts` and `compare.ts`).
//
// WHY THIS EXISTS. A fully healthy green `gui-smoke-linux` run measured 41.70 min against its own
// 45-minute job cap (PR #912's run 31871682587, head 856dbebc). Suite duration scales with spec count —
// bucketed by the spec count at each run's own head sha, 40 specs ran 27.50/28.15/29.25 min (n=6) and 41
// specs ran 29.57/29.95/30.42 (n=4): DISJOINT ranges, +1.80 min at the median per added spec, 0.73
// min/spec flat-averaged. The repo went 22 -> 39 -> 41 specs between 2026-08-01 and 08-15, so two to four
// more spec files would have breached the cap — at which point the job dies mid-suite on every PR with no
// verdict left for `if: always()` to rescue. A step-level cap was tried in CPE-1728 and removed: the
// usable window was ~2 minutes wide, narrower than one spec file's growth. Splitting the suite across N
// parallel jobs is the only fix whose margin does not shrink one-for-one with the spec count.
//
// THE DESIGN, in one paragraph. `gui-smoke.yml` builds the app ONCE (`gui-smoke-linux-build`), then fans
// out to a matrix of shard jobs that each run a DISJOINT subset of `specs/*.smoke.ts` chosen by
// `assignShardSpecs` below, then joins on a single `gui-smoke-linux-verdict` job that merges every
// shard's `.results/` and runs the ratchet over the WHOLE set. Each shard also runs the ratchet over its
// OWN subset for fast, local feedback — both gate, neither can mask the other.
//
// THE THING THAT MUST NOT GO WRONG, and the reason this module is separate and unit-tested. The ratchet's
// completeness check compares `reportedSpecCount` against an `expectedSpecCount` globbed live from disk.
// Sharding splits `.results/` across jobs, so a naive aggregation would compute the expectation FROM THE
// ARTIFACTS IT FOUND — and then a shard that never reported at all (cancelled, crashed, never scheduled)
// simply lowers the expectation to match, and the verdict goes green having verified a fraction of the
// suite. That is the exact silently-passing shape CPE-1728 exists to eliminate. So:
//   - the aggregate's `expectedSpecCount` is still globbed from the CHECKED-OUT `specs/` directory, never
//     derived from the downloaded results; and
//   - every shard writes a `shard-manifest-<n>.json` BEFORE its suite runs, and the verdict job asserts
//     it received exactly the shard indices `1..GUI_SMOKE_EXPECT_SHARDS` — a number that comes from the
//     workflow file, never from the manifests it found (see `lib/ratchet.ts` clause 9).
// A missing shard is therefore RED twice over: once because its indices are absent from the manifest set,
// and once because its spec files never reported a result against a live-globbed expectation.
//
// SHARD-COUNT DRIFT IS ALSO UNABLE TO GO GREEN. The shard jobs take their `shardTotal` from GitHub's own
// `strategy.job-total` (so it is literally the size of the matrix, not a second literal that can drift),
// and record it in their manifest. The verdict job carries the one literal, `GUI_SMOKE_EXPECT_SHARDS`.
// Add a matrix entry without bumping that literal and every manifest reports a `shardTotal` the verdict
// does not expect -> SHARD PLAN MISMATCH. Bump the literal without adding the matrix entry and one index
// never reports -> MISSING SHARD. Both directions red; there is no way to make them disagree quietly.

/** Env var naming ONE shard's position, 1-based (`1`..`shardTotal`). Set per matrix job in
 *  `gui-smoke.yml` from `${{ matrix.shard }}`. Read by `wdio.conf.ts` (to pick which spec files to run),
 *  by `scripts/write-shard-manifest.ts` (to declare what this shard is responsible for) and by
 *  `scripts/run-ratchet.ts` (to scope the shard-local verdict). */
export const SHARD_INDEX_ENV = "GUI_SMOKE_SHARD_INDEX";

/** Env var naming how many shards the suite is split into. Set from GitHub's `${{ strategy.job-total }}`
 *  so it IS the matrix size rather than a second literal that could drift from it. */
export const SHARD_TOTAL_ENV = "GUI_SMOKE_SHARD_TOTAL";

/** Env var that puts `scripts/run-ratchet.ts` in AGGREGATE mode: "I expect manifests from exactly this
 *  many shards, and I will red if any of them is missing." Deliberately a DIFFERENT variable from
 *  `SHARD_TOTAL_ENV`: the aggregate must get its expectation from the workflow file, never from the
 *  artifacts it happened to download, or a missing shard silently lowers the bar it is measured against
 *  (see this module's header). Setting it together with `SHARD_INDEX_ENV` is an error — one process is
 *  either one shard or the join, never both. */
export const EXPECT_SHARDS_ENV = "GUI_SMOKE_EXPECT_SHARDS";

/** Filename prefix for a shard's manifest inside `.results/`. Chosen so it can be told apart from the
 *  `@wdio/json-reporter` output files sharing that directory (`wdio-*.json`) by name alone — the manifest
 *  is NOT a result chunk and must never be fed to `reduceResultChunks` (it would parse as a chunk with no
 *  `suites`, contributing nothing, which is harmless but accidental rather than intended). */
export const SHARD_MANIFEST_PREFIX = "shard-manifest-";

/** One shard's position in the split. `shardIndex` is 1-based so it reads the same as the GitHub matrix
 *  value and the artifact names a human downloads. */
export interface ShardId {
  shardIndex: number;
  shardTotal: number;
}

/** What one shard job declares it is responsible for, written to `.results/shard-manifest-<n>.json`
 *  BEFORE the suite runs (see `scripts/write-shard-manifest.ts`).
 *
 *  Written before rather than after, deliberately: a shard that dies mid-suite has still told the verdict
 *  job which spec files it OWED, so the missing results show up as an incomplete run naming real spec
 *  files, instead of vanishing along with the shard. A manifest that is missing entirely means the shard
 *  never got as far as `npm ci` — or was cancelled/never scheduled — which is its own, distinct red. */
export interface ShardManifest {
  shardIndex: number;
  shardTotal: number;
  /** Spec file BASENAMES (e.g. `"samples.smoke.ts"`) this shard was assigned — the same identity
   *  `known-failing.json` and `lib/ratchet.ts#caseKey` use, so the three can be compared directly. */
  specs: string[];
}

/** Thrown for every malformed shard configuration. A separate type so a caller can tell "you configured
 *  the shard wrong" apart from an ordinary I/O failure, and so the message is guaranteed to name the env
 *  vars involved. */
export class ShardConfigError extends Error {
  constructor(message: string) {
    super(`[gui-smoke shard] ${message}`);
    this.name = "ShardConfigError";
  }
}

/** Reads `GUI_SMOKE_SHARD_INDEX`/`GUI_SMOKE_SHARD_TOTAL` out of an env-shaped object.
 *
 *  Returns `undefined` when NEITHER is set — that is the unsharded path (a local `npm test`, and the
 *  Windows leg), which must keep running the whole suite exactly as before.
 *
 *  THROWS when only one of the two is set. This is the important case: a half-configured shard is a
 *  workflow-editing mistake, and the two tempting "helpful" fallbacks are both silent disasters — running
 *  the WHOLE suite in every shard (N times the cost, and the fix this ticket exists for undone), or
 *  defaulting the total to 1 so shard 3 of 4 quietly runs shard 1 of 1's specs and three quarters of the
 *  suite is never executed while everything reports green. Refusing to start is the only safe answer.
 *  Also throws on a non-integer, a total below 1, or an index outside `1..total`. */
export function parseShardId(env: Record<string, string | undefined>): ShardId | undefined {
  const rawIndex = (env[SHARD_INDEX_ENV] ?? "").trim();
  const rawTotal = (env[SHARD_TOTAL_ENV] ?? "").trim();
  if (rawIndex === "" && rawTotal === "") return undefined;
  if (rawIndex === "" || rawTotal === "") {
    throw new ShardConfigError(
      `${SHARD_INDEX_ENV}=${JSON.stringify(rawIndex)} and ${SHARD_TOTAL_ENV}=${JSON.stringify(rawTotal)} — ` +
        `these two must be set together or not at all. Refusing to guess: defaulting the missing one would ` +
        `either run the whole suite in every shard (the sharding undone) or run one shard's subset while ` +
        `reporting as if the whole suite had run (a green verdict over a fraction of the tests).`,
    );
  }
  const shardIndex = parseStrictInt(rawIndex, SHARD_INDEX_ENV);
  const shardTotal = parseStrictInt(rawTotal, SHARD_TOTAL_ENV);
  assertShardId({ shardIndex, shardTotal });
  return { shardIndex, shardTotal };
}

/** Reads `GUI_SMOKE_EXPECT_SHARDS` (aggregate mode). Returns `undefined` when unset. Throws when it is
 *  set alongside `GUI_SMOKE_SHARD_INDEX` (a process is one shard or the join, never both) or when it is
 *  not a positive integer — an aggregate that does not know how many shards to expect cannot certify
 *  anything, exactly like `evaluate()`'s `expectedSpecCount < 1` clause. */
export function parseExpectedShards(env: Record<string, string | undefined>): number | undefined {
  const raw = (env[EXPECT_SHARDS_ENV] ?? "").trim();
  if (raw === "") return undefined;
  if ((env[SHARD_INDEX_ENV] ?? "").trim() !== "") {
    throw new ShardConfigError(
      `${EXPECT_SHARDS_ENV} and ${SHARD_INDEX_ENV} are both set. ${SHARD_INDEX_ENV} means "I am one ` +
        `shard, verify my own subset"; ${EXPECT_SHARDS_ENV} means "I am the join, verify that every shard ` +
        `reported". A process is one or the other — never both — so this is a workflow bug, not a mode.`,
    );
  }
  const expected = parseStrictInt(raw, EXPECT_SHARDS_ENV);
  if (expected < 1) {
    throw new ShardConfigError(`${EXPECT_SHARDS_ENV} is ${expected} (must be >= 1).`);
  }
  return expected;
}

function parseStrictInt(raw: string, varName: string): number {
  // `Number()` rather than `parseInt`, deliberately: `parseInt("4x")` is 4, which would let a typo'd
  // matrix value silently become a valid shard number.
  const value = Number(raw);
  if (!Number.isInteger(value)) {
    throw new ShardConfigError(`${varName}=${JSON.stringify(raw)} is not an integer.`);
  }
  return value;
}

function assertShardId({ shardIndex, shardTotal }: ShardId): void {
  if (shardTotal < 1) {
    throw new ShardConfigError(`${SHARD_TOTAL_ENV} is ${shardTotal} (must be >= 1).`);
  }
  if (shardIndex < 1 || shardIndex > shardTotal) {
    throw new ShardConfigError(
      `${SHARD_INDEX_ENV} is ${shardIndex}, which is outside 1..${shardTotal}. Shard numbers are 1-based ` +
        `and must match the matrix. An out-of-range shard would run an EMPTY spec list and, without this ` +
        `check, report a happy "0 of 0 spec files reported" — verifying nothing.`,
    );
  }
}

/**
 * Picks the spec files this shard owns, deterministically, from the full sorted list.
 *
 * Round-robin over a code-unit-sorted copy (`i % shardTotal === shardIndex - 1`), not contiguous blocks.
 * The reason is balance without a cost model: contiguous blocks would put every alphabetically-adjacent
 * heavy spec in the same shard. Adding one spec file changes each shard's SIZE by at most one — but do
 * not mistake that for stability of the assignment: measured, inserting one spec into the middle of the
 * sorted list moves 23 of 41 specs to a different shard, where contiguous blocks would move 3. (An
 * earlier version of this comment claimed the opposite, and the CPE-1753 review measured it.) That churn
 * is harmless here because nothing is cached per shard and the verdict is reassembled from all of them
 * every run; it would stop being harmless the moment anything memoises a spec-to-shard mapping.
 * Sorted with a plain code-unit comparison rather
 * than `localeCompare` so the partition is byte-identical on every runner and locale — a partition that
 * differed between the shard job and the verdict job would put a spec in two shards or none, and "none"
 * is the dangerous direction.
 *
 * Every spec appears in EXACTLY one shard, and the union is the whole list — a property `lib/ratchet.ts`
 * clause 9 re-checks at the join against the live-globbed `specs/` directory rather than trusting it.
 *
 * A shard with MORE shards than spec files legitimately gets an empty list; that is not an error here
 * (the run is still complete overall), and the verdict job's coverage check is what would catch a real
 * gap. `scripts/run-ratchet.ts` prints the empty assignment loudly so an over-sharded matrix is visible
 * rather than looking like a fast green.
 */
export function assignShardSpecs(allSpecs: string[], id: ShardId): string[] {
  assertShardId(id);
  const sorted = [...allSpecs].sort(compareSpecNames);
  return sorted.filter((_, i) => i % id.shardTotal === id.shardIndex - 1);
}

/** Locale-independent ordering — see `assignShardSpecs`'s note on why `localeCompare` is not used. */
export function compareSpecNames(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** Filename for one shard's manifest. Includes the total so a human staring at a merged download can see
 *  the plan the shard believed it was part of without opening the file. */
export function shardManifestFileName({ shardIndex, shardTotal }: ShardId): string {
  return `${SHARD_MANIFEST_PREFIX}${shardIndex}-of-${shardTotal}.json`;
}

/** Prefix stamped onto each shard's `@wdio/json-reporter` output filenames.
 *
 *  LOAD-BEARING. The verdict job downloads every shard's results artifact with `merge-multiple: true`,
 *  which flattens them all into one directory — and wdio's worker `cid`s restart at `0-0` in every shard,
 *  so without this prefix every shard would write a `wdio-0-0.json` and the merge would keep exactly one
 *  of them, silently discarding the rest. (That particular accident would still RED rather than pass —
 *  the discarded specs stop reporting and clause 4 fires against the live-globbed expectation — but a
 *  confusing red that names the wrong problem is not much better than a wrong green.) */
export function shardResultFilePrefix(id: ShardId | undefined): string {
  return id ? `shard-${id.shardIndex}-of-${id.shardTotal}-` : "";
}

/** True when `value` is a well-formed `ShardManifest`. Used by the verdict job's loader, which is reading
 *  JSON that arrived over an artifact download: a truncated or hand-edited manifest must be REJECTED
 *  loudly, never coerced into a plausible-looking shard that then counts as "reported". */
export function isShardManifest(value: unknown): value is ShardManifest {
  if (typeof value !== "object" || value === null) return false;
  const m = value as Record<string, unknown>;
  return (
    Number.isInteger(m.shardIndex) &&
    Number.isInteger(m.shardTotal) &&
    Array.isArray(m.specs) &&
    m.specs.every((s) => typeof s === "string")
  );
}
