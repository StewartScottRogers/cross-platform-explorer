/**
 * Keymap action registry + persisted override store (CPE-1547, epic CPE-1484 "hotkey
 * customization"). The single source of truth for every BUILT-IN action's id, human label,
 * group, and default chord — a data model a future UI can read/write to show or remap a
 * binding. Transcribed 1:1 from `shortcuts.ts`'s `SHORTCUT_GROUPS` (the existing read-only
 * cheat sheet) and App.svelte's `handleKeydown`, which together are still the actual source of
 * behavior. Chord parsing/formatting reuses `macroBindings.ts`'s `normalizeHotkey` rather than
 * re-deriving it.
 *
 * LIVE (CPE-1557): `App.svelte`'s `handleKeydown` resolves keystrokes through this registry via
 * `actionForChord`/`chordFromEvent`, so user remaps change key behavior. Pure logic, mirroring
 * `macroBindings.ts`'s shape — no DOM, no Tauri invoke.
 */
import { normalizeHotkey } from "./macroBindings";

const MODIFIER_ORDER = ["Ctrl", "Alt", "Shift"] as const;

/** Every built-in action that maps to one real, fixed keybinding. Excludes `SHORTCUT_GROUPS`'
 *  "Type a name" row (jump-to-item isn't a chord) and the Macros group's "(user-configured)"
 *  placeholder, which `macroBindings.ts` already owns per-macro. */
export type ActionId =
  | "back"
  | "forward"
  | "up"
  | "refresh"
  | "editAddress"
  | "searchFolder"
  | "findFiles"
  | "contentSearch"
  | "instantSearch"
  | "openItem"
  | "newTab"
  | "closeTab"
  | "reopenTab"
  | "nextTab"
  | "prevTab"
  | "selectAll"
  | "clearSelection"
  | "copy"
  | "cut"
  | "paste"
  | "duplicate"
  | "addToDropStack"
  | "undo"
  | "rename"
  | "deleteToTrash"
  | "deletePermanent"
  | "newFolder"
  | "copyAsPath"
  | "properties"
  | "toggleDetails"
  | "popOutPreview"
  | "commandPalette"
  | "docsHelp"
  | "shortcutsCheatSheet";

export interface ActionDef {
  id: ActionId;
  /** Matches a `SHORTCUT_GROUPS` title, so the registry keeps a 1:1 mental model with the
   *  existing read-only cheat sheet. */
  group: string;
  description: string;
  /** Canonical chord (see `normalizeChord`), or `""` for an action with no representable
   *  single-key default (none today, but the shape allows it). */
  defaultChord: string;
}

/**
 * Canonicalize a chord string using the same modifier-order + key-casing convention as
 * `macroBindings.ts`'s `normalizeHotkey`. Unlike `normalizeHotkey` — which rejects any combo
 * without `Ctrl`/`Alt` because it's built for USER-TYPED macro hotkeys that must never collide
 * with ordinary typing — several of the app's existing built-in actions are legitimately bound
 * to a bare function/navigation key (`F5`, `Enter`, `Escape`, `F2`, `Delete`, `?`) or `Shift+Delete`.
 * This tries `normalizeHotkey` first (covers every Ctrl/Alt-qualified chord identically) and
 * only falls back to a local re-derivation for the bare-key case, so a transcribed built-in
 * default is never silently dropped to `""`.
 */
export function normalizeChord(raw: string): string {
  const viaHotkeyRule = normalizeHotkey(raw);
  if (viaHotkeyRule) return viaHotkeyRule;
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
  if (!key) return "";
  return [...MODIFIER_ORDER.filter((m) => mods.has(m)), key].join("+");
}

