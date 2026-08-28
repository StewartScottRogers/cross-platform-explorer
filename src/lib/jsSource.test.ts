/**
 * CPE-1966 round 3, blocker 2 — the tests `stripJsComments` did not have.
 *
 * The stripper spent two rounds private to `scripts/dev-harness/launcher-contrast/engine.mjs`: this
 * repo's SIXTH hand-rolled one, imported nowhere, and exercised only by "the provenance check passed"
 * in a single CI job. A Reviewer ran 31 adversarial shapes at it; 7 were wrong, and 4 of those 7
 * DELETED real code. `shellScriptLines.ts` and `rustSource.ts` each carry a `.test.ts`; this is the
 * JS one, and every shape below — fixed or still a gap — is a case here rather than a paragraph.
 *
 * (That 31/7/4 tally is the Reviewer's round-2 count, recorded as provenance. It has never been
 * independently re-run and there is no artefact to re-run it from, so it is HISTORY rather than a
 * derived figure — CLAUDE.md → "if a claim is genuinely underivable, say at the site that it is
 * unverified and why". The seven wrong shapes themselves are not folklore: each is a named case
 * below, and each is asserted at its exact output.)
 *
 * ## The oracle that covers the shapes nobody thought of
 *
 * Case tables only ever contain what someone imagined (CLAUDE.md → "a shared case file catches
 * divergence, not shared blindness"). So every case that parses as JavaScript before stripping is
 * ALSO required to parse after: a stripper that deletes code overwhelmingly leaves something
 * unparseable behind, so `vm.Script` catches the whole FALSE-STRIP family without anyone naming its
 * members. That is the same oracle `stripScriptBodiesChecked` applies to launcher.html, and
 * it is red-proofed at the bottom of this file with a stripper that really does delete.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import vm from "node:vm";
import { htmlScriptBodies, stripJsComments, stripScriptBodiesChecked } from "./jsSource.mjs";
// The harness is the module that USES all of the above, and the `sessionChipColours` half of round
// 3's blocker 2 is about what it points the scanner at, so it is exercised here rather than described.
import { sessionChipColours } from "../../scripts/dev-harness/launcher-contrast/engine.mjs";

const LAUNCHER = join(process.cwd(), "sidecar/ai-console/src/launcher.html");

function parses(src: string): boolean {
  try {
    new vm.Script(src);
    return true;
  } catch {
    return false;
  }
}

/** input -> exact expected output, with why the shape is here. */
type Case = { name: string; input: string; want: string };

/**
 * FALSE-STRIP — real code silently deleted. All four had ONE root cause: the scanner tracked a single
 * previous CHARACTER, so every keyword ended in a word char, matched its "value-shaped" class, and
 * the regex branch was skipped — at which point the `/` inside a character class opened a comment.
 * `return /[/*]/;` was the worst of them: the `/*` ate everything to the next `*​/`, possibly pages away.
 */
const FALSE_STRIP: Case[] = [
  { name: "`return` + a regex whose class contains `//`", input: "return /[//]/;", want: "return /[//]/;" },
  { name: "`typeof` + the same", input: "typeof /[//]/;", want: "typeof /[//]/;" },
  {
    name: "`case` + the same, inside a switch",
    input: "switch(x){case /[//]/: break;}",
    want: "switch(x){case /[//]/: break;}",
  },
  {
    name: "`return` + a regex whose class contains `/*` — this one used to eat MANY lines",
    input: "return /[/*]/;\nconst survivor = 1;\nconst alsoSurvives = 2;",
    want: "return /[/*]/;\nconst survivor = 1;\nconst alsoSurvives = 2;",
  },
];

/**
 * The 191-character fixture round 4 was blocked on: valid JavaScript in, **144 characters deleted**,
 * unparseable out. Kept whole rather than reduced to its one line, because the deletion running past
 * the end of the function and taking the next two statements with it is the part that matters.
 */
const ROUND4_BLOCKER = `function looksLikePath(s) {
  if (s.length) /[/*]/.test(s);
  return s;
}
const SESSION_CHIP_COLORS = ["#3a72b5", "#3a9d4a"];
function sessionColor(id) { return SESSION_CHIP_COLORS[id % 2]; }`;

