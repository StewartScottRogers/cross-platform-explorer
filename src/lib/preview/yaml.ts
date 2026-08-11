/**
 * A BOUNDED-SUBSET YAML parser (CPE-1617, epic CPE-1568 slice 7). Framework-free (no Svelte import),
 * same "pure module behind the preview component" convention as `toml.ts`/`jsonTree.ts`/`notebook.ts`.
 *
 * **This is deliberately NOT a full YAML implementation.** Full YAML (anchors/aliases/merge keys, tags,
 * multi-document streams, complex mapping keys, flow collections spanning lines, …) is genuinely hard to
 * get right, and a half-correct parser that silently mis-parses a construct it doesn't fully understand
 * is worse than no structured view at all — it would show a confidently wrong tree for someone's real
 * config. Rather than attempt full coverage, {@link parseYaml} recognises a BOUNDED SUBSET covering
 * ordinary, hand-written config shapes AND the constructs a PR #833 review round found were common
 * enough in real files (a 20-real-file UAT) to be worth the extra work:
 *
 *   - block mappings (`key: value`, nested by indentation) and block sequences (`- item`, including the
 *     common `- key: value` shorthand that starts a mapping on the dash's own line)
 *   - **indentless sequences** (`status:\n- item\n- item` — the sequence sits at the SAME column as its
 *     own key, not indented under it). Standard, spec-legal YAML and extremely common in hand-written
 *     config; PyYAML parses it fine, and an earlier revision of this parser wrongly reported "unexpected
 *     indentation" on it (real file: a repo's own `gateway/assets/status_phrases.yaml`).
 *   - **block scalars** (`|` literal, `>` folded, with `-`/`+` chomping indicators) — see the dedicated
 *     doc comment on {@link consumeBlockScalarBody} for the full assessment of why these are tractable
 *     (unlike anchors/aliases/tags) and were added: they're everywhere in real YAML (every GitHub
 *     Actions `run: |` step) and are simple enough with this parser's line-based design to implement
 *     honestly, without the "guess and hope" risk anchors/tags/complex-keys carry.
 *   - plain/single/double-quoted scalars, `null`/`~`/empty, `true`/`false`, integers, floats. A
 *     double-quoted scalar MAY fold across physical lines (YAML line-folding) — see
 *     {@link foldMultilineDoubleQuoted}'s doc comment for the exact bounded support and its one
 *     documented risk. Single-quoted multi-line folding is NOT supported (rarer in practice; falls to
 *     the existing "unterminated string" error like before).
 *   - single-LINE flow collections (`[a, b]`, `{a: 1, b: 2}`) — flow collections that span multiple
 *     physical lines are explicitly unsupported (see below), not guessed at
 *   - `#` comments, blank lines, and a single leading `---` document-start marker
 *
 * **Anything outside that subset degrades EXPLICITLY, with a stated reason, rather than being ignored or
 * mis-rendered.** {@link YamlParseResult} distinguishes two failure shapes: `unsupported: true` for a
 * construct this parser recognises but deliberately doesn't implement (anchors `&`, aliases `*`,
 * explicit tags `!`, a block scalar's rare explicit-numeric-indentation-indicator form (`|2`), complex
 * `? key` mappings, multiple `---` documents, tab indentation, a flow collection spanning lines), and
 * `unsupported: false` for a genuine syntax error (bad indentation, an unterminated quote, `key: value:
 * value` — a bare unquoted colon inside a value, which every real YAML tool also rejects — a malformed
 * flow collection, …). **This distinction is deliberately kept sharp**: invalid input is always a syntax
 * error, never a silent "degrade" — conflating the two would hide a genuine mistake in someone's file
 * behind the same soft "not supported" framing a merely-unimplemented construct gets. The preview
 * component (`YamlTomlPreview.svelte`) renders both as "can't show a structured view: <reason>" plus the
 * raw text — never as an empty/blank pane, and never silently rendering a guessed, possibly-wrong tree.
 *
 * **Untrusted input / DoS safety**, same discipline as `toml.ts`: mapping objects use
 * `Object.create(null)` (no `__proto__` key footgun on attacker-controlled key strings), and both block
 * recursion and flow-collection recursion are bounded by {@link MAX_DEPTH} / the flow parser's own depth
 * cap, so a pathological file (deeply nested sequences, or thousands of nested `[[[[…` on one line)
 * fails with a clear error instead of a stack overflow. Line splitting + per-line scanning is a single
 * linear pass over the (already read-capped, see `PREVIEW_MAX_BYTES`) input text — no separate
 * line-count cap is needed here (unlike `logViewer.ts`'s `MAX_LINES`): the rendered TREE is what could
 * blow up the DOM, and that's already bounded by `jsonTree.ts`'s `MAX_CHILDREN`/`AUTO_COLLAPSE_DEPTH`,
 * reused as-is by feeding the parsed value through `JsonTree.svelte`. The multi-line quote fold
 * ({@link foldMultilineDoubleQuoted}) and block-scalar consumption ({@link consumeBlockScalarBody}) both
 * still only ever advance forward through the physical-line array once — no re-scanning — so neither
 * changes this module's overall linear-time bound.
 */