/**
 * Canonicalize a live `KeyboardEvent`-shaped object into a chord — same shape as
 * `macroBindings.ts`'s `hotkeyFromEvent` (builds from raw `event.key`, so arrow keys/Escape/etc.
 * come through as `"ArrowLeft"`/`"Escape"`/... exactly as the DOM reports them, never a display
 * glyph), but canonicalized via the permissive `normalizeChord` instead of the strict
 * `normalizeHotkey`. That match matters: a real keypress for a bare-key built-in default (`F5`,
 * `Enter`, `Escape`, `F2`, `Delete`, `Shift+Delete`, `F1`, `?`) must still resolve via
 * `actionForChord` against that default, which `hotkeyFromEvent`'s stricter Ctrl/Alt-required
 * rule can never allow (it's designed to keep user-typed MACRO hotkeys from colliding with
 * ordinary typing — not applicable to matching an existing built-in). This is the code path a
 * future live-capture remap UI (CPE-1549) uses to identify which built-in action a keystroke
 * hits; `setChord` (assigning a NEW override) deliberately keeps the stricter `normalizeHotkey`
 * rule so a freshly typed rebind still can't collide with typing.
 */
export function chordFromEvent(e: {
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
  return normalizeChord(parts.join("+"));
}

/** The full registry, grouped in `SHORTCUT_GROUPS` order (Navigation/Tabs/Selection/File
 *  actions/View/General). `defaultChord` values are transcribed from that group's `keys` column
 *  in the real `KeyboardEvent.key` form a live keystroke actually produces (canonicalized via
 *  `normalizeChord`) — NOT the display glyph shown there (`←`/`→`/`↑`/`Esc` become
 *  `ArrowLeft`/`ArrowRight`/`ArrowUp`/`Escape`), since a chord that can never match a real
 *  captured keypress would be useless to `actionForChord`/`findConflicts`/the future remap UI.
 *  Where a shortcut has a secondary alternate key (`Backspace` also does "up", `Alt+D` also does
 *  "edit address"), only the primary chord shown first in `SHORTCUT_GROUPS` is modeled — this
 *  registry tracks one fixed binding per action. */
export const ACTIONS: ActionDef[] = [
  // Navigation
  // NOTE: back/forward/up use the real `KeyboardEvent.key` form ("ArrowLeft"/"ArrowRight"/
  // "ArrowUp"), NOT the "←"/"→"/"↑" glyphs shortcuts.ts displays — a live keypress captured via
  // macroBindings.ts's hotkeyFromEvent never produces the glyph, so the stored default must match
  // the event form for actionForChord/findConflicts to ever resolve a real keystroke (CPE-1547
  // review). The glyph stays exactly as-is in shortcuts.ts for display.
  { id: "back", group: "Navigation", description: "Back", defaultChord: normalizeChord("Alt+ArrowLeft") },
  {
    id: "forward",
    group: "Navigation",
    description: "Forward",
    defaultChord: normalizeChord("Alt+ArrowRight"),
  },
  {
    id: "up",
    group: "Navigation",
    description: "Up one folder",
    defaultChord: normalizeChord("Alt+ArrowUp"),
  },
  { id: "refresh", group: "Navigation", description: "Refresh", defaultChord: normalizeChord("F5") },
  {
    id: "editAddress",
    group: "Navigation",
    description: "Edit address (type a path)",
    defaultChord: normalizeChord("Ctrl+L"),
  },
  {
    id: "searchFolder",
    group: "Navigation",
    description: "Search the current folder",
    defaultChord: normalizeChord("Ctrl+F"),
  },
  {
    id: "findFiles",
    group: "Navigation",
    description: "Find files by name (recursive)",
    defaultChord: normalizeChord("Ctrl+P"),
  },
  {
    id: "contentSearch",
    group: "Navigation",
    description: "Search inside files (content search)",
    defaultChord: normalizeChord("Ctrl+Shift+F"),
  },
  {
    id: "instantSearch",
    group: "Navigation",
    description: "Instant Search — every indexed folder, any drive",
    defaultChord: normalizeChord("Ctrl+K"),
  },
  {
    id: "openItem",
    group: "Navigation",
    description: "Open the selected item",
    defaultChord: normalizeChord("Enter"),
  },
  // Tabs
  { id: "newTab", group: "Tabs", description: "New tab", defaultChord: normalizeChord("Ctrl+T") },
  { id: "closeTab", group: "Tabs", description: "Close tab", defaultChord: normalizeChord("Ctrl+W") },
  {
    id: "reopenTab",
    group: "Tabs",
    description: "Reopen last closed tab",
    defaultChord: normalizeChord("Ctrl+Shift+T"),
  },
  { id: "nextTab", group: "Tabs", description: "Next tab", defaultChord: normalizeChord("Ctrl+Tab") },
  {
    id: "prevTab",
    group: "Tabs",
    description: "Previous tab",
    defaultChord: normalizeChord("Ctrl+Shift+Tab"),
  },
  // Selection
  { id: "selectAll", group: "Selection", description: "Select all", defaultChord: normalizeChord("Ctrl+A") },
  // NOTE: real KeyboardEvent.key form is "Escape", not the "Esc" shortcuts.ts displays — see the
  // back/forward/up note above (CPE-1547 review).
  {
    id: "clearSelection",
    group: "Selection",
    description: "Clear selection",
    defaultChord: normalizeChord("Escape"),
  },
  // File actions
  { id: "copy", group: "File actions", description: "Copy", defaultChord: normalizeChord("Ctrl+C") },
  { id: "cut", group: "File actions", description: "Cut", defaultChord: normalizeChord("Ctrl+X") },
  { id: "paste", group: "File actions", description: "Paste", defaultChord: normalizeChord("Ctrl+V") },
  {
    id: "duplicate",
    group: "File actions",
    description: "Duplicate",
    defaultChord: normalizeChord("Ctrl+D"),
  },
  {
    id: "addToDropStack",
    group: "File actions",
    description: "Add to Drop Stack",
    defaultChord: normalizeChord("Ctrl+Shift+D"),
  },
  { id: "undo", group: "File actions", description: "Undo", defaultChord: normalizeChord("Ctrl+Z") },
  { id: "rename", group: "File actions", description: "Rename", defaultChord: normalizeChord("F2") },
  {
    id: "deleteToTrash",
    group: "File actions",
    description: "Delete to Recycle Bin / Trash",
    defaultChord: normalizeChord("Delete"),
  },
  {
    id: "deletePermanent",
    group: "File actions",
    description: "Delete permanently",
    defaultChord: normalizeChord("Shift+Delete"),
  },
  {
    id: "newFolder",
    group: "File actions",
    description: "New folder",
    defaultChord: normalizeChord("Ctrl+Shift+N"),
  },
  {
    id: "copyAsPath",
    group: "File actions",
    description: "Copy as path",
    defaultChord: normalizeChord("Ctrl+Shift+C"),
  },
  {
    id: "properties",
    group: "File actions",
    description: "Properties",
    defaultChord: normalizeChord("Alt+Enter"),
  },
  // View
  {
    id: "toggleDetails",
    group: "View",
    description: "Toggle the details panel",
    defaultChord: normalizeChord("Alt+P"),
  },
  {
    id: "popOutPreview",
    group: "View",
    description: "Pop out the preview",
    defaultChord: normalizeChord("Ctrl+Shift+O"),
  },
  // General
  {
    id: "commandPalette",
    group: "General",
    description: "Command palette — find and run any action",
    defaultChord: normalizeChord("Ctrl+Shift+P"),
  },
  {
    id: "docsHelp",
    group: "General",
    description: "Documentation for the current section",
    defaultChord: normalizeChord("F1"),
  },
  {
    id: "shortcutsCheatSheet",
    group: "General",
    description: "Show this shortcuts list",
    defaultChord: normalizeChord("?"),
  },
];

const ACTION_IDS: readonly ActionId[] = ACTIONS.map((a) => a.id);
const ACTION_ID_SET = new Set<string>(ACTION_IDS);
const ACTION_BY_ID = new Map<ActionId, ActionDef>(ACTIONS.map((a) => [a.id, a]));

export function isActionId(x: unknown): x is ActionId {
  return typeof x === "string" && ACTION_ID_SET.has(x);
}

/** Chord per action; `""` = unbound. The full keymap persisted by `settings.ts` — every action
 *  present, whether its value is the built-in default or a user override. */
export type Keymap = Record<ActionId, string>;

/** Every action bound to its `defaultChord` — a fresh copy each call (safe to mutate). */
export function defaultKeymap(): Keymap {
  const out = {} as Keymap;
  for (const a of ACTIONS) out[a.id] = a.defaultChord;
  return out;
}

/** The chord currently effective for `id` in `keymap` (default-or-override, since `Keymap` is
 *  always the full effective map). Falls back to the built-in default if `id` is somehow absent
 *  (e.g. a stale/partial object bypassing `parseKeymap`) — never `undefined`. */
export function chordFor(keymap: Keymap, id: ActionId): string {
  const v = keymap[id];
  return v !== undefined ? v : (ACTION_BY_ID.get(id)?.defaultChord ?? "");
}

/** The action currently bound to `chord` (already normalized, e.g. from `hotkeyFromEvent`), if
 *  any. An empty chord never matches — several actions can legitimately sit at `""` (unbound). */
export function actionForChord(keymap: Keymap, chord: string): ActionId | undefined {
  if (!chord) return undefined;
  for (const id of ACTION_IDS) {
    if (keymap[id] === chord) return id;
  }
  return undefined;
}

/** Rebind `id` to `rawChord`, immutably. Normalizes via the strict `normalizeHotkey` — unlike
 *  built-in defaults (which may be grandfathered bare keys), a NEW user-typed override must
 *  carry a qualifying `Ctrl`/`Alt` modifier so it can never collide with ordinary typing;
 *  anything that fails to normalize (e.g. `Shift+K` alone) is stored as `""` (unbound). */
export function setChord(keymap: Keymap, id: ActionId, rawChord: string): Keymap {
  return { ...keymap, [id]: normalizeHotkey(rawChord) };
}

/** Reset `id` back to its built-in default, immutably. */
export function resetChord(keymap: Keymap, id: ActionId): Keymap {
  return { ...keymap, [id]: ACTION_BY_ID.get(id)?.defaultChord ?? "" };
}

/** A fresh keymap with every action reset to its built-in default. */
export function resetAll(): Keymap {
  return defaultKeymap();
}

/** Every action, paired with its currently effective chord, in registry order. */
export function listActions(keymap: Keymap): { action: ActionDef; chord: string }[] {
  return ACTIONS.map((action) => ({ action, chord: chordFor(keymap, action.id) }));
}

/** Every chord currently bound to 2+ actions. Unbound (`""`) chords never conflict — many
 *  actions can sit at `""` simultaneously without that counting as a collision. */
export function findConflicts(keymap: Keymap): { chord: string; ids: ActionId[] }[] {
  const byChord = new Map<string, ActionId[]>();
  for (const id of ACTION_IDS) {
    const chord = chordFor(keymap, id);
    if (!chord) continue;
    const list = byChord.get(chord);
    if (list) list.push(id);
    else byChord.set(chord, [id]);
  }
  const out: { chord: string; ids: ActionId[] }[] = [];
  for (const [chord, ids] of byChord) {
    if (ids.length >= 2) out.push({ chord, ids });
  }
  return out;
}

/** Display glyphs for a handful of keys whose real `KeyboardEvent.key` form reads awkwardly in a
 *  UI list (`ArrowLeft` vs. `←`, `Escape` vs. `Esc`) — the same substitutions
 *  `shortcuts.ts`'s static `SHORTCUT_GROUPS` table already uses for its `keys` column. Purely
 *  cosmetic: the canonical/stored chord form (what `chordFor`/`actionForChord`/`normalizeChord`
 *  operate on) is untouched by this map. */
const KEY_DISPLAY: Record<string, string> = {
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Escape: "Esc",
  Delete: "Del",
};

/** Friendly display form of a chord for a read-only viewer (CPE-1548): substitutes the handful of
 *  keys in `KEY_DISPLAY` for a nicer glyph and renders an unbound (`""`) chord as `"Unbound"`.
 *  Every other key/modifier passes through unchanged — this is purely presentational and must
 *  never be fed back into `setChord`/`actionForChord`, which expect the canonical event-derived
 *  form `chordFor` returns. */
export function formatChord(chord: string): string {
  if (!chord) return "Unbound";
  const parts = chord.split("+");
  const key = parts[parts.length - 1];
  const mods = parts.slice(0, -1);
  return [...mods, KEY_DISPLAY[key] ?? key].join("+");
}

/** Serialize a keymap for the settings store. */
export function serializeKeymap(keymap: Keymap): string {
  return JSON.stringify(keymap);
}

/**
 * Serialize a keymap for CLIPBOARD export (CPE-1550) — human-readable (pretty-printed) and
 * wrapped in a small envelope (`{ version, bindings }`) rather than `serializeKeymap`'s bare,
 * minified map. `serializeKeymap`/`parseKeymap` remain the settings-store's own persisted form
 * (unversioned, minified, used on every save/load); this is the deliberately distinct
 * user-facing "share/back up a keymap" form, versioned so a future breaking change to the
 * envelope shape has somewhere to branch. Exports the FULL effective keymap (every action, not
 * just overrides) — simpler to reason about than a diff against `defaultKeymap()`, and a full
 * snapshot is portable on its own even if the registry's set of default chords drifts between
 * versions.
 */
export function exportKeymap(keymap: Keymap): string {
  return JSON.stringify({ version: 1, bindings: keymap }, null, 2);
}

/**
 * Parse a clipboard-pasted keymap export (CPE-1550), tolerantly — same acceptance rules as
 * `parseKeymap` (known `ActionId` + a chord that normalizes cleanly or `""`), but instead of
 * silently dropping a bad entry, reports which action ids were actually applied vs. rejected so
 * the import UI can show the user what happened. Accepts both `exportKeymap`'s envelope
 * (`{ version, bindings }`) and a bare `{ actionId: chord }` map (so a hand-edited or
 * older/foreign snippet still imports). Chord CONFLICTS are allowed through unresolved — same as
 * every other edit path, they surface the next time `findConflicts` runs, not here. Never throws;
 * `base` (defaults to a fresh `defaultKeymap()`) supplies every action this import doesn't touch
 * or rejects, so the result is always a complete `Keymap`.
 */
export function importKeymap(
  json: string,
  base: Keymap = defaultKeymap(),
): { keymap: Keymap; applied: ActionId[]; rejected: string[] } {
  const out = { ...base };
  const applied: ActionId[] = [];
  const rejected: string[] = [];

  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    return { keymap: out, applied, rejected: ["<malformed JSON>"] };
  }

  const bindings =
    raw && typeof raw === "object" && !Array.isArray(raw) && "bindings" in (raw as Record<string, unknown>)
      ? (raw as Record<string, unknown>).bindings
      : raw;
  if (!bindings || typeof bindings !== "object" || Array.isArray(bindings)) {
    return { keymap: out, applied, rejected: ["<not a keymap object>"] };
  }

  for (const [key, value] of Object.entries(bindings as Record<string, unknown>)) {
    if (!isActionId(key)) {
      rejected.push(key);
      continue;
    }
    if (typeof value !== "string") {
      rejected.push(key);
      continue;
    }
    if (value === "") {
      out[key] = "";
      applied.push(key);
      continue;
    }
    const normalized = normalizeChord(value);
    if (normalized) {
      out[key] = normalized;
      applied.push(key);
    } else {
      rejected.push(key); // un-normalizable chord
    }
  }
  return { keymap: out, applied, rejected };
}

/**
 * Parse a persisted keymap, tolerantly. Keeps only entries whose key is a known `ActionId` and
 * whose value normalizes to a valid chord (via `normalizeChord`, so grandfathered bare-key
 * defaults survive a round-trip) or `""`; malformed JSON, non-object JSON, an unknown/renamed
 * action id, a non-string value, or a chord string that fails to normalize are all silently
 * dropped. Any action missing from the parsed object (partial/stale persisted map, or the
 * input being garbage entirely) is backfilled from `defaultKeymap()`, so this always returns a
 * complete `Keymap`. Never throws.
 */
export function parseKeymap(json: string | null | undefined): Keymap {
  const out = defaultKeymap();
  if (!json) return out;
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    return out;
  }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return out;
  for (const [key, value] of Object.entries(raw as Record<string, unknown>)) {
    if (!isActionId(key) || typeof value !== "string") continue;
    if (value === "") {
      out[key] = "";
      continue;
    }
    const normalized = normalizeChord(value);
    if (normalized) out[key] = normalized;
    // else: invalid chord string — drop, leaving the backfilled default in place.
  }
  return out;
}