/**
 * FALSE-STRIP, round 4 — the SAME defect as the four above, reached through `)` instead of a keyword.
 *
 * Round 3 fixed the keyword prefix and then *documented* `)` as a gap that "fails toward keeping
 * source rather than deleting it". That was false, and false in the one direction the whole module
 * claims to be safe in: `if (s.length) /[/*]/.test(s);` is valid JavaScript, the `/` after `)` was
 * read as division, the `[` was emitted as an ordinary character, and the next `/` reached the
 * comment branches and invented a `/*` that ate the rest of the file. The declared gap was standing
 * next to a green test, which is exactly CPE-1933's failure shape.
 *
 * Fixed by deciding `)` from what its `(` opened (`CONTROL_PAREN`), not from the `)` itself.
 */
const FALSE_STRIP_PAREN: Case[] = [
  {
    name: "`if (…)` + a regex whose class contains `//` — the round-4 blocker, minimal form",
    input: "if (x) /[//]/.test(s);",
    want: "if (x) /[//]/.test(s);",
  },
  {
    name: "`if (…)` + a regex whose class contains `/*` — this one ate to the next `*/`",
    input: "if (x) /[/*]/.test(s);\nconst survivor = 1;\nconst alsoSurvives = 2;",
    want: "if (x) /[/*]/.test(s);\nconst survivor = 1;\nconst alsoSurvives = 2;",
  },
  {
    name: "`while (…)` and `for (…)` conditions read the same way",
    input: "while (x) /[//]/.test(s);\nfor (const v of l) /[//]/.test(v);",
    want: "while (x) /[//]/.test(s);\nfor (const v of l) /[//]/.test(v);",
  },
  {
    name: "the Reviewer's 191-character fixture, verbatim — was -144 characters",
    input: ROUND4_BLOCKER,
    want: ROUND4_BLOCKER,
  },
];

/**
 * FALSE-STRIP, round 5a — `for await`, the ONE token the grammar lets sit between a control word and
 * its `(`.
 *
 * Round 4 decided a `)` by what its `(` opened, and tracked that with a `prevKind` of `"control"` set
 * by the control word. Comments between `for` and `(` are transparent (their branches write neither
 * `prevKind` nor `prevPunct`) — but `await` is a WORD, and it is also in `REGEX_AFTER`, so the word
 * branch overwrote `"control"` with `"keyword"` and pushed `false`. The `)` then resolved to a value,
 * the `/` after it read as division, and the character class hid a comment opener.
 *
 * Measured against round 4 (`ca25be56`) before the fix: `-14` on the `//` form, `-50` on the `/*` one,
 * parseable in and unparseable out in both. Fixed by letting `"control"` survive `await`.
 */
const FALSE_STRIP_AWAIT: Case[] = [
  {
    name: "`for await (…)` + a `//` class — was -14 characters, parseable in, unparseable out",
    input: "async function f(y){ for await (const x of y) /[//]/.test(x); }",
    want: "async function f(y){ for await (const x of y) /[//]/.test(x); }",
  },
  {
    name: "`for await (…)` + a `/*` class — was -50, eating the two statements after the function",
    input:
      "async function f(y){ for await (const x of y) /[/*]/.test(x); }\nconst survivor = 1;\nconst also = 2;",
    want:
      "async function f(y){ for await (const x of y) /[/*]/.test(x); }\nconst survivor = 1;\nconst also = 2;",
  },
  {
    name: "comments on either side of `await` are transparent, and the condition still reads as one",
    input: "async function f(y){ for /* c */ await /* c */ (const x of y) /[//]/.test(x); }",
    want: "async function f(y){ for   await   (const x of y) /[//]/.test(x); }",
  },
  {
    name: "the same inside a `${…}` — the frame's own paren stack resolves it",
    input: "async function f(y){ `${ (async () => { for await (const x of y) /[//]/.test(x); })() }`; }",
    want: "async function f(y){ `${ (async () => { for await (const x of y) /[//]/.test(x); })() }`; }",
  },
  // Blast radius: `await` must NOT become a control word in general, and the benign `/re/` form of
  // `for await` must be untouched either way.
  {
    name: "BLAST RADIUS: `await` NOT after a control word is still a plain regex prefix, not a condition",
    input: "async function f(s){ await /[//]/.test(s); }",
    want: "async function f(s){ await /[//]/.test(s); }",
  },
  {
    name: "BLAST RADIUS: `for await (…)` followed by an ordinary regex",
    input: "async function f(y){ for await (const x of y) /re/.test(x); }",
    want: "async function f(y){ for await (const x of y) /re/.test(x); }",
  },
];

