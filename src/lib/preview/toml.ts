/**
 * A hand-rolled TOML parser (CPE-1617, epic CPE-1568 slice 7). Framework-free (no Svelte import), same
 * "pure module behind the preview component" convention as `jsonTree.ts`/`notebook.ts`/`logViewer.ts`.
 *
 * **Why hand-rolled instead of a dependency.** The ticket as filed suggested pulling in a small
 * third-party TOML parser (`smol-toml`/`@iarna/toml`). That cuts against this repo's lean-core,
 * no-new-deps discipline (PURPOSE.md's fast/small/predictable tiebreaker) — and this repo already
 * hand-rolls harder formats than TOML (a hand-written ECMA-335 .NET metadata reader lives in
 * `crates/server`). TOML's grammar is small and well-specified (tables, dotted keys, a handful of
 * scalar types, arrays, inline tables) — tractable to implement directly and keep honest about what it
 * does and doesn't cover, rather than taking on a dependency for it.
 *
 * **Scope.** Implements: comments, top-level key/value pairs, dotted keys (`a.b.c = 1`), `[table]` and
 * `[[array.of.tables]]` headers (including dotted/nested paths), basic (`"…"`) and literal (`'…'`)
 * single-line strings with standard escapes, arrays (which MAY span multiple lines — very common in
 * real TOML like `pyproject.toml`'s `dependencies = [...]`), single-line inline tables (`{a = 1, b =
 * 2}`, matching the TOML spec's own restriction that inline tables can't span lines), integers (dec/hex
 * /oct/bin, with `_` digit separators), floats (incl. `inf`/`nan`), and booleans.
 *
 * **Deliberately out of scope**, each surfaced as a clear, specific parse error rather than a silent
 * mis-parse (never guess): multi-line basic/literal strings (`"""…"""`/`'''…'''`). Dates/times ARE
 * accepted (TOML's offset/local date-time and date/time-only forms) but represented as their literal
 * source string rather than a real date value — this preview's tree can only render the JSON value
 * shapes (object/array/string/number/boolean/null) `jsonTree.ts` already knows, and a string is an
 * honest, lossless-enough representation for a viewer (never fed back into a write path).
 *
 * **Untrusted input.** A `.toml` file is arbitrary attacker-influenced input like any other previewed
 * file. Table objects are built with `Object.create(null)` rather than `{}` so a key literally named
 * `__proto__` can never repoint an object's prototype (a real footgun with plain object literals +
 * computed keys on fully attacker-controlled key strings). Recursion (nested arrays/inline
 * tables/table paths) is bounded by {@link MAX_DEPTH} so a pathological file (e.g. thousands of nested
 * `[[[[…`) fails with a clear error instead of a stack overflow — the same "cap bounds work examined,
 * not just output" discipline as every other preview parser in this module (see `notebook.ts`'s module
 * doc comment for the font-cache bug this crew learned it from). No other cap is needed: parsing is a
 * single linear pass over the (already read-capped, see `PREVIEW_MAX_BYTES`) input text.
 */

export type TomlParseResult = { ok: true; value: Record<string, unknown> } | { ok: false; error: string };

/** Bounds recursion depth for nested tables/arrays/inline-tables — see the module doc comment. */
const MAX_DEPTH = 200;

class TomlError extends Error {}

const BARE_KEY_CHAR = /[A-Za-z0-9_-]/;

class Parser {
  private pos = 0;
  private line = 1;
  private readonly len: number;

  constructor(private readonly s: string) {
    this.len = s.length;
  }

  private peek(offset = 0): string | undefined {
    return this.s[this.pos + offset];
  }

  private eof(): boolean {
    return this.pos >= this.len;
  }

  private advance(): string {
    const c = this.s[this.pos++];
    if (c === "\n") this.line++;
    return c;
  }

  private err(msg: string): never {
    throw new TomlError(`Line ${this.line}: ${msg}`);
  }

  private skipInlineWs(): void {
    while (!this.eof() && (this.peek() === " " || this.peek() === "\t")) this.pos++;
  }

  /** Skips blank lines and full-line/trailing comments, respecting newlines (used between top-level
   *  entries and inside multi-line arrays, where TOML allows free-form whitespace/comments). */
  private skipBlankAndComments(): void {
    while (!this.eof()) {
      const c = this.peek();
      if (c === " " || c === "\t" || c === "\r" || c === "\n") {
        this.advance();
        continue;
      }
      if (c === "#") {
        while (!this.eof() && this.peek() !== "\n") this.pos++;
        continue;
      }
      break;
    }
  }

