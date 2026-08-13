// CPE-1701 — guards that gui-smoke/lib/ stays flat (no *.test.ts file below its top level) and that
// package.json's `test:unit` script string is not silently reverted to a `**` glob.
//
// Both matter for the same reason. `npm run test:unit` runs `tsx --test lib/*.test.ts` — a
// SINGLE-star, non-recursive glob (CPE-1694, PR #878), deliberately narrowed from `lib/**/*.test.ts`
// because `**` depends on bash's `globstar` option, which is OFF by default on GitHub Actions' `bash -e`
// step shell and unsupported by npm's `sh -c` wrapper. Under that real shell, the un-narrowed `**`
// wasn't a safety net either: replayed with globstar off it matched ONLY the deepest nested file and
// silently DROPPED every flat suite (measured in CPE-1701 — 1 suite/1 test collected instead of the real
// 24 suites/71 tests). So the fix is the single-star glob, and that fix has a blind spot: a test file
// added under a subdirectory of lib/ is invisible to it. `npm run test:unit` then reports green having
// never run the nested file — CPE-1694's own bug ("a test that cannot fail is not evidence") one
// directory down. This file is the guard against both ways that blind spot can reopen: a nested test
// file appearing, or the glob itself drifting back to `**`.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const LIB_DIR = path.dirname(fileURLToPath(import.meta.url));
const PACKAGE_JSON_PATH = path.resolve(LIB_DIR, "../package.json");
const PINNED_TEST_UNIT_SCRIPT = "tsx --test lib/*.test.ts";

describe("gui-smoke/lib layout guard (CPE-1701)", () => {
  it("has no *.test.ts file nested below lib/'s top level", () => {
    const topLevelTestFiles = fs.readdirSync(LIB_DIR).filter((entry) => entry.endsWith(".test.ts"));
    const allTestFiles = (fs.readdirSync(LIB_DIR, { recursive: true }) as string[]).filter((entry) =>
      entry.endsWith(".test.ts"),
    );
    const nested = allTestFiles.filter((entry) => !topLevelTestFiles.includes(entry));

    assert.deepEqual(
      nested,
      [],
      "Found *.test.ts file(s) nested below gui-smoke/lib/'s top level: " +
        nested.join(", ") +
        ". " +
        "`npm run test:unit` runs `tsx --test lib/*.test.ts` — a single-star glob, deliberately NOT " +
        "`lib/**/*.test.ts` — because `**` depends on bash's globstar option, which is OFF by default on " +
        "GitHub Actions' `bash -e` step shell (and unsupported by npm's `sh -c` wrapper): replayed under " +
        "that real shell, `**` matched ONLY the deepest nested file and silently dropped every flat suite " +
        "(CPE-1694/CPE-1701). A nested file is therefore never collected, and the suite will report green " +
        "having never run it. FIX: move the file back to the top level of lib/ (flatten the path, e.g. " +
        "lib/foo/bar.test.ts -> lib/foo.bar.test.ts). Do not delete this guard and do not restore the " +
        "`**` glob — see CPE-1701 for why that glob is not a safety net either.",
    );
  });

  it("package.json's test:unit script stays pinned to the single-star, non-recursive glob", () => {
    // Pinned here (not just proven in CI) so a revert to `**` reds on a plain local `npm run test:unit`
    // instead of only on a real GitHub Actions run — see CPE-1701's decision to pin this string, made
    // because the whole point of the narrowing is that local shells (with globstar on, or on a
    // non-bash default shell) would happily run `**` and never reveal the CI-only failure mode.
    const packageJson = JSON.parse(fs.readFileSync(PACKAGE_JSON_PATH, "utf-8")) as {
      scripts?: Record<string, string>;
    };

    assert.equal(
      packageJson.scripts?.["test:unit"],
      PINNED_TEST_UNIT_SCRIPT,
      "gui-smoke/package.json's `test:unit` script changed from the pinned `" +
        PINNED_TEST_UNIT_SCRIPT +
        "`. If it now reads `tsx --test lib/**/*.test.ts`, DO NOT merge that: bash's non-globstar `**` is " +
        "not a safety net here — replayed under CI's real shell (globstar off) it matches ONLY the " +
        "deepest nested file and silently drops every flat suite (measured in CPE-1694/CPE-1701). If the " +
        "script needs to change for a real reason, update PINNED_TEST_UNIT_SCRIPT in this file in the " +
        "same PR, deliberately.",
    );
  });
});