/**
 * FALSE-STRIP, round 5b — a MIS-READ regex swallowing the source's own parens.
 *
 * New-shaped rather than newly-broken: round 4 introduced the paren stack that can desynchronise.
 * `}` is a punctuator, so `{} / f(1 / 2)` scans as a regex literal `/ f(1 /`; the `(` inside it was
 * consumed by the regex branch and never pushed, so the condition's own `)` popped nothing and the
 * OUTER `)` took the `true` meant for it.
 *
 * Fixed by accounting for every paren the regex branch consumes, whichever branch consumes it. It is a
 * no-op for a real regex literal — its unescaped, out-of-class parens are balanced, and an unbalanced
 * one is a SyntaxError — and it is exactly right for a mis-read division, which really did eat those
 * parens. Measured against round 4 before the fix: `-14`, parseable in, unparseable out, in both the
 * swallowed-`(` and swallowed-`)` directions.
 */
const FALSE_STRIP_EATEN_PAREN: Case[] = [
  {
    name: "a mis-read regex that swallows an unmatched `(` — was -14, the outer `)` stole the condition",
    input: "function f(x){return x} if ({} / f(1 / 2)) /[//]/.test('a');",
    want: "function f(x){return x} if ({} / f(1 / 2)) /[//]/.test('a');",
  },
  {
    name: "and one that swallows an unmatched `)` — the other direction, also -14",
    input: "function g(a){return a} if (g({} / a) / 2) /[//]/.test('b');",
    want: "function g(a){return a} if (g({} / a) / 2) /[//]/.test('b');",
  },
  {
    name: "BLAST RADIUS: a REAL regex's balanced parens leave the condition stack untouched",
    input: "if (/(a)/.test(s)) /[//]/.test(s);",
    want: "if (/(a)/.test(s)) /[//]/.test(s);",
  },
  {
    name: "BLAST RADIUS: a mis-read regex whose swallowed parens balance was already right",
    input: "function f(x){return x} if ({} / f(1) / 2) /[//]/.test('a');",
    want: "function f(x){return x} if ({} / f(1) / 2) /[//]/.test('a');",
  },
];

/**
 * FALSE-KEEP — a comment left in place. Harmless to a parse, NOT harmless to `includes(claim)`:
 * round 1's whole defect was a provenance claim satisfied by a comment quoting the old value.
 */
const FALSE_KEEP: Case[] = [
  {
    name: "a backtick inside a nested string used to close the template early",
    input: '`a${ "`" }b`; // c',
    want: '`a${ "`" }b`;  ',
  },
  {
    name: "a comment inside a `${}` substitution — substitutions were never re-scanned",
    input: "`a${x /* c */}b`",
    want: "`a${x  }b`",
  },
  {
    name: "division after a STRING literal was read as a regex, swallowing the trailing comment",
    input: 'const n = "5" / 2; // c',
    want: 'const n = "5" / 2;  ',
  },
];

