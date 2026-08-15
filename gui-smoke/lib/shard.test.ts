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
import { describe, it } from "node:test";
import {
  assignShardSpecs,
  isShardManifest,
  parseExpectedShards,
  parseShardId,
  ShardConfigError,
  shardManifestFileName,
  shardResultFilePrefix,
  SHARD_INDEX_ENV,
  SHARD_TOTAL_ENV,
  EXPECT_SHARDS_ENV,
} from "./shard.js";

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

  it("splits the work rather than concentrating it", () => {
    const sizes = [1, 2, 3, 4].map((shardIndex) => assignShardSpecs(SPECS, { shardIndex, shardTotal: 4 }).length);
    // 7 specs over 4 shards: round-robin gives 2/2/2/1. The property that matters is that no shard gets
    // everything and none is starved by more than one spec.
    assert.equal(Math.max(...sizes) - Math.min(...sizes) <= 1, true, `unbalanced: ${sizes.join("/")}`);
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
