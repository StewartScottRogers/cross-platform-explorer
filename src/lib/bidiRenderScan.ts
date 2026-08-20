/**
 * CPE-1757 round 2 — the analysis engine behind `src/lib/bidiEscape.guard.test.ts`.
 *
 * Round 1's guard matched a small, named set of "risky shapes" (`X.name`, `X.path`, bare `root`/`path`/
 * `name`, `baseName(…)`/`basename(…)`) — a regex zoo. UAT + review found it missed a raw `{revertOnePath}`
 * in a file it already covered, and a probe component proved it recognizes only 3 of 17 real render
 * shapes (template-literal interpolation, an intermediate `const n = entry.name`, `{fileName}`,
 * `{@html …}`, `{#each … as { name }}`, a render sitting right after `{#if}`/`{:else}`/`{/each}` instead
 * of `>`, `parentDir(…)`, `p.split("/").pop()`, locally-named helpers like `baseOf`/`base`, `entry.oldName`,
 * `entry.fullPath`, …). Naming more shapes only grows the zoo one miss behind the next author's typo.
 *
 * This is the inversion the reviewer prescribed instead: inside a render position (plain text content,
 * or a `title=`/`aria-label=`/`alt=` attribute, or `{@html …}`), **every** `{…}` expression must be a
 * literal, a `displaySafeName(…)`/`displaySafePath(…)` call (or an `||`/`??`/ternary combination of
 * those), or an explicitly registered line (`REGISTRY`, `bidiEscape.guard.test.ts`). There is no third
 * bucket — an expression that isn't provably safe is unsafe by default, regardless of what it's named or
 * shaped like. That is why this module does not special-case `.name`/`.path`/`baseName` at all:
 * `isSafeExpr` doesn't know or care what an identifier is called.
 *
 * CPE-1761 changed three things, each closing one way a render could go unreported without the guard
 * saying so — read this as narrowing specific failure shapes, NOT as a claim the computed set is exhaustive
 * (it is not; see "still cannot see" below for what remains genuinely invisible to this engine):
 *   1. A registered entry pins `line:expr`, not just `line` — editing an already-registered line's
 *      expression in place (e.g. swapping a harmless `title={$t(...)}` for a raw `title={entry.name}`)
 *      changes the recorded key and reds the guard, instead of the line number alone staying "found"
 *      either way.
 *   2. Markup this module cannot confidently PARSE — an unterminated `{…}`/`<!--…-->`/`</…>`, a bare `<`
 *      that doesn't open a real tag/comment/closing-tag, or a tag/quoted-attribute that never closes
 *      before EOF — throws `RenderScanError` (naming the file and position) instead of silently
 *      truncating the scan at that point the way round 2's `i = markup.length` fallback did (returning
 *      whatever was found so far — often `[]`, indistinguishable from "clean").
 *   3. `isRenderPosition`'s inTag branch recognizes `attr='…{…}…'` exactly like `attr="…{…}…"` (attempt
 *      3): the single-quoted form used to be silently classified as a non-render — not an EOF/parse
 *      failure, just dropped — whenever a mustache was reached with `quoteChar === "'"`, which happens
 *      even when the quote is LATER re-balanced by an unrelated apostrophe elsewhere in the file (so #2's
 *      EOF check never saw anything wrong). Keeping the two quote styles symmetric removes the drop
 *      entirely rather than trading it for a false-positive hard-error on legitimate single-quoted markup.
 *
 * CPE-1766/CPE-1767/CPE-1776 (this round) fixed four more pre-existing holes the CPE-1761 UAT/review found
 * while probing shapes nobody had asked about — all in the same fail-open direction as everything above:
 *   4. CPE-1766: a mustache preceded by ordinary body-text PROSE, not just a tag boundary — `<div>File:
 *      {entry.name}</div>` — used to fail `isRenderPosition`'s `/[>}]\s*$/` lookback entirely and scan
 *      clean. Fixed by removing the lookback for this case: the state machine already knows when it's
 *      outside a tag (script/style are stripped before scanning starts; an attribute value's mustache goes
 *      through the separate `inTag` branch), so every `!inTag` mustache reached at all IS body text — full
 *      stop, nothing left to disambiguate by re-reading nearby characters.
 *   5. CPE-1776: `isRenderPosition`'s `inTag` branch re-derived the current attribute from a FIXED
 *      200-character textual slice (`markup.slice(idx - 200, idx)`) and a `[^"]*$`/`[^']*$` regex. Once an
 *      `alt=`/`aria-label=`/`title=` value's own text ran past ~194 characters — exactly what verbose,
 *      accessible label text looks like — the attribute-name token fell outside the slice and the render
 *      was silently dropped. Fixed at the design level, not by widening the window: the scanner now tracks
 *      the CURRENT attribute name live (`currentAttrName` in `findUnsafeRenderLines`, set the instant it
 *      sees `attrName=`, cleared the instant that value ends) and passes it straight to `isRenderPosition`,
 *      so there is no attribute-VALUE length to exceed — only the attribute NAME is ever looked at, and
 *      names are always short. The escaped-quote half of CPE-1776 (`title="…\"…{name}"`) was checked
 *      against the real Svelte compiler and does not compile — HTML/Svelte attribute values have no
 *      backslash-escape syntax, so that shape is unreachable in valid Svelte — but the redesign closes it
 *      structurally regardless, since attribute-value TEXT (escaped or not) is never re-inspected at all.
 *   6. CPE-1767: `findMatchingBrace` (and `findMatchingParen`/`hasUnsafeIdentifier`, which share the same
 *      quote-scanning shape) had no concept of a `//` or `/* … *​/` JS comment, so an apostrophe (or any
 *      unbalanced quote) written INSIDE a comment inside an inline tag-attribute expression —
 *      `on:click={() => { /* it's fine *​/ }}` — read as opening a real string literal. The quote never
 *      closed, the brace never matched, and #2 above turned that into a hard `RenderScanError` on valid,
 *      compiling Svelte, telling the developer to "fix the malformed brace" when there wasn't one. Fixed by
 *      a shared `skipJsComment`: `//` runs to end-of-line, `/* *​/` to its terminator, and anything inside
 *      either is inert to brace/quote/depth tracking. **Stated boundary**: regex literals are not parsed,
 *      so a `/` opening a regex that itself contains a literal `//`/`/*` is misread as a comment start —
 *      see the boundary item below.
 *
 * **What this still cannot see** (the honest boundary — read before trusting a green run):
 *   - A raw name/path placed in any DOM attribute OTHER than `title`/`aria-label`/`alt` — `data-*`,
 *     `placeholder`, `value`, `style`, etc. UAT round 1 confirmed `data-fullpath={e.path}` staying green
 *     is the INTENDED behavior (those attributes aren't examined by a sighted user the way a tooltip or
 *     visible text is), so this module keeps that boundary rather than widening it. If that boundary is
 *     ever wrong for a specific attribute, it needs a deliberate decision, not a silent regex tweak.
 *   - A component prop pass-through whose LEAF doesn't escape its own render — `<Foo label={entry.name}>`
 *     is invisible here by design (same boundary `<DiffSideBySide path={sbs.path}>` relies on, correctly,
 *     since that leaf DOES escape internally) — nothing currently proves every such leaf holds up its end.
 *   - `<script>`/`<style>` block content is stripped before scanning (this module only looks at markup),
 *     so a bidi-unsafe string built in script and only later interpolated through an ALREADY-flagged
 *     mustache is still caught at the render site, but a raw name written straight to a non-DOM sink
 *     (console.log, an exception message, a clipboard write) is out of scope for a DOM-rendering guard.
 *   - An inline ternary's CONDITION is not parsed out: `kind === "dir" ? displaySafeName(a) :
 *     displaySafePath(b)` reads as unsafe (the leftover `kind`/`dir` tokens look identical to a real
 *     miss) even though both branches are properly wrapped. This is a false POSITIVE, not a false
 *     negative — the safe direction to be wrong in, since it costs a `REGISTRY` entry rather than lets a
 *     spoof through. Rewriting as `{#if kind === "dir"}{displaySafeName(a)}{:else}…{/if}` (this repo's
 *     own idiom) sidesteps it entirely, since each branch is then its own independently-checked mustache.
 *   - CPE-1761's fail-loud fix is a trade, not a free lunch: a genuinely LITERAL `<` sitting in body text
 *     (not opening a tag — a stray comparison typo, say) now hard-errors the whole scan rather than being
 *     silently tolerated. That is deliberate (a loud false failure beats a quiet false pass), but it does
 *     mean such text must be rewritten (or escaped) for the guard to run at all — it will not render a
 *     verdict around markup it isn't sure it understood.
 *   - CPE-1767's comment handling does not parse regex literals (a stated, deliberate limit — "a
 *     correct-enough parser with a stated limit beats a subtly wrong one that looks complete"): a `/` that
 *     opens a regex literal containing a literal `//` or `/*` (e.g. `/a\/\/b/`) is misread as the start of
 *     a comment. The real blast radius (N2, reviewer round 2): since `findMatchingBrace` tracks `{`/`}`
 *     depth as it scans, skipping a real code chunk as an "inert comment" can make the mustache's depth
 *     count never re-balance, or land on the wrong closing `}` — and `handleMustache` moves its cursor
 *     from whatever `close` index this returns, so a wrong one shifts where the REST OF THE MARKUP is
 *     parsed from, not just the misread expression. Every case probed while fixing CPE-1767 failed CLOSED
 *     (a loud `RenderScanError`, either "unterminated {" or the tag/text state machine hitting something
 *     it can't parse next) rather than silently producing a wrong answer — but that isn't proven
 *     impossible in general, which is exactly why this stays a listed boundary. Regex literals inside a
 *     Svelte mustache are rare (an inline arrow function occasionally has one; a raw filesystem name
 *     render never does) — distinguishing regex-`/` from divide-`/` needs real expression parsing this
 *     module deliberately doesn't do.
 *   - Membership is enforced mechanically (CPE-1768): see `bidiEscape.guard.test.ts`'s membership-criterion
 *     test for the stated rule for which `.svelte` files must be in `REGISTRY` at all, and why a file
 *     outside that criterion (no filesystem-derived render) is legitimately never scanned rather than
 *     silently skipped.
 */