/** Shapes that were already right — two of them only BY LUCK, which is worth pinning as such. */
const ALREADY_RIGHT: Case[] = [
  {
    name: "LUCKY: `return /a\\/\\/b/` — the `\\` before the `/` re-entered the regex branch",
    input: "return /a\\/\\/b/;",
    want: "return /a\\/\\/b/;",
  },
  {
    name: "LUCKY: `return /a\\/\\*b/` — same accident",
    input: "return /a\\/\\*b/;",
    want: "return /a\\/\\*b/;",
  },
  { name: "a `//` inside a string is not a comment", input: 'const u = "http://x"; // c', want: 'const u = "http://x";  ' },
  { name: "a `/* */` inside a string is not a comment", input: "const s = '/* keep */'; // c", want: "const s = '/* keep */';  " },
  { name: "plain division chains", input: "a / b / c; // c", want: "a / b / c;  " },
  { name: "a regex with flags", input: "const r = /x/gi; // c", want: "const r = /x/gi;  " },
  { name: "a keyword used as a property name is a value, so `/` is division", input: "obj.return / 2; // c", want: "obj.return / 2;  " },
  { name: "a `//` inside a template literal is text", input: "const t = `line // text`; // c", want: "const t = `line // text`;  " },
  { name: "nested templates", input: "const t = `a ${ `b ${ 1 } c` } d`; // c", want: "const t = `a ${ `b ${ 1 } c` } d`;  " },
  { name: "an escaped quote inside a string", input: "const q = 'it\\'s'; // c", want: "const q = 'it\\'s';  " },
  { name: "a trailing comment after code on the same line", input: "const a = 1; // trailing", want: "const a = 1;  " },
  { name: "a block comment between tokens becomes ONE space, never a join", input: "a/*x*/b", want: "a b" },
  // The blast radius of round 4's `CONTROL_PAREN` change: everything a `)` can be followed by that
  // is NOT a regex. A real comment after a condition, a division after one, a `)` that closes a
  // call or an arrow's parameter list, and a control word demoted by a `.` in front of it.
  { name: "a REAL block comment after a condition is still a comment", input: "while (x) /* c */ y();", want: "while (x)   y();" },
  { name: "a REAL line comment after a condition is still a comment", input: "if (x) // c\ny();", want: "if (x)  \ny();" },
  { name: "division after a condition is still division", input: "if (a) b / c; // c", want: "if (a) b / c;  " },
  { name: "`/=` after a condition is still an operator", input: "while (i--) total /= 2; // c", want: "while (i--) total /= 2;  " },
  { name: "a NESTED `)` closes the call, the outer one closes the condition", input: "if (f(x)) /re/.test(s); // c", want: "if (f(x)) /re/.test(s);  " },
  { name: "an arrow's parameter list is a value, so `/` after it is division", input: "const f = (a) => a / 2; // c", want: "const f = (a) => a / 2;  " },
  { name: "a control word used as a property name is a value", input: "obj.if / 2; // c", want: "obj.if / 2;  " },
  { name: "`catch (e)` and `switch (x)` are not in CONTROL_PAREN and need not be", input: "try { a(); } catch (e) { b(); } // c", want: "try { a(); } catch (e) { b(); }  " },
  { name: "parens inside a `${}` cannot leak into the enclosing code", input: "`a${ (x) / 2 }b`; // c", want: "`a${ (x) / 2 }b`;  " },
];

/**
 * KNOWN GAPS that fail toward KEEPING source — pinned at the behaviour they actually have.
 *
 * Keeping is the defensible direction and the reason it is acceptable to ship a scanner rather than
 * a parser: a kept comment can only ever make a provenance claim FAIL to match (a loud red naming
 * the fixture), while deleted code makes one pass on a mutilated file. If any of these ever flips to
 * deleting, the `parses()` oracle below reds it.
 *
 * Round 4 is the reason this table is now split in two. "All of which fail toward keeping" was
 * written here and in `jsSource.mjs` as a property of the whole gap list, and it was untrue of the
 * `)` entry — whose case in this table happened to be the benign `/re/` form, so the oracle iterated
 * it and agreed. The deleting shapes live in `DELETING_GAPS` below, named as what they are.
 */
