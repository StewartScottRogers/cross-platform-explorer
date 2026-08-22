// CPE-1753 — headless unit tests for the spec-sharding partition and its env parsing. Runs under Node's
// built-in test runner via `tsx` (same convention as `ratchet.test.ts`), so the whole sharding contract
// is verifiable WITHOUT a `tauri build`, a `tauri-driver` session, or a CI round trip:
//   npm run test:unit          (from gui-smoke/)
//
// The tests are written against the failure modes, not the happy path. The happy path of a sharded suite
// is trivially green — every dangerous property here is about what happens when the configuration is
// WRONG, because a partition that silently drops a spec file, or a shard that silently runs the whole
// suite, produces a fast green run that has verified less than it claims. See `shard.ts`'s header.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { listSpecFiles } from "./specFiles.js";
import {
  assignShardSpecs,
  isShardManifest,
  parseExpectedShards,
  parseShardId,
  partitionSpecs,
  ShardConfigError,
  shardManifestFileName,
  shardResultFilePrefix,
  specWeightMs,
  MEASURED_SPEC_RUNTIME_MS,
  SHARD_INDEX_ENV,
  SHARD_TOTAL_ENV,
  SPEC_SESSION_OVERHEAD_MS,
  EXPECT_SHARDS_ENV,
} from "./shard.js";

const GUI_SMOKE_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const REAL_SPECS_DIR = path.join(GUI_SMOKE_DIR, "specs");

/**
 * The balance property, stated once so the fixture suite and the REAL spec set assert the same thing.
 *
 * The slowest shard may exceed the perfect mean, but only up to the two things that genuinely bound it:
 *   - the HEAVIEST SINGLE SPEC, because a spec file cannot be split across shards — no partition and no
 *     shard count can beat that floor (today it is `samples.smoke.ts` at ~8.5 min); and
 *   - ONE spec-slot of session overhead, because the partition's granularity is a whole spec file: with
 *     41 files over 4 shards somebody has to carry the eleventh.
 * Anything beyond that is a real imbalance. This is deliberately tight enough to have FAILED on the
 * CPE-1753 round-robin split it replaces — that split put ~13.4 min on one shard against a ~7.6 min
 * mean, roughly 5 min past this bound — and loose enough never to red merely because a spec was added.
 */
function assertBalanced(allSpecs: string[], shardTotal: number, loads: number[]): void {
  const total = allSpecs.reduce((sum, s) => sum + specWeightMs(s), 0);
  const mean = total / shardTotal;
  const heaviest = Math.max(...allSpecs.map(specWeightMs));
  const bound = Math.max(mean, heaviest) + SPEC_SESSION_OVERHEAD_MS;
  const slowest = Math.max(...loads);
  assert.equal(
    slowest <= bound,
    true,
    `the slowest shard carries ${(slowest / 60_000).toFixed(2)} min but the floor for this spec set is ` +
      `${(Math.max(mean, heaviest) / 60_000).toFixed(2)} min (mean ${(mean / 60_000).toFixed(2)}, heaviest ` +
      `single spec ${(heaviest / 60_000).toFixed(2)}) — over budget by more than one spec-slot of session ` +
      `overhead. Loads: ${loads.map((l) => (l / 60_000).toFixed(2)).join("/")} min. If a spec's runtime has ` +
      `moved, re-measure and update MEASURED_SPEC_RUNTIME_MS — see lib/shard.ts's "THE COST MODEL" block ` +
      `for the gh-run-download recipe. Do NOT widen this bound to make it pass.`,
  );
}

/** Deliberately NOT pre-sorted, and deliberately not in the order `readdirSync` would return them on any
 *  particular OS — the partition must not depend on the caller's ordering. */
const SPECS = [
  "samples.smoke.ts",
  "open-dir.smoke.ts",
  "vault.smoke.ts",
  "archive-browse.smoke.ts",
  "network.smoke.ts",
  "radar.smoke.ts",
  "spotlight.smoke.ts",
];

