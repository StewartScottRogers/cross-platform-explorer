// CPE-1753 — `npm run shard-manifest`. Declares, in writing and BEFORE the suite runs, which spec files
// this shard job is responsible for.
//
// This is the file the join job counts. `gui-smoke-linux-verdict` expects exactly the shard indices
// `1..GUI_SMOKE_EXPECT_SHARDS` to be present among the downloaded results; an absent one is a MISSING
// SHARD red (`lib/ratchet.ts` clause 9) rather than a quietly reduced expectation — the whole point of
// this ticket. See `lib/shard.ts`'s header for why the expectation must come from the workflow file and
// never from the manifests that happened to arrive.
//
// Run BEFORE the suite step, deliberately, and as early in the job as the checkout allows:
//   - a shard killed mid-suite has still declared what it OWED, so the missing results surface as an
//     incomplete run naming real spec files rather than disappearing along with the shard;
//   - a shard that fails during setup (no app binary, apt failure) still has a manifest, so the verdict
//     distinguishes "the shard ran and did not finish" from "the shard never existed" — two different
//     investigations;
//   - and it fails fast on a mis-set matrix (index outside 1..total, only one of the two env vars set)
//     before ten minutes of build/test time have been spent — `parseShardId` refuses to guess.
//
// Prints the assignment too. That log line is the answer to "which shard runs the spec I care about?",
// which is otherwise only derivable by re-implementing the partition by hand.
import fs from "node:fs";
import path from "node:path";
import { listSpecFiles } from "../lib/specFiles.js";
import {
  assignShardSpecs,
  parseShardId,
  shardManifestFileName,
  type ShardManifest,
} from "../lib/shard.js";

const RESULTS_DIR = process.env.GUI_SMOKE_RESULTS_DIR ?? path.resolve(process.cwd(), ".results");
const SPECS_DIR = process.env.GUI_SMOKE_SPECS_DIR ?? path.resolve(process.cwd(), "specs");

function main(): void {
  const shardId = parseShardId(process.env);
  if (!shardId) {
    // Not an error: an unsharded run (a local `npm test`, the Windows leg) has no manifest to write and
    // no join job to satisfy. Exits 0 so the workflow step can stay unconditional if it ever needs to be.
    // eslint-disable-next-line no-console
    console.log(
      "[gui-smoke shard] GUI_SMOKE_SHARD_INDEX/GUI_SMOKE_SHARD_TOTAL are not set — unsharded run, no " +
        "manifest written.",
    );
    return;
  }

  const allSpecs = listSpecFiles(SPECS_DIR);
  const specs = assignShardSpecs(allSpecs, shardId);
  const manifest: ShardManifest = { ...shardId, specs };

  fs.mkdirSync(RESULTS_DIR, { recursive: true });
  const target = path.join(RESULTS_DIR, shardManifestFileName(shardId));
  fs.writeFileSync(target, `${JSON.stringify(manifest, null, 2)}\n`, "utf-8");

  // eslint-disable-next-line no-console
  console.log(
    `[gui-smoke shard] shard ${shardId.shardIndex} of ${shardId.shardTotal} owns ${specs.length} of ` +
      `${allSpecs.length} spec file(s); manifest -> ${target}`,
  );
  for (const spec of specs) {
    // eslint-disable-next-line no-console
    console.log(`[gui-smoke shard]   • ${spec}`);
  }

  if (specs.length === 0) {
    // Loud, and non-fatal here on purpose: the ratchet step is what owns exit codes, and it refuses an
    // empty shard with a full explanation. Saying it twice costs nothing and this is the earlier of the
    // two, so a reader scrolling the log from the top sees the cause before the effect.
    // eslint-disable-next-line no-console
    console.log(
      `[gui-smoke shard] WARNING: this shard was assigned NO spec files — the matrix has more shards ` +
        `(${shardId.shardTotal}) than the suite has spec files (${allSpecs.length}). Shrink the matrix in ` +
        `.github/workflows/gui-smoke.yml (and GUI_SMOKE_EXPECT_SHARDS with it).`,
    );
  }
}

main();