const KNOWN_GAPS: Case[] = [
  {
    name: "GAP: a regex after a `)` that closed a CALL is division — the text survives verbatim",
    input: "f(x) /re/.test(s); // c",
    want: "f(x) /re/.test(s);  ",
  },
  {
    name: "GAP: a regex directly after `]` is read as division — the text survives verbatim",
    input: "a[0] /re/.test(s); // c",
    want: "a[0] /re/.test(s);  ",
  },
  {
    name: "GAP: no ASI awareness — a regex opening a line after a value is division; text survives",
    input: "const a = b\n/re/.test(c) // c",
    want: "const a = b\n/re/.test(c)  ",
  },
  {
    name: "GAP: an unterminated string stops at the newline rather than swallowing the file",
    input: "const bad = 'oops\nconst next = 1; // c",
    want: "const bad = 'oops\nconst next = 1;  ",
  },
];

/**
 * KNOWN GAPS that DELETE — the honest half, and the reason the bare stripper is not the entry point.
 *
 * When a mis-read regex's character class hides a `//` or a `/*`, the emitted `/` and `[` are
 * ordinary characters and the next `/` reaches the comment branches: a comment is invented and the
 * region is deleted, to end of line or to the next `*​/`. Round 3 fixed this for keyword prefixes and
 * round 4 for control-statement conditions; what is left is `]` and a call's `)`.
 *
 * **Say exactly what the test below derives, and no more: NO ENTRY IN THIS TABLE PARSES before
 * stripping.** That is a statement about `DELETING_GAPS`, not about JavaScript. It is checked with
 * `vm.Script` rather than written down, so an entry added later that *does* parse reds — but a
 * deleting shape nobody has added here is not covered by it, and no green run in this file can be
 * read as saying one does not exist.
 *
 * Round 5 is why that distinction is spelled out. Round 4 wrote "neither is reachable from valid
 * JavaScript" over this list, next to this green filter, and two parseable inputs still deleted:
 * `for await (const x of y) /[//]/…` (-14) and `if ({} / f(1 / 2)) /[//]/…` (-14). Both are fixed
 * above. Generalising a per-entry derivation into a property of the whole mechanism is round 3's
 * defect one level up, and the module header names that failure shape.
 *
 * What IS true of each entry here, individually and by inspection: real JS reads that `/` as division
 * too, so `a[0] / [//]/…` opens an array literal the `//` comments away, and the input was already
 * broken before the stripper saw it. That is a much weaker safety property than "fails toward
 * keeping", which is the point of listing them separately: the `vm.Script` oracle is blind here by
 * construction, because it only asks about inputs that parsed.
 */
const DELETING_GAPS: Case[] = [
  {
    name: "DELETES: `]` + a class containing `//` — to end of line",
    input: "a[0] /[//]/.test(s);\nconst survivor = 1;",
    want: "a[0] /[ \nconst survivor = 1;",
  },
  {
    name: "DELETES: a call's `)` + a class containing `/*` — to the next `*/`, here the end of input",
    input: "f(x) /[/*]/.test(s);\nconst survivor = 1;\nconst also = 2;",
    want: "f(x) /[ ",
  },
  {
    name: "DELETES: the ASI gap's variant of the same shape",
    input: "const a = b\n/[//]/.test(c);\nconst survivor = 1;",
    want: "const a = b\n/[ \nconst survivor = 1;",
  },
];

describe("stripJsComments — the four shapes that used to DELETE code (CPE-1966 round 3)", () => {
  for (const c of FALSE_STRIP) {
    it(c.name, () => {
      expect(stripJsComments(c.input)).toBe(c.want);
    });
  }
});

describe("stripJsComments — the `)` shapes that used to DELETE code (CPE-1966 round 4)", () => {
  for (const c of FALSE_STRIP_PAREN) {
    it(c.name, () => {
      expect(stripJsComments(c.input)).toBe(c.want);
    });
  }

  it("the Reviewer's fixture is valid JavaScript, so the old behaviour really was a deletion", () => {
    // Derived, not asserted (CPE-1933): the claim "144 characters of real code were deleted" is only
    // meaningful if the input was real code. Both halves are measured here.
    expect(parses(ROUND4_BLOCKER)).toBe(true);
    expect(ROUND4_BLOCKER.length).toBe(191);
    const out = stripJsComments(ROUND4_BLOCKER);
    expect(out.length - ROUND4_BLOCKER.length).toBe(0);
    expect(parses(out)).toBe(true);
  });
});

