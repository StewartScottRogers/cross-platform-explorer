// CPE-1964 — the `std::env::temp_dir()` enumeration, derived at run time rather than remembered,
// plus the one property this ticket's fix has to keep true.
//
// ## Why this file exists
//
// CPE-1952 (PR #1075) fixed one temp-directory site and listed the residuals from a hand enumeration.
// Its stated recipe — `git ls-files '*.rs'`, minus `tests/`, minus everything after each file's first
// `#[cfg(test)]` — is right in spirit and wrong as written: "the first `#[cfg(test)]`" also matches
// the **indented** attribute on an in-function test module and the token as it appears inside a doc
// comment, so run literally it amputates production code and finds **10** sites where the corrected
// recipe finds **15**. The five it dropped included both swarm sites — which is to say, the recipe as
// written hid the very defect CPE-1964 is about. That is CPE-1932's point precisely: an enumeration
// nobody else can re-run is halfway back to recall, and one that silently under-counts is worse than
// recall, because it reads as a measurement.
//
// So the corrected recipe lives here as code:
//   1. `git ls-files '*.rs'` — the tree decides what exists, not a list someone typed;
//   2. drop integration-test files (a `tests/` path segment);
//   3. strip comments FIRST (`stripRustComments`, shared — CLAUDE.md's "anchor on code, never on
//      prose"), so a doc comment quoting `#[cfg(test)]` or `temp_dir()` cannot move or fake a site;
//   4. cut each file at its first **column-0** `#[cfg(test)]` — the unit-test module — keeping the
//      indented in-function form out of it.
//
// ## What is asserted, and what deliberately is not
//
// There is **no stored count of sites** here. A number-that-may-not-rise would be a ratchet
// (docs/design/RATCHETS.md) and would need registering; more to the point, "how many places call
// `temp_dir()`" is not itself a defect measure — plenty of those sites are fine. What is asserted is
// (a) that the enumerator still works at all, so this never becomes a green scan of nothing, and
// (b) the single property CPE-1964's fix rests on: the mission-directory name is spelled in exactly
// one production file, so the guessable `format!("cpe-swarm-{}", now_millis())` shape cannot come
// back somewhere else while `swarm_mission_dir.rs` stays hardened.
import { describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { stripRustComments } from "./rustSource";

const ROOT = resolve(__dirname, "..", "..");

/**
 * The one production file allowed to spell the mission-directory prefix. Everything else reaches it
 * through `swarm_mission_dir::MISSION_PREFIX` / `is_mission_name`, so there is one place to harden
 * and one place to audit.
 */
const MISSION_PREFIX_HOME = "sidecar/ai-console/src/swarm_mission_dir.rs";

/** The prefix itself, spelled here so the guard fails loudly if the Rust constant is renamed. */
const MISSION_PREFIX = "cpe-swarm-";

/**
 * A tracked Rust file's production half: comments blanked, then everything from the first column-0
 * `#[cfg(test)]` onward dropped.
 *
 * ## The fallback, and why it is safe *here* specifically
 *
 * `stripRustComments` ends with a tripwire — "no line may still begin with `//`" — that catches a
 * desync of any cause (CPE-1950). Three files in this repo trip it without being desynced at all,
 * because a `//` line legitimately *survives* when it lives inside a string literal:
 * `crates/server/src/net_share.rs` (a `/proc/mounts` fixture whose CIFS row starts `//fileserver/…`,
 * in a backslash-continued string), `sidecar/host/src/scaffold.rs` (an `r#"…"#` template of a
 * generated `main.rs`, doc comments and all) and `sidecar/agent-board/src/ui.rs` (an `r#"…"#` page
 * with inline JS). The strip is correct in all three; the invariant is stricter than the property.
 *
 * Fixing that belongs to `rustSource.ts`, not to a caller (its doc says so in as many words), so this
 * enumerator falls back to the **raw** source for such a file. That is conservative in the only
 * direction that matters here: raw text is a superset of stripped text, so every assertion below can
 * only gain matches, never lose them — a comment could make one of these three files red, never
 * green. It would be the wrong fallback for a guard that reads a *value* out of Rust.
 */
function productionCode(rel: string): string {
  const raw = readFileSync(join(ROOT, rel), "utf8");
  let stripped: string;
  try {
    stripped = stripRustComments(raw);
  } catch {
    stripped = raw;
  }
  const cut = stripped.search(/^#\[cfg\(test\)\]/m);
  return cut === -1 ? stripped : stripped.slice(0, cut);
}

/** Every tracked `.rs` file that is not an integration test. */
function productionRustFiles(): string[] {
  return execFileSync("git", ["ls-files", "*.rs"], { cwd: ROOT, encoding: "utf8", maxBuffer: 64 << 20 })
    .split("\n")
    .map((l) => l.trim().split("\\").join("/"))
    .filter(Boolean)
    .filter((p) => !p.split("/").includes("tests"));
}

/** `file:line` for every `std::env::temp_dir()` in production code, derived by the corrected recipe. */
function tempDirSites(): string[] {
  const out: string[] = [];
  for (const rel of productionRustFiles()) {
    const code = productionCode(rel);
    if (!code.includes("std::env::temp_dir()")) continue;
    code.split("\n").forEach((line, i) => {
      if (line.includes("std::env::temp_dir()")) out.push(`${rel}:${i + 1}`);
    });
  }
  return out.sort();
}

describe("the temp_dir() enumeration (CPE-1964)", () => {
  it("still enumerates a real tree, so this can never be a green scan of nothing", () => {
    const files = productionRustFiles();
    // The repo has hundreds of tracked `.rs` files. A handful means `git ls-files` failed, the cwd is
    // wrong, or the filter ate everything — all of which would make every assertion below vacuous.
    // CPE-1932: fail loudly when the list comes back near-empty.
    expect(files.length).toBeGreaterThan(100);
    expect(tempDirSites().length).toBeGreaterThan(0);
  });

  it("keeps the mission-directory prefix spelled in exactly one production file", () => {
    // The escape and the leak both rode on the mission path being built inline in a request handler.
    // The fix moved every part of that — the name, the exclusive create, the sweep's filter and the
    // `/api/swarm/activity` id check — behind `swarm_mission_dir`. If the literal reappears anywhere
    // else in production code, some caller is minting or matching mission paths on its own again and
    // is not covered by `tests/swarm_mission_dir_containment.rs`.
    const spellers = productionRustFiles().filter((rel) => productionCode(rel).includes(MISSION_PREFIX));
    expect(spellers).toEqual([MISSION_PREFIX_HOME]);
  });

  it("no longer builds a mission directory name out of the clock", () => {
    // The exact pre-fix expression, and the shape of it: a `cpe-swarm-` name interpolated from
    // anything at all is guessable in a way 32 hex characters are not.
    for (const rel of productionRustFiles()) {
      expect(productionCode(rel)).not.toContain(`format!("${MISSION_PREFIX}{}"`);
    }
  });
});
