// CPE-1966 — an adversarial sweep over `src/lib/jsSource.mjs`'s comment stripper.
//
// WHY THIS IS IN THE REPO. Round 5 ran a sweep like this, reported "1,904 structured + 36,861 fuzzed
// parseable inputs, 0 desyncs, no third family", and did not commit the script. The next reviewer
// could not re-run it, wrote their own, and found a third family in minutes. A negative result that
// nobody else can reproduce is not evidence — it is a claim. So the generator lives here, runs in a
// few seconds, and can be pointed at any two revisions of the stripper.
//
// WHAT A GREEN RUN DOES AND DOES NOT MEAN. It means: no input THIS GENERATOR PRODUCES parses before
// stripping and fails to parse after. It says nothing whatever about JavaScript at large — a shape
// the generator cannot express is simply absent, and 38,765 samples missed a 27-character input in
// round 5. Treat the output as a floor on coverage, never as proof of absence. The only leg that
// speaks for shapes nobody enumerated is compiling the RESULT (`stripScriptBodiesChecked`).
//
// Run:  node scripts/dev-harness/js-strip-sweep.mjs
//       node scripts/dev-harness/js-strip-sweep.mjs --all
//       node scripts/dev-harness/js-strip-sweep.mjs --compare <git-ref> [--all]
//
// `--all` lifts the output cap. Without it a plain run prints the first 20 desyncs and a "... and N
// more" line, so re-running the tool re-takes the TOTAL but not the per-family split that any triage
// note quotes — you would have had to edit the slice to check it. Use `--all` whenever you are
// reproducing a split rather than a count.
//
// `--compare <git-ref>` needs a ref that resolves in a FRESH CLONE. Do not paste a raw SHA from this
// branch: every one of them is rewritten by a rebase onto main, and a SHA that still resolves in your
// working clone may only be a loose object left behind by an older worktree. Address a round by its
// commit subject instead, which survives rebasing:
//   node scripts/dev-harness/js-strip-sweep.mjs \
//     --compare "$(git log -1 --format=%H --grep='CPE-1966 round 4' origin/main..HEAD)"
// Two caveats on that addressing, stated rather than designed around. `--grep` searches commit
// BODIES as well as subjects and `-1` takes the NEWEST match, so a later commit whose body quotes
// "CPE-1966 round 4" would silently resolve to the wrong revision. And `origin/main..HEAD` is empty
// once this branch merges — after that, widen the range (or drop it) or the command resolves to
// nothing at all rather than to the wrong thing.
//
// `--compare` extracts the stripper from <git-ref> into a temp module and runs both head to head, so
// a change can be scored as "N fixed, M regressed" instead of asserted to be an improvement. That
// comparison is how round 6 found that round 5's paren accounting fixed 45 shapes and broke 4.
//
// WHAT A LISTED DESYNC IS, AND WHICH TABLE TO TRIAGE IT AGAINST. Every input here PARSES before
// stripping — `corpus()` keeps nothing else — so **everything this tool lists is valid JavaScript
// being corrupted**. In particular nothing it lists can be a `DELETING_GAPS` entry: `jsSource.test.ts`
// derives that no entry in that table parses, which makes the intersection with this corpus empty by
// construction. The tables to check a desync against are the two that hold shapes which delete VALID
// JavaScript — `DELETING_ON_VALID_JS` and `DELETING_ON_VALID_JS_CONTEXTUAL`. A desync matching
// neither is a family nobody has written down yet; add it there.
//
// (Rounds 1-6 of this file said the opposite — "check it against `DELETING_GAPS`… a declared gap is
// expected to show up here" — which sent the reader to the one table it is impossible to hit.)
//
// EXIT CODE. This is an EXPLORATION tool, not a gate — the gate is `src/lib/jsSource.test.ts`. A plain
// run exits 0 even when it lists desyncs, because the declared-and-mitigated families above show up
// here on every run and duplicating them into this file would just be a second copy to rot. What
// makes shipping those families survivable is not this exit code, it is `stripScriptBodiesChecked`
// throwing on the result. `--compare` exits 1 on a REGRESSION — a shape the older revision got right
// and this one does not — which is the question the tool exists to answer.

import vm from "node:vm";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");
const STRIPPER = path.join(REPO_ROOT, "src", "lib", "jsSource.mjs");

const parses = (s) => {
  try {
    new vm.Script(s);
    return true;
  } catch {
    return false;
  }
};

