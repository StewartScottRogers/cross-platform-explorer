/**
 * Reading facts out of JavaScript source, for the guards that DERIVE a provenance claim instead of
 * asserting one (CLAUDE.md → "Derive provenance, don't claim it", rule 2: *do not hand-roll the
 * stripper*).
 *
 * ## Why this file exists at all
 *
 * `src/lib/shellScriptLines.ts` owns shell, `src/lib/rustSource.ts` owns Rust, and its header already
 * records that this repo wrote **four** separate hand-rolled strippers before the fifth was caught.
 * CPE-1966's `scripts/dev-harness/launcher-contrast/engine.mjs` made it **six** — a JS comment
 * stripper, private to a `.mjs` harness, imported nowhere, exercised only by "the provenance check
 * passed" in one CI job. JS genuinely needs its own scanner (template literals and the
 * regex-vs-division ambiguity have no shell or Rust analogue), so the answer is not "reuse the Rust
 * one" — it is "put the JS one HERE, with one set of tests", which is what this is.
 *
 * It is a `.mjs` rather than a `.ts` for one reason: `engine.mjs` is run by plain `node` with no build
 * step, so the module both it and vitest import has to be runnable JavaScript. `checkJs` covers it via
 * the JSDoc types below.
 *
 * ## The entry point is `stripScriptBodiesChecked`
 *
 * Not `stripJsComments`. The checked one compiles the result and throws when source that parsed
 * before stripping does not after — the only leg that covers shapes nobody wrote a case for. Call
 * the bare stripper only when there is genuinely no parseable-JS baseline to compare against, and
 * say at the call site why.
 *
 * ## What this module is NOT
 *
 * It is not a JavaScript parser, and the sections marked KNOWN GAP below are real. Every gap is
 * pinned by a case in `jsSource.test.ts` asserting the ACTUAL behaviour, so the blind spot is a
 * failing-if-it-changes test rather than a paragraph nobody re-reads. Read the gaps there, with their
 * inputs, rather than trusting this prose.
 *
 * Round 4's lesson, and the reason the gap list is now split by direction: **a declared gap is a
 * claim like any other.** "All of which fail toward KEEPING source" was asserted over a LIST rather
 * than derived per entry, its `)` case in the test table happened to be the benign form, and the
 * oracle iterating that benign case read as coverage for the whole claim. Gaps that delete are now
 * their own group, and the property that makes each one survivable is asserted with `vm.Script`
 * rather than written down.
 *
 * Round 5's lesson is the same defect one level up, so read this one carefully before writing the
 * next sentence about this module: round 4 replaced "all of which fail toward keeping" with
 * **"neither is reachable from valid JavaScript"**, over the same list, next to the same green
 * filter — and two parseable inputs still deleted (`for await (…)`, and a mis-read regex swallowing
 * an unmatched `(`; both fixed, both pinned). **A per-entry derivation over a table is a fact about
 * the table.** Generalising it to the mechanism turns a green test into a vouch for a claim it never
 * measured. State the scope the assertion actually has, and let `stripScriptBodiesChecked`'s compile
 * be the only thing that speaks for the shapes nobody enumerated.
 */

/**
 * Words after which a `/` opens a REGEX LITERAL rather than a division.
 *
 * This set is the whole of CPE-1966 round 3's false-strip defect. The original scanner tracked a
 * single previous CHARACTER, so `return`, `typeof` and `case` all ended in a word character, matched
 * its "value-shaped token" class, and their regex literals were read as division — at which point the
 * `/` inside `return /[//]/;` opened a line comment and **deleted the rest of the line**, and the `/*`
 * inside `return /[/*]/;` deleted everything up to the next `*​/`, possibly many lines away. Silently.
 * A stripper that fails toward KEEPING code is defensible; one that drops it is not.
 */
import vm from "node:vm";

