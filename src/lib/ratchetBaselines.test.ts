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
// CPE-1934 is about). The "SABOTAGE FIXTURES" block below is the important half: three one-line
// edits a real developer could make innocently, each of which took the FIRST version of this guard
// all-green while a baseline was genuinely raised. Every one is a permanent test case now, because
// the first round proved the SAFE variant (a plain rename reds) and never tried the dangerous one.

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
  maskNonCode,
  isUnmeasurable,
  numericConst,
  arrayLength,
  jsonArrayLength,
  recordOfArraysTotal,
} from "../../scripts/ratchet-baselines.mjs";

const ROOT = resolve(__dirname, "..", "..");

/**
 * The declaration shapes a ratchet takes. Deliberately matches DECLARATIONS, not prose: keying on the
 * bare words lit up on every test file that merely discusses an allowlist in a comment, which would
 * have made the exclusion list below meaningless noise.
 *
 * Widened in review round 2: the first cut keyed only on ALLOWLIST/ALLOWED_LINES/KNOWN_GAPS/
 * KNOWN_FAILING/BASELINE, so a future `const FOO_OFFENDERS = [...]` in a NEW file would have escaped
 * entirely — `APP_MARKUP_OFFENDERS` was covered only by the accident of living in a file that also
 * declares an `*_ALLOWLIST`. OFFENDER, SUPPRESS, TOLERAT, WAIVER, OPTOUT, EXEMPT, EXCLUD, DEBT,
 * GRANDFATHER, LEGACY_, REGISTRY, CEILING, THRESHOLD, PENDING and EXISTING are the rest of the
 * vocabulary this class of list gets named with.
 */
const RATCHET_SHAPED =
  /(?:^|\n)[ \t]*(?:export[ \t]+)?const[ \t]+[A-Za-z0-9_]*(?:ALLOWLIST|ALLOW_LIST|ALLOWED_LINES|ALLOWED_|KNOWN_GAPS|KNOWN_FAILING|BASELINE|OFFENDER|SUPPRESS|TOLERAT|WAIVER|WAIVED|OPTOUT|OPT_OUT|EXEMPT|EXCLUD|DEBT|GRANDFATHER|LEGACY_|REGISTRY|CEILING|THRESHOLD|PENDING|EXISTING)[A-Za-z0-9_]*[ \t]*(?::[^=\n]*)?=/;

/** Source roots a ratchet could plausibly live in. */
const SCAN_ROOTS = ["src", "gui-smoke", "scripts"];
const SCANNED_EXT = /\.(ts|mts|mjs|js)$/;

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "dist" || name === "target") continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (SCANNED_EXT.test(name)) out.push(p);
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

/** Registered baselines whose file the scan above is actually capable of seeing. */
function registeredScannableFiles(): string[] {
  return [
    ...new Set(
      REGISTRY.map((b) => b.file).filter(
        (f) => SCANNED_EXT.test(f) && SCAN_ROOTS.some((r) => f === r || f.startsWith(`${r}/`)),
      ),
    ),
  ].sort();
}

// -------------------------------------------------------------------------------------------------
// SABOTAGE FIXTURES — the three all-green bypasses found in review, each now permanent.
//
// The rule they all encode: a measurement this guard cannot make must be RED, never a number and
// never a skip. A measurer that returns the WRONG value passes a raise, which is the whole defect.
// -------------------------------------------------------------------------------------------------

describe("SABOTAGE F1 — a baseline constant that stops being a plain integer", () => {
  // Review input, verbatim: `const BASELINE_TOTAL_HEX_OCCURRENCES = 200 + 78;` is a real 277 -> 278
  // raise. The first version's `=\s*(\d[\d_]*)` took the first integer and stopped, measuring 200 and
  // reporting `277 -> 200 LOWERED` with exit 0 — a complete all-green bypass from one line.
  it("THROWS on `= 200 + 78` rather than measuring 200", () => {
    expect(() => numericConst("BASELINE_TOTAL_HEX_OCCURRENCES")(`const BASELINE_TOTAL_HEX_OCCURRENCES = 200 + 78;`))
      .toThrow(/no longer a plain integer literal/);
  });

  it("throws on every other expression form that hides the real value", () => {
    const N = numericConst("N");
    expect(() => N(`const N = 277 + 1;`)).toThrow(/plain integer/);
    expect(() => N(`const N = Number("278");`)).toThrow(/plain integer/);
    expect(() => N(`const N = OTHER;`)).toThrow(/plain integer/);
    expect(() => N(`const N = 278 as number;`)).toThrow(/plain integer/);
  });

  it("still reads the honest forms", () => {
    expect(numericConst("N")(`const N = 85;`)).toBe(85);
    expect(numericConst("N")(`export const N: number = 1_234;`)).toBe(1234);
    expect(numericConst("N")(`const N = 85; // CPE-1534 baseline`)).toBe(85);
  });

  it("an unmeasurable baseline reaches evaluate as a RED, not as a number", () => {
    const v = evaluate(FAKE, { demo: 10 }, { demo: { failed: "src/x.ts: is no longer a plain integer literal" } }, []);
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toContain("never green and never a guessed number");
  });
});