  /** After a value/header on the CURRENT line: skip trailing inline whitespace + an optional comment,
   *  then require a newline or EOF — catches trailing garbage like `key = 1 2` or `[foo] junk`. */
  private consumeNewlineOrEof(): void {
    this.skipInlineWs();
    if (!this.eof() && this.peek() === "#") {
      while (!this.eof() && this.peek() !== "\n") this.pos++;
    }
    if (this.eof()) return;
    if (this.peek() === "\r") this.pos++;
    if (this.peek() === "\n") {
      this.advance();
      return;
    }
    this.err("expected end of line");
  }

  private parseKeySegment(): string {
    this.skipInlineWs();
    const c = this.peek();
    if (c === '"') return this.readBasicStringBody();
    if (c === "'") return this.readLiteralStringBody();
    const start = this.pos;
    while (!this.eof() && BARE_KEY_CHAR.test(this.peek() as string)) this.pos++;
    if (this.pos === start) this.err("expected a key");
    return this.s.slice(start, this.pos);
  }

  private parseKeyPath(): string[] {
    const segs = [this.parseKeySegment()];
    this.skipInlineWs();
    while (this.peek() === ".") {
      this.pos++;
      this.skipInlineWs();
      segs.push(this.parseKeySegment());
      this.skipInlineWs();
    }
    return segs;
  }

  private readBasicStringBody(): string {
    this.pos++; // opening quote
    let out = "";
    while (true) {
      if (this.eof()) this.err("unterminated string");
      const c = this.advance();
      if (c === '"') return out;
      if (c === "\n") this.err("unterminated string (newline before closing quote)");
      if (c === "\\") {
        if (this.eof()) this.err("unterminated string");
        const e = this.advance();
        switch (e) {
          case '"': out += '"'; break;
          case "\\": out += "\\"; break;
          case "b": out += "\b"; break;
          case "f": out += "\f"; break;
          case "n": out += "\n"; break;
          case "r": out += "\r"; break;
          case "t": out += "\t"; break;
          case "u": out += this.readUnicodeEscape(4); break;
          case "U": out += this.readUnicodeEscape(8); break;
          default: this.err(`invalid escape sequence '\\${e}'`);
        }
      } else {
        out += c;
      }
    }
  }

  private readUnicodeEscape(n: number): string {
    let hex = "";
    for (let i = 0; i < n; i++) {
      if (this.eof()) this.err("unterminated unicode escape");
      hex += this.advance();
    }
    const code = parseInt(hex, 16);
    if (Number.isNaN(code)) this.err("invalid unicode escape");
    return String.fromCodePoint(code);
  }

  private readLiteralStringBody(): string {
    this.pos++; // opening quote
    const start = this.pos;
    while (true) {
      if (this.eof()) this.err("unterminated string");
      const c = this.peek();
      if (c === "'") {
        const out = this.s.slice(start, this.pos);
        this.pos++;
        return out;
      }
      if (c === "\n") this.err("unterminated string (newline before closing quote)");
      this.pos++;
    }
  }

  private parseValue(depth: number): unknown {
    if (depth > MAX_DEPTH) this.err("value nested too deeply");
    this.skipInlineWs();
    if (this.eof()) this.err("expected a value");
    const c = this.peek() as string;
    if (c === '"') {
      if (this.peek(1) === '"' && this.peek(2) === '"') {
        this.err('multi-line strings ("""…""") are not supported by this preview');
      }
      return this.readBasicStringBody();
    }
    if (c === "'") {
      if (this.peek(1) === "'" && this.peek(2) === "'") {
        this.err("multi-line strings ('''…''') are not supported by this preview");
      }
      return this.readLiteralStringBody();
    }
    if (c === "[") return this.parseArray(depth + 1);
    if (c === "{") return this.parseInlineTable(depth + 1);
    return this.parseBareValue();
  }