/** Strip every `<script>…</script>` / `<style>…</style>` block, replacing its content with the same
 *  number of newlines so every surviving line keeps its real line number. Only markup is scanned below —
 *  script-side code legitimately contains countless `{…}` that are not template renders at all. */
export function stripNonMarkup(src: string): string {
  return src.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi, (m) => "\n".repeat(m.split("\n").length - 1));
}

/** CPE-1767: the index just past a `//` line comment or `/* … *​/` block comment starting at `i` (caller
 *  has already checked `s[i] === "/"` and `s[i + 1]` is `/` or `*`). A line comment runs to (but does not
 *  consume) the next `\n`, or EOF; an unterminated block comment runs to EOF. Quotes/braces/backticks
 *  inside either are inert — the whole point of this function is that callers skip straight past them
 *  without ever looking at their content.
 *
 *  **Stated boundary** (CPE-1767 AC: "decide how far to go and write down the boundary"): this does NOT
 *  parse regex literals. A `/` that opens a regex containing a literal `//` or `/*` (e.g. `/a\/\/b/`,
 *  `/a\/*b/`) will be misread as starting a comment at that inner `/` — real blast radius (N2, reviewer
 *  round 2): `findMatchingBrace` counts `{`/`}` depth as it goes, so treating a chunk of real code as an
 *  inert "comment" skips whatever `{`/`}` characters happen to fall inside it, which can make the whole
 *  mustache's depth count land on the WRONG closing `}` — or, just as likely, never balance back to zero at
 *  all. `handleMustache` sets `i = close + 1` from whatever this function returns, so a wrong `close`
 *  doesn't just affect the misread expression: it moves the scanner's cursor to the wrong point in the
 *  MARKUP that follows, and everything after that is parsed relative to that wrong position. In practice
 *  this is expected to (and every case probed while fixing CPE-1767 did) fail CLOSED rather than silently:
 *  either the depth never re-balances (this function returns -1, and `handleMustache` throws "unterminated
 *  {" immediately) or the cursor lands somewhere the outer tag/text state machine can't make sense of
 *  (an unmatched quote, an unrecognized `<`, …), which throws its own `RenderScanError` shortly after. It
 *  is not proven exhaustively impossible for a misread `/` to land on some other real `}` and silently
 *  produce a plausible-looking wrong answer instead — that is exactly why this is listed as a boundary
 *  rather than dismissed, and why regex literals inside a Svelte mustache (rare — an inline arrow function
 *  occasionally has one, a raw filesystem name render never does) are worth avoiding until real expression
 *  parsing replaces this module's comment-skipping, which is a heuristic, not a parser, by design. */
