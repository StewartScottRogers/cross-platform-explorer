// Pure macro-binding store (CPE-1191, epic CPE-739). Mirrors `userCommands.ts`'s
// `commandsForSurface`/reorder pattern, but keyed by macro NAME rather than a generated id — a
// macro's identity IS its name (`macro_save`/`macro_load`/`macro_delete` all key on it, CPE-1188), so
// a binding just needs `{name, surfaces, hotkey}`. Pure list ops + tolerant persistence, exactly like
// `userCommands.ts` / `colorRulesStore.ts`; actually running a macro is the separate, confirmed
// `MacroRunConfirm` flow (App.svelte) — no DOM/IO here.

/** Where a bound macro appears in the UI. (No "toolbar" — macros aren't offered there yet.) */
export type MacroSurface = "context" | "palette";

const SURFACES: readonly MacroSurface[] = ["context", "palette"];
const MODIFIER_ORDER = ["Ctrl", "Alt", "Shift"] as const;

/** A macro's UI binding: which surfaces it's exposed on, plus an optional hotkey. */
export interface MacroBinding {
  /** The macro's name — the join key into the CPE-1188 catalog (`MacroSummary.name`). */
  name: string;
  surfaces: MacroSurface[];
  /** Canonical `"Ctrl+Alt+K"`-style combo (see `normalizeHotkey`), or `""` for no hotkey. */
  hotkey: string;
}

/**
 * Canonicalize a hotkey string (however typed/cased, e.g. `"ctrl+alt+k"`) into `"Ctrl+Alt+Shift+K"`
 * modifier order, or `""` if it carries no key or no qualifying `Ctrl`/`Alt` modifier. A Shift-only or
 * bare-letter combo is deliberately rejected — either would collide with ordinary typing / type-ahead
 * find in App.svelte's keydown handler, which only ever consults this for combos that already survived
 * every built-in binding check.
 */
export function normalizeHotkey(raw: string): string {
  const tokens = raw
    .split("+")
    .map((t) => t.trim())
    .filter(Boolean);
  const mods = new Set<string>();
  let key = "";
  for (const t of tokens) {
    const low = t.toLowerCase();
    if (low === "ctrl" || low === "control" || low === "cmd" || low === "meta") mods.add("Ctrl");
    else if (low === "alt" || low === "option") mods.add("Alt");
    else if (low === "shift") mods.add("Shift");
    else key = t.length === 1 ? t.toUpperCase() : t;
  }
  if (!key || (!mods.has("Ctrl") && !mods.has("Alt"))) return "";
  return [...MODIFIER_ORDER.filter((m) => mods.has(m)), key].join("+");
}

/** The same canonicalization applied to a live `KeyboardEvent`-shaped object — what App.svelte's
 *  keydown handler builds to look up a match via `matchHotkey`. `metaKey` counts as `Ctrl` (Cmd on
 *  macOS), matching the app's existing `ctrl = event.ctrlKey || event.metaKey` convention. */
export function hotkeyFromEvent(e: {
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  key: string;
}): string {
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  parts.push(e.key.length === 1 ? e.key.toUpperCase() : e.key);
  return normalizeHotkey(parts.join("+"));
}

function isSurface(x: unknown): x is MacroSurface {
  return typeof x === "string" && (SURFACES as readonly string[]).includes(x);
}

function isBinding(x: unknown): x is MacroBinding {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  return (
    typeof o.name === "string" &&
    o.name.length > 0 &&
    Array.isArray(o.surfaces) &&
    o.surfaces.every(isSurface) &&
    typeof o.hotkey === "string"
  );
}

/** Upsert a binding by macro name (insert if new, patch in place if it already exists). Immutable. */
export function setBinding(
  list: MacroBinding[],
  name: string,
  patch: Partial<{ surfaces: MacroSurface[]; hotkey: string }>,
): MacroBinding[] {
  const hotkey = patch.hotkey !== undefined ? normalizeHotkey(patch.hotkey) : undefined;
  const i = list.findIndex((b) => b.name === name);
  if (i === -1) {
    return [...list, { name, surfaces: patch.surfaces ?? [], hotkey: hotkey ?? "" }];
  }
  const next = [...list];
  next[i] = {
    ...next[i],
    ...(patch.surfaces !== undefined ? { surfaces: patch.surfaces } : {}),
    ...(hotkey !== undefined ? { hotkey } : {}),
  };
  return next;
}

/** Drop a macro's binding entirely (e.g. the macro itself was deleted from the catalog). */
export function removeBinding(list: MacroBinding[], name: string): MacroBinding[] {
  return list.filter((b) => b.name !== name);
}

/** The binding for a given macro name, if any. */
export function bindingFor(list: MacroBinding[], name: string): MacroBinding | undefined {
  return list.find((b) => b.name === name);
}

/** Bindings exposing `surface`, in list order — mirrors `userCommands.commandsForSurface`. */
export function bindingsForSurface(list: MacroBinding[], surface: MacroSurface): MacroBinding[] {
  return list.filter((b) => b.surfaces.includes(surface));
}

/** The binding whose normalized hotkey matches `combo` (already canonical, e.g. from
 *  `hotkeyFromEvent`). `undefined` for an empty combo or no match — never throws. */
export function matchHotkey(list: MacroBinding[], combo: string): MacroBinding | undefined {
  if (!combo) return undefined;
  return list.find((b) => b.hotkey && normalizeHotkey(b.hotkey) === combo);
}

/** Serialize a binding list for the settings store. */
export function serializeBindings(list: MacroBinding[]): string {
  return JSON.stringify(list);
}

/** Parse a persisted binding list, dropping malformed entries (a bad entry never breaks startup). */
export function parseBindings(json: string | null | undefined): MacroBinding[] {
  if (!json) return [];
  try {
    const raw = JSON.parse(json);
    return Array.isArray(raw) ? raw.filter(isBinding) : [];
  } catch {
    return [];
  }
}
