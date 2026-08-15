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
 * those), or an explicitly allowlisted line. There is no third bucket — an expression that isn't
 * provably safe is unsafe by default, regardless of what it's named or shaped like. That is why this
 * module does not special-case `.name`/`.path`/`baseName` at all: `isSafeExpr` doesn't know or care what
 * an identifier is called.
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
 *     negative — the safe direction to be wrong in, since it costs an allowlist entry rather than lets a
 *     spoof through. Rewriting as `{#if kind === "dir"}{displaySafeName(a)}{:else}…{/if}` (this repo's
 *     own idiom) sidesteps it entirely, since each branch is then its own independently-checked mustache.
 */

/** Strip every `<script>…</script>` / `<style>…</style>` block, replacing its content with the same
 *  number of newlines so every surviving line keeps its real line number. Only markup is scanned below —
 *  script-side code legitimately contains countless `{…}` that are not template renders at all. */
export function stripNonMarkup(src: string): string {
  return src.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi, (m) => "\n".repeat(m.split("\n").length - 1));
}

/** Find the index of the `}` that closes the `{` at `openIdx`, skipping over nested `'...'`/`"..."`
 *  string literals and `` `...` `` template literals (including their own nested `${…}`). Returns -1 if
 *  unterminated. */
export function findMatchingBrace(s: string, openIdx: number): number {
  let depth = 0;
  let i = openIdx;
  while (i < s.length) {
    const ch = s[i];
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

/** Find the index of the `)` that closes the `(` at `openIdx`, with the same string/template awareness
 *  as `findMatchingBrace`. Returns -1 if unterminated. */
export function findMatchingParen(s: string, openIdx: number): number {
  let depth = 0;
  let i = openIdx;
  while (i < s.length) {
    const ch = s[i];
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
 *  whole engine: no shape is named, so no shape can be missed by omission. */
export function hasUnsafeIdentifier(code: string): boolean {
  let i = 0;
  while (i < code.length) {
    const ch = code[i];
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

/** True if the character(s) immediately before `idx` (the position of a mustache's `{`) put it in a
 *  render position. `inTag` (see `findUnsafeRenderLines`'s state machine) disambiguates the two contexts
 *  that share the "preceded by `}`" shape:
 *   - OUTSIDE a tag (body text): preceded by `>` (a normal tag boundary) or `}` (CPE-1757 round 2's fix —
 *     a render sitting directly after `{#if}`/`{:else}`/`{/each}`'s own closing brace, not just after
 *     `>`, the exact shape review's probe caught this engine missing).
 *   - INSIDE a tag's attribute list: `}` here almost always closes a PRIOR shorthand prop or directive
 *     (`{density} {currentPath}`, `bind:value={x}`), never a render — so only `title=`/`aria-label=`/
 *     `alt=`, given directly as `attr={…}` or embedded in a quoted `attr="…{…}…"` value, count. */
function isRenderPosition(markup: string, idx: number, inTag: boolean): boolean {
  const before = markup.slice(Math.max(0, idx - 200), idx);
  if (!inTag) return /[>}]\s*$/.test(before);
  if (/\b(?:title|aria-label|alt)=\{?\s*$/.test(before)) return true;
  if (/\b(?:title|aria-label|alt)="[^"]*$/.test(before)) return true;
  return false;
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
export function findUnsafeRenderLines(fileSrc: string): number[] {
  const markup = stripNonMarkup(fileSrc);
  const offenderLines = new Set<number>();
  let i = 0;
  let inTag = false;
  let quoteChar: string | null = null;

  const handleMustache = (renderCandidate: boolean) => {
    const close = findMatchingBrace(markup, i);
    const end = close === -1 ? markup.length : close;
    const inner = markup.slice(i + 1, end);
    const trimmed = inner.trimStart();
    const isControlTag = !inTag && /^[#:/]|^@const\b|^@debug\b/.test(trimmed);
    const isHtmlTag = !inTag && /^@html\b/.test(trimmed);
    if (renderCandidate && !isControlTag) {
      const expr = isHtmlTag ? trimmed.replace(/^@html\s*/, "") : inner;
      if (isRenderPosition(markup, i, inTag) && !isSafeExpr(expr)) {
        offenderLines.add(markup.slice(0, i).split("\n").length);
      }
    }
    i = close === -1 ? markup.length : close + 1;
  };

  while (i < markup.length) {
    const ch = markup[i];
    if (inTag) {
      if (quoteChar) {
        if (ch === "\\") {
          i += 2;
          continue;
        }
        if (ch === quoteChar) {
          quoteChar = null;
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
        continue;
      }
      if (ch === ">") {
        inTag = false;
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
        i = end === -1 ? markup.length : end + 3;
        continue;
      }
      if (markup[i + 1] === "/") {
        const end = markup.indexOf(">", i);
        i = end === -1 ? markup.length : end + 1;
        continue;
      }
      inTag = true;
      i++;
      continue;
    }
    if (ch === "{") {
      handleMustache(true);
      continue;
    }
    i++;
  }
  return [...offenderLines].sort((a, b) => a - b);
}