describe("SABOTAGE F1b — an allowlist that spreads another list into itself", () => {
  // Review input: replacing four literal KNOWN_GAPS_ALLOWLIST entries with `...MORE_GAPS` (6 names) is
  // a real 14 -> 17 raise. The first version counted the spread as ONE element and reported
  // `14 -> 12 LOWERED`, exit 0.
  it("THROWS on a spread element rather than counting it as one", () => {
    expect(() => splitTopLevel(`"a", "b", ...MORE_GAPS`)).toThrow(/spreads another value into itself/);
    expect(() => arrayLength("A")(`const A: string[] = ["a", "b", ...MORE_GAPS];`)).toThrow(/spreads another value/);
  });

  it("throws when the whole array is a spread of another array", () => {
    expect(() => arrayLength("A")(`const A = [...MORE_GAPS];`)).toThrow(/spreads another value/);
  });

  it("throws when a Record's value array spreads", () => {
    expect(() => recordOfArraysTotal("R")(`const R = { "a.svelte": ["1", ...MORE] };`)).toThrow(/spreads another value/);
  });

  it("refuses a literal that is not the whole initialiser (the count would ignore the rest)", () => {
    expect(() => arrayLength("A")(`const A = ["a"].concat(MORE_GAPS);`)).toThrow(/not the whole initialiser/);
    expect(() => arrayLength("A")(`const A = ["a", "b"].slice(1);`)).toThrow(/not the whole initialiser/);
  });

  it("still accepts the honest forms, `as const` included", () => {
    expect(arrayLength("A")(`const A: string[] = ["x", "y"];`)).toBe(2);
    expect(arrayLength("A")(`const A = ["x", "y"] as const;`)).toBe(2);
    expect(arrayLength("A")(`const A: string[] = [];`)).toBe(0);
  });
});

describe("SABOTAGE F2 — reusing a ledger row that already existed at the base revision", () => {
  // Review input: commit `| hex-occurrences | 277 -> 278 | CPE-1111 |` as the BASE with no baseline
  // change, then bump 277 -> 278 in the working tree alone. The first version read only the
  // working-tree ledger and exited 0, citing that row. Realistic: hex-occurrences went 276 -> 277 last
  // week, so burn back down and re-raise later and it passes silently under someone else's ticket.
  const row = [{ id: "demo", from: 10, to: 11, ticket: "CPE-1111", reason: "an older, already-spent raise" }];

  it("FAILS when the ledger is unchanged from the base — this diff added no row", () => {
    const v = evaluate(FAKE, { demo: 10 }, { demo: 11 }, row, row);
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toContain("did not ADD a ledger row");
  });

  it("PASSES the same row when this diff is the one that adds it", () => {
    const v = evaluate(FAKE, { demo: 10 }, { demo: 11 }, row, []);
    expect(v.ok).toBe(true);
    expect(v.messages.join("\n")).toContain("RAISED, and declared");
  });

  it("an unrelated pre-existing row does not spend this one", () => {
    const other = [{ id: "demo", from: 5, to: 6, ticket: "CPE-9", reason: "a different, older raise" }];
    expect(evaluate(FAKE, { demo: 10 }, { demo: 11 }, [...row, ...other], other).ok).toBe(true);
  });
});

