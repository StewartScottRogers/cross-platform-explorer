// CPE-1934 — the guard that guards the ratchets.
//
// This repo's one-way ratchets each store their baseline as a plain literal INSIDE the file they
// guard, so a PR that adds an offender and edits the number upward in the same diff used to pass
// trivially. `scripts/ratchet-baselines.mjs` closes that: it measures every baseline at HEAD and at
// the merge base and reds CI on an increase, unless the same diff declares the raise in
// `docs/design/RATCHETS.md`. This file is that script's own test — and, because a guard nobody
// registers is a guard that does not exist, it is also the thing that stops a NEW ratchet from
// landing unregistered.
//
// Red-proofed in BOTH directions on purpose (a guard only ever seen to pass is the exact defect
// CPE-1934 is about): `evaluate` is pure, so the raise case and the lower case are both driven here
// with real inputs, not just observed once by hand.

import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import {
  REGISTRY,
  NOT_A_RATCHET,
  LEDGER_PATH,
  evaluate,
  parseLedger,
  splitTopLevel,
  endOfSpan,
  measureWorkingTree,
  numericConst,
  arrayLength,
  jsonArrayLength,
  recordOfArraysTotal,
} from "../../scripts/ratchet-baselines.mjs";

const ROOT = resolve(__dirname, "..", "..");

/**
 * The declaration shapes a ratchet takes in this tree. Deliberately matches DECLARATIONS, not prose:
 * an earlier draft keyed on the bare words and lit up on every test file that merely discusses an
 * allowlist in a comment, which would have made the exclusion list below meaningless noise.
 */
const RATCHET_SHAPED =
  /(?:^|\n)[ \t]*(?:export[ \t]+)?const[ \t]+[A-Za-z0-9_]*(?:ALLOWLIST|ALLOW_LIST|ALLOWED_LINES|KNOWN_GAPS|KNOWN_FAILING|BASELINE)[A-Za-z0-9_]*[ \t]*(?::[^=\n]*)?=/;

/** Source roots a ratchet could plausibly live in. */
const SCAN_ROOTS = ["src", "gui-smoke", "scripts"];

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "dist" || name === "target") continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(ts|mts|mjs|js)$/.test(name)) out.push(p);
  }
  return out;
}

function ratchetShapedFiles(): string[] {
  const hits: string[] = [];
  for (const root of SCAN_ROOTS) {
    const abs = join(ROOT, root);
    if (!existsSync(abs)) continue;
    for (const f of walk(abs)) {
      if (RATCHET_SHAPED.test(readFileSync(f, "utf8"))) hits.push(relative(ROOT, f).split("\\").join("/"));
    }
  }
  return hits.sort();
}

// -------------------------------------------------------------------------------------------------
// The scanner. Counting literal entries with a naive regex is how this class of tool starts lying —
// a comma inside a string, a nested object, or a commented-out entry all shift the count silently.
// -------------------------------------------------------------------------------------------------

describe("literal scanner", () => {
  it("counts plain elements", () => {
    expect(splitTopLevel(`"a", "b", "c"`)).toHaveLength(3);
  });

  it("does not count a comma inside a string", () => {
    expect(splitTopLevel(`"a, b, c"`)).toHaveLength(1);
    expect(splitTopLevel(`"a, b", 'c, d', \`e, f\``)).toHaveLength(3);
  });

  it("does not count a comma inside a nested literal", () => {
    expect(splitTopLevel(`{ a: 1, b: 2 }, { c: 3, d: 4 }`)).toHaveLength(2);
    expect(splitTopLevel(`[1, 2, 3], [4, 5]`)).toHaveLength(2);
  });

  it("ignores a trailing comma and a comment-only tail", () => {
    expect(splitTopLevel(`"a", "b",`)).toHaveLength(2);
    expect(splitTopLevel(`"a", "b", // "c" was fixed in CPE-1\n`)).toHaveLength(2);
    expect(splitTopLevel(`"a", /* "b", "c" */`)).toHaveLength(1);
  });

  it("does not count a commented-out entry", () => {
    expect(splitTopLevel(`"a",\n  // "b",\n  "c"`)).toHaveLength(2);
  });

  it("handles an escaped quote and a template substitution containing brackets", () => {
    expect(splitTopLevel(`"he said \\"hi, there\\"", "b"`)).toHaveLength(2);
    expect(splitTopLevel(`\`x \${ f({ a: 1, b: 2 }) } y\`, "b"`)).toHaveLength(2);
  });

  it("endOfSpan refuses a non-bracket start rather than guessing", () => {
    expect(() => endOfSpan(`abc`, 0)).toThrow(/not an opening bracket/);
  });

  it("endOfSpan refuses an unterminated span rather than returning a plausible number", () => {
    expect(() => endOfSpan(`[1, 2`, 0)).toThrow(/unterminated/);
  });
});