describe("assignShardSpecs — the partition", () => {
  it("covers every spec exactly once across the whole matrix", () => {
    for (const shardTotal of [1, 2, 3, 4, 7]) {
      const union: string[] = [];
      for (let shardIndex = 1; shardIndex <= shardTotal; shardIndex += 1) {
        union.push(...assignShardSpecs(SPECS, { shardIndex, shardTotal }));
      }
      assert.equal(union.length, SPECS.length, `shardTotal=${shardTotal} lost or duplicated a spec`);
      assert.deepEqual([...union].sort(), [...SPECS].sort(), `shardTotal=${shardTotal} partition is not a bijection`);
    }
  });

  it("is independent of the order the caller lists the specs in", () => {
    const forwards = assignShardSpecs(SPECS, { shardIndex: 2, shardTotal: 3 });
    const backwards = assignShardSpecs([...SPECS].reverse(), { shardIndex: 2, shardTotal: 3 });
    // A partition that depended on readdir order would put a spec in a different shard on a different
    // runner — which, at the join, reads as one spec claimed twice and another claimed by nobody.
    assert.deepEqual(forwards, backwards);
  });

  it("shardTotal 1 runs everything (the unsharded shape, still expressible)", () => {
    assert.deepEqual(assignShardSpecs(SPECS, { shardIndex: 1, shardTotal: 1 }), [...SPECS].sort());
  });

  it("splits the work rather than concentrating it — by COST, which is not the same as by count", () => {
    // CPE-1858 replaced the old "no shard holds more than one spec more than another" assertion, which
    // was a guard on the wrong quantity and was GREEN for the whole three-run stretch in which shard 2
    // ran at twice the wall-clock of its siblings. Sizes were 11/10/10/10 the entire time; the shard
    // holding `samples.smoke.ts` was carrying 78% of the suite's measured test time. Counting units of
    // work only measures balance when the units cost the same, and here one costs 145x the median.
    const loads = partitionSpecs(SPECS, 4).map((bucket) => bucket.reduce((sum, s) => sum + specWeightMs(s), 0));
    assert.equal(Math.min(...loads) > 0, true, `a shard was starved with 7 specs to deal: ${loads.join("/")}`);
    assertBalanced(SPECS, 4, loads);
  });

  it("gives a shard beyond the spec count an EMPTY list rather than someone else's specs", () => {
    // Over-sharding is a config error, but it must fail as "this shard has nothing", which
    // `run-ratchet.ts` refuses loudly — never as a shard quietly re-running another shard's slice.
    assert.deepEqual(assignShardSpecs(["a.smoke.ts"], { shardIndex: 2, shardTotal: 3 }), []);
  });

  it("refuses an out-of-range or nonsensical shard id", () => {
    assert.throws(() => assignShardSpecs(SPECS, { shardIndex: 0, shardTotal: 4 }), ShardConfigError);
    assert.throws(() => assignShardSpecs(SPECS, { shardIndex: 5, shardTotal: 4 }), ShardConfigError);
    assert.throws(() => assignShardSpecs(SPECS, { shardIndex: 1, shardTotal: 0 }), ShardConfigError);
  });
});