describe("SABOTAGE R2-F2b — the F2 fix must not make a legitimate repeat raise impossible", () => {
  // Review input, round 3: the round-2 fix asked "does the base ledger contain a row for this
  // movement?" — so the SAME from -> to could never legitimately happen twice. Base carries
  // `| hex-occurrences | 277 -> 278 | CPE-1111 |`; the working tree bumps 277 -> 278 AND appends a new
  // row under CPE-2222. That exited 1 saying "Add a NEW row, under the ticket that owns THIS raise" —
  // which the author had already done, leaving deleting or falsifying the historical row as the only
  // way through. And it is exactly the realistic path: hex went 276 -> 277 last week, so
  // 277 -> 278 -> burn down -> 277 -> 278 again is how this repo actually moves.
  //
  // The fix is to COUNT rather than `find`: authorise when the working tree holds strictly more rows
  // for that (id, from, to) than the base did.
  const historical = { id: "demo", from: 10, to: 11, ticket: "CPE-1111", reason: "historical, already spent" };
  const fresh = { id: "demo", from: 10, to: 11, ticket: "CPE-2222", reason: "this diff: a new raise, new ticket" };

  it("PASSES when this diff appends a second row for the same movement under a new ticket", () => {
    const v = evaluate(FAKE, { demo: 10 }, { demo: 11 }, [historical, fresh], [historical]);
    expect(v.ok).toBe(true);
    // ...and cites the NEW row, not the historical one.
    expect(v.messages.join("\n")).toContain("CPE-2222");
  });

  it("still FAILS when the ledger carries the historical row and this diff adds nothing", () => {
    const v = evaluate(FAKE, { demo: 10 }, { demo: 11 }, [historical], [historical]);
    expect(v.ok).toBe(false);
    const msg = v.errors.join("\n");
    expect(msg).toContain("carries 1 row(s) for it and the base revision already carried 1");
    // The remedy must not be advice the author has already followed, and must never be "delete a row".
    expect(msg).toContain("APPEND a new row");
    expect(msg).toContain("keep the historical row");
  });

  it("counts rather than finds — three in the tree against two at the base is one new licence", () => {
    const third = { ...fresh, ticket: "CPE-3333" };
    expect(evaluate(FAKE, { demo: 10 }, { demo: 11 }, [historical, fresh, third], [historical, fresh]).ok).toBe(true);
    expect(evaluate(FAKE, { demo: 10 }, { demo: 11 }, [historical, fresh], [historical, fresh]).ok).toBe(false);
  });

  it("applies the same counting rule to a `new ->` declaration", () => {
    const old = { id: "demo", from: null, to: 17, ticket: "CPE-1", reason: "an earlier introduction" };
    const now = { id: "demo", from: null, to: 17, ticket: "CPE-2", reason: "this diff" };
    expect(evaluate(FAKE, { demo: null }, { demo: 17 }, [old, now], [old]).ok).toBe(true);
    expect(evaluate(FAKE, { demo: null }, { demo: 17 }, [old], [old]).ok).toBe(false);
  });
});