export type YamlParseResult =
  | { ok: true; value: unknown }
  | { ok: false; error: string; unsupported: boolean };

const MAX_DEPTH = 200;
const MAX_FLOW_DEPTH = 100;

class YamlUnsupported extends Error {}
class YamlSyntaxError extends Error {}

interface Line {
  indent: number;
  content: string;
  lineNo: number;
  /** Set only when {@link tokenizeLines} recognised this line's value as a block-scalar header (`|`/
   *  `>`, optionally chomped) and already consumed + resolved its body from the raw physical lines —
   *  the parse-phase (`parseMapping`/`parseSequence`) reads this directly instead of treating `content`
   *  as an ordinary scalar/nested-block trigger. See {@link consumeBlockScalarBody}. */
  blockScalarValue?: string;
}

/** Strips a `#` comment, respecting single/double-quoted runs so a `#` inside a string isn't treated
 *  as a comment start. Matches common YAML practice: an unquoted `#` at column 0 or preceded by
 *  whitespace starts a comment. */
function stripComment(s: string): string {
  let inSingle = false;
  let inDouble = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (inSingle) {
      if (c === "'") {
        if (s[i + 1] === "'") {
          i++;
          continue;
        }
        inSingle = false;
      }
      continue;
    }
    if (inDouble) {
      if (c === "\\") {
        i++;
        continue;
      }
      if (c === '"') inDouble = false;
      continue;
    }
    if (c === "'") {
      inSingle = true;
      continue;
    }
    if (c === '"') {
      inDouble = true;
      continue;
    }
    if (c === "#" && (i === 0 || s[i - 1] === " " || s[i - 1] === "\t")) {
      return s.slice(0, i);
    }
  }
  return s;
}

/** Whether `s` (a line already run through {@link stripComment}) ends while still "inside" an open
 *  double-quote — i.e. it contains an opening `"` with no matching close on the same line. Reuses the
 *  same single/double quote state machine as `stripComment` (so the two never disagree about where a
 *  quote starts) and honours `\"`/`\\` escapes. Used to detect a double-quoted scalar that needs
 *  {@link foldMultilineDoubleQuoted} to fold in continuation lines (finding #7, CPE-1617 PR #833
 *  review). Safe to call on a line whose quote DOES close normally — `stripComment` never alters
 *  anything while a quote is open, so this sees exactly the same quote boundaries either function would. */
function endsInsideDoubleQuote(s: string): boolean {
  let inSingle = false;
  let inDouble = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (inSingle) {
      if (c === "'") {
        if (s[i + 1] === "'") {
          i++;
          continue;
        }
        inSingle = false;
      }
      continue;
    }
    if (inDouble) {
      if (c === "\\") {
        i++;
        continue;
      }
      if (c === '"') inDouble = false;
      continue;
    }
    if (c === "'") {
      inSingle = true;
      continue;
    }
    if (c === '"') {
      inDouble = true;
      continue;
    }
  }
  return inDouble;
}

/** Index of the `"` that closes an ALREADY-OPEN double-quoted span within `s` (i.e. `s` is read as if
 *  scanning started already inside the quote) — honours `\"`/`\\` escapes. -1 if `s` doesn't close it. */
function findClosingDoubleQuote(s: string): number {
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "\\") {
      i++;
      continue;
    }
    if (s[i] === '"') return i;
  }
  return -1;
}

/** Safety valve for {@link foldMultilineDoubleQuoted}: how many CONTINUATION physical lines one folded
 *  scalar may consume before this parser gives up folding and falls back to normal (single-line)
 *  handling — which then surfaces the pre-existing "unterminated double-quoted string" error, same as
 *  before this feature existed. Real folded strings (translation catalogs, long descriptions) wrap
 *  across a handful of lines; this is generous headroom without risking an ACTUALLY unterminated quote
 *  silently swallowing hundreds of unrelated subsequent lines into one bogus scalar. */
const MAX_QUOTE_FOLD_LINES = 200;