function skipJsComment(s: string, i: number): number {
  if (s[i + 1] === "/") {
    const nl = s.indexOf("\n", i);
    return nl === -1 ? s.length : nl;
  }
  const end = s.indexOf("*/", i + 2);
  return end === -1 ? s.length : end + 2;
}

/** True if `s[i]` opens a JS comment (`//` or `/*`) — the one shared check every brace/paren/identifier
 *  scanner below runs before its normal quote/brace handling, so a comment's content never reaches it. */
function startsJsComment(s: string, i: number): boolean {
  return s[i] === "/" && (s[i + 1] === "/" || s[i + 1] === "*");
}

/** Find the index of the `}` that closes the `{` at `openIdx`, skipping over nested `'...'`/`"..."`
 *  string literals, `` `...` `` template literals (including their own nested `${…}`), and `//`/`/* *​/`
 *  JS comments (CPE-1767). Returns -1 if unterminated. */
export function findMatchingBrace(s: string, openIdx: number): number {
  let depth = 0;
  let i = openIdx;
  while (i < s.length) {
    const ch = s[i];
    if (startsJsComment(s, i)) {
      i = skipJsComment(s, i);
      continue;
    }
    if (ch === '"' || ch === "'") {
      i++;
      while (i < s.length && s[i] !== ch) {
        if (s[i] === "\\") i++;
        i++;
      }
      i++;
      continue;
    }
    if (ch === "`") {
      i = skipTemplateLiteral(s, i);
      continue;
    }
    if (ch === "{") {
      depth++;
      i++;
      continue;
    }
    if (ch === "}") {
      depth--;
      if (depth === 0) return i;
      i++;
      continue;
    }
    i++;
  }
  return -1;
}

/** Skip a full `` `...` `` template literal starting at `backtickIdx`, correctly stepping over any
 *  nested `${…}` (which may itself contain nested template literals, strings, or braces). Returns the
 *  index just past the closing backtick (or `s.length` if unterminated). */
function skipTemplateLiteral(s: string, backtickIdx: number): number {
  let i = backtickIdx + 1;
  while (i < s.length) {
    if (s[i] === "\\") {
      i += 2;
      continue;
    }
    if (s[i] === "`") return i + 1;
    if (s[i] === "$" && s[i + 1] === "{") {
      const close = findMatchingBrace(s, i + 1);
      i = close === -1 ? s.length : close + 1;
      continue;
    }
    i++;
  }
  return i;
}

/** Find the index of the `)` that closes the `(` at `openIdx`, with the same string/template/comment
 *  awareness as `findMatchingBrace`. Returns -1 if unterminated. */
