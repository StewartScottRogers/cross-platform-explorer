// Pure user-command list store (CPE-783, epic CPE-711). A user command is a named CPE-781 template plus
// where it surfaces (toolbar / context menu / palette) and how it runs over a selection (once per entry, or
// once with the entries joined). This is the immutable CRUD + reorder + tolerant persistence + surface
// filtering the binding UI is a thin layer over; actually launching a command is a separate, confirmed
// backend exec (never in this module — no DOM/IO here). Mirrors the stores in backup.ts / colorRulesStore.ts
// / workspaces.ts. Expansion is delegated to cmdTemplate.ts so there's one runner.

import type { DirEntry } from "./types";
import { expandForSelection } from "./cmdTemplate";

/** Where a command appears in the UI. */
export type CommandSurface = "toolbar" | "context" | "palette";

const SURFACES: readonly CommandSurface[] = ["toolbar", "context", "palette"];

/** A user-defined templated command. */
export interface UserCommand {
  id: string;
  name: string;
  /** CPE-781 template, e.g. `git add {path}`. */
  template: string;
  /** Run once per selected entry (`each`) or once with all entries joined (`joined`). */
  mode: "each" | "joined";
  /** Surfaces this command is bound to. */
  surfaces: CommandSurface[];
}

function newId(): string {
  return `uc_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
}

/** Append a new command. Returns a new list. */
export function addCommand(
  list: UserCommand[],
  name: string,
  template: string,
  opts: { mode?: "each" | "joined"; surfaces?: CommandSurface[] } = {},
): UserCommand[] {
  const { mode = "each", surfaces = ["context"] } = opts;
  return [...list, { id: newId(), name, template, mode, surfaces }];
}

/** Patch a command by id (id itself is immutable). Unknown id → no-op copy. */
export function updateCommand(
  list: UserCommand[],
  id: string,
  patch: Partial<Omit<UserCommand, "id">>,
): UserCommand[] {
  return list.map((c) => (c.id === id ? { ...c, ...patch } : c));
}

/** Remove a command by id. */
export function removeCommand(list: UserCommand[], id: string): UserCommand[] {
  return list.filter((c) => c.id !== id);
}

/** Move a command one step earlier (`dir < 0`) / later (`dir > 0`) — its order in menus. Clamped at ends. */
export function moveCommand(list: UserCommand[], id: string, dir: number): UserCommand[] {
  const i = list.findIndex((c) => c.id === id);
  if (i === -1 || dir === 0) return [...list];
  const j = dir < 0 ? i - 1 : i + 1;
  if (j < 0 || j >= list.length) return [...list];
  const next = [...list];
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}

/** The commands bound to a given surface, in list order. Pure. */
export function commandsForSurface(list: UserCommand[], surface: CommandSurface): UserCommand[] {
  return list.filter((c) => c.surfaces.includes(surface));
}

/**
 * Resolve a command to the concrete command line(s) it would run over `entries`, delegating to the CPE-781
 * expander. `each` → one line per entry; `joined` → a single line. The result is what the confirm dialog
 * shows before the backend exec — this module never launches anything.
 */
export function resolveCommand(cmd: UserCommand, entries: DirEntry[]): string[] {
  return expandForSelection(cmd.template, entries, cmd.mode);
}

function isSurface(x: unknown): x is CommandSurface {
  return typeof x === "string" && (SURFACES as readonly string[]).includes(x);
}

function isCommand(x: unknown): x is UserCommand {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  return (
    typeof o.id === "string" &&
    typeof o.name === "string" &&
    typeof o.template === "string" &&
    (o.mode === "each" || o.mode === "joined") &&
    Array.isArray(o.surfaces) &&
    o.surfaces.every(isSurface)
  );
}

/** Serialize a command list for the settings store. */
export function serializeCommands(list: UserCommand[]): string {
  return JSON.stringify(list);
}

/** Parse a persisted command list, dropping malformed entries (a bad command never breaks startup). */
export function parseCommands(json: string | null | undefined): UserCommand[] {
  if (!json) return [];
  try {
    const raw = JSON.parse(json);
    return Array.isArray(raw) ? raw.filter(isCommand) : [];
  } catch {
    return [];
  }
}
