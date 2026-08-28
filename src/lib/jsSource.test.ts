/**
 * CPE-1966 round 3, blocker 2 — the tests `stripJsComments` did not have.
 *
 * The stripper spent two rounds private to `scripts/dev-harness/launcher-contrast/engine.mjs`: this
 * repo's SIXTH hand-rolled one, imported nowhere, and exercised only by "the provenance check passed"
 * in a single CI job. A Reviewer ran 31 adversarial shapes at it; 7 were wrong, and 4 of those 7
 * DELETED real code. `shellScriptLines.ts` and `rustSource.ts` each carry a `.test.ts`; this is the
 * JS one, and every shape below — fixed or still a gap — is a case here rather than a paragraph.
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
];

/**
 * KNOWN GAPS — pinned at the behaviour they actually have, with the direction they fail in.
 *
 * Every one fails toward KEEPING source. That is the defensible direction and the reason it is
 * acceptable to ship a scanner rather than a parser: a kept comment can only ever make a provenance
 * claim FAIL to match (a loud red naming the fixture), while deleted code makes one pass on a
 * mutilated file. If any of these ever flips to deleting, the `parses()` oracle below reds it.
 */
const KNOWN_GAPS: Case[] = [
  {
    name: "GAP: a regex directly after `)` is read as division — the text survives verbatim",
    input: "if (x) /re/.test(s); // c",
    want: "if (x) /re/.test(s);  ",
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

describe("stripJsComments — the four shapes that used to DELETE code (CPE-1966 round 3)", () => {
  for (const c of FALSE_STRIP) {
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

describe("stripJsComments — the oracle that does not depend on anyone writing the case", () => {
  const all = [...FALSE_STRIP, ...FALSE_KEEP, ...ALREADY_RIGHT, ...KNOWN_GAPS];

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
    const withProse = inject("<p>the agent's log</p>");
    expect(htmlScriptBodies(withProse)).toEqual(htmlScriptBodies(raw));
    expect(sessionChipColours(withProse)).toEqual(sessionChipColours(raw));
  });
});
