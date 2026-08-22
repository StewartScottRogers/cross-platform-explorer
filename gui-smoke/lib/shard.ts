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
// `assignShardSpecs` below (CPE-1858: by MEASURED cost, not by count — see "THE COST MODEL" further
// down for why round-robin could not balance this suite), then joins on a single
// `gui-smoke-linux-verdict` job that merges every
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

// ===================================================================================================
// CPE-1858 — THE COST MODEL. Read this before touching `MEASURED_SPEC_RUNTIME_MS`.
//
// CPE-1753 partitioned by round-robin over the sorted names, explicitly "balance without a cost model".
// That is the right default when every unit costs about the same. It is not this suite. MEASURED, from
// the `@wdio/json-reporter` chunks of three consecutive green runs (32585350872, 32589428833,
// 32592641384 — download `gui-smoke-results-ubuntu-shard-<n>`, each `wdio-*.json`'s top-level
// `start`/`end` is one spec file's in-session wall time):
//
//   samples.smoke.ts        479.3 s   (479.5 / 479.7 / 478.8 — n=3, spread 0.9 s)
//   preview-pane.smoke.ts    18.2 s
//   network.smoke.ts         16.2 s
//   saved-search.smoke.ts    12.0 s
//   ...the other 37          1.3-4.0 s
//   ------------------------------------------------------------------------------
//   all 41 spec files       611.5 s, of which ONE spec file is 78%.
//
// Round-robin therefore cannot balance this suite at any shard count: whichever shard draws
// `samples.smoke.ts` runs ~8 minutes of test before it starts on its share of the other 2.2 minutes.
// That is the whole of the CPE-1858 observation — shard 2 at ~14 min against ~6-7 min for the other
// three, on three consecutive runs, with nothing causal in any diff. Shard 2 held `samples.smoke.ts`.
//
// THE SECOND MEASURED NUMBER, which is why the cost model is as coarse as it is. Each spec file costs a
// fixed ~29.5 s of session setup/teardown on top of its own runtime (per shard, `span - sum(spec
// durations)` over `n` specs: 29.9 / 29.0 / 30.6 / 29.0 s — run 32592641384). For 40 of the 41 specs
// that fixed cost DWARFS the spec's own work (29.5 s vs a 1.3-18.2 s spread), so counting them is
// already the correct cost model and a per-spec measured table would buy nothing while rotting 41 ways.
//
// THE INCLUSION RULE, stated so the table cannot grow by taste: a spec earns an entry only when its
// measured runtime EXCEEDS the per-spec session overhead — i.e. when the spec's own work, not the
// session it runs in, is the dominant term. Exactly one spec qualifies today (479.3 s vs 29.5 s); the
// runner-up, `preview-pane.smoke.ts` at 18.2 s, does not, and adding it would move a shard by less than
// half of one session's overhead.
//
// WHAT HAPPENS WHEN A RUNTIME CHANGES — i.e. how this rots, stated plainly rather than wished away.
// There is NO self-correcting static proxy available: `it()` count, line count and byte count were all
// measured against the durations above and all three FAIL. `samples.smoke.ts` is 3 top-level `it()`
// blocks and 186 lines — mid-pack on every static measure — because it generates one case per file in
// the repo's `samples/` tree at spec-load time. `preview-pane.smoke.ts` has the MOST `it()` blocks (8)
// and is 26x faster. So the table is measured, and it is hand-maintained. The failure modes:
//   - a table entry goes stale (samples gets faster/slower): balance degrades toward what round-robin
//     would have given. Correctness is untouched — the partition is still a bijection.
//   - a NEW spec becomes heavy and is not listed: it is costed as ordinary, lands on some shard, and
//     that shard grows. Again balance only.
//   - an entry names a spec that was renamed or deleted: caught LOUDLY by a test in `shard.test.ts`,
//     because that is the one rot a static check CAN see.
// The floor is set by the heaviest single spec no matter what: no partition and no shard count can put
// `samples.smoke.ts` in two places. If ~8.5 min ever stops being acceptable, the lever is SPLITTING
// that spec file (or trimming `samples/`), not editing this table.
//
// RE-MEASURING is a five-minute job and is how this table should be updated, never by argument:
//   gh run download <run-id> -n gui-smoke-results-ubuntu-shard-<n> -D shard<n>   # for n in 1..4
//   then, per `wdio-*.json`: basename(specs[0]) -> Date.parse(end) - Date.parse(start).
// ===================================================================================================

/** Fixed per-spec-file cost of the session the spec runs in (app launch, driver session, teardown),
 *  in milliseconds. MEASURED at 29.0-30.6 s across the four shards of run 32592641384. Every spec pays
 *  it, so it is the floor of any spec's weight — and for 40 of 41 specs it IS essentially the weight,
 *  which is why counting is already the right cost model for everything not in the table below. */
export const SPEC_SESSION_OVERHEAD_MS = 29_500;

/** Weight for a spec with no measured entry, in milliseconds: the mean in-session runtime of the 40
 *  non-`samples` specs (132.2 s / 40 = 3.3 s). Deliberately a single flat number — the real spread
 *  (1.3-18.2 s) is smaller than half of `SPEC_SESSION_OVERHEAD_MS`, so pretending to know it per spec
 *  would be false precision with a maintenance bill attached. */
export const DEFAULT_SPEC_RUNTIME_MS = 3_300;

/** Measured in-session runtimes, in milliseconds, for the specs whose OWN work dominates their session
 *  overhead. See the block comment above for the measurement, the inclusion rule and the rot analysis.
 *  Integers, not seconds-as-floats, so the bin-packing arithmetic below is exact rather than merely
 *  reproducible — a partition that differed by one ULP between two runners would be a real bug. */