// ---------------------------------------------------------------------------------------------------
// CPE-1858 — the cost model, and the two ways it can go wrong. A weight table is hand-maintained data,
// so it rots; `lib/shard.ts`'s header lays out the rot analysis in full. These pin the halves of it a
// static check can actually see.
// ---------------------------------------------------------------------------------------------------
describe("the measured cost model (CPE-1858)", () => {
  it("balances the REAL spec set — the assertion that would have caught the shard-2 outlier", () => {
    // Against the live `specs/` directory, not a fixture, because the imbalance this ticket exists for
    // was a property of the real spec set: three consecutive green runs at ~7/~14/~6/~7 min. A fixture
    // suite would have stayed green through all three.
    const allSpecs = listSpecFiles(REAL_SPECS_DIR);
    const loads = partitionSpecs(allSpecs, 4).map((b) => b.reduce((sum, s) => sum + specWeightMs(s), 0));
    assertBalanced(allSpecs, 4, loads);
  });

  it("has no weight entry naming a spec file that no longer exists", () => {
    // The one rot a static check CAN see, and the one most likely to happen: a spec is renamed or
    // deleted and its measured weight silently stops applying to anything. Nothing else would notice —
    // the partition stays a perfect bijection, every job stays green, and the balance quietly reverts to
    // round-robin's, which is exactly the state this ticket was filed about.
    const present = new Set(listSpecFiles(REAL_SPECS_DIR));
    const orphans = Object.keys(MEASURED_SPEC_RUNTIME_MS)
      .filter((name) => !present.has(name))
      .sort();
    assert.deepEqual(
      orphans,
      [],
      `MEASURED_SPEC_RUNTIME_MS names spec file(s) that are not in specs/: ${orphans.join(", ")}. ` +
        `Either the spec was renamed (move the entry) or it was deleted (drop the entry). Leaving a dead ` +
        `entry costs nothing visible and quietly un-balances the shards — see lib/shard.ts's cost model.`,
    );
  });

  it("costs every spec by basename alone — no clock, no filesystem, no input position", () => {
    // `specWeightMs` is the only place a weight is decided, so if it is pure the whole partition is.
    const first = specWeightMs("samples.smoke.ts");
    assert.equal(specWeightMs("samples.smoke.ts"), first);
    // A name with no entry falls back to the default rather than to `undefined` — a NaN weight would
    // make every comparison false and quietly deal every spec to shard 1.
    const unknown = specWeightMs("a-spec-that-does-not-exist.smoke.ts");
    assert.equal(Number.isInteger(unknown), true);
    assert.equal(unknown > 0, true);
    // `constructor`/`toString` are inherited Object properties: a naive `TABLE[name] ?? DEFAULT` lookup
    // returns a FUNCTION for these, and `overhead + function` is NaN. Not hypothetical — it is why the
    // lookup uses `hasOwnProperty`.
    for (const inherited of ["constructor", "toString", "hasOwnProperty", "__proto__"]) {
      assert.equal(Number.isInteger(specWeightMs(inherited)), true, `weight for ${inherited} is not an integer`);
    }
  });
});

