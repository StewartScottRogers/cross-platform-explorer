// Pure command-template runner (CPE-781, epic CPE-711). Expand a user command template against a file
// entry (and a selection) by substituting {path}/{name}/{dir}/{ext}/{stem}. No DOM/IO; unit-tested — the
// binding UI (CPE-783) renders + runs the expansion via a confirmed backend exec. Shell escaping is the
// backend's job; this only builds the strings.

import type { DirEntry } from "./types";

// Matches an escaped brace ({{ or }}) or a {token}. Tokens are word-chars only.
const TOKEN_RE = /\{\{|\}\}|\{(\w+)\}/g;

function dirOf(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i >= 0 ? path.slice(0, i) : "";
}
function extOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1) : "";
}
function stemOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/** The value for a known token, or `null` for an unknown one (left verbatim). */
function valueFor(token: string, entry: DirEntry): string | null {
  switch (token) {
    case "path":
      return entry.path;
    case "name":
      return entry.name;
    case "dir":
      return dirOf(entry.path);
    case "ext":
      return extOf(entry.name);
    case "stem":
      return stemOf(entry.name);
    default:
      return null;
  }
}

/**
 * Expand `tpl` against one entry: `{path}` `{name}` `{dir}` `{ext}` `{stem}` are substituted, unknown
 * `{tokens}` are left verbatim, and `{{` / `}}` escape to literal braces. Pure.
 */
export function expandTemplate(tpl: string, entry: DirEntry): string {
  return tpl.replace(TOKEN_RE, (m, token: string | undefined) => {
    if (m === "{{") return "{";
    if (m === "}}") return "}";
    const v = valueFor(token as string, entry);
    return v === null ? m : v;
  });
}

/**
 * Expand `tpl` over a selection. In `"each"` mode → one expanded string per entry. In `"joined"` mode →
 * a single string where each known token becomes the space-joined, double-quoted values across the
 * selection (e.g. `open {path}` over two files → `open "a" "b"`). Empty selection → `[]`. Pure.
 */
export function expandForSelection(tpl: string, entries: DirEntry[], mode: "each" | "joined"): string[] {
  if (entries.length === 0) return [];
  if (mode === "each") return entries.map((e) => expandTemplate(tpl, e));
  const joined = tpl.replace(TOKEN_RE, (m, token: string | undefined) => {
    if (m === "{{") return "{";
    if (m === "}}") return "}";
    const first = valueFor(token as string, entries[0]);
    if (first === null) return m; // unknown token stays verbatim
    return entries.map((e) => `"${valueFor(token as string, e)}"`).join(" ");
  });
  return [joined];
}