export const MEASURED_SPEC_RUNTIME_MS: Readonly<Record<string, number>> = {
  // 479.5 / 479.7 / 478.8 s over runs 32585350872 / 32589428833 / 32592641384. Generates one case per
  // file under `samples/`, so it grows with the fixture tree rather than with its own source.
  "samples.smoke.ts": 479_300,
};

/** What one spec file costs a shard: its session overhead plus its measured (or defaulted) runtime.
 *  Total milliseconds, always a positive integer, and a pure function of the BASENAME alone — no clock,
 *  no filesystem, no iteration order. That purity is what lets four independent runner processes agree. */
export function specWeightMs(basename: string): number {
  const runtime = Object.prototype.hasOwnProperty.call(MEASURED_SPEC_RUNTIME_MS, basename)
    ? MEASURED_SPEC_RUNTIME_MS[basename]
    : DEFAULT_SPEC_RUNTIME_MS;
  return SPEC_SESSION_OVERHEAD_MS + runtime;
}

/**
 * The WHOLE partition — every shard's spec list, in shard order — computed the same way by every job.
 *
 * Longest-processing-time-first (LPT) greedy bin-packing: cost every spec with `specWeightMs`, sort
 * heaviest-first, and hand each one to the currently-lightest shard. Classic, and its 4/3-of-optimal
 * bound is far more than this suite needs — with one spec at 78% of the total, LPT simply gives that
 * spec a shard of its own and deals the rest evenly, which IS the optimum here.
 *
 * DETERMINISM, which matters more than balance and is the property most easily broken by a rebalance.
 * Four shard jobs run this in four separate processes on four separate runners; the verdict job joins
 * their manifests. If two processes disagreed about the partition, a spec would run twice (wasteful but
 * visible) or run NOWHERE while every job reported green (invisible, and the exact silent-coverage-hole
 * shape CPE-1728/CPE-1753 exist to eliminate). So every input is pinned:
 *   - the spec list is re-sorted here with `compareSpecNames`, a plain code-unit comparison, so neither
 *     `readdir` order nor the runner's locale can reach the result (`localeCompare` would);
 *   - weights are integer milliseconds from a committed constant, so the load comparison is exact
 *     integer arithmetic — no float, no ULP, no ordering surprise;
 *   - the heaviest-first sort breaks weight ties by NAME, never by input position;
 *   - the "lightest shard" search breaks load ties by LOWEST INDEX (strict `<`), never by anything
 *     ambient;
 *   - and there is no `Date`, no `Math.random`, no `process.*`, no `for...in` over an object anywhere
 *     in the path.
 * `lib/shard.test.ts` pins this by running the REAL `scripts/write-shard-manifest.ts` in four separate
 * child processes and asserting the union of the four manifests is exactly the spec set, once each —
 * deliberately not by calling this function twice in one process, which would pass even if the answer
 * depended on the clock.
 *
 * Every spec appears in EXACTLY one shard, and the union is the whole list — a property `lib/ratchet.ts`
 * clause 9 re-checks at the join against the live-globbed `specs/` directory rather than trusting it.
 *
 * ASSIGNMENT CHURN is unchanged in kind from CPE-1753's round-robin: adding a spec can move others
 * between shards. Harmless because nothing is cached per shard and the verdict is reassembled from all
 * of them every run; it would stop being harmless the moment anything memoises a spec-to-shard mapping.
 */
export function partitionSpecs(allSpecs: string[], shardTotal: number): string[][] {
  assertShardId({ shardIndex: 1, shardTotal });
  const buckets: string[][] = Array.from({ length: shardTotal }, () => []);
  const loadMs: number[] = new Array(shardTotal).fill(0);

  // Heaviest first, ties by name. Sorting by name FIRST and then by weight would rely on sort stability
  // for the tie-break; doing both in one comparator makes the total order explicit instead.
  const heaviestFirst = [...allSpecs].sort((a, b) => {
    const byWeight = specWeightMs(b) - specWeightMs(a);
    return byWeight !== 0 ? byWeight : compareSpecNames(a, b);
  });

  for (const spec of heaviestFirst) {
    // argmin over `loadMs`, strict `<` so an exact tie keeps the LOWEST index. With all-equal weights
    // this degenerates to plain round-robin, which is why an unweighted suite behaves exactly as it did
    // before CPE-1858.
    let target = 0;
    for (let i = 1; i < shardTotal; i += 1) {
      if (loadMs[i] < loadMs[target]) target = i;
    }
    buckets[target].push(spec);
    loadMs[target] += specWeightMs(spec);
  }

  // Name order within a shard: the manifest, the log line and `known-failing.json` are all read by
  // humans, and bin-packing order is meaningless to them.
  return buckets.map((bucket) => bucket.sort(compareSpecNames));
}

/**
 * Picks the spec files this shard owns, deterministically. Thin slice of `partitionSpecs` — every job
 * computes the WHOLE partition and keeps its own row, which is what makes the four rows provably
 * disjoint rather than four independent guesses that happen to agree.
 *
 * A shard with MORE shards than spec files legitimately gets an empty list; that is not an error here
 * (the run is still complete overall), and the verdict job's coverage check is what would catch a real
 * gap. `scripts/run-ratchet.ts` prints the empty assignment loudly so an over-sharded matrix is visible
 * rather than looking like a fast green.
 */
export function assignShardSpecs(allSpecs: string[], id: ShardId): string[] {
  assertShardId(id);
  return partitionSpecs(allSpecs, id.shardTotal)[id.shardIndex - 1];
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
