/**
 * CPE-1954 — the guard that keeps `VerifiedIndex` the **only** door onto a catalog index.
 *
 * ## The invariant
 *
 * A catalog index is a signed document, and every field in it — `id` above all — is
 * attacker-controlled until the detached signature verifies against a key the reader trusts. Each
 * consumer then interpolates `entry.id` into a filesystem path or a fetch URL. CPE-1940 made that
 * safe **structurally** rather than by convention: `VerifiedIndex::open` verifies first and parses
 * second, so a caller holding one cannot have used an entry field too early, and CPE-1949 folded the
 * `is_valid_entry_id` charset rule into the same gate.
 *
 * All of which is worth exactly as much as the number of ways *around* it. `catalog-sign verify` was
 * the one route around: it parsed with `CatalogIndex::from_json` and then did
 * `dir.join(format!("{}.json", entry.id))`. Not a broken guard — a **path that did not reach one**,
 * which is the shape this repo keeps rediscovering (CPE-1958's `carry_protections` was the same
 * species: the guard still worked, a change elsewhere simply stopped calling it).
 *
 * So the invariant this file makes statable is: **no site outside `catalog.rs` turns index bytes
 * into a `CatalogIndex`.** With that true, "verified" is not a property a reader has to remember to
 * ask for; it is the only thing on offer.
 *
 * ## Why a scanner as well as the compiler
 *
 * `CatalogIndex::from_json` is `pub(crate)` as of this ticket, so rustc itself refuses the
 * convenient spelling from outside the module — a stronger guarantee than any test. But
 * `CatalogIndex` is still a `pub` type deriving `Deserialize`, so an outside caller can write
 * `serde_json::from_str::<CatalogIndex>(…)` and get the identical unchecked document. **The
 * compiler closes the front door; this closes the back one.** Neither alone is the invariant.
 *
 * ## Anchoring (CPE-1933)
 *
 * The list of files is derived with `git ls-files` at run time, never recalled (CPE-1932), and a
 * near-empty result is a hard failure rather than a clean bill of health. Comments are blanked with
 * the shared `stripRustComments` before scanning, because `catalog.rs` and `catalog_sign.rs` are
 * both full of prose naming the very call this forbids — a raw-text scan would fail on the fix it is
 * guarding. `stripRustComments` throws on three known files in this repo (raw string literals whose
 * content starts with `//`); those fall back to a raw scan, which can only ever produce a **false
 * refusal**, never a false pass.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { stripRustComments } from "./rustSource";

const ROOT = resolve(__dirname, "..", "..");

/** The one module allowed to turn index bytes into a `CatalogIndex`: the one that defines both. */
const THE_ONE_DOOR = "sidecar/host/src/catalog.rs";

/** The site this ticket rerouted, asserted by name so deleting the routing reds here too. */
const THE_FIXED_SITE = "sidecar/host/src/bin/catalog_sign.rs";

/**
 * Enough tracked `.rs` files that the sweep plainly ran. The repo has 353 today; this only has to be
 * far enough above zero that an enumeration which silently returned nothing cannot read as a pass.
 */
const MIN_RUST_FILES = 200;

/** Every spelling that yields an unchecked `CatalogIndex` from bytes or text. */
const UNCHECKED_PARSE: { what: string; re: RegExp }[] = [
  { what: "CatalogIndex::from_json", re: /CatalogIndex::from_json/g },
  {
    what: "a turbofished serde deserialisation into CatalogIndex",
    re: /from_(?:str|slice|value|reader)::<\s*(?:[A-Za-z0-9_]+::)*CatalogIndex\s*>/g,
  },
  {
    what: "a type-annotated serde deserialisation into CatalogIndex",
    re: /:\s*(?:[A-Za-z0-9_]+::)*CatalogIndex\s*=/g,
  },
];

interface Scanned {
  path: string;
  hits: { what: string; count: number }[];
  stripped: boolean;
}