// ── The generated space ──────────────────────────────────────────────────────────────────────────
//
// Every context is a place a REGEX LITERAL can legally start, which is where regex-vs-division goes
// wrong. `%R%` is the hole. The list is grouped by the mechanism each group exercises so a gap in
// coverage is visible as a missing group rather than invisible as a missing string.

/** Statement and expression positions a regex can open in. */
const PLAIN = [
  "%R%.test(s);", "return %R%.test(s);", "typeof %R%;", "void %R%;", "throw %R%;", "delete %R%.x;",
  "new RegExp(%R%.source);", "const r = %R%;", "const r = (%R%);", "const r = [%R%];",
  "const r = {k: %R%};", "f(%R%);", "f(a, %R%);", "x = a ? %R% : b;", "x = a, %R%;",
  "x = a || %R%.test(s);", "x = !%R%.test(s);", "l: %R%.test(s);", "{ %R%.test(s); }",
  "const f2 = () => %R%.test(s);", "const f2 = (a) => %R%.test(a);", "const f2 = a => %R%.test(a);",
  "function* g(){ yield %R%; }", "class C { m() { return %R%.test(s); } }",
  "try { %R%.test(s); } catch (e) { %R%.test(e); }", "try { a(); } finally { %R%.test(s); }",
  "switch(x){case %R%: break;}", "switch(x){default: %R%.test(s); break;}",
  "do %R%.test(s); while(0);", "`t${ %R%.test(s) }`;", "`t${ (x) ? %R% : y }`;",
];

/** A regex after a control-statement CONDITION — round 4's family. */
const CONDITIONS = [
  "if (x) %R%.test(s);", "if (x) %R%.test(s); else %R%.test(s);", "while (x) %R%.test(s);",
  "with (o) %R%.test(s);", "for (;;) { %R%.test(s); break; }", "for (const v of l) %R%.test(v);",
  "for (const k in o) %R%.test(k);", "if (f(x)) %R%.test(s);", "if (a && b) %R%.test(s);",
  "if ((a)) %R%.test(s);", "l: if (x) %R%.test(s);", "if (x) /* c */ %R%.test(s);",
  "if (x) // c\n%R%.test(s);", "for /* c */ (const v of l) %R%.test(v);", "if\n(x)\n%R%.test(s);",
  "if (/(a)/.test(s)) %R%.test(s);", "if (x) { } %R%.test(s);",
];

/** `for await` — round 5's family. Only whitespace, comments and `await` may precede the `(`. */
const AWAIT = [
  "async function g(y){ for await (const x of y) %R%.test(x); }",
  "async function g(y){ for await /* c */ (const x of y) %R%.test(x); }",
  "async function g(y){ for /* c */ await (const x of y) %R%.test(x); }",
  "async function g(y){ for\nawait\n(const x of y) %R%.test(x); }",
  "async function g(y){ l: for await (const x of y) %R%.test(x); }",
  "async function g(y){ for await (const x of y) { %R%.test(x); } }",
  "async function g(){ return %R%.test(await p); }",
  "async function g(){ await p; %R%.test(s); }",
  "async function g(){ await (p); %R%.test(s); }",
  "async function g(s2){ await %R%.test(s2); }",
];

/**
 * Conditions whose own text can be MIS-READ as a regex literal, so the scan swallows source parens.
 *
 * `}` is a punctuator, so `{} / …` opens a regex-shaped scan. Where that scan terminates decides
 * everything, and the three cases are genuinely different — round 5's fix covers the first two and
 * cannot touch the third:
 *   * it stops INSIDE the parens        -> parens are swallowed, accounting repairs the stack;
 *   * it stops after a `)` and a `(`    -> both are swallowed, and their KINDS matter (round 6);
 *   * it stops on the FOLLOWING regex's opening `/` -> the tail is consumed and the comment branch
 *     is reached before any paren state is read. This one DELETES VALID JAVASCRIPT — it is not a
 *     gap, and it is pinned in `DELETING_ON_VALID_JS`, not `DELETING_GAPS`. (Per the header: every
 *     input here parses, so nothing this tool lists can be a `DELETING_GAPS` entry.)
 */