  private parseArray(depth: number): unknown[] {
    this.pos++; // [
    const arr: unknown[] = [];
    while (true) {
      this.skipBlankAndComments();
      if (this.eof()) this.err("unterminated array");
      if (this.peek() === "]") {
        this.pos++;
        return arr;
      }
      arr.push(this.parseValue(depth));
      this.skipBlankAndComments();
      if (this.eof()) this.err("unterminated array");
      if (this.peek() === ",") {
        this.pos++;
        continue;
      }
      if (this.peek() === "]") {
        this.pos++;
        return arr;
      }
      this.err("expected ',' or ']' in array");
    }
  }

  private parseInlineTable(depth: number): Record<string, unknown> {
    this.pos++; // {
    const obj: Record<string, unknown> = Object.create(null);
    this.skipInlineWs();
    if (this.peek() === "}") {
      this.pos++;
      return obj;
    }
    while (true) {
      this.skipInlineWs();
      const path = this.parseKeyPath();
      this.skipInlineWs();
      if (this.peek() !== "=") this.err("expected '=' in inline table");
      this.pos++;
      this.skipInlineWs();
      const value = this.parseValue(depth);
      this.assign(obj, path, value);
      this.skipInlineWs();
      if (this.peek() === ",") {
        this.pos++;
        continue;
      }
      if (this.peek() === "}") {
        this.pos++;
        return obj;
      }
      if (this.eof() || this.peek() === "\n") this.err("inline tables must be on a single line");
      this.err("expected ',' or '}' in inline table");
    }
  }

  private parseBareValue(): unknown {
    const start = this.pos;
    while (!this.eof() && !/[,\]}#\n\r \t]/.test(this.peek() as string)) this.pos++;
    let raw = this.s.slice(start, this.pos);
    if (raw === "") this.err("expected a value");
    // A TOML date-time may separate its date and time halves with a single space instead of 'T' — the
    // one case where a bare value legitimately contains whitespace. Everything else stops at the first
    // whitespace, so `a = 1 2` correctly fails as trailing garbage rather than being folded into one
    // (invalid) token.
    if (/^\d{4}-\d{2}-\d{2}$/.test(raw) && this.peek() === " ") {
      const save = this.pos;
      this.pos++; // the space
      const timeStart = this.pos;
      while (!this.eof() && !/[,\]}#\n\r \t]/.test(this.peek() as string)) this.pos++;
      const timePart = this.s.slice(timeStart, this.pos);
      if (/^\d{2}:\d{2}:\d{2}/.test(timePart)) {
        raw = `${raw} ${timePart}`;
      } else {
        this.pos = save; // not a date-time continuation — rewind, let the trailing space end the value
      }
    }
    return this.classifyBare(raw);
  }