/** Blank comments if we can; on a known-unstrippable source fall back to raw text (fail-closed). */
function codeOf(text: string): { code: string; stripped: boolean } {
  const normalised = text.replace(/\r\n/g, "\n");
  try {
    return { code: stripRustComments(normalised), stripped: true };
  } catch {
    return { code: normalised, stripped: false };
  }
}

/** The detector, exposed as one function so the red-proofs below drive the same code as the sweep. */
export function uncheckedParsesIn(text: string): { what: string; count: number }[] {
  const { code } = codeOf(text);
  const found: { what: string; count: number }[] = [];
  for (const { what, re } of UNCHECKED_PARSE) {
    const count = code.match(new RegExp(re.source, "g"))?.length ?? 0;
    if (count > 0) found.push({ what, count });
  }
  return found;
}

function trackedRustFiles(): string[] {
  const out = spawnSync("git", ["ls-files", "*.rs"], { cwd: ROOT, encoding: "utf8" });
  if (out.status !== 0) {
    throw new Error(
      `git ls-files failed (status ${out.status}): ${out.stderr}. Refusing to report a clean sweep ` +
        `from an enumeration that did not run — that is the "npm said nothing" defect (CLAUDE.md).`,
    );
  }
  return out.stdout
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
}

function scanAll(): Scanned[] {
  return trackedRustFiles().map((path) => {
    const raw = readFileSync(join(ROOT, path), "utf8");
    const { code, stripped } = codeOf(raw);
    const hits: { what: string; count: number }[] = [];
    for (const { what, re } of UNCHECKED_PARSE) {
      const count = code.match(new RegExp(re.source, "g"))?.length ?? 0;
      if (count > 0) hits.push({ what, count });
    }
    return { path, hits, stripped };
  });
}

describe("CPE-1954: VerifiedIndex is the only door onto a catalog index", () => {
  const files = trackedRustFiles();

  it("enumerated the repo rather than recalling it", () => {
    expect(
      files.length,
      `git ls-files '*.rs' returned ${files.length} files, which is not a repo — a sweep over ` +
        `nothing must fail, never pass (CPE-1932)`,
    ).toBeGreaterThan(MIN_RUST_FILES);
    expect(files, "the door itself must be in the enumeration").toContain(THE_ONE_DOOR);
    expect(files, "the site this ticket fixed must be in the enumeration").toContain(THE_FIXED_SITE);
  });

  const scanned = scanAll();

  it("no site outside catalog.rs parses an index", () => {
    const offenders = scanned.filter((s) => s.path !== THE_ONE_DOOR && s.hits.length > 0);
    expect(
      offenders.map((o) => `${o.path}: ${o.hits.map((h) => `${h.what} ×${h.count}`).join(", ")}`),
      `these files turn bytes into a CatalogIndex without going through VerifiedIndex. Every field ` +
        `of the result — \`id\` above all, which callers interpolate into paths and URLs — is ` +
        `attacker-controlled until the index signature verifies. Use ` +
        `\`VerifiedIndex::open\` (or \`open_reported\` if you need to tell a human why it was ` +
        `refused). CPE-1954.`,
    ).toEqual([]);
  });

  it("the door still exists, so the sweep above is not vacuous", () => {
    const door = scanned.find((s) => s.path === THE_ONE_DOOR);
    expect(door, `${THE_ONE_DOOR} vanished — this guard is reading the wrong tree`).toBeTruthy();
    expect(
      door!.hits.length,
      `the detector found no parse at all in ${THE_ONE_DOOR}. Either the parse moved, or the ` +
        `detector no longer matches real source — in which case every "no offenders" result above ` +
        `is meaningless.`,
    ).toBeGreaterThan(0);
    expect(door!.stripped, `${THE_ONE_DOOR} must be comment-strippable for this scan to mean much`)
      .toBe(true);

    const src = readFileSync(join(ROOT, THE_ONE_DOOR), "utf8");
    // The compiler half of the invariant: rustc, not this test, refuses the convenient spelling
    // from outside the module.
    expect(
      src,
      "CatalogIndex::from_json must stay pub(crate) — the visibility IS the front-door guard",
    ).toContain("pub(crate) fn from_json");
    expect(src).toContain("pub fn open_reported(");
  });

  it("the site this ticket rerouted still goes through the door", () => {
    const src = readFileSync(join(ROOT, THE_FIXED_SITE), "utf8");
    const { code } = codeOf(src);
    expect(
      code,
      `${THE_FIXED_SITE} must open its index through VerifiedIndex. It is the one input that never ` +
        `passes sign_bundle — an operator appraising a bundle someone else built — and it forms a ` +
        `path from entry.id immediately afterwards.`,
    ).toContain("VerifiedIndex::open_reported(");
    // And the path formation it guards is still there, so the assertion above is about something.
    expect(code).toContain('format!("{}.json", entry.id)');
  });

  it("says which files could not be comment-stripped, and fails closed on them", () => {
    // Not an allowance: an unstrippable file is scanned RAW, so a mention in a comment would red.
    // Recorded here so the next reader knows the sweep is conservative rather than skipping.
    const unstrippable = scanned.filter((s) => !s.stripped);
    for (const f of unstrippable) {
      expect(
        f.hits,
        `${f.path} could not be comment-stripped and mentions a catalog-index parse. It was ` +
          `scanned as raw text, so this may be a comment — but the guard will not guess. Narrow ` +
          `the scan or fix stripRustComments; do not add an exception.`,
      ).toEqual([]);
    }
  });
});