const MISREAD = [
  "if ({} / f(1 / 2)) %R%.test(s);", "if (a[0] / f(1 / 2)) %R%.test(s);",
  "if (f(1 / 2) / f(3)) %R%.test(s);", "if ({} / f(1) / 2) %R%.test(s);",
  "if (g({} / a) / 2) %R%.test(s);", "if ({} / a) if (b / c) %R%.test(s);",
  "if ({} / a) while (b / c) %R%.test(s);", "if ({} / a) for (;b / c;) %R%.test(s);",
  "while ({} / a) if (b / c) %R%.test(s);", "if ({} / a) { } if (b / c) %R%.test(s);",
  "if (f({} / a) / 2) if (b / c) %R%.test(s);",
  // The third family: the scan runs off the end of the condition and terminates on the OPENING `/`
  // of the following regex literal. Nothing to do with parens; these delete VALID JavaScript and are
  // pinned in DELETING_ON_VALID_JS (jsSource.test.ts), never in DELETING_GAPS.
  "if ({} / a) %R%.test(s);", "while ({} / a) %R%.test(s);", "if ({} / a) %R%.test(s);\nconst z = 1;",
  "for (;{} / a;) %R%.test(s);", "if (({}) / a) %R%.test(s);",
];

/** Shapes with no regex hole at all — division, comments, ASI, template nesting. */
const NO_HOLE = [
  "a = b\n/re/.test(c);", "a[0], /re/.test(s);", "f(x), /re/.test(s);", "obj.if(2), /re/.test(s);",
  "f?.(2), /re/.test(s);", "const o2 = {}; /re/.test(s);", "while (i--) total /= 2; // c",
  "const n = \"5\" / 2; // c", "`a${ \"`\" }b`; // c", "`a${x /* c */}b`",
  "if ({} / a) / b; if (x) /re/.test(s);", "a/*x*/b", "const u = \"http://x\"; // c",
];

const CONTEXTS = [...PLAIN, ...CONDITIONS, ...AWAIT, ...MISREAD];

/** Regex literals: the deleting ones hide a comment opener in a character class. */
const REGEXES = [
  "/[//]/", "/[/*]/", "/[//(]/", "/[/*)]/", "/re/", "/a\\/b/", "/[/]/g", "/[(]/", "/[)]/",
  "/(a)/", "/\\(/", "/[[]/", "/a{1,2}/", "/(?:a\\/\\/b)/",
];

const PRELUDE =
  "var s='a', x=1, a=1, b=2, c=3, l=[], o={}, p=1, i=1, k=0, y=[];\n" +
  "function f(){return 1;}\nfunction g(v){return v;}\nvar obj={if(){return 1;}};\n";