describe("measurement shapes", () => {
  it("numericConst reads the integer, underscores and type annotations included", () => {
    expect(numericConst("N")(`const N = 85;`)).toBe(85);
    expect(numericConst("N")(`export const N: number = 1_234;`)).toBe(1234);
    expect(() => numericConst("N")(`const M = 1;`)).toThrow(/no numeric/);
  });

  it("arrayLength counts top-level entries of a typed array literal", () => {
    expect(arrayLength("A")(`const A: string[] = ["x", "y"];`)).toBe(2);
    expect(arrayLength("A")(`const A: string[] = [];`)).toBe(0);
  });

  it("recordOfArraysTotal counts recorded entries, not keys", () => {
    const src = `const R: Record<string, string[]> = {\n "a.svelte": ["1","2","3"],\n "b.svelte": ["4"],\n};`;
    expect(recordOfArraysTotal("R")(src)).toBe(4);
  });

  it("jsonArrayLength reads the named array", () => {
    expect(jsonArrayLength("cases")(`{"cases":[1,2,3]}`)).toBe(3);
    expect(() => jsonArrayLength("cases")(`{"cases":5}`)).toThrow(/not an array/);
  });
});

// -------------------------------------------------------------------------------------------------
// The registry itself.
// -------------------------------------------------------------------------------------------------

describe("the enumeration", () => {
  it("has unique ids and a real file for every entry", () => {
    const ids = REGISTRY.map((b) => b.id);
    expect(new Set(ids).size, `duplicate baseline ids: ${ids.join(", ")}`).toBe(ids.length);
    for (const b of REGISTRY) {
      expect(existsSync(join(ROOT, b.file)), `${b.id} points at ${b.file}, which does not exist`).toBe(true);
      expect(b.what.length, `${b.id} has no description of what it counts`).toBeGreaterThan(10);
    }
  });

  it("every entry measures the real file to a finite, non-negative number", () => {
    const measured = measureWorkingTree();
    for (const b of REGISTRY) {
      const v = measured[b.id];
      expect(Number.isInteger(v), `${b.id} measured ${v}, which is not an integer`).toBe(true);
      expect(v, `${b.id} measured a negative count`).toBeGreaterThanOrEqual(0);
    }
  });

  it("an entry that is enumerated but not gated must say why", () => {
    for (const b of REGISTRY.filter((x) => x.unenforced)) {
      expect((b.unenforcedReason ?? "").length, `${b.id} is unenforced with no reason recorded`).toBeGreaterThan(80);
    }
  });

  // Independent cross-check of two measurements against a completely different mechanism, so a bug in
  // the scanner cannot make the whole registry quietly agree with itself.
  it("agrees with an independent count of the two easiest baselines to verify by other means", () => {
    const measured = measureWorkingTree();
    const kf = JSON.parse(readFileSync(join(ROOT, "gui-smoke/known-failing.json"), "utf8")) as { cases: unknown[] };
    expect(measured["gui-smoke-known-failing"]).toBe(kf.cases.length);

    const css = readFileSync(join(ROOT, "src/app.css.test.ts"), "utf8");
    const hex = /const BASELINE_FILES_WITH_HEX = (\d+);/.exec(css);
    expect(hex, "src/app.css.test.ts no longer declares BASELINE_FILES_WITH_HEX the way this check expects").toBeTruthy();
    expect(measured["hex-files"]).toBe(Number(hex![1]));
  });
});