describe("the detector's own red-proof", () => {
  // CPE-1933 rule 3: change the referenced source and watch it fail. These drive the SAME function
  // the sweep uses, so a detector that stopped detecting reds here rather than passing silently.
  const wrap = (body: string) => `fn f() {\n${body}\n}\n`;

  it.each([
    ["the convenient spelling", "    let i = CatalogIndex::from_json(&text).unwrap();"],
    ["a turbofished serde parse", "    let i = serde_json::from_str::<CatalogIndex>(&text)?;"],
    ["a fully-qualified turbofish", "    let i = serde_json::from_slice::<catalog::CatalogIndex>(b)?;"],
    ["a type-annotated parse", "    let i: CatalogIndex = serde_json::from_slice(&bytes)?;"],
    ["a path-qualified annotation", "    let i: catalog::CatalogIndex = serde_json::from_str(s)?;"],
  ])("reds on %s", (_what, line) => {
    expect(uncheckedParsesIn(wrap(line))).not.toEqual([]);
  });

  it.each([
    ["a line comment quoting the call", "    // was CatalogIndex::from_json(&text) before CPE-1954"],
    ["a trailing comment", "    let x = 1; // CatalogIndex::from_json used to live here"],
    ["a block comment", "    /* let i: CatalogIndex = serde_json::from_slice(b)?; */"],
    ["a doc comment", "    /// see CatalogIndex::from_json for the unchecked form"],
  ])("stays green on %s", (_what, line) => {
    expect(uncheckedParsesIn(wrap(line))).toEqual([]);
  });

  it("stays green on the verified door itself", () => {
    expect(
      uncheckedParsesIn(wrap("    let i = VerifiedIndex::open(&bytes, sig, &keys)?;")),
    ).toEqual([]);
    expect(
      uncheckedParsesIn(wrap("    let i = VerifiedIndex::open_reported(&bytes, sig, &keys)?;")),
    ).toEqual([]);
  });

  it("reds on a reintroduced bare parse + join, the exact pre-fix shape", () => {
    const prefix = [
      "fn verify(dir: &Path, keys: &[String]) {",
      "    let index_bytes = read(\"catalog-index.json\");",
      "    let index = CatalogIndex::from_json(&String::from_utf8_lossy(&index_bytes)).unwrap();",
      "    for entry in &index.entries {",
      "        let m = read(&format!(\"{}.json\", entry.id));",
      "    }",
      "}",
      "",
    ].join("\n");
    expect(uncheckedParsesIn(prefix)).not.toEqual([]);
  });
});
