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
 * ## What this module is NOT
 *
 * It is not a JavaScript parser, and the sections marked KNOWN GAP below are real. Every gap is
 * pinned by a case in `jsSource.test.ts` asserting the ACTUAL behaviour, so the blind spot is a
 * failing-if-it-changes test rather than a paragraph nobody re-reads. Read the gaps there, with their
 * inputs, rather than trusting this prose.
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
 * - A keyword used as a property name (`obj.return / 2`) — the `.` in front demotes it to a value, so
 *   the `/` stays a division.
 *
 * ## KNOWN GAPS, all of which fail toward KEEPING source rather than deleting it
 * 1. **A regex literal directly after `)` or `]` is read as division.** `if (x) /re/.test(s)` and
 *    `a[0] /re/.test(s)` are genuinely ambiguous without a parser; division is the conservative
 *    reading, and the observed effect is that the text is emitted verbatim (the scanner re-syncs on
 *    the closing `/`), never that a comment is invented. Pinned by test.
 * 2. **No ASI awareness.** `a = b` newline `/re/.test(c)` is division for the same reason.
 * 3. **Unterminated literals** (a lone `"` or a `/` starting nothing) are emitted as themselves and
 *    scanning continues from the next character, rather than swallowing the rest of the file.
 *
 * The backstop for all three, and for anything not yet imagined, is to compile the RESULT: a caller
 * that can afford it should run `new vm.Script(stripped)` and fail loudly when source that parsed
 * before stripping no longer parses after. `engine.mjs`'s `checkFixtureProvenance` does exactly that.
 * A desync that deletes code is overwhelmingly likely to leave something unparseable behind, and a
 * desync that keeps too much cannot break a parse at all — which is the whole reason the KEEP
 * direction is the safe one.
 *
 * @param {string} src JavaScript source.
 * @returns {string} the same source with every comment replaced by a single space.
 */
export function stripJsComments(src) {
  let out = "";
  let i = 0;
  /**
   * The kind of the last significant token. `"value"` (identifier, number, string, template, regex,
   * `)`, `]`) means a following `/` is division; `"punct"`, `"keyword"` and `""` (start of input)
   * mean it opens a regex.
   * @type {"" | "value" | "punct" | "keyword"}
   */
  let prevKind = "";
  /** The last punctuator emitted, so `.return` can be told from `return`. */
  let prevPunct = "";
  /**
   * Nesting stack. `template` is inside a `` ` `` literal; `code` is ordinary source, and a `code`
   * frame with `subst: true` is the inside of a `${…}` — its matching `}` pops back to the template.
   * @type {{ mode: "code" | "template", depth: number, subst: boolean }[]}
   */
  const stack = [{ mode: "code", depth: 0, subst: false }];

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
        stack.push({ mode: "code", depth: 0, subst: true });
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
      stack.push({ mode: "template", depth: 0, subst: false });
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
    if (c === "/" && (prevKind === "" || prevKind === "punct" || prevKind === "keyword")) {
      let j = i + 1;
      let inClass = false;
      let terminated = false;
      while (j < src.length) {
        if (src[j] === "\\") { j += 2; continue; }
        if (src[j] === "\n") break;                       // unterminated: treat the `/` as itself
        if (src[j] === "[") inClass = true;
        else if (src[j] === "]") inClass = false;
        else if (src[j] === "/" && !inClass) { terminated = true; break; }
        j++;
      }
      if (terminated) {
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
      // `obj.return` is a property, not the keyword — the `/` after it is a division.
      prevKind = REGEX_AFTER.has(word) && prevPunct !== "." ? "keyword" : "value";
      prevPunct = "";
      i = j;
      continue;
    }
    out += c;
    i++;
    if (!/\s/.test(c)) {
      prevKind = c === ")" || c === "]" ? "value" : "punct";
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
 * @param {string} html
 * @returns {string[]}
 */
export function htmlScriptBodies(html) {
  return [...html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
}

/**
 * Script bodies with comments stripped, and the parse backstop that catches a stripper desync.
 *
 * `stripJsComments` is a scanner, not a parser, and its declared gaps are real. They all fail toward
 * KEEPING source, which cannot break a parse — so compiling the result is a near-free oracle for the
 * one direction that matters: a desync that DELETED code overwhelmingly leaves something unparseable
 * behind. This is the JS equivalent of `rustSource.ts`'s `SURVIVING_COMMENT_LINE`, and it covers the
 * shapes nobody has thought of yet, which a case table by construction cannot.
 *
 * Only a body that parsed BEFORE stripping is checked after it, so a `<script type="application/json">`
 * or a minified bundle that was never JavaScript cannot red a caller.
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