/** Every generated input, deduped, keeping only those that are valid JavaScript to begin with. */
function corpus() {
  const seen = new Set();
  for (const ctx of [...CONTEXTS.flatMap((c) => REGEXES.map((r) => c.split("%R%").join(r))), ...NO_HOLE]) {
    for (const src of [ctx, PRELUDE + ctx]) if (parses(src)) seen.add(src);
  }
  // A token fuzzer on top, for shapes nobody listed. Deterministic so a failure is reproducible.
  const TOKENS = [
    "if", "for", "while", "with", "await", "async", "function", "return", "typeof", "of", "in",
    "new", "case", "do", "else", "yield", "throw", "void", "delete", "const", "let", "var", "class",
    "try", "catch", "finally", "switch", "default", "g", "x", "y", "s", "a", "b", "l", "o", "f",
    "(", ")", "{", "}", "[", "]", ";", ",", ".", ":", "?", "=", "=>", "+", "-", "*", "/", "/=", "!",
    "&&", "||", "<", ">", "`", "'", '"', "\\", "\n", " ", "$", "${", "//", "/*", "*/",
    "/[//]/", "/[/*]/", "/re/", "'str'", '"str"', "`tpl`", "`a${b}c`", "0", "1",
  ];
  // mulberry32, with Math.imul so the state stays a true uint32. A plain `rng * 1103515245 + 12345`
  // in JS doubles loses the low bits and degenerates: measured, it produced 400,000 draws that
  // deduped to about 120 distinct programs, so the "36,861 parseable inputs" round 5 reported was
  // almost entirely the same handful of strings counted over and over.
  let rng = 0x2f6e2b1;
  const rand = () => {
    rng = (rng + 0x6d2b79f5) | 0;
    let t = Math.imul(rng ^ (rng >>> 15), 1 | rng);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  for (let n = 0; n < 400000; n++) {
    let src = "";
    const len = 2 + Math.floor(rand() * 14);
    for (let t = 0; t < len; t++) src += TOKENS[Math.floor(rand() * TOKENS.length)];
    if (parses(src)) seen.add(src);
  }
  return [...seen];
}

/** Inputs this stripper turns from parseable into unparseable. */
function desyncs(strip, inputs) {
  const out = new Map();
  for (const src of inputs) {
    let stripped;
    try {
      stripped = strip(src);
    } catch (e) {
      out.set(src, `threw: ${e instanceof Error ? e.message : String(e)}`);
      continue;
    }
    if (!parses(stripped)) out.set(src, `${stripped.length - src.length}`);
  }
  return out;
}

/**
 * Temp dirs to remove on the way out. ONE `exit` listener for all of them, registered once at module
 * scope: registering it inside `stripperAt`'s `finally` added a listener per call, which is a leak
 * the moment anything calls that function more than once (round 7's nit; one call today).
 *
 * An imported ES module cannot be unloaded, so the directory has to outlive the import and can only
 * be cleaned up at exit.
 */
const TEMP_DIRS = [];
process.on("exit", () => {
  for (const dir of TEMP_DIRS) { try { rmSync(dir, { recursive: true, force: true }); } catch {} }
});

/** The stripper as it was at `ref`, loaded from a temp file (the module has no runtime deps). */
async function stripperAt(ref) {
  const dir = mkdtempSync(path.join(tmpdir(), "cpe-jsstrip-"));
  TEMP_DIRS.push(dir);
  const file = path.join(dir, "jsSource.mjs");
  writeFileSync(file, execFileSync("git", ["show", `${ref}:src/lib/jsSource.mjs`], {
    cwd: REPO_ROOT, encoding: "utf8", maxBuffer: 1 << 24,
  }));
  return (await import(pathToFileURL(file).href)).stripJsComments;
}

const compareTo = process.argv.includes("--compare")
  ? process.argv[process.argv.indexOf("--compare") + 1]
  : null;
// Without `--all` the listing is capped, which re-takes a total but not a split. See the header.
const CAP = process.argv.includes("--all") ? Infinity : 20;
const PRE_EXISTING_CAP = process.argv.includes("--all") ? Infinity : 10;

const { stripJsComments } = await import(pathToFileURL(STRIPPER).href);
const inputs = corpus();
if (inputs.length < 1000) {
  console.error(`GENERATOR BROKEN — only ${inputs.length} parseable inputs; expected thousands.`);
  process.exit(2);
}
const now = desyncs(stripJsComments, inputs);
let regressions = 0;

console.log(`${inputs.length} deduped parseable inputs generated`);
console.log(`working tree: ${now.size} desync(s) (parsed in, does NOT parse out)`);
console.log("(every input above PARSES before stripping, so every desync listed is VALID JavaScript being");
console.log(" corrupted — none of them can be a DELETING_GAPS entry, since no entry in that table parses.");
console.log(" Triage against DELETING_ON_VALID_JS and DELETING_ON_VALID_JS_CONTEXTUAL in");
console.log(" src/lib/jsSource.test.ts; a desync matching neither is a family nobody has tabled yet.)");

if (compareTo) {
  const old = desyncs(await stripperAt(compareTo), inputs);
  const fixed = [...old.keys()].filter((s) => !now.has(s));
  const broke = [...now.keys()].filter((s) => !old.has(s));
  const both = [...now.keys()].filter((s) => old.has(s));
  regressions = broke.length;
  console.log(`${compareTo}: ${old.size} desync(s)`);
  console.log(`\n  fixed     (${compareTo} broken, working tree clean): ${fixed.length}`);
  console.log(`  REGRESSED (${compareTo} clean, working tree broken):  ${broke.length}`);
  console.log(`  broken in BOTH (pre-existing):                        ${both.length}`);
  for (const s of broke) console.log(`\n  REGRESSION ${JSON.stringify(s)}\n    delta ${now.get(s)}`);
  for (const s of both.slice(0, PRE_EXISTING_CAP)) console.log(`\n  PRE-EXISTING ${JSON.stringify(s)}\n    delta ${now.get(s)}`);
  if (both.length > PRE_EXISTING_CAP) {
    console.log(`\n  ... and ${both.length - PRE_EXISTING_CAP} more pre-existing (re-run with --all to list them)`);
  }
} else {
  for (const [s, d] of [...now].slice(0, CAP)) console.log(`\n  DESYNC ${JSON.stringify(s)}\n    delta ${d}`);
  if (now.size > CAP) console.log(`\n  ... and ${now.size - CAP} more (re-run with --all to list them)`);
}

process.exit(regressions ? 1 : 0);