/**
 * YAML double-quoted flow scalars MAY wrap across physical lines ("line folding"): an unescaped line
 * break inside the quotes folds to a single space, and each continuation line's own leading whitespace
 * is not part of the content. This is common in real-world files (a real 403-line Afrikaans locale
 * catalog needed exactly this — CPE-1617 PR #833 review finding #7) — supporting it, rather than
 * reporting "unterminated string" on ordinary valid YAML, is the difference between a false positive and
 * a real feature.
 *
 * Only DOUBLE-quoted scalars fold this way here (single-quoted scalars fold too in real YAML, but
 * aren't handled — a documented, bounded gap: unclosed single-quoted values still report "unterminated
 * string" as before). A `#` inside the fold is literal, never a comment, so continuation lines are read
 * from the RAW physical text (never through {@link stripComment}).
 *
 * **Known limitation, stated plainly**: this is a heuristic, not a real multi-line-aware tokenizer — it
 * decides "does this line need folding" purely from whether it ends with an unclosed `"`, then keeps
 * consuming subsequent RAW physical lines until one closes it. A genuinely unterminated quote (a real
 * mistake in the file) could in principle keep matching some LATER unrelated `"` and silently fold
 * unrelated lines together instead of erroring — {@link MAX_QUOTE_FOLD_LINES} bounds how far this can
 * run before giving up and falling back to the normal (safe) "unterminated string" error path.
 *
 * Returns `null` when `content` doesn't end with an open double quote (the common case, no folding
 * needed) OR the fold never closes within the line cap (falls back to normal single-line handling).
 */
function foldMultilineDoubleQuoted(
  physical: string[],
  startIndex: number,
  content: string,
): { content: string; nextPhysicalIndex: number } | null {
  if (!endsInsideDoubleQuote(content)) return null;

  const parts = [content];
  let i = startIndex + 1;
  let closed = false;
  while (i < physical.length && parts.length <= MAX_QUOTE_FOLD_LINES) {
    const raw = physical[i].replace(/^[ \t]+/, "");
    const close = findClosingDoubleQuote(raw);
    if (close === -1) {
      parts.push(raw);
      i++;
      continue;
    }
    parts.push(raw.slice(0, close + 1));
    closed = true;
    i++;
    break;
  }
  if (!closed) return null; // never closed (or exceeded the cap) — fall back to normal handling
  return { content: parts.join(" "), nextPhysicalIndex: i };
}

function isSeqItem(content: string): boolean {
  return content === "-" || content.startsWith("- ");
}

/** Parses a possibly-quoted scalar starting at `s[start]` (must be `'` or `"`). Returns the decoded
 *  value plus the index just past the closing quote. */
function readQuotedScalar(s: string, start: number): { value: string; end: number } {
  const quote = s[start];
  let i = start + 1;
  let out = "";
  if (quote === "'") {
    while (i < s.length) {
      if (s[i] === "'") {
        if (s[i + 1] === "'") {
          out += "'";
          i += 2;
          continue;
        }
        return { value: out, end: i + 1 };
      }
      out += s[i];
      i++;
    }
    throw new YamlSyntaxError("unterminated single-quoted string");
  }
  while (i < s.length) {
    const c = s[i];
    if (c === "\\") {
      const esc = s[i + 1];
      if (esc === undefined) throw new YamlSyntaxError("unterminated double-quoted string");
      switch (esc) {
        case '"': out += '"'; i += 2; break;
        case "\\": out += "\\"; i += 2; break;
        case "n": out += "\n"; i += 2; break;
        case "t": out += "\t"; i += 2; break;
        case "r": out += "\r"; i += 2; break;
        case "0": out += "\0"; i += 2; break;
        case "u": {
          const hex = s.slice(i + 2, i + 6);
          if (hex.length < 4 || /[^0-9A-Fa-f]/.test(hex)) throw new YamlSyntaxError("invalid \\u escape");
          out += String.fromCodePoint(parseInt(hex, 16));
          i += 6;
          break;
        }
        default: out += esc; i += 2;
      }
      continue;
    }
    if (c === '"') return { value: out, end: i + 1 };
    out += c;
    i++;
  }
  throw new YamlSyntaxError("unterminated double-quoted string");
}

/** Splits `key: value` (or a bare `key:`) from a logical line, tolerant of a quoted key. Returns `null`
 *  when `content` doesn't look like a mapping entry at all — the caller decides whether that's a
 *  syntax error or "not a mapping line" depending on context. */
function splitKeyValue(content: string): { key: string; rest: string } | null {
  if (content[0] === '"' || content[0] === "'") {
    let parsed: { value: string; end: number };
    try {
      parsed = readQuotedScalar(content, 0);
    } catch {
      return null;
    }
    let j = parsed.end;
    while (content[j] === " ") j++;
    if (content[j] !== ":") return null;
    j++;
    return { key: parsed.value, rest: content.slice(j).trim() };
  }
  for (let i = 0; i < content.length; i++) {
    if (content[i] === ":" && (i === content.length - 1 || content[i + 1] === " ")) {
      const key = content.slice(0, i).trim();
      if (key === "") return null;
      return { key, rest: content.slice(i + 1).trim() };
    }
  }
  return null;
}