describe("SABOTAGE R2-F1c — a decoy declaration outranking the live constant", () => {
  // Review input, round 3: F1 again in a different costume. The declaration SEARCH ran on raw source
  // and took the first match — `stripComments` was only ever applied to the captured initialiser,
  // never before the search. The `[ \t]*` before `const` made a `//`-commented decoy safe, but a
  // `/* … */` block or a template literal was not:
  //
  //     /*
  //     Historical note, kept for context:
  //     const BASELINE_TOTAL_HEX_OCCURRENCES = 277;
  //     */
  //     const BASELINE_TOTAL_HEX_OCCURRENCES = 278;   <- live, a real 277 -> 278 raise
  //
  // measured 277, said "unchanged", exit 0, 72/72 vitest green. Two-part fix: mask comments and
  // string/template interiors before searching, AND treat more than one matching declaration as a red
  // in itself — the second half is the durable one, because it removes the question rather than
  // answering it.
  const BLOCK_DECOY = `/*
Historical note, kept for context:
const BASELINE_TOTAL_HEX_OCCURRENCES = 277;
*/
const BASELINE_FILES_WITH_HEX = 85;
const BASELINE_TOTAL_HEX_OCCURRENCES = 278;`;

  it("reads the LIVE 278, not the block-commented 277", () => {
    expect(numericConst("BASELINE_TOTAL_HEX_OCCURRENCES")(BLOCK_DECOY)).toBe(278);
    expect(numericConst("BASELINE_FILES_WITH_HEX")(BLOCK_DECOY)).toBe(85);
  });

  it("reads the LIVE value past a template-literal decoy", () => {
    const src = ["const doc = `", "const N = 1;", "`;", "const N = 278;"].join("\n");
    expect(numericConst("N")(src)).toBe(278);
  });

  it("reads the LIVE array past a block-comment decoy (the reviewer's 5-entry case measured 2)", () => {
    const src = `/*
Old shape:
const A: string[] = ["one", "two"];
*/
const A: string[] = ["a", "b", "c", "d", "e"];`;
    expect(arrayLength("A")(src)).toBe(5);
  });

  it("reads the LIVE array past a template-literal decoy (the reviewer's 3-entry case measured 1)", () => {
    const src = ["const help = `", 'const A = ["only"];', "`;", 'const A = ["a", "b", "c"];'].join("\n");
    expect(arrayLength("A")(src)).toBe(3);
  });

  it("THROWS on two LIVE declarations — the shape no masker can see through", () => {
    const src = `const N = 277;\nconst N = 278;`;
    expect(() => numericConst("N")(src)).toThrow(/2 declarations of `const N` found \(lines 1, 2\)/);
    expect(() => numericConst("N")(src)).toThrow(/There must be exactly one/);
    expect(() => arrayLength("A")(`const A = ["a"];\nconst A = ["a","b"];`)).toThrow(/2 declarations/);
  });

  it("a decoy that is only a comment is masked, so it is not counted as a second declaration", () => {
    // The masker and the sole-declaration rule must not fight each other: a commented-out old value is
    // ordinary, harmless housekeeping and must keep working.
    expect(numericConst("N")(`// const N = 1;\nconst N = 278;`)).toBe(278);
    expect(numericConst("N")(`/* const N = 1; */\nconst N = 278;`)).toBe(278);
  });

  describe("maskNonCode", () => {
    it("preserves length and newlines so every index stays valid against the original", () => {
      const src = `const a = "xy"; // note\n/* b */ const c = 1;\n`;
      const masked = maskNonCode(src);
      expect(masked).toHaveLength(src.length);
      expect(masked.split("\n")).toHaveLength(src.split("\n").length);
    });

    it("blanks comment bodies and string interiors but keeps the quotes", () => {
      expect(maskNonCode(`const a = "const N = 1;";`)).toBe(`const a = "${" ".repeat("const N = 1;".length)}";`);
      expect(maskNonCode(`// const N = 1;`)).toBe(" ".repeat("// const N = 1;".length));
    });

    it("bounds an unterminated single-line quote at its own line, not the rest of the file", () => {
      const masked = maskNonCode(`const bad = /'/;\nconst N = 278;`);
      expect(masked).toContain("const N = 278;"); // the live declaration survives
    });
  });
});