describe("stripJsComments — `for await`, the token that used to break the condition (CPE-1966 round 5)", () => {
  for (const c of FALSE_STRIP_AWAIT) {
    it(c.name, () => {
      expect(stripJsComments(c.input)).toBe(c.want);
    });
  }
});

describe("stripJsComments — a mis-read regex desynchronising the paren stack (CPE-1966 round 5)", () => {
  for (const c of FALSE_STRIP_EATEN_PAREN) {
    it(c.name, () => {
      expect(stripJsComments(c.input)).toBe(c.want);
    });
  }
});

describe("stripJsComments — the three shapes that used to KEEP a comment", () => {
  for (const c of FALSE_KEEP) {
    it(c.name, () => {
      expect(stripJsComments(c.input)).toBe(c.want);
    });
  }
});

describe("stripJsComments — shapes that were already right", () => {
  for (const c of ALREADY_RIGHT) {
    it(c.name, () => {
      expect(stripJsComments(c.input)).toBe(c.want);
    });
  }
});

describe("stripJsComments — declared gaps, pinned at their real behaviour", () => {
  for (const c of KNOWN_GAPS) {
    it(c.name, () => {
      expect(stripJsComments(c.input)).toBe(c.want);
    });
  }
});

describe("stripJsComments — the declared gaps that DELETE, and why they stay declared", () => {
  for (const c of DELETING_GAPS) {
    it(c.name, () => {
      expect(stripJsComments(c.input)).toBe(c.want);
      // Deletion is asserted as deletion, not left to be inferred from `want`.
      expect(stripJsComments(c.input).length).toBeLessThan(c.input.length);
    });
  }

  it("no entry IN THIS TABLE parses before stripping — a property of the table, not of JavaScript", () => {
    // The whole justification for shipping these as gaps rather than fixing them, stated at the scope
    // it is actually measured at. If a shape lands in DELETING_GAPS that DOES parse before stripping,
    // this reds — and it should, because that would be a live deletion bug. It says nothing about a
    // deleting shape nobody has written down; round 5 found two of those with this leg green.
    const parseable = DELETING_GAPS.filter((c) => parses(c.input)).map((c) => c.name);
    expect(parseable, "a DELETING gap accepts valid JavaScript — that is a bug, not a gap").toEqual([]);
    expect(DELETING_GAPS.length, "the enumeration measured nothing").toBeGreaterThan(2);
  });
});

describe("stripJsComments — the oracle that does not depend on anyone writing the case", () => {
  const all = [
    ...FALSE_STRIP, ...FALSE_STRIP_PAREN, ...FALSE_STRIP_AWAIT, ...FALSE_STRIP_EATEN_PAREN,
    ...FALSE_KEEP, ...ALREADY_RIGHT, ...KNOWN_GAPS, ...DELETING_GAPS,
  ];

  it("every case that parses before stripping still parses after", () => {
    // A case table only contains what someone imagined. This leg is the one that catches the rest:
    // deleting code leaves unparseable text behind, keeping too much cannot break a parse at all.
    const broke: string[] = [];
    let checked = 0;
    for (const c of all) {
      if (!parses(c.input)) continue;
      checked++;
      if (!parses(stripJsComments(c.input))) broke.push(c.name);
    }
    // A run that parsed nothing is a broken oracle, not a clean bill (CPE-1932).
    expect(checked, "no case in the table is parseable JavaScript — the oracle measured nothing").toBeGreaterThan(10);
    expect(broke, "stripping turned parseable JavaScript into unparseable text").toEqual([]);
  });

  it("no case gains a comment marker it did not already have inside a literal", () => {
    // The FALSE-KEEP direction, measured rather than asserted: after stripping, a `//` or `/*` may
    // only survive where the input had one inside a string, template or regex.
    for (const c of all) {
      const out = stripJsComments(c.input);
      const markers = (out.match(/\/\/|\/\*/g) ?? []).length;
      const inputMarkers = (c.input.match(/\/\/|\/\*/g) ?? []).length;
      expect(markers, `${c.name}: stripping INVENTED a comment marker`).toBeLessThanOrEqual(inputMarkers);
    }
  });
});