/** Block-scalar header pattern (no explicit numeric indentation indicator — see the module doc comment
 *  and `rejectUnsupportedPrefix` for why that rarer form stays an explicit "unsupported" degrade). */
const BLOCK_SCALAR_HEADER_RE = /^([|>])([+-]?)$/;

/** Whether `content`'s VALUE portion (after a mapping key's `key:`, after a sequence item's `- `, or
 *  `content` itself for a bare top-level line) is exactly a block-scalar header (`|`, `>`, `|-`, `|+`,
 *  `>-`, `>+`). Reuses `splitKeyValue`/`isSeqItem` (safe to call from the tokenizer — both are pure
 *  functions of a single line's text) so this recognises the SAME two shapes `parseMapping`/
 *  `parseSequence` would otherwise treat as an ordinary scalar value, including the `- key: |` dash
 *  shorthand. */
function blockScalarHeader(content: string): { style: "|" | ">"; chomp: "" | "+" | "-" } | null {
  let candidate = content;
  if (isSeqItem(content)) {
    candidate = content === "-" ? "" : content.slice(1).trimStart();
  }
  const kv = splitKeyValue(candidate);
  if (kv) candidate = kv.rest;
  const m = BLOCK_SCALAR_HEADER_RE.exec(candidate);
  return m ? { style: m[1] as "|" | ">", chomp: m[2] as "" | "+" | "-" } : null;
}

/** Folded style (`>`): a line break between two non-blank content lines becomes a single space; a blank
 *  line becomes a real newline (YAML's folding rule). Bounded implementation — doesn't special-case "a
 *  more-indented line stays literal", a real `>` nuance this preview doesn't attempt (documented
 *  simplification, same spirit as `toml.ts`'s date-as-string choice). */
function foldBlockLines(lines: string[]): string {
  let out = "";
  for (let i = 0; i < lines.length; i++) {
    if (i > 0) out += lines[i] === "" || lines[i - 1] === "" ? "\n" : " ";
    out += lines[i];
  }
  return out;
}

/**
 * Consumes a block scalar's body directly off the RAW physical lines starting at `startIndex` (NOT the
 * already blank-line-filtered {@link Line} array — blank lines inside a block scalar are significant
 * content, unlike blank lines between ordinary block-mapping/sequence entries, which is exactly why this
 * has to run during {@link tokenizeLines}'s raw pass rather than later against the filtered stream).
 *
 * **Why block scalars were worth doing (and anchors/aliases/tags weren't):** a PR #833 review asked for
 * a deliberate assessment, not silence. Unlike anchors/aliases (which need a document-wide symbol table
 * and reference resolution) or tags (open-ended, arbitrary semantics) or complex `? key` mappings
 * (fundamentally break the "one physical line, one logical unit" model this whole parser is built on),
 * a block scalar's body is just "the physical lines indented more than the header, joined by one of two
 * simple rules, then chomped" — a linear scan this line-based design already does naturally. It's also
 * the single biggest real-world payoff: every GitHub Actions workflow's `run: |` step is a block scalar,
 * and it was the largest driver of "valid YAML degrades to plain text" in real repos.
 *
 * Indentation is auto-detected from the first non-blank content line (must be indented more than
 * `parentIndent`, i.e. the block-scalar HEADER's own column) — a content line indented further still
 * than that detected base keeps its extra leading whitespace as literal text (e.g. a nested `if` block
 * inside a `run: |` shell script). Trailing blank lines are never treated as part of the scalar's own
 * content (they're the gap before whatever comes next); chomping then governs exactly how many trailing
 * newlines the final value keeps: default ("clip") keeps exactly one, `-` ("strip") keeps none, and `+`
 * ("keep") is treated the same as clip here — a documented simplification (this parser discards
 * trailing blank lines before chomping ever sees them, so `+`'s "preserve every trailing blank line"
 * nuance isn't reproduced; `+` is the least common of the three chomping indicators in practice).
 */
