/**
 * A BOUNDED-SUBSET YAML parser (CPE-1617, epic CPE-1568 slice 7). Framework-free (no Svelte import),
 * same "pure module behind the preview component" convention as `toml.ts`/`jsonTree.ts`/`notebook.ts`.
 *
 * **This is deliberately NOT a full YAML implementation.** Full YAML (anchors/aliases/merge keys, tags,
 * block scalars, multi-document streams, complex mapping keys, flow collections spanning lines, …) is
 * genuinely hard to get right, and a half-correct parser that silently mis-parses a construct it
 * doesn't fully understand is worse than no structured view at all — it would show a confidently wrong
 * tree for someone's real config. Rather than attempt full coverage, {@link parseYaml} recognises a
 * BOUNDED SUBSET that covers ordinary, hand-written config shapes:
 *
 *   - block mappings (`key: value`, nested by indentation) and block sequences (`- item`, including
 *     the common `- key: value` shorthand that starts a mapping on the dash's own line)
 *   - plain/single/double-quoted scalars, `null`/`~`/empty, `true`/`false`, integers, floats
 *   - single-LINE flow collections (`[a, b]`, `{a: 1, b: 2}`) — flow collections that span multiple
 *     physical lines are explicitly unsupported (see below), not guessed at
 *   - `#` comments, blank lines, and a single leading `---` document-start marker
 *
 * **Anything outside that subset degrades EXPLICITLY, with a stated reason, rather than being ignored
 * or mis-rendered.** {@link YamlParseResult} distinguishes two failure shapes: `unsupported: true` for
 * a construct this parser recognises but deliberately doesn't implement (anchors `&`, aliases `*`,
 * explicit tags `!`, block scalars `|`/`>`, complex `? key` mappings, multiple `---` documents, tab
 * indentation, a flow collection spanning lines), and `unsupported: false` for a genuine syntax error
 * (bad indentation, an unterminated quote, a malformed flow collection, …). The preview component
 * (`YamlTomlPreview.svelte`) renders both as "can't show a structured view: <reason>" plus the raw text
 * — never as an empty/blank pane, and never silently rendering a guessed, possibly-wrong tree.
 *
 * **Untrusted input / DoS safety**, same discipline as `toml.ts`: mapping objects use
 * `Object.create(null)` (no `__proto__` key footgun on attacker-controlled key strings), and both block
 * recursion and flow-collection recursion are bounded by {@link MAX_DEPTH} / the flow parser's own
 * depth cap, so a pathological file (deeply nested sequences, or thousands of nested `[[[[…` on one
 * line) fails with a clear error instead of a stack overflow. Line splitting + per-line scanning is a
 * single linear pass over the (already read-capped, see `PREVIEW_MAX_BYTES`) input text — no separate
 * line-count cap is needed here (unlike `logViewer.ts`'s `MAX_LINES`): the rendered TREE is what could
 * blow up the DOM, and that's already bounded by `jsonTree.ts`'s `MAX_CHILDREN`/`AUTO_COLLAPSE_DEPTH`,
 * reused as-is by feeding the parsed value through `JsonTree.svelte`.
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

/** Splits raw text into non-blank, comment-stripped logical lines with their indent depth. Throws
 *  {@link YamlUnsupported} for tab-indented lines or a directive line (`%YAML`, `%TAG`) — both real
 *  YAML constructs this parser doesn't attempt. */
function tokenizeLines(raw: string): Line[] {
  const physical = raw.split(/\r\n|\n/);
  const out: Line[] = [];
  for (let i = 0; i < physical.length; i++) {
    const rawLine = physical[i];
    if (rawLine.trim() === "") continue;
    const leading = /^[ \t]*/.exec(rawLine)?.[0] ?? "";
    if (leading.includes("\t")) {
      throw new YamlUnsupported("tab characters in indentation are not supported by this preview");
    }
    const indent = leading.length;
    const content = stripComment(rawLine.slice(indent)).replace(/\s+$/, "");
    if (content === "") continue;
    if (indent === 0 && content.startsWith("%")) {
      throw new YamlUnsupported("YAML directives are not supported by this preview");
    }
    out.push({ indent, content, lineNo: i + 1 });
  }
  return out;
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

/** Throws {@link YamlUnsupported} when `rest` (a value, right after `:` or `- `) starts with a
 *  construct this parser deliberately doesn't implement. Bounded detection — see the module doc
 *  comment: this catches the common case (the construct appearing at the point of assignment), not
 *  every position a real YAML anchor/tag could legally appear. */
function rejectUnsupportedPrefix(rest: string): void {
  if (rest === "") return;
  if (rest[0] === "&") throw new YamlUnsupported("YAML anchors (&) are not supported by this preview");
  if (rest[0] === "*") throw new YamlUnsupported("YAML aliases (*) are not supported by this preview");
  if (rest[0] === "!") throw new YamlUnsupported("YAML tags (!) are not supported by this preview");
  if (/^[|>][+-]?\d*$/.test(rest)) {
    throw new YamlUnsupported("block scalars (| and >) are not supported by this preview");
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

function parseScalarOrFlow(rest: string): unknown {
  rejectUnsupportedPrefix(rest);
  if (rest[0] === "[" || rest[0] === "{") return parseFlow(rest);
  if (rest[0] === '"' || rest[0] === "'") {
    const q = readQuotedScalar(rest, 0);
    const trailing = rest.slice(q.end).trim();
    if (trailing !== "") throw new YamlSyntaxError(`unexpected content after quoted string: ${trailing}`);
    return q.value;
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
    rejectUnsupportedPrefix(rest);
    if (key in obj) throw new YamlSyntaxError(`duplicate key '${key}'`);

    let value: unknown;
    if (rest === "") {
      const next = i + 1;
      if (next < lines.length && lines[next].indent > indent) {
        const sub = parseBlock(lines, next, lines[next].indent, undefined, depth + 1);
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