describe("launcher-contrast harness — the `vm.Script` desync backstop (CPE-1933 rule 3)", () => {
  const bodies = htmlScriptBodies(readFileSync(LAUNCHER, "utf8"));

  it("launcher.html's six script bodies survive the real stripper", () => {
    expect(bodies.length).toBeGreaterThanOrEqual(6);
    expect(() => stripScriptBodiesChecked(bodies)).not.toThrow();
  });

  it("RED-PROOF: a stripper that deletes code makes the backstop throw", () => {
    // Not a hypothetical. The launcher happens to contain none of the shapes the round-2 stripper
    // mangled, so reinstating that exact bug does NOT red against the real file — which is why the
    // stripper is injectable here. This one opens a line comment at every `/`.
    const deleting = (s: string) => {
      let out = "";
      let i = 0;
      while (i < s.length) {
        if (s[i] === "/") {
          const nl = s.indexOf("\n", i);
          out += " ";
          i = nl === -1 ? s.length : nl;
          continue;
        }
        out += s[i];
        i++;
      }
      return out;
    };
    expect(() => stripScriptBodiesChecked(bodies, deleting)).toThrow(/COMMENT STRIPPER DESYNC/);
  });

  it("a body that never parsed as JavaScript cannot red the run", () => {
    // `<script type="application/json">` and minified bundles are not JS to compile; the backstop
    // only asks whether stripping BROKE something that worked, never whether it was JS to begin with.
    const notJs = ["<<< not javascript >>>"];
    expect(() => stripScriptBodiesChecked(notJs, () => "still <<< not javascript")).not.toThrow();
  });
});

describe("launcher-contrast harness — sessionChipColours reads SCRIPT BODIES, not the document", () => {
  const raw = readFileSync(LAUNCHER, "utf8");
  const inject = (html: string) => raw.replace(/(<body[^>]*>)/, `$1\n${html}`);

  it("a decoy palette in HTML prose, ahead of the real one, is not picked up", () => {
    // The decisive one. Round 2's `sessionChipColours` ran the JS tokenizer over the WHOLE HTML
    // DOCUMENT and took the first match, so prose above the scripts won. Measured on this input, the
    // whole-document read returns "#111111","#222222"; the script-body read returns the real palette.
    const decoy = '<p>const SESSION_CHIP_COLORS = ["#111111", "#222222"] is what it used to be</p>';
    const withDecoy = inject(decoy);
    const real = sessionChipColours(raw);
    expect(real.length).toBeGreaterThanOrEqual(2);
    expect(sessionChipColours(withDecoy)).toEqual(real);
    expect(sessionChipColours(withDecoy)).not.toContain("#111111");

    // And the same input read the old way DOES take the decoy — so this test would fail if the
    // reader ever went back to the whole document. Derived, not claimed.
    const wholeDocument = stripJsComments(withDecoy).match(/const SESSION_CHIP_COLORS = \[([^\]]*)\]/);
    expect(wholeDocument?.[1]).toContain("#111111");
  });

  it("an apostrophe in HTML prose outside every script changes nothing", () => {
    // The shape the Reviewer measured at 11,872 characters of net deletion: `<p>the agent's log</p>`
    // opened a string literal in a JS scanner pointed at HTML. Script bodies cannot contain it.
    //
    // That figure is HISTORY and is deliberately not asserted (round 4). It was taken against round
    // 2's stripper, which swallowed to the next `'` anywhere in the file; round 3 made an
    // unterminated string stop at the newline, so re-measured today the same injection shifts a
    // whole-document strip by 0. What is asserted below is the property that actually matters and
    // that survives any future change to the stripper: the extractor's output does not move.
    const withProse = inject("<p>the agent's log</p>");
    expect(htmlScriptBodies(withProse)).toEqual(htmlScriptBodies(raw));
    expect(sessionChipColours(withProse)).toEqual(sessionChipColours(raw));
  });
});