function consumeBlockScalarBody(
  physical: string[],
  startIndex: number,
  parentIndent: number,
  style: "|" | ">",
  chomp: "" | "+" | "-",
): { value: string; next: number } {
  let i = startIndex;
  let blockIndent = -1;
  const rawContentLines: (string | null)[] = []; // null marks a blank line

  while (i < physical.length) {
    const line = physical[i];
    if (line.trim() === "") {
      rawContentLines.push(null);
      i++;
      continue;
    }
    const leading = /^[ \t]*/.exec(line)?.[0] ?? "";
    const indent = leading.length;
    if (blockIndent === -1) {
      if (indent <= parentIndent) break; // no content lines at all — an empty block scalar
      blockIndent = indent;
    }
    if (indent < blockIndent) break; // dedent — end of this block scalar's span
    rawContentLines.push(line.slice(blockIndent));
    i++;
  }

  // Trailing blank lines collected above but not followed by more real content aren't part of the
  // span — give the corresponding physical lines back to the normal tokenizer (harmless: they're
  // blank, so it just skips them) and let chomping alone decide the value's trailing newline count.
  while (rawContentLines.length > 0 && rawContentLines[rawContentLines.length - 1] === null) {
    rawContentLines.pop();
    i--;
  }

  const lines = rawContentLines.map((l) => l ?? "");
  let text = style === "|" ? lines.join("\n") : foldBlockLines(lines);

  if (chomp === "-") {
    text = text.replace(/\n+$/, "");
  } else {
    // Default ("clip") and "+" ("keep", simplified to clip — see doc comment): exactly one trailing
    // newline when there was any content at all, none for a genuinely empty block scalar.
    text = lines.length > 0 ? text.replace(/\n+$/, "") + "\n" : "";
  }

  return { value: text, next: i };
}

/** Splits raw text into non-blank, comment-stripped logical lines with their indent depth — folding a
 *  multi-line double-quoted scalar ({@link foldMultilineDoubleQuoted}) or consuming a block scalar's
 *  body ({@link consumeBlockScalarBody}) into a single logical {@link Line} entry where applicable.
 *  Throws {@link YamlUnsupported} for tab-indented lines or a directive line (`%YAML`, `%TAG`) — both
 *  real YAML constructs this parser doesn't attempt. */
function tokenizeLines(raw: string): Line[] {
  const physical = raw.split(/\r\n|\n/);
  const out: Line[] = [];
  let i = 0;
  while (i < physical.length) {
    const rawLine = physical[i];
    if (rawLine.trim() === "") {
      i++;
      continue;
    }
    const leading = /^[ \t]*/.exec(rawLine)?.[0] ?? "";
    if (leading.includes("\t")) {
      throw new YamlUnsupported("tab characters in indentation are not supported by this preview");
    }
    const indent = leading.length;
    const content = stripComment(rawLine.slice(indent)).replace(/\s+$/, "");
    if (content === "") {
      i++;
      continue;
    }
    if (indent === 0 && content.startsWith("%")) {
      throw new YamlUnsupported("YAML directives are not supported by this preview");
    }

    const folded = foldMultilineDoubleQuoted(physical, i, content);
    if (folded) {
      out.push({ indent, content: folded.content, lineNo: i + 1 });
      i = folded.nextPhysicalIndex;
      continue;
    }

    const header = blockScalarHeader(content);
    if (header) {
      const body = consumeBlockScalarBody(physical, i + 1, indent, header.style, header.chomp);
      out.push({ indent, content, lineNo: i + 1, blockScalarValue: body.value });
      i = body.next;
      continue;
    }

    out.push({ indent, content, lineNo: i + 1 });
    i++;
  }
  return out;
}

/** Throws {@link YamlUnsupported} when `rest` (a value, right after `:` or `- `) starts with a
 *  construct this parser deliberately doesn't implement. Bounded detection — see the module doc
 *  comment: this catches the common case (the construct appearing at the point of assignment), not
 *  every position a real YAML anchor/tag could legally appear. The plain (no-digit) block-scalar forms
 *  are handled earlier, in `tokenizeLines`/`blockScalarHeader` — by the time `rest` reaches this
 *  function, only the rarer explicit-numeric-indentation-indicator form (`|2`, `>1-`) can still match. */
function rejectUnsupportedPrefix(rest: string): void {
  if (rest === "") return;
  if (rest[0] === "&") throw new YamlUnsupported("YAML anchors (&) are not supported by this preview");
  if (rest[0] === "*") throw new YamlUnsupported("YAML aliases (*) are not supported by this preview");
  if (rest[0] === "!") throw new YamlUnsupported("YAML tags (!) are not supported by this preview");
  if (/^[|>][+-]?\d+$/.test(rest)) {
    throw new YamlUnsupported(
      "a block scalar's explicit numeric indentation indicator (e.g. |2) is not supported by this preview",
    );
  }
}

function interpretPlainScalar(raw: string): unknown {
  const s = raw.trim();
  if (s === "" || s === "~" || s === "null" || s === "Null" || s === "NULL") return null;
  if (s === "true" || s === "True" || s === "TRUE") return true;
  if (s === "false" || s === "False" || s === "FALSE") return false;
  if (/^[+-]?\d+$/.test(s)) return Number(s);
  if (/[.eE]/.test(s) && /^[+-]?(\d+\.\d*|\.\d+|\d+)([eE][+-]?\d+)?$/.test(s)) return Number(s);
  return s;
}