const REGEX_AFTER = new Set([
  "return", "typeof", "instanceof", "in", "of", "new", "delete", "void", "throw",
  "case", "do", "else", "yield", "await",
]);

/**
 * Words whose `(…)` is a CONTROL-STATEMENT CONDITION, so the `)` closing it is followed by a
 * statement — and a `/` opening that statement is a REGEX LITERAL, not a division.
 *
 * This set is CPE-1966 round 4's blocker, and it is round 3's defect reached through a different
 * door. Round 3 fixed the KEYWORD prefix (`return /[/*]/;`) and left `)` documented as a gap that
 * "fails toward keeping source" — which was false for exactly the same reason: `if (s.length)
 * /[/*]/.test(s);` is valid JavaScript, the `/` after `)` was read as division, the `[` was emitted
 * as itself, and the NEXT `/` reached the `//` and `/*` branches and invented a comment.
 * Measured on the Reviewer's 191-character fixture: **144 characters deleted**, parseable in,
 * unparseable out. Deciding `)` by what its `(` opened is what real tokenizers do, and it is why
 * that gap is now closed rather than reworded. The `(` kinds are tracked on a per-frame stack, so
 * `if (f(x)) /re/.test(s)` resolves the inner `)` to a value and the outer one to a regex position.
 *
 * `switch` and `catch` are deliberately absent — their `)` is followed by `{`, never by a regex, so
 * adding them would only widen the regex reading with no shape to justify it.
 *
 * **The set is not the whole rule — REACHING the `(` intact is the other half (round 5).** Only
 * whitespace, comments and `await` may sit between a control word and its `(`. Comments already pass
 * through (their branches write neither `prevKind` nor `prevPunct`), but `await` is a WORD and is also
 * in `REGEX_AFTER`, so the word branch below demoted `"control"` to `"keyword"` and
 * `for await (const x of y) /[//]/…` deleted 14 characters of valid JavaScript. Membership in this set
 * only matters if the state it sets survives to the `(`.
 *
 * RED-PROOFS (CPE-1933 rule 3), all three re-run at round 5 and recorded here rather than only in the
 * PR. Each isolates ONE mechanism, because a single sabotage that reds everything cannot tell you
 * which part was load-bearing:
 *   - Emptying this set to `new Set([])` reds **14** of `jsSource.test.ts`'s 58 — the four
 *     `FALSE_STRIP_PAREN` cases, the `-144 -> 0` measurement, the four `FALSE_STRIP_AWAIT` cases
 *     (which reach their `(` through `for`), the four `FALSE_STRIP_EATEN_PAREN` cases, and,
 *     decisively, the `vm.Script` oracle itself. The oracle catching it is the part that was missing
 *     before: round 3's gap entry for `)` was the benign `/re/` form, so the oracle iterated a shape
 *     that could not fail.
 *   - Disabling only the `await` clause in the word branch (`false && …`) reds **5** — the four
 *     `for await` cases and the oracle. That is the round-5 `for await` bug, on its own.
 *   - Disabling only the eaten-paren accounting in the regex branch (`for (const p of [])`) reds
 *     **3** — the two mis-read-regex cases and the oracle. That is the stack desync, on its own.
 */
const CONTROL_PAREN = new Set(["if", "for", "while", "with"]);

const WORD = /[A-Za-z0-9_$]/;

