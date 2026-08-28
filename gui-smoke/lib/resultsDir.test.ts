// CPE-1910 — first unit coverage for the `.results/` reader. It ran in production for four tickets
// (CPE-1594/1677/1728/1753) inside `scripts/run-ratchet.ts`, where `test:unit`'s `lib/*.test.ts` glob
// could never reach it — the same blind spot CPE-1680 found in `toCaseStatus`.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, describe, it } from "node:test";

import { SHARD_MANIFEST_PREFIX } from "./shard.js";
import { readResultChunks } from "./resultsDir.js";

const made: string[] = [];
function tmpdir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "cpe-1910-results-"));
  made.push(dir);
  return dir;
}
after(() => {
  for (const dir of made) fs.rmSync(dir, { recursive: true, force: true });
});

describe("readResultChunks", () => {
  it("distinguishes a MISSING directory from an EMPTY one", () => {
    // The whole point of the `chunks: undefined` shape. Collapsing these two into "0 results" is the
    // "the tool said nothing, so nothing is wrong" family: a suite that never started and a suite that
    // ran and reported nothing are different facts, and only the caller knows which one matters.
    const absent = readResultChunks(path.join(tmpdir(), "nope"));
    assert.equal(absent.chunks, undefined);

    const empty = readResultChunks(tmpdir());
    assert.deepEqual(empty.chunks, []);
    assert.deepEqual(empty.files, []);
  });

  it("reads reporter chunks and excludes shard manifests by name", () => {
    const dir = tmpdir();
    fs.writeFileSync(path.join(dir, "wdio-shard-2-of-4-a.smoke.json"), JSON.stringify({ suites: [] }));
    fs.writeFileSync(path.join(dir, `${SHARD_MANIFEST_PREFIX}2-of-4.json`), JSON.stringify({ specs: [] }));
    fs.writeFileSync(path.join(dir, "notes.txt"), "ignored");

    const read = readResultChunks(dir);
    assert.deepEqual(read.files, ["wdio-shard-2-of-4-a.smoke.json"]);
    assert.equal(read.chunks?.length, 1);
  });

  it("THROWS on a file it cannot parse, naming it", () => {
    // A truncated or half-written artifact must never be counted as "that spec reported nothing" — that
    // reads as a small clean run to every clause downstream, and to CPE-1910's retry decision it would
    // read as an incomplete run worth re-running.
    const dir = tmpdir();
    fs.writeFileSync(path.join(dir, "wdio-broken.json"), "{ not json");
    assert.throws(() => readResultChunks(dir), /failed to parse wdio-broken\.json as JSON/);
  });
});