export function findMatchingParen(s: string, openIdx: number): number {
  let depth = 0;
  let i = openIdx;
  while (i < s.length) {
    const ch = s[i];
    if (startsJsComment(s, i)) {
      i = skipJsComment(s, i);
      continue;
    }
    if (ch === '"' || ch === "'") {
      i++;
      while (i < s.length && s[i] !== ch) {
        if (s[i] === "\\") i++;
        i++;
      }
      i++;
      continue;
    }
    if (ch === "`") {
      i = skipTemplateLiteral(s, i);
      continue;
    }
    if (ch === "(") {
      depth++;
      i++;
      continue;
    }
    if (ch === ")") {
      depth--;
      if (depth === 0) return i;
      i++;
      continue;
    }
    i++;
  }
  return -1;
}

/** Delete every `displaySafeName(…)`/`displaySafePath(…)` call textually found in `expr` — at the top
 *  level or nested inside a template-literal `${…}` — using a balanced-paren scan so an argument
 *  containing its own parens/strings/nested calls doesn't truncate the match early. */
export function stripSafeCalls(expr: string): string {
  let out = "";
  let i = 0;
  while (i < expr.length) {
    const m = /^displaySafe(?:Name|Path)\(/.exec(expr.slice(i));
    if (m) {
      const openIdx = i + m[0].length - 1;
      const closeIdx = findMatchingParen(expr, openIdx);
      if (closeIdx !== -1) {
        i = closeIdx + 1;
        continue;
      }
    }
    out += expr[i];
    i++;
  }
  return out;
}

const SAFE_BARE_WORDS = new Set(["true", "false", "null", "undefined"]);

/** True if `code` (already run through `stripSafeCalls`) still contains an identifier reference OUTSIDE
 *  a string/template's static text — i.e. something that isn't a `displaySafe*` call, a literal, or one
 *  of the bare keywords `true`/`false`/`null`/`undefined`. Recurses into a template literal's `${…}`
 *  sections (their static text is safe; their interpolations are checked the same way). This is the
 *  whole engine: no shape is named, so no shape can be missed by omission. Also comment-aware (CPE-1767):
 *  a word inside a `//`/`/* *​/` comment is never mistaken for a real identifier reference. */
export function hasUnsafeIdentifier(code: string): boolean {
  let i = 0;
  while (i < code.length) {
    const ch = code[i];
    if (startsJsComment(code, i)) {
      i = skipJsComment(code, i);
      continue;
    }
    if (ch === '"' || ch === "'") {
      i++;
      while (i < code.length && code[i] !== ch) {
        if (code[i] === "\\") i++;
        i++;
      }
      i++;
      continue;
    }
    if (ch === "`") {
      i++;
      while (i < code.length && code[i] !== "`") {
        if (code[i] === "\\") {
          i += 2;
          continue;
        }
        if (code[i] === "$" && code[i + 1] === "{") {
          const close = findMatchingBrace(code, i + 1);
          const inner = close === -1 ? code.slice(i + 2) : code.slice(i + 2, close);
          if (hasUnsafeIdentifier(inner)) return true;
          i = close === -1 ? code.length : close + 1;
          continue;
        }
        i++;
      }
      i++;
      continue;
    }
    if (/[A-Za-z_$]/.test(ch)) {
      let j = i;
      while (j < code.length && /[\w$]/.test(code[j])) j++;
      const word = code.slice(i, j);
      if (!SAFE_BARE_WORDS.has(word)) return true;
      i = j;
      continue;
    }
    i++;
  }
  return false;
}

/** A mustache expression is SAFE only if, once every `displaySafeName(…)`/`displaySafePath(…)` call is
 *  removed, nothing but literals/operators/the four bare keywords is left. Handles `||`/`??`/ternary
 *  combinations of safe calls (e.g. `displaySafeName(baseName(root)) || displaySafePath(root)`,
 *  FileNameSearchDialog's own fallback shape) for free — no operator-precedence parsing needed, because
 *  deleting the calls first and then scanning for leftover identifiers doesn't care how they're joined. */
export function isSafeExpr(raw: string): boolean {
  return !hasUnsafeIdentifier(stripSafeCalls(raw));
}

export interface RenderSite {
  /** 1-based line number of the mustache's opening `{`. */
  line: number;
  /** The raw expression text (for diagnostics only). */
  expr: string;
}

/** CPE-1761: raised instead of silently truncating the scan when the state machine hits markup it
 *  cannot confidently interpret (an unterminated `{…}`, `<!--…-->`, or `</…>`, or a `<` that isn't
 *  followed by a tag name/`/`/`!--`). Round 2's engine used to fall back to `i = markup.length` in every
 *  one of these spots, which silently ends the scan of the WHOLE REST OF THE FILE and returns whatever
 *  offenders were found so far — often `[]`, indistinguishable from "clean". A guard whose entire job is
 *  to catch what a human would miss must never let its own confusion look like a clean bill of health;
 *  failing loudly (naming the file and the exact position) is the only safe direction to fail in. */
export class RenderScanError extends Error {
  constructor(fileLabel: string, reason: string, line: number, col: number) {
    super(
      `${fileLabel}: the render-guard scan could not be completed — ${reason} at line ${line}, col ${col}. ` +
        `This does NOT mean the file is clean; the scan stopped because it could not safely interpret the ` +
        `markup from that point on. Fix the malformed brace/tag (or, if it's deliberately literal text, ` +
        `escape it) so the scan can see everything below it.`,
    );
    this.name = "RenderScanError";
  }
}

/** 1-based line + 1-based column of `idx` within `s`, for error messages. */
function lineCol(s: string, idx: number): { line: number; col: number } {
  const before = s.slice(0, idx);
  const line = before.split("\n").length;
  const lastNl = before.lastIndexOf("\n");
  const col = idx - lastNl;
  return { line, col };
}

/** Collapse a render expression's whitespace to single spaces for a readable, diffable registry entry
 *  (an expression can itself span multiple source lines). Diagnostics only — never re-parsed. */
function normalizeExprForLabel(expr: string): string {
  return expr.trim().replace(/\s+/g, " ");
}

/** Sort `${line}:${expr}` offender strings by their leading line number (numerically, not lexically —
 *  "10:…" must sort after "2:…"), then by the rest of the string for a stable, deterministic order when
 *  two offenders share a line. Exported so `bidiEscape.guard.test.ts` sorts REGISTRY's recorded entries
 *  with the exact same rule `findUnsafeRenderLines` uses internally, instead of a numeric `a - b` sort
 *  that would silently produce `NaN`-driven (i.e. no-op) ordering once entries are strings. */
export function compareOffenders(a: string, b: string): number {
  const la = parseInt(a, 10);
  const lb = parseInt(b, 10);
  return la - lb || a.localeCompare(b);
}

/** True if a mustache is in a *render position* — somewhere its content is actually drawn to the page.
 *  `inTag` (see `findUnsafeRenderLines`'s state machine) tells the two contexts apart:
 *   - OUTSIDE a tag: this state machine only ever reaches "outside a tag" for real body text — script/
 *     style content is stripped before scanning starts (`stripNonMarkup`), and an attribute value's `{…}`
 *     is handled by the `inTag` branch below, never this one. So EVERY mustache reached with `inTag ===
 *     false` is body text, full stop — CPE-1766: there is no longer any lookback at what character
 *     preceded the `{`, because that was never actually the disambiguating fact; the state machine already
 *     knows the context, and re-deriving it from a textual prefix (originally just `>`, widened in CPE-1757
 *     round 2 to `[>}]` to also catch a render sitting directly after `{#if}`/`{:else}`/`{/each}`'s own
 *     closing brace) is exactly the kind of two-mechanisms-that-can-disagree design CPE-1776 diagnoses for
 *     the attribute case below — ordinary prose between a tag boundary and the mustache (`hello {name}`) is
 *     the disagreement CPE-1766 tracks. (A `{#if …}`/`{:else}`/`{/each}`/`{@const …}`/`{@debug …}` control
 *     tag itself is still never treated as a render — that's `isControlTag`, computed from the mustache's
 *     own CONTENT in `handleMustache`, entirely independent of this function.)
 *   - INSIDE a tag's attribute list: only `title=`/`aria-label=`/`alt=` count, given directly as
 *     `attr={…}` or embedded in a quoted `attr="…{…}…"`/`attr='…{…}…'` value — a bare `{density}`
 *     shorthand prop or a directive's `{x}` (`bind:value={x}`) never does. CPE-1776: this used to be
 *     re-derived from a fixed 200-char textual lookback (`/\b(?:title|aria-label|alt)="[^"]*$/` etc.),
 *     which silently dropped the render once the attribute's own text ran past ~194 chars (verbose,
 *     accessible `alt`/`aria-label` text is exactly what pushes past that) — a second mechanism
 *     re-deriving state the caller already has, guaranteed to disagree with it eventually. Now the caller
 *     (`findUnsafeRenderLines`'s state machine) tracks `currentAttrName` directly as it scans — set the
 *     instant it sees `attrName=`, cleared the instant that attribute's value ends — and simply passes it
 *     in, so there is no length limit and nothing to keep in sync. */
function isRenderPosition(inTag: boolean, currentAttrName: string | null): boolean {
  if (!inTag) return true;
  return currentAttrName === "title" || currentAttrName === "aria-label" || currentAttrName === "alt";
}

/** Scan Svelte markup (script/style already stripped) for every render-position mustache — text
 *  content, `title=`/`aria-label=`/`alt=`, and `{@html …}` — and return the 1-based lines of the ones
 *  that are NOT provably safe per `isSafeExpr`. A `{#if …}`/`{:else …}`/`{/each}`/`{#each … as X}`/
 *  `{@const …}`/`{@debug …}` control tag is never itself treated as a render (its condition/binding
 *  isn't drawn), but DOES count as a valid "preceding token" for whatever mustache follows it.
 *
 *  Runs a small tag/text state machine over the markup (rather than a pure lookback regex) specifically
 *  so a component's shorthand-prop attribute list — `<Sidebar {density} {currentPath} … />`, extremely
 *  common in this codebase — is never mistaken for a run of text-content renders just because each `{…}`
 *  sits right after the previous one's `}`; that ambiguity is exactly why `inTag` is threaded through. */
export function findUnsafeRenderLines(fileSrc: string, fileLabel = "<source>"): string[] {
  const markup = stripNonMarkup(fileSrc);
  const offenders = new Set<string>();
  let i = 0;
  let inTag = false;
  let quoteChar: string | null = null;
  // CPE-1776: the attribute name the scanner is CURRENTLY inside the value of, tracked live instead of
  // re-derived later from a lookback slice. Set the instant an `attrName=` is seen; cleared the instant
  // that attribute's value fully ends (bare `{…}` value consumed, or its quote closes). Attribute NAMES
  // are always short (unlike values, which can run to any length), so the bounded lookback used to find
  // the name at the `=` sign is safe in a way the old value-side lookback never was.
  let currentAttrName: string | null = null;

  const fail = (idx: number, reason: string): never => {
    const { line, col } = lineCol(markup, idx);
    throw new RenderScanError(fileLabel, reason, line, col);
  };

  const handleMustache = (renderCandidate: boolean) => {
    const openIdx = i;
    const close = findMatchingBrace(markup, i);
    // CPE-1761 #1: a `{` that never finds its closing `}` used to silently truncate the scan
    // (`i = markup.length`), ending the WHOLE file's scan and reporting whatever was found so far —
    // often `[]`. That is fail-open in the worst possible direction for a guard. Fail loudly instead.
    if (close === -1) {
      fail(openIdx, `unterminated "{" — no matching "}" was found for it`);
    }
    const inner = markup.slice(openIdx + 1, close);
    const trimmed = inner.trimStart();
    const isControlTag = !inTag && /^[#:/]|^@const\b|^@debug\b/.test(trimmed);
    const isHtmlTag = !inTag && /^@html\b/.test(trimmed);
    if (renderCandidate && !isControlTag) {
      const expr = isHtmlTag ? trimmed.replace(/^@html\s*/, "") : inner;
      if (isRenderPosition(inTag, currentAttrName) && !isSafeExpr(expr)) {
        const { line } = lineCol(markup, openIdx);
        offenders.add(`${line}:${normalizeExprForLabel(expr)}`);
      }
    }
    i = close + 1;
  };

  while (i < markup.length) {
    const ch = markup[i];
    if (inTag) {
      if (quoteChar) {
        // B2 (reviewer, this round): a backslash here used to be treated as an ESCAPE (skipping it and
        // whatever followed), but real HTML/Svelte markup attribute values have no backslash-escape
        // syntax at all — that rule only applies inside a JS string literal (which findMatchingBrace/
        // findMatchingParen above correctly handle, since those scan actual JS INSIDE a mustache, not
        // this outer markup). Treating "\\" as an escape here was a silent fail-open in one direction and
        // a false hard-fail in the other, confirmed against the real Svelte compiler:
        //   - `title="C:\{entry.name}"` compiles with {entry.name} genuinely interpolated into the
        //     tooltip (the backslash is just a literal character before it) — but the old code skipped
        //     straight over the "\" AND the "{" that opens the mustache, so the render was never seen.
        //   - `data-x="a\">{entry.name}` has its quote legitimately CLOSED by the un-escaped '"' right
        //     after the backslash — but the old code treated `\"` as an escaped quote and kept
        //     `quoteChar` open past where the value actually ends, running the scanner to EOF still
        //     "inside" a quote and throwing CPE-1761's unterminated-attribute error on markup that
        //     compiles and renders fine.
        // Deleting this branch lets "\" fall through to the plain "any other character" case below,
        // matching what the compiler actually does with it.
        if (ch === quoteChar) {
          quoteChar = null;
          currentAttrName = null; // the attribute's value just ended
          i++;
          continue;
        }
        if (ch === "{") {
          handleMustache(true);
          continue;
        }
        i++;
        continue;
      }
      if (ch === '"' || ch === "'") {
        quoteChar = ch;
        i++;
        continue;
      }
      if (ch === "{") {
        handleMustache(true);
        currentAttrName = null; // a bare `attr={…}` value just ended
        continue;
      }
      if (ch === ">") {
        inTag = false;
        currentAttrName = null;
        i++;
        continue;
      }
      if (ch === "=") {
        // Attribute names are short and simple (`\w`/`-`), so a small bounded lookback is safe here in a
        // way it never was for the VALUE side (which is exactly CPE-1776's defect): this only has to span
        // one identifier, not an arbitrarily long accessible label.
        //
        // N3 (reviewer, round 2 — undocumented until now): this captures the FULL hyphenated attribute
        // name (`[\w-]+`), so `currentAttrName` for `data-title=` is the literal string `"data-title"`,
        // not `"title"`. isRenderPosition's `currentAttrName === "title"` check is therefore an EXACT
        // match, not a substring one — a genuine behavior change from CPE-1776's old textual-lookback
        // regex (`/\btitle=\{?\s*$/`), which matched on the WORD "title" anywhere a `\b` boundary put it,
        // including right after the hyphen in `data-title=` (`-` is non-word, `t` is word, so `\b` sits
        // exactly there) — i.e. the old code treated `data-title=`/`data-alt=` as equivalent to the real
        // `title=`/`alt=` attributes, a false positive. The new exact-match behavior is MORE correct and
        // matches this module's own documented `data-*` boundary above ("data-fullpath={e.path} staying
        // green is the INTENDED behavior"), but it is still a real behavior change from `main`, recorded
        // here rather than left implicit.
        const nameLookback = markup.slice(Math.max(0, i - 64), i);
        const nameMatch = /([\w-]+)\s*$/.exec(nameLookback);
        currentAttrName = nameMatch ? nameMatch[1] : null;
        i++;
        continue;
      }
      i++;
      continue;
    }
    // Outside any tag (body text).
    if (ch === "<") {
      if (markup.startsWith("<!--", i)) {
        const end = markup.indexOf("-->", i);
        // Same fail-open shape as the unmatched-brace case: an unterminated comment used to silently
        // truncate the scan to EOF instead of erroring.
        if (end === -1) fail(i, `unterminated comment "<!--" — no matching "-->" was found for it`);
        i = end + 3;
        continue;
      }
      if (markup[i + 1] === "/") {
        const end = markup.indexOf(">", i);
        if (end === -1) fail(i, `unterminated closing tag "</" — no matching ">" was found for it`);
        i = end + 1;
        continue;
      }
      // CPE-1761 #3: a bare `<` that is NOT the start of a real tag/comment/closing-tag (e.g. a
      // comparison written straight into text content, `a < b`) used to be treated as an opening tag
      // anyway, flipping the state machine into `inTag` mode until the next real `>` — silently
      // misclassifying every render in between as an ATTRIBUTE-position mustache (only counted for
      // title=/aria-label=/alt=) instead of a body-text one, suppressing it. Same fail-open direction as
      // #1: fail loudly instead of guessing.
      if (/[A-Za-z]/.test(markup[i + 1] ?? "")) {
        inTag = true;
        i++;
        continue;
      }
      fail(i, `"<" is not followed by a tag name, "/", or "!--", so the scan cannot tell whether this opens markup`);
    }
    if (ch === "{") {
      handleMustache(true);
      continue;
    }
    i++;
  }
  // F1 (reviewer, CPE-1761 attempt 2): reaching EOF still inside a quoted attribute string, or still
  // inside a tag's attribute list at all, is the same fail-open shape as everything above — the loop
  // just falls off the end of `markup` and returns whatever was found so far. An odd number of `'`/`"`
  // in an attribute (e.g. `title='it's a file'`) used to silently swallow every render below it as an
  // ATTRIBUTE-position mustache (suppressed unless title=/aria-label=/alt=) for the rest of the file.
  // Fail loudly instead of letting the scan quietly run out of markup mid-tag.
  if (quoteChar !== null) {
    fail(markup.length - 1, `reached end of file inside an unterminated ${quoteChar} attribute string`);
  }
  if (inTag) {
    fail(markup.length - 1, `reached end of file inside an unterminated tag — no closing ">" was found`);
  }
  return [...offenders].sort(compareOffenders);
}

/** CPE-1768 (widened this round — reviewer B1; widened again CPE-1790): the membership CRITERION — a
 *  `.svelte` file must carry a `REGISTRY` entry in `bidiEscape.guard.test.ts` (even `[]`, once
 *  `findUnsafeRenderLines` proves it draws nothing unsafe) if its source references a value shaped like a
 *  filesystem entry's identity, in ANY of the concrete shapes this codebase actually uses for one:
 *   - a `.name`/`.path`/`.fullPath`/`.oldName`/`.cwd`/`.root`/`.folder`/`.dir`/`.target`/`.fileName`/
 *     `.filePath`/`.linkPath` property access (`entry.name`, `e.path`, `f.fullPath`, `rc.oldName`,
 *     `s.cwd`, `rule.root`, `job.source`, `sig.target`, …),
 *   - a variable of that same vocabulary DECLARED with `let`/`export let` (`export let root`, `export let
 *     fileName`, or a plain `let folder = outDir` deriving one from a differently-named prop — the exact
 *     "intermediate variable" shape this module's own header (line 7) already names as a render pattern),
 *   - a call to one of this codebase's own path-splitting helpers: `baseName(`/`basename(`/`parentDir(`,
 *   - an `{#each … as { name }}`/`{ path }` (or plain JS `const { name } = …`) destructuring pattern,
 *   - `["name"]`/`['path']` bracket property access, or
 *   - CPE-1790: a call to the escape helpers themselves, `displaySafeName(`/`displaySafePath(`. Every
 *     other trigger above recognizes a component by the SHAPE of the value it receives; this one instead
 *     recognizes a component by the shape of what it DOES with an opaque value it received under a
 *     generic prop name (`title`/`message`/`error` — none of which match any bullet above) — the "leaf
 *     escapes what it renders" model CPE-1760 confirmed for `MediaPlayer` and CPE-1790 applied to
 *     `ConfirmDialog`/`PasswordPromptDialog`. A component that already calls the escape helper is, by
 *     construction, handling filesystem-derived text and must be pinned in `REGISTRY` so a regression
 *     that later deletes the call (reverting to a raw `{message}`) reds the exact-equality check instead
 *     of silently going unwatched again. Confirmed non-disruptive: every `.svelte` file under
 *     `src/lib/components/` that already calls either helper was independently already a `REGISTRY` key
 *     before this bullet existed (verified by sweeping the codebase at CPE-1790 time), so this addition
 *     only ever newly flags a file the moment it starts (or stops, at the git-blame level) doing exactly
 *     the thing REGISTRY exists to pin.
 *  The first CPE-1768 pass shipped only the first bullet's five spellings plus `export let name`/`path` —
 *  literally a five-property regex, the same "regex zoo" shape the header above (lines 4-10) says this
 *  module's CORE engine already replaced, just relocated from expressions to filenames. Review confirmed
 *  it missed real, already-shipping renders: `WorkbenchView.svelte`'s `export let root` (rendered raw in
 *  body text), `CreateCertDialog.svelte`'s `let folder = outDir` (rendered raw in a `title=` tooltip), and
 *  `RepairLinkDialog.svelte`'s `export let linkPath` (a symlink target, the component's whole subject).
 *
 *  **In scope** (example): `<div title={entry.path}>{entry.name}</div>` — the ordinary shape of a
 *  directory-listing row, a dialog operating on selected files, or a component fed a name/path prop by
 *  its parent (`<TagEditor name={entry.name} />`); equally, `export let root: string;` — any prop whose
 *  OWN NAME reads as a filesystem location, regardless of what expression eventually renders it; equally,
 *  `<p>{displaySafeName(message)}</p>` — a component whose OWN prop name is generic but which itself
 *  calls the escape helper on arrival.
 *  **Out of scope** (example): `StatusBar.svelte` as it stands today — item counts, a disk-free label, git
 *  branch/ahead/behind, a plain `notice` string. Nothing in it is a filesystem entry's name or path (or a
 *  variable NAMED like one), and it doesn't call either escape helper, so it doesn't match this pattern
 *  and carries no `REGISTRY` entry. The moment it (or any file) starts rendering one of these shapes,
 *  `isCandidateComponent` flags it and the membership test in `bidiEscape.guard.test.ts` reds until it's
 *  registered — registration becomes the thing you cannot forget, not the thing you must remember. See
 *  that test's own mutation-probe demonstration (CPE-1768 AC: the review's original `StatusBar.svelte`
 *  injected-render probe, reproduced without leaving a permanent unregistered fixture in the tree).
 *
 *  **Still not exhaustive (CPE-1790's own honest boundary):** a component with a generic prop name that
 *  neither matches a name/path SHAPE nor yet calls `displaySafeName`/`displaySafePath` — the exact bug
 *  this ticket fixed for `ConfirmDialog`/`PasswordPromptDialog` before they called the helper — is still
 *  invisible to this criterion until either it starts calling the helper (this bullet) or a future caller
 *  happens to pass it something matching a name/path SHAPE (the bullets above). There is no general
 *  textual signal for "this opaque prop will someday carry a filesystem name": that is exactly the
 *  component-prop-pass-through boundary this module's header already states rather than hides. CPE-1790's
 *  sweep of `export let (label|caption|text|detail|body|notice|heading|desc|description|content|summary)`
 *  across every component found one further structural sibling, `MacroParamPrompt.svelte` (same
 *  `title`/`message` shape as `PasswordPromptDialog`, reused for the macro-parameter prompt) — currently
 *  fed only static copy by its one caller, so left unregistered rather than defensively escaped, but
 *  worth the same treatment the moment a caller composes it around a real name.
 *
 *  Deliberately broad and heuristic, not a parser — the exact inversion of the "regex zoo" this module's
 *  core engine (`isSafeExpr`/`findUnsafeRenderLines`) replaced. That's fine here because a false positive
 *  costs one `REGISTRY` line (often `[]`) while a false negative is the actual hazard CPE-1768 exists to
 *  close, so over-inclusion is the safe direction to err in — `root`/`folder`/`dir`/`target` are common
 *  enough English words that this WILL also match plenty of non-filesystem uses (a macro's name, a saved
 *  search's name, an HTML `<input name>`, a DOM/menu "root" element) — that's expected, not a bug: those
 *  files still get a `REGISTRY` entry (usually `[]` or a benign-recorded one), same as any other
 *  candidate, rather than being silently exempted by a narrower pattern that could just as easily miss a
 *  real one. This is still not a claim of exhaustiveness — it is the concrete vocabulary this codebase is
 *  KNOWN to use for a filesystem location today; a genuinely novel prop name (`export let whereItLives`)
 *  is still invisible to it, same honest boundary every heuristic in this file states rather than hides. */
export const CANDIDATE_PATTERN =
  /\.(?:name|path|fullPath|oldName|cwd|root|folder|dir|target|fileName|filePath|linkPath)\b|(?:export\s+)?let\s+(?:name|path|fullPath|oldName|cwd|root|folder|dir|target|fileName|filePath|linkPath)\b|\b(?:baseName|basename|parentDir|displaySafeName|displaySafePath)\s*\(|\{\s*(?:name|path)\s*\}|\[\s*["'](?:name|path)["']\s*\]/;

/** True if `src` (a whole `.svelte` file's text — script and markup both, unlike `findUnsafeRenderLines`
 *  which only ever looks at markup) matches `CANDIDATE_PATTERN` and therefore must carry a `REGISTRY`
 *  entry. Exported separately from the membership test itself so that test can be demonstrated against an
 *  in-memory string (CPE-1768's AC: "adding a new component… shows CI red") without needing a permanent
 *  new fixture file on disk — the same convention this file's own `RenderScanError`/render-position tests
 *  already use for reproducing a mechanism instead of just asserting it in prose. */
export function isCandidateComponent(src: string): boolean {
  return CANDIDATE_PATTERN.test(src);
}