/**
 * Comments and their contents removed from JavaScript source, quote-, template- and regex-aware.
 *
 * Each comment becomes a single space rather than being deleted, so a caller's `includes(claim)`
 * cannot be satisfied by two fragments joining up across the hole a comment used to fill.
 *
 * ## Handled
 * - `//` and `/* *​/` comments, including **trailing** ones (a whole-line filter is not enough — a
 *   trailing comment walks straight through one, which is how CPE-1933's first draft reintroduced
 *   the hole it was closing).
 * - `'…'` and `"…"` string literals with `\` escapes, so a `//` inside a URL is not a comment.
 * - Template literals INCLUDING their `${…}` substitutions, which are re-scanned as code. Both halves
 *   of that matter: a nested `` `a${ "`" }b` `` used to close the template early (the backtick inside
 *   the nested string), and `` `a${x /* c *​/}b` `` used to survive with its comment intact because
 *   substitutions were never entered.
 * - Regex literals, including `/` and `*` inside a `[…]` character class, decided by the previous
 *   TOKEN (see `REGEX_AFTER`) rather than the previous character.
 * - A regex literal after a control-statement condition — `if (x) /re/.test(s)`, `while (…)`,
 *   `for (…)` — decided by what the matching `(` opened (see `CONTROL_PAREN`), not by the `)`.
 * - A keyword used as a property name (`obj.return / 2`) — the `.` in front demotes it to a value, so
 *   the `/` stays a division.
 *
 * ## DO NOT CALL THIS BARE
 *
 * Use `stripScriptBodiesChecked` (below), which compiles the result and throws on a desync. This
 * function is exported without that backstop only so the backstop's own tests, and callers that
 * genuinely have no parseable-JS baseline to compare against, can reach it. Round 4's blocker was a
 * *documented* gap that turned out to delete code, and the reason it was survivable in-tree is that
 * the one caller went through the checked entry point. The gaps below are the ones known today; the
 * `vm.Script` oracle is what covers the ones that are not.
 *
 * ## KNOWN GAPS
 *
 * 1. **A regex literal after `]`, or after a `)` that closes a CALL or a grouping, is read as
 *    division.** `a[0] /re/.test(s)` and `f(x) /re/.test(s)` are genuinely ambiguous without a
 *    parser, and division is what a real tokenizer reads there too. Two sub-cases, and they differ:
 *    - the text is emitted verbatim (the scanner re-syncs on the closing `/`) — `a[0] /re/.test(s)`;
 *    - **the region is DELETED when the mis-read regex hides a `//` or a `/*`** — `a[0] /[//]/…`
 *      deletes to end of line, `f(x) /[/*]/…` deletes to the next `*​/`, possibly pages away. The
 *      emitted `/` and `[` are ordinary characters, so the next `/` reaches the comment branches.
 *
 *    Both deleting sub-cases are pinned by test, and what `jsSource.test.ts` derives about them is
 *    exactly this: **no entry in its `DELETING_GAPS` table parses before stripping** — a statement
 *    about that table, checked with `vm.Script`, not a statement about JavaScript. For each entry
 *    individually the reason holds by inspection: real JS reads that `/` as division too, so
 *    `a[0] / [//]/…` opens an array literal that the `//` comments away, and the input does not parse
 *    *before* stripping either. That is why they are listed as gaps rather than as bugs, and it is a
 *    much weaker claim than "fails toward keeping".
 *
 *    **Do not read it as "no valid JavaScript reaches this mechanism."** Round 4 wrote that sentence
 *    over this list and round 5 measured two parseable inputs that still deleted — `for await (…)`
 *    and a mis-read regex swallowing an unmatched `(`, both -14 characters, both now fixed above.
 *    A green table says the shapes in the table are safe; it cannot say a shape nobody wrote down
 *    does not exist. The only leg that speaks to that is `stripScriptBodiesChecked`'s compile.
 * 2. **No ASI awareness.** `a = b` newline `/re/.test(c)` is division, with exactly the same two
 *    sub-cases and the same reason they are unreachable from parseable input.
 * 3. **Unterminated literals** (a lone `"` or a `/` starting nothing) are emitted as themselves and
 *    scanning continues from the next character, rather than swallowing the rest of the file. This
 *    one really does fail toward keeping.
 *
 * The backstop for all three, and for anything not yet imagined, is to compile the RESULT: run
 * `new vm.Script(stripped)` and fail loudly when source that parsed before stripping no longer
 * parses after — which is what `stripScriptBodiesChecked` does, and what `engine.mjs`'s
 * `checkFixtureProvenance` gets by using it. A desync that deletes code is overwhelmingly likely to
 * leave something unparseable behind. Note what that oracle can and cannot see: it catches deletion
 * on input that parsed, and it is blind to a desync on input that never parsed — which is precisely
 * the corner gaps 1 and 2 now live in.
 *
 * @param {string} src JavaScript source.
 * @returns {string} the same source with every comment replaced by a single space.
 */