describe("assignment determinism across SEPARATE PROCESSES (CPE-1858)", () => {
  // WHY THIS IS A CHILD-PROCESS TEST AND NOT A LOOP. The property that matters is not "the function is
  // repeatable" — it is "four jobs, on four runners, in four OS processes, with only their own env, all
  // reach the SAME partition". Computing the partition twice inside one process and comparing would pass
  // even if the answer depended on the wall clock, on the process id, or on a module-level cache warmed
  // by the first call — and a partition that differs between two shard jobs puts a spec in two shards
  // (wasteful, visible) or in NONE (invisible: the verdict job's coverage check is the only net, and
  // every shard still reports green about the work it did do).
  //
  // So this runs the REAL `scripts/write-shard-manifest.ts` — the same file CI's "Declare this shard's
  // spec assignment" step runs — four times, as four separate `node` processes, each told only its own
  // shard index, and joins their manifests exactly as `gui-smoke-linux-verdict` does.
  const TSX_CLI = path.join(GUI_SMOKE_DIR, "node_modules", "tsx", "dist", "cli.mjs");
  const MANIFEST_SCRIPT = path.join(GUI_SMOKE_DIR, "scripts", "write-shard-manifest.ts");
  const SHARD_TOTAL = 4;

  it("four independent processes produce four manifests whose union is every spec, exactly once", () => {
    // Assert the runner exists rather than skipping when it does not: a silently skipped determinism
    // test is worth less than no test, because it reports green.
    assert.equal(fs.existsSync(TSX_CLI), true, `tsx runner not found at ${TSX_CLI} — cannot run the real script`);
    assert.equal(fs.existsSync(MANIFEST_SCRIPT), true, `write-shard-manifest.ts not found at ${MANIFEST_SCRIPT}`);

    const resultsDir = fs.mkdtempSync(path.join(os.tmpdir(), "cpe-1858-shard-"));
    try {
      const manifests: { shardIndex: number; specs: string[] }[] = [];
      for (let shardIndex = 1; shardIndex <= SHARD_TOTAL; shardIndex += 1) {
        execFileSync(process.execPath, [TSX_CLI, MANIFEST_SCRIPT], {
          cwd: GUI_SMOKE_DIR,
          env: {
            ...process.env,
            [SHARD_INDEX_ENV]: String(shardIndex),
            [SHARD_TOTAL_ENV]: String(SHARD_TOTAL),
            GUI_SMOKE_RESULTS_DIR: resultsDir,
            GUI_SMOKE_SPECS_DIR: REAL_SPECS_DIR,
          },
          stdio: "pipe",
        });
        const file = path.join(resultsDir, shardManifestFileName({ shardIndex, shardTotal: SHARD_TOTAL }));
        const parsed: unknown = JSON.parse(fs.readFileSync(file, "utf-8"));
        assert.equal(isShardManifest(parsed), true, `shard ${shardIndex} wrote a malformed manifest`);
        manifests.push(parsed as { shardIndex: number; specs: string[] });
      }

      // The join, done the way the verdict job does it: pool everything that was claimed and compare
      // against the live-globbed spec directory — never against what the manifests happened to contain.
      const claimed = manifests.flatMap((m) => m.specs);
      const expected = listSpecFiles(REAL_SPECS_DIR);

      const duplicates = claimed.filter((s, i) => claimed.indexOf(s) !== i).sort();
      assert.deepEqual(
        duplicates,
        [],
        `spec file(s) claimed by more than one shard: ${duplicates.join(", ")}. The four processes did not ` +
          `agree on the partition, so those specs run twice — and by conservation, others run nowhere.`,
      );

      const unclaimed = expected.filter((s) => !claimed.includes(s)).sort();
      assert.deepEqual(
        unclaimed,
        [],
        `spec file(s) claimed by NO shard: ${unclaimed.join(", ")}. These would run nowhere while every ` +
          `shard job reported green about the specs it did run — the silent coverage hole CPE-1753 exists ` +
          `to prevent. The partition must be a pure function of the spec names and nothing else.`,
      );

      assert.deepEqual([...claimed].sort(), [...expected].sort());
    } finally {
      fs.rmSync(resultsDir, { recursive: true, force: true });
    }
  });
});

describe("parseShardId — refusing to guess", () => {
  it("returns undefined when neither var is set (an unsharded run)", () => {
    assert.equal(parseShardId({}), undefined);
    assert.equal(parseShardId({ [SHARD_INDEX_ENV]: "", [SHARD_TOTAL_ENV]: "  " }), undefined);
  });

  it("THROWS when only one var is set", () => {
    // The two silent alternatives are both disasters: run the whole suite in every shard (the sharding
    // undone, 4x the cost), or treat the total as 1 so shard 3 runs shard 1-of-1's specs and three
    // quarters of the suite never executes while everything reports green.
    assert.throws(() => parseShardId({ [SHARD_INDEX_ENV]: "2" }), ShardConfigError);
    assert.throws(() => parseShardId({ [SHARD_TOTAL_ENV]: "4" }), ShardConfigError);
  });

  it("THROWS on a non-integer rather than coercing it", () => {
    // `parseInt("4x")` is 4 — a typo'd matrix value must not become a valid shard number.
    assert.throws(() => parseShardId({ [SHARD_INDEX_ENV]: "1", [SHARD_TOTAL_ENV]: "4x" }), ShardConfigError);
    assert.throws(() => parseShardId({ [SHARD_INDEX_ENV]: "1.5", [SHARD_TOTAL_ENV]: "4" }), ShardConfigError);
  });

  it("THROWS on an index outside 1..total", () => {
    assert.throws(() => parseShardId({ [SHARD_INDEX_ENV]: "0", [SHARD_TOTAL_ENV]: "4" }), ShardConfigError);
    assert.throws(() => parseShardId({ [SHARD_INDEX_ENV]: "5", [SHARD_TOTAL_ENV]: "4" }), ShardConfigError);
  });

  it("accepts a well-formed pair", () => {
    assert.deepEqual(parseShardId({ [SHARD_INDEX_ENV]: "3", [SHARD_TOTAL_ENV]: "4" }), {
      shardIndex: 3,
      shardTotal: 4,
    });
  });
});

