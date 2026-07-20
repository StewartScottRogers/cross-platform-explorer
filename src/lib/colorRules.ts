// Pure rule-evaluation engine for file coloring & labels (CPE-774, epic CPE-709). Given a file entry and
// an ordered rule set, resolve the row's style. No DOM — unit-tested here — so the renderer (CPE-775) is a
// thin apply and the editor (CPE-776) a thin CRUD. First enabled matching rule wins.

import { matchesGlob } from "./glob";
import type { DirEntry } from "./types";

/** A single condition a rule tests an entry against. */
export type Condition =
  | { kind: "ext"; exts: string[] } // extension in the list (case-insensitive, leading dot optional)
  | { kind: "glob"; pattern: string } // name matches a glob
  | { kind: "size"; min?: number; max?: number } // size in bytes, inclusive bounds
  | { kind: "olderThan"; days: number } // modified more than N days ago
  | { kind: "newerThan"; days: number } // modified within the last N days
  | { kind: "isDir"; value: boolean };

/** A user rule: when the condition matches, apply the color and/or label. */
export interface ColorRule {
  id: string;
  when: Condition;
  color?: string;
  label?: string;
  /** Defaults to true; a disabled rule is skipped. */
  enabled?: boolean;
}

/** The resolved styling for a row (empty when no rule matched). */
export interface RuleStyle {
  color?: string;
  label?: string;
}

const DAY_MS = 86_400_000;

/** Lowercased extension without the dot, or "" for dotfiles / no extension. */
function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** Whether `entry` satisfies `cond`, evaluated against wall-clock `now` (epoch ms). Pure. */
export function matchesCondition(entry: DirEntry, cond: Condition, now: number): boolean {
  switch (cond.kind) {
    case "ext": {
      const e = extensionOf(entry.name);
      return e !== "" && cond.exts.some((x) => x.replace(/^\./, "").toLowerCase() === e);
    }
    case "glob":
      return matchesGlob(entry.name, cond.pattern);
    case "size":
      return (
        (cond.min === undefined || entry.size >= cond.min) &&
        (cond.max === undefined || entry.size <= cond.max)
      );
    case "olderThan":
      return entry.modified !== null && now - entry.modified > cond.days * DAY_MS;
    case "newerThan":
      return entry.modified !== null && now - entry.modified < cond.days * DAY_MS;
    case "isDir":
      return entry.is_dir === cond.value;
    default:
      return false;
  }
}

/**
 * Resolve the style for `entry` from an ordered rule set: the first enabled rule whose condition matches
 * supplies the `{ color?, label? }`. Returns `{}` when nothing matches. Pure.
 */
export function evaluateRules(entry: DirEntry, rules: ColorRule[], now: number): RuleStyle {
  for (const r of rules) {
    if (r.enabled === false) continue;
    if (matchesCondition(entry, r.when, now)) {
      return { color: r.color, label: r.label };
    }
  }
  return {};
}