export function stripJsComments(src) {
  let out = "";
  let i = 0;
  /**
   * The kind of the last significant token. `"value"` (identifier, number, string, template, regex,
   * `]`, a `)` closing a call or grouping) means a following `/` is division; `"punct"`, `"keyword"`,
   * `"control"` and `""` (start of input) mean it opens a regex.
   * @type {"" | "value" | "punct" | "keyword" | "control"}
   */
  let prevKind = "";
  /** The last punctuator emitted, so `.return` can be told from `return`. */
  let prevPunct = "";
  /**
   * Nesting stack. `template` is inside a `` ` `` literal; `code` is ordinary source, and a `code`
   * frame with `subst: true` is the inside of a `${…}` — its matching `}` pops back to the template.
   *
   * `parens` is per-frame so a `${…}` cannot leak an unbalanced `(` into the enclosing code: each
   * entry is one open `(`, `true` when it opened a control-statement condition.
   * @type {{ mode: "code" | "template", depth: number, subst: boolean, parens: boolean[] }[]}
   */
  const stack = [{ mode: "code", depth: 0, subst: false, parens: [] }];

  while (i < src.length) {
    const top = stack[stack.length - 1];
    const c = src[i];
    const d = src[i + 1];

    if (top.mode === "template") {
      if (c === "\\") { out += src.slice(i, i + 2); i += 2; continue; }
      if (c === "`") { out += c; i++; stack.pop(); prevKind = "value"; prevPunct = ""; continue; }
      if (c === "$" && d === "{") {
        out += "${";
        i += 2;
        stack.push({ mode: "code", depth: 0, subst: true, parens: [] });
        prevKind = "";
        continue;
      }
      out += c;
      i++;
      continue;
    }

    if (c === "/" && d === "/") {
      const nl = src.indexOf("\n", i);
      out += " ";
      i = nl === -1 ? src.length : nl;
      continue;
    }
    if (c === "/" && d === "*") {
      const end = src.indexOf("*/", i + 2);
      out += " ";
      i = end === -1 ? src.length : end + 2;
      continue;
    }
    if (c === '"' || c === "'") {
      let j = i + 1;
      while (j < src.length) {
        if (src[j] === "\\") { j += 2; continue; }
        if (src[j] === c || src[j] === "\n") break;
        j++;
      }
      // An unterminated string (we ran into a newline or the end) is emitted as far as it got and
      // scanning resumes — swallowing the rest of the file on one stray quote is the failure mode
      // that made `sessionChipColours` sensitive to an apostrophe in HTML prose.
      const closed = j < src.length && src[j] === c;
      out += src.slice(i, closed ? j + 1 : j);
      i = closed ? j + 1 : j;
      prevKind = "value";
      prevPunct = "";
      continue;
    }
    if (c === "`") {
      out += c;
      i++;
      stack.push({ mode: "template", depth: 0, subst: false, parens: [] });
      continue;
    }
    if (c === "(") {
      top.parens.push(prevKind === "control");
      out += c;
      i++;
      prevKind = "punct";
      prevPunct = c;
      continue;
    }
    if (c === ")") {
      // What this `)` closes decides the next `/`: a control-statement condition is followed by a
      // STATEMENT, where `/` opens a regex; a call or a grouping is a value, where it divides.
      const condition = top.parens.pop() === true;
      out += c;
      i++;
      prevKind = condition ? "keyword" : "value";
      prevPunct = c;
      continue;
    }
    if (c === "{") { top.depth++; out += c; i++; prevKind = "punct"; prevPunct = c; continue; }
    if (c === "}") {
      if (top.depth === 0 && top.subst) {
        out += c;
        i++;
        stack.pop();
        continue;              // back inside the template literal; prevKind is irrelevant there
      }
      if (top.depth > 0) top.depth--;
      out += c;
      i++;
      prevKind = "punct";
      prevPunct = c;
      continue;
    }
    if (c === "/" && prevKind !== "value") {
      let j = i + 1;
      let inClass = false;
      let terminated = false;
      /**
       * Unescaped, out-of-class parens this literal SWALLOWS, applied to the frame only if it really
       * turns out to be a literal. See the accounting note below for why they cannot just be ignored.
       * @type {string[]}
       */
      const eaten = [];
      while (j < src.length) {
        if (src[j] === "\\") { j += 2; continue; }
        if (src[j] === "\n") break;                       // unterminated: treat the `/` as itself
        if (src[j] === "[") inClass = true;
        else if (src[j] === "]") inClass = false;
        else if (src[j] === "/" && !inClass) { terminated = true; break; }
        else if (!inClass && (src[j] === "(" || src[j] === ")")) eaten.push(src[j]);
        j++;
      }
      if (terminated) {
        // ACCOUNT FOR EVERY PAREN CONSUMED, whichever branch consumed it. The `(`/`)` branches above
        // are not the only code that can eat a paren: this one eats every paren between the two `/`s.
        // For a real regex literal that is a no-op — its parens are balanced, and an unbalanced one is
        // a SyntaxError in JS — but a MIS-READ division swallows the *source's own* parens, and a
        // swallowed `(` that never reached the stack leaves the matching `)` popping the frame beneath
        // it. Measured: `if ({} / f(1 / 2)) /[//]/…` scanned `/ f(1 /` as a regex, the condition's `)`
        // popped nothing, the outer `)` took the `true` meant for it, and 14 characters were deleted.
        // This restores balance; it does NOT recover the KINDS inside the swallowed text (every eaten
        // `(` is recorded as a value-opening one), which only matters on input that already failed to
        // parse — see the gap list.
        for (const p of eaten) {
          if (p === "(") top.parens.push(false);
          else top.parens.pop();
        }
        out += src.slice(i, j + 1);
        i = j + 1;
        prevKind = "value";
        prevPunct = "";
        continue;
      }
      out += c;
      i++;
      prevKind = "punct";
      prevPunct = c;
      continue;
    }
    if (WORD.test(c)) {
      let j = i;
      while (j < src.length && WORD.test(src[j])) j++;
      const word = src.slice(i, j);
      out += word;
      // `obj.return` and `obj.if` are properties, not keywords — the `/` after them is a division.
      const keyword = prevPunct !== ".";
      prevKind = prevKind === "control" && keyword && word === "await"
        // `for await (…)` — `await` is the ONE token the grammar lets sit between a control word and
        // its `(`, and it is also in REGEX_AFTER, so without this it overwrote `"control"` with
        // `"keyword"`, the `)` resolved to a value, and `for await (const x of y) /[//]/…` deleted to
        // end of line. Comments may sit there too and already pass through untouched (their branches
        // write neither `prevKind` nor `prevPunct`).
        ? "control"
        : keyword && CONTROL_PAREN.has(word)
          ? "control"
          : keyword && REGEX_AFTER.has(word)
            ? "keyword"
            : "value";
      prevPunct = "";
      i = j;
      continue;
    }
    out += c;
    i++;
    if (!/\s/.test(c)) {
      // `(` and `)` never reach here — they have their own branches above, because `)` needs the
      // paren stack to know whether it closed a condition or a value.
      prevKind = c === "]" ? "value" : "punct";
      prevPunct = c;
    }
  }
  return out;
}