describe("SABOTAGE F3 — renaming a baseline's file to reset its ratchet", () => {
  // Review input: `git mv src/docs.coverage.test.ts src/docsCoverage.test.ts` + registry update + 3 new
  // allowlist entries. The first version mapped a failed `git show <base>:<file>` to null and treated a
  // null base as "new at this revision" -> pass: a real 14 -> 17 went unnoticed at exit 0. Note the
  // asymmetry that made it a bug rather than a choice: head-side unmeasurable was RED, base-side GREEN.
  it("FAILS when a baseline has no value at the base and no rename was detected", () => {
    const v = evaluate(FAKE, { demo: null }, { demo: 17 }, []);
    expect(v.ok).toBe(false);
    const msg = v.errors.join("\n");
    expect(msg).toContain("no value at the base revision");
    expect(msg).toContain("reset the ratchet");
    expect(msg).toContain("new -> 17"); // and tells you the exact declaration that would make it legal
  });

  it("FAILS when the base file exists but does not measure — base-side unmeasurable is red too", () => {
    const v = evaluate(FAKE, { demo: { failed: "demo.ts at abc123: no `const X` declaration found" } }, { demo: 11 }, []);
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toContain("could not be measured at the BASE revision");
  });

  it("PASSES a genuinely new baseline when this diff declares it new", () => {
    const declared = [{ id: "demo", from: null, to: 17, ticket: "CPE-2", reason: "brand-new guard landing here" }];
    const v = evaluate(FAKE, { demo: null }, { demo: 17 }, declared, []);
    expect(v.ok).toBe(true);
    expect(v.messages.join("\n")).toContain("new at this revision (17), declared");
  });

  it("does not accept a `new ->` declaration that was already spent at the base", () => {
    const declared = [{ id: "demo", from: null, to: 17, ticket: "CPE-2", reason: "brand-new guard landing here" }];
    expect(evaluate(FAKE, { demo: null }, { demo: 17 }, declared, declared).ok).toBe(false);
  });

  it("parses `| id | new -> N | CPE-N | why |` from the ledger", () => {
    const rows = parseLedger("| `docs-known-gaps` | new -> 17 | CPE-1234 | brand-new guard |");
    expect(rows).toHaveLength(1);
    expect(rows[0].from).toBeNull();
    expect(rows[0].to).toBe(17);
  });
});

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
  it("recordOfArraysTotal counts recorded entries, not keys", () => {
    const src = `const R: Record<string, string[]> = {\n "a.svelte": ["1","2","3"],\n "b.svelte": ["4"],\n};`;
    expect(recordOfArraysTotal("R")(src)).toBe(4);
  });

  it("jsonArrayLength reads the named array", () => {
    expect(jsonArrayLength("cases")(`{"cases":[1,2,3]}`)).toBe(3);
    expect(() => jsonArrayLength("cases")(`{"cases":5}`)).toThrow(/not an array/);
  });

  it("a renamed constant is a measurement failure, not a number", () => {
    expect(() => arrayLength("GONE")(`const OTHER = ["a"];`)).toThrow(/no \`const GONE\` declaration found/);
    expect(() => numericConst("GONE")(`const OTHER = 5;`)).toThrow(/no \`const GONE\` declaration found/);
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
      expect(isUnmeasurable(v), `${b.id} failed to measure: ${isUnmeasurable(v) ? v.failed : ""}`).toBe(false);
      expect(Number.isInteger(v), `${b.id} measured ${JSON.stringify(v)}, which is not an integer`).toBe(true);
      expect(v as number, `${b.id} measured a negative count`).toBeGreaterThanOrEqual(0);
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

  // Non-vacuity, derived from the registry rather than a magic number. The first round used a floor of
  // 8 against 11 real hits, which goes thin the moment a couple of allowlists burn down and get
  // deleted. This says the real thing instead: the scan must actually SEE every file it is supposed to
  // be policing, so a broken regex or a broken walk reds instead of reporting a comfortable count.
  // Round 3: the derived requirement below is filtered by `SCAN_ROOTS.some(...)`, so narrowing
  // SCAN_ROOTS narrowed the requirement in lockstep — `SCAN_ROOTS = ["src/lib/preview"]` made "finds
  // every registered file it is capable of seeing" pass with ZERO files capable of being seen. Round
  // 1's absolute `>= 8` would have caught that head-on. Both halves are asserted here instead: the
  // roots must actually COVER the registry, and what they cover must be non-trivial.
  it("SCAN_ROOTS covers the registry, so the derived requirement below cannot be made vacuous", () => {
    const uncovered = REGISTRY.map((b) => b.file)
      .filter((f) => SCANNED_EXT.test(f))
      .filter((f) => !SCAN_ROOTS.some((r) => f === r || f.startsWith(`${r}/`)));
    expect(
      uncovered,
      `these registered baseline files sit OUTSIDE SCAN_ROOTS, so the completeness scan cannot see them ` +
        `at all: ${uncovered.join(", ")}. Widen SCAN_ROOTS — never narrow it to make a check pass.`,
    ).toEqual([]);

    expect(
      registeredScannableFiles().length,
      "the completeness scan is required to cover almost nothing — SCAN_ROOTS or SCANNED_EXT has been " +
        "narrowed until the requirement below is vacuous, which is the CPE-1932 zero-enumeration false green",
    ).toBeGreaterThanOrEqual(6);
  });

  it("the scan finds every registered file it is capable of seeing", () => {
    const shaped = new Set(ratchetShapedFiles());
    const missed = registeredScannableFiles().filter((f) => !shaped.has(f));
    expect(
      missed,
      `the ratchet-shape scan no longer matches these REGISTERED baseline files: ${missed.join(", ")}. The scan is ` +
        `what stops a NEW ratchet landing unregistered, so if it cannot even see the ones we know about it is ` +
        `not enumerating anything — fix RATCHET_SHAPED or the walk, do not lower this expectation.`,
    ).toEqual([]);
  });

  it("the scan also finds every file the exclusion list claims to be about", () => {
    const shaped = new Set(ratchetShapedFiles());
    const missed = NOT_A_RATCHET.map((e) => e.file).filter((f) => !shaped.has(f));
    expect(missed, `NOT_A_RATCHET names files the scan no longer matches: ${missed.join(", ")}`).toEqual([]);
  });

  it("no NOT_A_RATCHET entry is stale (it must exist, still match, and not be registered)", () => {
    const registered = new Set(REGISTRY.map((b) => b.file));
    for (const e of NOT_A_RATCHET) {
      expect(existsSync(join(ROOT, e.file)), `NOT_A_RATCHET names ${e.file}, which no longer exists`).toBe(true);
      expect(registered.has(e.file), `${e.file} is both registered AND excluded — pick one`).toBe(false);
      expect(e.reason.length, `NOT_A_RATCHET entry for ${e.file} has no real reason`).toBeGreaterThan(30);
    }
  });

  it("the widened shape would catch a future `const FOO_OFFENDERS = [...]` in a brand-new file", () => {
    // The first cut keyed only on ALLOWLIST-ish words, so this exact declaration escaped unless it
    // happened to share a file with one.
    expect(RATCHET_SHAPED.test(`\nconst FOO_OFFENDERS: string[] = ["a"];\n`)).toBe(true);
    expect(RATCHET_SHAPED.test(`\nexport const SUPPRESSED_RULES = [];\n`)).toBe(true);
    expect(RATCHET_SHAPED.test(`\nconst TOLERATED_WARNINGS: string[] = [];\n`)).toBe(true);
    expect(RATCHET_SHAPED.test(`\nconst PENDING_MIGRATIONS = [];\n`)).toBe(true);
    // ...without matching ordinary code.
    expect(RATCHET_SHAPED.test(`\nconst rows = await load();\n`)).toBe(false);
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

  it("does NOT accept a ledger row whose numbers don't match the real movement", () => {
    const stale = [{ id: "demo", from: 9, to: 10, ticket: "CPE-1", reason: "an older, already-spent raise" }];
    expect(evaluate(FAKE, { demo: 10 }, { demo: 11 }, stale).ok).toBe(false);
    const wrongId = [{ id: "other", from: 10, to: 11, ticket: "CPE-1", reason: "a row for a different baseline" }];
    expect(evaluate(FAKE, { demo: 10 }, { demo: 11 }, wrongId).ok).toBe(false);
  });

  it("a net-zero diff does not mask a raise — each baseline is judged on its own", () => {
    const two = [FAKE[0], { id: "other", file: "o.ts", what: "other offenders", measure: () => 0 }];
    const v = evaluate(two, { demo: 10, other: 10 }, { demo: 13, other: 7 }, []);
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toContain("demo");
    expect(v.messages.join("\n")).toContain("other: 10 -> 7 LOWERED");
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

  it("says a row is spent by the diff that adds it, not a standing permit", () => {
    expect(ledgerSrc.toLowerCase()).toContain("one-time");
  });

  it("parses the real file without inventing rows from the enumeration table above it", () => {
    // The doc holds two tables. Only rows shaped `| id | N -> M | CPE-N | why |` are raises; the
    // enumeration table must not leak in as a phantom authorisation.
    for (const row of parseLedger(ledgerSrc)) {
      expect(REGISTRY.some((b) => b.id === row.id), `ledger row names unknown baseline ${row.id}`).toBe(true);
      if (row.from !== null) expect(row.to).toBeGreaterThan(row.from);
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
    // A shallow checkout would make `git show <base>:<file>` fail for every baseline — which is now an
    // error rather than a silent all-green, but depth is still what makes the guard useful at all.
    expect(ratchetGuardJob()).toContain("fetch-depth: 0");
  });

  it("passes a base revision for both trigger shapes, so neither push nor PR runs uncompared", () => {
    const job = ratchetGuardJob();
    expect(job).toContain("github.event.pull_request.base.sha");
    expect(job).toContain("github.event.before");
  });
});

describe("the written guidance matches the code", () => {
  it("CLAUDE.md and RATCHETS.md both say the row must be NEW in this diff (F2)", () => {
    const claude = readFileSync(join(ROOT, "CLAUDE.md"), "utf8");
    const doc = readFileSync(join(ROOT, LEDGER_PATH), "utf8");
    // The first round's wording said "the same diff adds a row" while the code accepted any row in the
    // working tree, base included. The docs and the code have to agree, or the doc is the lie.
    expect(claude).toContain("not already present at the base");
    expect(doc).toContain("not already present at the base");
  });

  it("RATCHETS.md records the counts-not-identities limitation", () => {
    expect(readFileSync(join(ROOT, LEDGER_PATH), "utf8")).toContain("counts, not identities");
  });
});