  private classifyBare(raw: string): unknown {
    if (raw === "true") return true;
    if (raw === "false") return false;
    if (/^[+-]?\d(_?\d)*$/.test(raw)) return Number(raw.replace(/_/g, ""));
    if (/^[+-]?0x[0-9A-Fa-f](_?[0-9A-Fa-f])*$/i.test(raw)) return parseInt(raw.replace(/_/g, ""), 16);
    if (/^0o[0-7](_?[0-7])*$/.test(raw)) return parseInt(raw.slice(2).replace(/_/g, ""), 8);
    if (/^0b[01](_?[01])*$/.test(raw)) return parseInt(raw.slice(2).replace(/_/g, ""), 2);
    if (
      /^[+-]?(\d(_?\d)*)?\.\d(_?\d)*([eE][+-]?\d+)?$/.test(raw) ||
      /^[+-]?\d(_?\d)*[eE][+-]?\d+$/.test(raw)
    ) {
      return Number(raw.replace(/_/g, ""));
    }
    if (/^[+-]?inf$/.test(raw)) return raw.startsWith("-") ? -Infinity : Infinity;
    if (/^[+-]?nan$/.test(raw)) return NaN;
    // TOML date-time / local-date / local-time forms — kept as their literal text (see module doc
    // comment: this preview has no separate "date" node type, and a string is an honest rendering).
    if (/^\d{4}-\d{2}-\d{2}([Tt ]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})?)?$/.test(raw)) return raw;
    if (/^\d{2}:\d{2}:\d{2}(\.\d+)?$/.test(raw)) return raw;
    this.err(`invalid value: ${JSON.stringify(raw)}`);
  }

  /** Walks/creates nested tables for a dotted key path and sets the final segment, erroring on a
   *  duplicate key or an attempt to use a non-table value as a table (matches TOML's own rules). */
  private assign(root: Record<string, unknown>, path: string[], value: unknown): void {
    let node = root;
    for (let i = 0; i < path.length - 1; i++) {
      const k = path[i];
      if (!(k in node)) node[k] = Object.create(null);
      const next = node[k];
      if (typeof next !== "object" || next === null || Array.isArray(next)) {
        this.err(`cannot use '${k}' as a table: already defined as a different type`);
      }
      node = next as Record<string, unknown>;
    }
    const finalKey = path[path.length - 1];
    if (finalKey in node) this.err(`duplicate key '${finalKey}'`);
    node[finalKey] = value;
  }

  /** Walks/creates nested tables for a `[table]`/`[[array.of.tables]]` header path, returning the
   *  object subsequent key/value lines should be written into. Descends into the LAST element when a
   *  path segment resolves to an array (nested array-of-tables). */
  private navigateTable(root: Record<string, unknown>, path: string[]): Record<string, unknown> {
    if (path.length === 0) this.err("malformed table header: empty name");
    if (path.length > MAX_DEPTH) this.err("table path nested too deeply");
    let node = root;
    for (const key of path) {
      if (!(key in node)) node[key] = Object.create(null);
      const existing = node[key];
      if (Array.isArray(existing)) {
        const last = existing[existing.length - 1];
        if (typeof last !== "object" || last === null || Array.isArray(last)) {
          this.err(`cannot use '${key}' as a table: it's an array of non-table values`);
        }
        node = last as Record<string, unknown>;
      } else if (typeof existing === "object" && existing !== null) {
        node = existing as Record<string, unknown>;
      } else {
        this.err(`cannot redefine '${key}' as a table`);
      }
    }
    return node;
  }

  private parseTableHeader(root: Record<string, unknown>): Record<string, unknown> {
    this.pos++; // [
    this.skipInlineWs();
    const path = this.parseKeyPath();
    this.skipInlineWs();
    if (this.peek() !== "]") this.err("malformed table header: expected ']'");
    this.pos++;
    this.consumeNewlineOrEof();
    return this.navigateTable(root, path);
  }

  private parseArrayTableHeader(root: Record<string, unknown>): Record<string, unknown> {
    this.pos += 2; // [[
    this.skipInlineWs();
    const path = this.parseKeyPath();
    this.skipInlineWs();
    if (!(this.peek() === "]" && this.peek(1) === "]")) {
      this.err("malformed array-of-tables header: expected ']]'");
    }
    this.pos += 2;
    this.consumeNewlineOrEof();
    if (path.length === 0) this.err("malformed array-of-tables header: empty name");
    const parentPath = path.slice(0, -1);
    const lastKey = path[path.length - 1];
    const parent = parentPath.length ? this.navigateTable(root, parentPath) : root;
    let arr = parent[lastKey];
    if (arr === undefined) {
      arr = [];
      parent[lastKey] = arr;
    }
    if (!Array.isArray(arr)) this.err(`cannot redefine '${lastKey}' as an array of tables`);
    const entry: Record<string, unknown> = Object.create(null);
    (arr as unknown[]).push(entry);
    return entry;
  }

  private parseKeyValue(target: Record<string, unknown>): void {
    const path = this.parseKeyPath();
    this.skipInlineWs();
    if (this.peek() !== "=") this.err("expected '=' after key");
    this.pos++;
    this.skipInlineWs();
    const value = this.parseValue(0);
    this.consumeNewlineOrEof();
    this.assign(target, path, value);
  }

  parseDocument(): Record<string, unknown> {
    const root: Record<string, unknown> = Object.create(null);
    let current = root;
    while (true) {
      this.skipBlankAndComments();
      if (this.eof()) break;
      if (this.peek() === "[") {
        current = this.peek(1) === "[" ? this.parseArrayTableHeader(root) : this.parseTableHeader(root);
      } else {
        this.parseKeyValue(current);
      }
    }
    return root;
  }
}

/** Parse raw TOML text. Never throws — every failure (a genuine syntax error, or a construct this
 *  parser deliberately doesn't implement, e.g. a multi-line string) resolves to `{ok:false,error}`
 *  with a specific message, never a silent guess. See the module doc comment for exact scope. */
export function parseToml(raw: string): TomlParseResult {
  try {
    return { ok: true, value: new Parser(raw).parseDocument() };
  } catch (e) {
    if (e instanceof TomlError) return { ok: false, error: e.message };
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}