/** A single-line flow-collection parser (`[...]`/`{...}`), used for a value that starts with `[`/`{`.
 *  Bounded by {@link MAX_FLOW_DEPTH} against pathological nesting on one very long line. */
class FlowParser {
  pos = 0;
  constructor(private readonly s: string) {}

  eof(): boolean {
    return this.pos >= this.s.length;
  }
  private peek(offset = 0): string | undefined {
    return this.s[this.pos + offset];
  }
  skipWs(): void {
    while (!this.eof() && (this.peek() === " " || this.peek() === "\t")) this.pos++;
  }

  parseValue(depth = 0): unknown {
    if (depth > MAX_FLOW_DEPTH) throw new YamlSyntaxError("flow collection is nested too deeply");
    this.skipWs();
    if (this.eof()) throw new YamlSyntaxError("expected a value");
    const c = this.peek() as string;
    if (c === "[") return this.parseArray(depth);
    if (c === "{") return this.parseObject(depth);
    if (c === '"' || c === "'") {
      const q = readQuotedScalar(this.s, this.pos);
      this.pos = q.end;
      return q.value;
    }
    return this.parseScalarToken();
  }

  private parseArray(depth: number): unknown[] {
    this.pos++; // [
    const arr: unknown[] = [];
    this.skipWs();
    if (this.peek() === "]") {
      this.pos++;
      return arr;
    }
    while (true) {
      arr.push(this.parseValue(depth + 1));
      this.skipWs();
      if (this.peek() === ",") {
        this.pos++;
        this.skipWs();
        if (this.peek() === "]") {
          this.pos++;
          return arr;
        }
        if (this.eof()) {
          throw new YamlUnsupported("a flow collection spanning multiple lines is not supported by this preview");
        }
        continue;
      }
      if (this.peek() === "]") {
        this.pos++;
        return arr;
      }
      if (this.eof()) {
        throw new YamlUnsupported("a flow collection spanning multiple lines is not supported by this preview");
      }
      throw new YamlSyntaxError("expected ',' or ']' in flow sequence");
    }
  }

  private parseObject(depth: number): Record<string, unknown> {
    this.pos++; // {
    const obj: Record<string, unknown> = Object.create(null);
    this.skipWs();
    if (this.peek() === "}") {
      this.pos++;
      return obj;
    }
    while (true) {
      this.skipWs();
      let key: string;
      if (this.peek() === '"' || this.peek() === "'") {
        const q = readQuotedScalar(this.s, this.pos);
        this.pos = q.end;
        key = q.value;
      } else {
        const start = this.pos;
        while (!this.eof() && !/[:,}\]]/.test(this.peek() as string)) this.pos++;
        key = this.s.slice(start, this.pos).trim();
        if (key === "") throw new YamlSyntaxError("expected a key in flow mapping");
      }
      this.skipWs();
      if (this.peek() !== ":") throw new YamlSyntaxError("expected ':' in flow mapping");
      this.pos++;
      obj[key] = this.parseValue(depth + 1);
      this.skipWs();
      if (this.peek() === ",") {
        this.pos++;
        this.skipWs();
        if (this.peek() === "}") {
          this.pos++;
          return obj;
        }
        if (this.eof()) {
          throw new YamlUnsupported("a flow collection spanning multiple lines is not supported by this preview");
        }
        continue;
      }
      if (this.peek() === "}") {
        this.pos++;
        return obj;
      }
      if (this.eof()) {
        throw new YamlUnsupported("a flow collection spanning multiple lines is not supported by this preview");
      }
      throw new YamlSyntaxError("expected ',' or '}' in flow mapping");
    }
  }

  private parseScalarToken(): unknown {
    const start = this.pos;
    while (!this.eof() && !/[,\]}]/.test(this.peek() as string)) this.pos++;
    const raw = this.s.slice(start, this.pos).trim();
    if (raw === "") throw new YamlSyntaxError("expected a value");
    return interpretPlainScalar(raw);
  }
}

function parseFlow(text: string): unknown {
  const p = new FlowParser(text);
  const value = p.parseValue();
  p.skipWs();
  if (!p.eof()) throw new YamlSyntaxError(`unexpected content after value: ${text.slice(p.pos)}`);
  return value;
}

