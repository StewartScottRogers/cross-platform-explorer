import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { stripRustComments, rustStringLiteralAfter } from "./rustSource";

/**
 * CPE-1950 — the gui-smoke fixture literals that are restated in a second file under a
 * "keep in sync with…" comment, derived instead.
 *
 * Two claims, both untested by construction:
 *
 *  - `gui-smoke/specs/vault-create.smoke.ts` and `trash-titlebar.smoke.ts` re-declare fixture names
 *    that `wdio.conf.ts` already **exports**, under the note "Duplicated literals rather than
 *    importing across the runner/worker boundary … keep in sync with wdio.conf.ts". The duplication
 *    itself is a deliberate, documented convention (wdio's config module is loaded by the runner, the
 *    specs by the worker) and CPE-1950 leaves it alone — but "keep in sync" was doing the work of a
 *    test, and a drifted name makes a smoke spec look for a file the seeder never wrote. It fails as
 *    a **timeout**, i.e. as flake, which is the worst possible way for it to fail.
 *  - `crates/server/examples/gen_vault_fixture.rs` says its `PASSPHRASE` and the sealed inner file
 *    name "must match" `wdio.conf.ts`'s. A Rust example cannot import a TypeScript module, so this
 *    guard reads both files and compares them here instead.
 *
 * This runs in the ROOT vitest project (not gui-smoke's own npm project) purely because that is where
 * a suite runs on every PR; it changes nothing about how gui-smoke executes.
 *
 * Both sides are read as **code, not prose**: the TS extractors anchor at column 0 on a real
 * `export const NAME =` / `const NAME =` declaration, so a commented-out or quoted copy (`// const
 * SHRED_DIR_NAME = "old"`) cannot be matched, and each is asserted to appear exactly once. The Rust
 * side is comment-stripped first via `rustSource.ts`. Anchoring on a bare `NAME = "…"` anywhere in the
 * file is exactly the hole CPE-1933 warns about.
 *
 * **Red-proofed, not assumed.** Changing `wdio.conf.ts`'s `SHRED_DIR_NAME` to
 * `"CPE-1241-shred-folder-X"` reds the vault-create leg; changing `gen_vault_fixture.rs`'s
 * `PASSPHRASE` to `"open-sesame-1250"` reds the vault-fixture leg. Both reverted.
 */

const ROOT = process.cwd();
const WDIO_CONF = readFileSync(join(ROOT, "gui-smoke", "wdio.conf.ts"), "utf8");

/** The value of a `const NAME = "…"` declared at column 0 in `src`, asserted to occur exactly once. */
function tsStringConst(src: string, name: string, exported: boolean, where: string): string {
  const decl = exported ? "export const" : "const";
  // Column-0 anchored: a `//`-commented or indented copy cannot match. `m` for line starts only.
  const re = new RegExp(`^${decl} ${name} = "([^"]*)";`, "gm");
  const hits = [...src.matchAll(re)];
  expect(hits.length, `${where} must declare \`${decl} ${name} = "…"\` exactly once`).toBe(1);
  return hits[0][1];
}

describe("gui-smoke spec fixture names are DERIVED from wdio.conf.ts, not kept in sync by comment (CPE-1950)", () => {
  const CASES: { spec: string; names: string[] }[] = [
    {
      spec: "vault-create.smoke.ts",
      names: [
        "SHRED_DIR_NAME",
        "VAULT_CREATE_PARENT_DIR",
        "VAULT_CREATE_SRC_DIR",
        "VAULT_CREATE_BLOB_NAME",
      ],
    },
    { spec: "trash-titlebar.smoke.ts", names: ["TRASH_TITLEBAR_FILE_NAME"] },
  ];

  for (const { spec, names } of CASES) {
    it(`${spec} restates wdio.conf.ts's exported fixture names verbatim`, () => {
      const src = readFileSync(join(ROOT, "gui-smoke", "specs", spec), "utf8");
      for (const name of names) {
        expect(
          tsStringConst(src, name, false, `gui-smoke/specs/${spec}`),
          `gui-smoke/specs/${spec}'s ${name} has drifted from gui-smoke/wdio.conf.ts's exported ` +
            `${name}. The seeder writes one name and the spec looks for another, so the spec fails ` +
            `as a TIMEOUT rather than as a clear assertion. Update whichever one is wrong.`,
        ).toBe(tsStringConst(WDIO_CONF, name, true, "gui-smoke/wdio.conf.ts"));
      }
    });
  }
});

describe("gen_vault_fixture.rs's 'must match wdio.conf.ts' constants are DERIVED (CPE-1950)", () => {
  const EXAMPLE = stripRustComments(
    readFileSync(join(ROOT, "crates", "server", "examples", "gen_vault_fixture.rs"), "utf8"),
  );

  /** The value of a `const NAME: &str = "…";` in the (comment-stripped) example. */
  function rustStrConst(name: string): string {
    const at = EXAMPLE.indexOf(`const ${name}: &str =`);
    expect(at, `gen_vault_fixture.rs no longer declares \`const ${name}: &str\``).toBeGreaterThan(-1);
    return rustStringLiteralAfter(EXAMPLE, at);
  }

  it("the fixture passphrase is the same string on both sides of the Rust/TS boundary", () => {
    expect(
      rustStrConst("PASSPHRASE"),
      "crates/server/examples/gen_vault_fixture.rs seals the fixture with a passphrase that " +
        "gui-smoke/wdio.conf.ts's VAULT_FIXTURE_PASSPHRASE no longer matches — regenerating the " +
        "blob would produce one vault.smoke.ts cannot unlock.",
    ).toBe(tsStringConst(WDIO_CONF, "VAULT_FIXTURE_PASSPHRASE", true, "gui-smoke/wdio.conf.ts"));
  });

  it("the sealed inner file name the example writes is the one the spec looks for", () => {
    // Read out of the example's own `TreeEntry { path: "…" }` list rather than a const, because that
    // list IS the sealed tree. Anchored on the first `path:` inside main(), comments stripped.
    const at = EXAMPLE.indexOf("path:");
    expect(at, "gen_vault_fixture.rs no longer builds a TreeEntry list").toBeGreaterThan(-1);
    expect(rustStringLiteralAfter(EXAMPLE, at)).toBe(
      tsStringConst(WDIO_CONF, "VAULT_FIXTURE_INNER_NAME", true, "gui-smoke/wdio.conf.ts"),
    );
  });

  it("a commented-out copy cannot satisfy either side (the anchors are on code)", () => {
    // The adversarial shape CPE-1933 documents, executed rather than described.
    const hostileTs = ['// export const X = "stale";', 'export const X = "current";'].join("\n");
    expect(tsStringConst(hostileTs, "X", true, "fixture")).toBe("current");
    const hostileRust = ['// const P: &str = "stale";', 'const P: &str = "current";'].join("\n");
    expect(rustStringLiteralAfter(stripRustComments(hostileRust), 0)).toBe("current");
  });
});
