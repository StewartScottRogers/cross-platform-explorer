/**
 * Pure-JS helpers behind the JSON tree preview (CPE-1573, epic CPE-1568). Framework-free (no Svelte
 * import) so the parse/format/path logic is unit-testable without mounting a component — `JsonTree.svelte`
 * and `JsonTreeNode.svelte` are thin rendering shells over these functions, and `preview/provider.ts`'s
 * `json` actions (Format/Validate) call `formatJson`/`parseJson` directly.
 */

export type JsonNodeType = "object" | "array" | "string" | "number" | "boolean" | "null";

/** Which of the six JSON value shapes `value` is — the single classifier the tree renderer, the type
 *  badges, and the copy/format actions all agree on. */
export function typeOf(value: unknown): JsonNodeType {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  const t = typeof value;
  if (t === "string" || t === "number" || t === "boolean") return t;
  return "object"; // a plain object (or anything JSON.parse could produce that isn't one of the above)
}

/** Whether `value` has children a tree node can expand into. */
export function isExpandable(value: unknown): boolean {
  const k = typeOf(value);
  return k === "object" || k === "array";
}

/** One child of an expandable node: `key` is the display label, `segment` is the raw path component
 *  (a string for an object property, a number for an array index) fed back into `pathToString`. */
export interface JsonChildEntry {
  key: string;
  segment: string | number;
  value: unknown;
}

/** An expandable node's children in display order. Empty for a primitive (nothing to expand). */
export function childEntries(value: unknown): JsonChildEntry[] {
  if (Array.isArray(value)) {
    return value.map((v, i) => ({ key: String(i), segment: i, value: v }));
  }
  if (value && typeof value === "object") {
    return Object.entries(value as Record<string, unknown>).map(([k, v]) => ({ key: k, segment: k, value: v }));
  }
  return [];
}

/** A primitive's display text: strings are JSON-quoted/escaped like source, everything else prints as
 *  its literal (`42`, `true`, `null`). Never called for object/array — those render via `childEntries`. */
export function formatPrimitive(value: unknown): string {
  if (typeof value === "string") return JSON.stringify(value);
  return String(value);
}

/** The text the "Copy value" action puts on the clipboard for a focused node: an unquoted, directly
 *  usable string for a string leaf, the literal for other primitives, and pretty JSON for a whole
 *  object/array subtree. */
export function copyableValue(value: unknown): string {
  const k = typeOf(value);
  if (k === "object" || k === "array") return JSON.stringify(value, null, 2);
  if (k === "string") return String(value);
  return formatPrimitive(value);
}

const IDENTIFIER_RE = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

/** Render a node's ancestor path as a JS-property-access expression (`$.a[0]["odd key"]`) — copyable and
 *  pasteable into code, unlike a bare breadcrumb string. Root (no segments) is just `$`. */
export function pathToString(segments: (string | number)[]): string {
  let out = "$";
  for (const seg of segments) {
    if (typeof seg === "number") out += `[${seg}]`;
    else if (IDENTIFIER_RE.test(seg)) out += `.${seg}`;
    else out += `[${JSON.stringify(seg)}]`;
  }
  return out;
}

export type JsonParseResult = { ok: true; value: unknown } | { ok: false; error: string };

/** `JSON.parse` with the exception turned into a plain result — the single parse path the tree's
 *  invalid-JSON state and the Validate action both use, so they never disagree. */
export function parseJson(raw: string): JsonParseResult {
  try {
    return { ok: true, value: JSON.parse(raw) };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

export type JsonFormatResult = { ok: true; text: string } | { ok: false; error: string };

/** Re-serializes `raw` as normalized, 2-space-indented JSON (the Format action) — the same shape the
 *  pane's raw view already pretty-prints to, just produced on demand for copying/saving. */
export function formatJson(raw: string): JsonFormatResult {
  const parsed = parseJson(raw);
  if (!parsed.ok) return parsed;
  return { ok: true, text: JSON.stringify(parsed.value, null, 2) };
}

/** Large-JSON safety (CPE-1573): nodes past this depth start collapsed, so opening a deeply nested
 *  document never renders thousands of DOM nodes on mount. Any branch — or "Expand all" — still opens
 *  it on demand. */
export const AUTO_COLLAPSE_DEPTH = 2;

/** Whether a node at `depth` (root = 0) starts expanded. */
export function defaultExpanded(depth: number): boolean {
  return depth < AUTO_COLLAPSE_DEPTH;
}

/** Large-JSON safety (CPE-1573): a single object/array only renders this many children at once — a
 *  huge flat array shows the first page plus a "N more" affordance instead of freezing the pane. */
export const MAX_CHILDREN = 200;