describe("parseExpectedShards — the join's expectation", () => {
  it("returns undefined when unset", () => {
    assert.equal(parseExpectedShards({}), undefined);
  });

  it("THROWS when combined with a shard index", () => {
    // A process is one shard or the join, never both — and the difference is whether it verifies a
    // quarter of the suite or all of it.
    assert.throws(
      () => parseExpectedShards({ [EXPECT_SHARDS_ENV]: "4", [SHARD_INDEX_ENV]: "1", [SHARD_TOTAL_ENV]: "4" }),
      ShardConfigError,
    );
  });

  it("THROWS on zero or a non-integer", () => {
    assert.throws(() => parseExpectedShards({ [EXPECT_SHARDS_ENV]: "0" }), ShardConfigError);
    assert.throws(() => parseExpectedShards({ [EXPECT_SHARDS_ENV]: "many" }), ShardConfigError);
  });

  it("accepts a positive integer", () => {
    assert.equal(parseExpectedShards({ [EXPECT_SHARDS_ENV]: "4" }), 4);
  });
});

describe("naming", () => {
  it("gives every shard a distinct result-file prefix", () => {
    // Load-bearing: wdio worker cids restart at `0-0` in every shard, so the verdict job's
    // `merge-multiple: true` download would keep exactly one `wdio-0-0.json` without this.
    const prefixes = [1, 2, 3, 4].map((shardIndex) => shardResultFilePrefix({ shardIndex, shardTotal: 4 }));
    assert.equal(new Set(prefixes).size, 4);
    assert.equal(shardResultFilePrefix(undefined), "", "an unsharded run must keep its original filenames");
  });

  it("gives every shard a distinct manifest filename", () => {
    const names = [1, 2, 3, 4].map((shardIndex) => shardManifestFileName({ shardIndex, shardTotal: 4 }));
    assert.equal(new Set(names).size, 4);
  });
});

describe("isShardManifest — rejecting a manifest that arrived damaged", () => {
  it("accepts a well-formed manifest", () => {
    assert.equal(isShardManifest({ shardIndex: 1, shardTotal: 4, specs: ["a.smoke.ts"] }), true);
    assert.equal(isShardManifest({ shardIndex: 1, shardTotal: 4, specs: [] }), true);
  });

  it("rejects anything it cannot fully trust", () => {
    // A truncated or hand-edited manifest must NOT be coerced into a plausible shard — a manifest that
    // counts is a shard that reported, and this is the only thing standing between "the download was
    // damaged" and "that shard ran".
    for (const bad of [
      null,
      undefined,
      "shard-1",
      {},
      { shardIndex: 1, shardTotal: 4 },
      { shardIndex: "1", shardTotal: 4, specs: [] },
      { shardIndex: 1.5, shardTotal: 4, specs: [] },
      { shardIndex: 1, shardTotal: 4, specs: "a.smoke.ts" },
      { shardIndex: 1, shardTotal: 4, specs: [1, 2] },
    ]) {
      assert.equal(isShardManifest(bad), false, `wrongly accepted ${JSON.stringify(bad)}`);
    }
  });
});