/** A bare (unquoted) colon-space (or a trailing bare colon) inside a plain scalar VALUE is invalid YAML
 *  — every real tool (js-yaml, PyYAML) rejects it, because it's indistinguishable from the start of a
 *  nested mapping key that isn't actually indented as one (`message: Error: file not found` needs the
 *  value quoted). This is *the* classic YAML gotcha (CPE-1617 PR #833 review finding #1) — treated as a
 *  genuine syntax error (never a silent accept, never an "unsupported" degrade: it's invalid input, not
 *  an unimplemented construct). A colon NOT followed by a space and not at the very end (`http://x:8080`)
 *  is unaffected — only the ambiguous `": "`/trailing-`":"` shape is rejected. */
const BARE_MAPPING_COLON_RE = /: |:$/;

function parseScalarOrFlow(rest: string): unknown {
  rejectUnsupportedPrefix(rest);
  if (rest[0] === "[" || rest[0] === "{") return parseFlow(rest);
  if (rest[0] === '"' || rest[0] === "'") {
    const q = readQuotedScalar(rest, 0);
    const trailing = rest.slice(q.end).trim();
    if (trailing !== "") throw new YamlSyntaxError(`unexpected content after quoted string: ${trailing}`);
    return q.value;
  }
  if (BARE_MAPPING_COLON_RE.test(rest)) {
    throw new YamlSyntaxError(
      `mapping values are not allowed here — quote the value if the colon is intentional: ${JSON.stringify(rest)}`,
    );
  }
  return interpretPlainScalar(rest);
}

function looksLikeMappingEntry(content: string): boolean {
  return splitKeyValue(content) !== null;
}

function isComplexKeyMarker(content: string): boolean {
  return content === "?" || content.startsWith("? ");
}

/** Reads one line's content/indent, using `virtualContent`/`virtualIndent` in place of the physical
 *  line at `index` for the FIRST line only — the mechanism that lets a `- key: value` dash line be
 *  reparsed as an ordinary mapping/sequence entry starting at the dash's own physical line, with
 *  subsequent sibling lines (real physical lines) continuing it normally. */
function effectiveLine(
  lines: Line[],
  i: number,
  index: number,
  virtualContent: string | undefined,
  virtualIndent: number,
): { content: string; indent: number } {
  if (i === index && virtualContent !== undefined) return { content: virtualContent, indent: virtualIndent };
  return { content: lines[i].content, indent: lines[i].indent };
}

function parseBlock(
  lines: Line[],
  index: number,
  indent: number,
  virtualContent: string | undefined,
  depth: number,
): { value: unknown; next: number } {
  if (depth > MAX_DEPTH) throw new YamlSyntaxError("document is nested too deeply");
  const first = effectiveLine(lines, index, index, virtualContent, indent);
  if (isSeqItem(first.content)) return parseSequence(lines, index, indent, virtualContent, depth);
  return parseMapping(lines, index, indent, virtualContent, depth);
}

function parseSequence(
  lines: Line[],
  index: number,
  indent: number,
  virtualContent: string | undefined,
  depth: number,
): { value: unknown[]; next: number } {
  const arr: unknown[] = [];
  let i = index;
  while (i < lines.length) {
    const { content, indent: lineIndent } = effectiveLine(lines, i, index, virtualContent, indent);
    if (lineIndent !== indent || !isSeqItem(content)) break;

    let rest: string;
    let contentCol: number;
    if (content === "-") {
      rest = "";
      contentCol = indent + 1;
    } else {
      const afterDash = content.slice(1);
      const spaceCount = afterDash.length - afterDash.trimStart().length;
      rest = afterDash.trimStart();
      contentCol = indent + 1 + spaceCount;
    }

    // A block scalar consumed at tokenize time (finding #8) — see `blockScalarValue`'s doc comment.
    // Guarded by `BLOCK_SCALAR_HEADER_RE.test(rest)`: this only claims the item when the DASH ITSELF
    // directly holds the block header (`- |`) — a bare block scalar as the sequence item's own value.
    // When the dash instead introduces an inline key whose value is a block scalar (`- run: |`), `rest`
    // is `"run: |"`, not a bare header — that case falls through to the `looksLikeMappingEntry` branch
    // below, which recurses into `parseMapping` for this SAME physical line; `parseMapping` finds
    // `lines[i].blockScalarValue` there and attaches it to the `run` key correctly, one level down.
    const blockValue = i < lines.length ? lines[i].blockScalarValue : undefined;
    if (blockValue !== undefined && BLOCK_SCALAR_HEADER_RE.test(rest)) {
      arr.push(blockValue);
      i += 1;
      continue;
    }

    rejectUnsupportedPrefix(rest);

    if (rest === "") {
      const next = i + 1;
      if (next < lines.length && lines[next].indent > indent) {
        const sub = parseBlock(lines, next, lines[next].indent, undefined, depth + 1);
        arr.push(sub.value);
        i = sub.next;
      } else {
        arr.push(null);
        i = next;
      }
    } else if (isSeqItem(rest) || looksLikeMappingEntry(rest)) {
      const sub = parseBlock(lines, i, contentCol, rest, depth + 1);
      arr.push(sub.value);
      i = sub.next;
    } else {
      arr.push(parseScalarOrFlow(rest));
      i += 1;
    }
  }
  return { value: arr, next: i };
}