describe("the enumeration stays complete (CPE-1932: enumerate, don't recall)", () => {
  it("every ratchet-shaped file is registered or excluded with a reason", () => {
    const registered = new Set(REGISTRY.map((b) => b.file));
    const excluded = new Set(NOT_A_RATCHET.map((e) => e.file));
    const unaccounted = ratchetShapedFiles().filter((f) => !registered.has(f) && !excluded.has(f));
    expect(
      unaccounted,
      `these files declare something ratchet-shaped but appear in neither REGISTRY nor NOT_A_RATCHET in ` +
        `scripts/ratchet-baselines.mjs: ${unaccounted.join(", ")}. If it is a stored count or allowlist that ` +
        `should only ever shrink, register it (it then gets the merge-base guard for free — see ` +
        `${LEDGER_PATH}). If it is not, add it to NOT_A_RATCHET with a one-line reason.`,
    ).toEqual([]);
  });

  it("the scan actually finds things — a zero-enumeration false green is the failure this exists to stop", () => {
    expect(ratchetShapedFiles().length).toBeGreaterThanOrEqual(8);
  });

  it("no NOT_A_RATCHET entry is stale (it must exist, still match, and not be registered)", () => {
    const registered = new Set(REGISTRY.map((b) => b.file));
    const shaped = new Set(ratchetShapedFiles());
    for (const e of NOT_A_RATCHET) {
      expect(existsSync(join(ROOT, e.file)), `NOT_A_RATCHET names ${e.file}, which no longer exists`).toBe(true);
      expect(shaped.has(e.file), `NOT_A_RATCHET names ${e.file}, which no longer matches the ratchet-shaped scan — drop it`).toBe(true);
      expect(registered.has(e.file), `${e.file} is both registered AND excluded — pick one`).toBe(false);
      expect(e.reason.length, `NOT_A_RATCHET entry for ${e.file} has no real reason`).toBeGreaterThan(30);
    }
  });
});

// -------------------------------------------------------------------------------------------------
// The verdict — driven in BOTH directions.
// -------------------------------------------------------------------------------------------------

const FAKE = [{ id: "demo", file: "demo.ts", what: "demo offenders that should only ever shrink", measure: () => 0 }];

describe("evaluate — a lowered baseline sails through", () => {
  it("passes and says so when the count went down", () => {
    const v = evaluate(FAKE, { demo: 10 }, { demo: 4 }, []);
    expect(v.ok).toBe(true);
    expect(v.errors).toEqual([]);
    expect(v.messages.join("\n")).toContain("10 -> 4 LOWERED");
  });

  it("passes when the count is unchanged", () => {
    expect(evaluate(FAKE, { demo: 10 }, { demo: 10 }, []).ok).toBe(true);
  });

  it("passes when the baseline is new at this revision (nothing to compare against)", () => {
    const v = evaluate(FAKE, { demo: null }, { demo: 7 }, []);
    expect(v.ok).toBe(true);
    expect(v.messages.join("\n")).toContain("new at this revision");
  });
});