/**
 * Every `<script>` body in an HTML document, in document order.
 *
 * Trivial, and here rather than at the call site for one reason: CPE-1966 round 3 found a JS
 * tokenizer pointed at a whole HTML DOCUMENT. HTML prose is not JavaScript — one apostrophe in
 * `<p>the agent's log</p>`, outside every script, opened a string literal and shifted the parse; the
 * measured effect was 11,872 characters of net deletion, and the thing that depended on it survived
 * only because the swallowed region happened to be copied through verbatim. Pairing the extractor
 * with the stripper in one module is the cheapest way to keep the two from being used apart.
 *
 * **11,872 is a HISTORICAL figure and cannot be reproduced against this stripper (round 4).** It was
 * taken before round 3 made an unterminated string stop at the newline instead of running to the next
 * `'` anywhere in the file; re-measured at round 4 the same injection shifts a whole-document strip by
 * **0**. It is kept as the reason the rule exists, not as a number anyone should expect to re-derive.
 * The live property is asserted in `jsSource.test.ts` and does not depend on the figure.
 *
 * @param {string} html
 * @returns {string[]}
 */
export function htmlScriptBodies(html) {
  return [...html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
}

/**
 * Script bodies with comments stripped, and the parse backstop that catches a stripper desync.
 *
 * **This is the entry point. Prefer it to bare `stripJsComments` everywhere.**
 *
 * `stripJsComments` is a scanner, not a parser, and its declared gaps are real. Some of them DELETE
 * (see its KNOWN GAPS 1 and 2) — round 3 wrote "they all fail toward KEEPING source" and round 4
 * measured a documented gap deleting 144 characters of valid JavaScript, so that sentence is gone
 * rather than softened. Compiling the result is a near-free oracle for the direction that matters: a
 * desync that DELETED code overwhelmingly leaves something unparseable behind. This is the JS
 * equivalent of `rustSource.ts`'s `SURVIVING_COMMENT_LINE`, and it covers the shapes nobody has
 * thought of yet, which a case table by construction cannot.
 *
 * Only a body that parsed BEFORE stripping is checked after it, so a `<script type="application/json">`
 * or a minified bundle that was never JavaScript cannot red a caller. Say the cost of that out loud:
 * it is also the blind spot. Both surviving deleting gaps live on input that never parsed, so this
 * oracle cannot see them — which is why they are pinned as explicit cases instead.
 *
 * The stripper is a parameter so the backstop itself can be RED-PROOFED (CPE-1933 rule 3) — see
 * `jsSource.test.ts`, which hands it one that really does delete. A backstop nobody has watched fail
 * is a claim, not a guard.
 *
 * @param {string[]} bodies script bodies, in document order.
 * @param {(src: string) => string} [strip] the stripper to apply; defaults to `stripJsComments`.
 * @returns {string[]} the stripped bodies, in the same order.
 */
export function stripScriptBodiesChecked(bodies, strip = stripJsComments) {
  /** @type {string[]} */
  const broke = [];
  const stripped = bodies.map((body, k) => {
    const out = strip(body);
    if (parsesAsJs(body) && !parsesAsJs(out)) {
      broke.push(`<script> #${k + 1}: parsed before stripping, does not parse after — ${parseError(out)}`);
    }
    return out;
  });
  if (broke.length) {
    throw new Error(
      "COMMENT STRIPPER DESYNC — stripping comments turned parseable JavaScript into unparseable text,\n" +
        "which means it deleted code rather than comments. Anything derived from the result would be\n" +
        "derived from a mutilated file:\n  " + broke.join("\n  "),
    );
  }
  return stripped;
}

/** @param {string} src */
function parsesAsJs(src) {
  try {
    new vm.Script(src);
    return true;
  } catch {
    return false;
  }
}

/** @param {string} src */
function parseError(src) {
  try {
    new vm.Script(src);
    return "(it parses)";
  } catch (e) {
    return String(e instanceof Error ? e.message : e);
  }
}