function parseMapping(
  lines: Line[],
  index: number,
  indent: number,
  virtualContent: string | undefined,
  depth: number,
): { value: Record<string, unknown>; next: number } {
  const obj: Record<string, unknown> = Object.create(null);
  let i = index;
  while (i < lines.length) {
    const { content, indent: lineIndent } = effectiveLine(lines, i, index, virtualContent, indent);
    if (lineIndent !== indent || isSeqItem(content)) break;
    if (content === "?" || content.startsWith("? ")) {
      throw new YamlUnsupported("complex (multi-line) mapping keys are not supported by this preview");
    }
    const split = splitKeyValue(content);
    if (!split) {
      const lineNo = i < lines.length ? lines[i].lineNo : "?";
      throw new YamlSyntaxError(`Line ${lineNo}: expected "key: value"`);
    }
    const { key, rest } = split;
    if (key in obj) throw new YamlSyntaxError(`duplicate key '${key}'`);

    let value: unknown;
    // A block scalar consumed at tokenize time (finding #8) — see `blockScalarValue`'s doc comment.
    const blockValue = i < lines.length ? lines[i].blockScalarValue : undefined;
    if (blockValue !== undefined) {
      value = blockValue;
      i += 1;
    } else {
      rejectUnsupportedPrefix(rest);
      if (rest === "") {
        const next = i + 1;
        if (next < lines.length && lines[next].indent > indent) {
          // Ordinarily-nested value: the next line is indented MORE than this key.
          const sub = parseBlock(lines, next, lines[next].indent, undefined, depth + 1);
          value = sub.value;
          i = sub.next;
        } else if (next < lines.length && lines[next].indent === indent && isSeqItem(lines[next].content)) {
          // Indentless sequence (finding #6): a block sequence written at the SAME column as its own
          // key, not indented under it — spec-legal YAML (`status:\n- item\n- item`), extremely common
          // in hand-written config. Parse it as a sequence at THIS key's own indent; `parseSequence`'s
          // own indent check naturally stops it at the first line that isn't a same-indent dash, so
          // control returns to this mapping's loop for the next sibling key exactly as normal.
          const sub = parseSequence(lines, next, indent, undefined, depth + 1);
          value = sub.value;
          i = sub.next;
        } else {
          value = null;
          i = next;
        }
      } else {
        value = parseScalarOrFlow(rest);
        i += 1;
      }
    }
    obj[key] = value;
  }
  return { value: obj, next: i };
}

/** Parse raw YAML text against the bounded subset described in the module doc comment. Never throws:
 *  a genuine syntax error resolves to `{ok:false, unsupported:false}`; a recognised-but-unimplemented
 *  construct resolves to `{ok:false, unsupported:true}` with a specific reason naming the construct. */
export function parseYaml(raw: string): YamlParseResult {
  try {
    const allLines = tokenizeLines(raw);
    if (allLines.length === 0) return { ok: true, value: null }; // empty document

    let start = 0;
    if (allLines[0].content === "---") start = 1;
    for (let i = start; i < allLines.length; i++) {
      if (allLines[i].content === "---" || allLines[i].content === "...") {
        throw new YamlUnsupported("multiple YAML documents in one file are not supported by this preview");
      }
    }
    if (start >= allLines.length) return { ok: true, value: null };

    const first = allLines[start];
    if (!isSeqItem(first.content) && !looksLikeMappingEntry(first.content) && !isComplexKeyMarker(first.content)) {
      // A single top-level scalar document (e.g. a bare `42` or `"hello"`).
      const value = parseScalarOrFlow(first.content);
      if (start + 1 < allLines.length) {
        throw new YamlSyntaxError(`Line ${allLines[start + 1].lineNo}: unexpected content after top-level scalar`);
      }
      return { ok: true, value };
    }

    const { value, next } = parseBlock(allLines, start, first.indent, undefined, 0);
    if (next < allLines.length) {
      throw new YamlSyntaxError(`Line ${allLines[next].lineNo}: unexpected indentation`);
    }
    return { ok: true, value };
  } catch (e) {
    if (e instanceof YamlUnsupported) return { ok: false, error: e.message, unsupported: true };
    if (e instanceof YamlSyntaxError) return { ok: false, error: e.message, unsupported: false };
    return { ok: false, error: e instanceof Error ? e.message : String(e), unsupported: false };
  }
}