describe("evaluate — a raised baseline is loud", () => {
  it("FAILS an undeclared raise, and the message says the number is not the defect", () => {
    const v = evaluate(FAKE, { demo: 10 }, { demo: 11 }, []);
    expect(v.ok).toBe(false);
    const msg = v.errors.join("\n");
    expect(msg).toContain("went UP: 10 -> 11");
    expect(msg).toContain("demo.ts"); // names the file, not just a number (the CPE-1931 lesson)
    expect(msg).toContain("the number is not the defect");
    expect(msg).toContain(LEDGER_PATH); // and tells you exactly how to make a real raise legal
  });

  it("PASSES a raise declared by an exactly-matching ledger row, and still shouts about it", () => {
    const ledger = [{ id: "demo", from: 10, to: 11, ticket: "CPE-1", reason: "vendored file we do not own" }];
    const v = evaluate(FAKE, { demo: 10 }, { demo: 11 }, ledger);
    expect(v.ok).toBe(true);
    expect(v.messages.join("\n")).toContain("RAISED, and declared");
    expect(v.messages.join("\n")).toContain("CPE-1");
  });

  it("does NOT accept a ledger row whose numbers don't match the real movement", () => {
    const stale = [{ id: "demo", from: 9, to: 10, ticket: "CPE-1", reason: "an older, already-spent raise" }];
    expect(evaluate(FAKE, { demo: 10 }, { demo: 11 }, stale).ok).toBe(false);
    const wrongId = [{ id: "other", from: 10, to: 11, ticket: "CPE-1", reason: "a row for a different baseline" }];
    expect(evaluate(FAKE, { demo: 10 }, { demo: 11 }, wrongId).ok).toBe(false);
  });

  it("lets an explicitly unenforced baseline rise, and prints the reason instead of the number alone", () => {
    const unenforced = [{ ...FAKE[0], unenforced: true, unenforcedReason: "audits legitimately add rows" }];
    const v = evaluate(unenforced, { demo: 10 }, { demo: 12 }, []);
    expect(v.ok).toBe(true);
    expect(v.messages.join("\n")).toContain("audits legitimately add rows");
  });

  it("goes RED, not green, when a baseline could not be measured at all", () => {
    const v = evaluate(FAKE, { demo: 10 }, {}, []);
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toContain("which is a red, not a pass");
  });
});

// -------------------------------------------------------------------------------------------------
// The ledger and the CI wiring — a guard that isn't actually run is the CPE-1929 failure.
// -------------------------------------------------------------------------------------------------

describe("the raise ledger", () => {
  const ledgerSrc = readFileSync(join(ROOT, LEDGER_PATH), "utf8");

  it("exists and documents every registered baseline by id", () => {
    for (const b of REGISTRY) {
      expect(ledgerSrc, `${LEDGER_PATH} does not mention the baseline id \`${b.id}\``).toContain(b.id);
    }
  });

  it("parses the real file without inventing rows from the enumeration table above it", () => {
    // The doc holds two tables. Only rows shaped `| id | N -> M | CPE-N | why |` are raises; the
    // enumeration table must not leak in as a phantom authorisation.
    for (const row of parseLedger(ledgerSrc)) {
      expect(REGISTRY.some((b) => b.id === row.id), `ledger row names unknown baseline ${row.id}`).toBe(true);
      expect(row.to).toBeGreaterThan(row.from);
    }
  });

  it("parses a well-formed row and rejects a malformed one", () => {
    const rows = parseLedger("| `hex-files` | 85 -> 86 | CPE-1234 | one vendored component |");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ id: "hex-files", from: 85, to: 86, ticket: "CPE-1234" });
    expect(parseLedger("| hex-files | 85 -> 86 | | no ticket |")).toHaveLength(0);
    expect(parseLedger("| hex-files | 85 | CPE-1 | no movement |")).toHaveLength(0);
  });
});

describe("CI wiring", () => {
  const ci = readFileSync(join(ROOT, ".github/workflows/ci.yml"), "utf8");

  /** The `ratchet-guard:` job's own YAML block, up to the next top-level job key. */
  function ratchetGuardJob(): string {
    const start = ci.indexOf("\n  ratchet-guard:");
    expect(start, "no `ratchet-guard:` job in .github/workflows/ci.yml — the guard is not wired in").toBeGreaterThan(-1);
    const rest = ci.slice(start + 1);
    const next = /\n {2}[a-z0-9][a-z0-9_-]*:\n/.exec(rest.slice(1));
    return next ? rest.slice(0, next.index + 1) : rest;
  }

  it("ci.yml actually runs the guard, inside a job of its own", () => {
    expect(ratchetGuardJob()).toContain("node scripts/ratchet-baselines.mjs compare");
  });

  it("checks out enough history for the base revision to resolve", () => {
    // A shallow checkout would make `git show <base>:<file>` fail for every baseline, which the script
    // reports as "new at this revision" — a silent all-green. Depth is what stops that.
    expect(ratchetGuardJob()).toContain("fetch-depth: 0");
  });

  it("passes a base revision for both trigger shapes, so neither push nor PR runs uncompared", () => {
    const job = ratchetGuardJob();
    expect(job).toContain("github.event.pull_request.base.sha");
    expect(job).toContain("github.event.before");
  });
});
