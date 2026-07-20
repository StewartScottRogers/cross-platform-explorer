// Pure rule-list store for the coloring/labels editor (CPE-776, epic CPE-709). Immutable CRUD + reorder +
// tolerant serialize/parse over an ordered `ColorRule[]`, so the editor (a thin CRUD over these) and the
// settings persistence layer stay dumb. Order is meaningful — `evaluateRules` (colorRules.ts) takes the
// first enabled matching rule — so `moveRule` is how a user changes precedence. No DOM/IO; unit-tested.
// Mirrors the job-list store in backup.ts and the other CPE-77x/79x models.

import type { ColorRule, Condition } from "./colorRules";

const KNOWN_KINDS = ["ext", "glob", "size", "olderThan", "newerThan", "isDir"] as const;

function newId(): string {
  return `cr_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
}

/** Append a new enabled rule. Returns a new list. */
export function addRule(
  list: ColorRule[],
  when: Condition,
  opts: { color?: string; label?: string; enabled?: boolean } = {},
): ColorRule[] {
  const { color, label, enabled = true } = opts;
  return [...list, { id: newId(), when, color, label, enabled }];
}

/** Patch a rule by id (id itself is immutable). Returns a new list; unknown id is a no-op copy. */
export function updateRule(
  list: ColorRule[],
  id: string,
  patch: Partial<Omit<ColorRule, "id">>,
): ColorRule[] {
  return list.map((r) => (r.id === id ? { ...r, ...patch } : r));
}

/** Remove a rule by id. Returns a new list. */
export function removeRule(list: ColorRule[], id: string): ColorRule[] {
  return list.filter((r) => r.id !== id);
}

/** Enable/disable a rule. With `enabled` omitted, flips the current state (defaulting true→false). */
export function toggleRule(list: ColorRule[], id: string, enabled?: boolean): ColorRule[] {
  return list.map((r) => (r.id === id ? { ...r, enabled: enabled ?? !(r.enabled ?? true) } : r));
}

/**
 * Move a rule one step earlier (`dir < 0`) or later (`dir > 0`) in precedence order. Clamped at the ends
 * (moving the first rule up is a no-op). Returns a new list; unknown id is a no-op copy.
 */
export function moveRule(list: ColorRule[], id: string, dir: number): ColorRule[] {
  const i = list.findIndex((r) => r.id === id);
  if (i === -1 || dir === 0) return [...list];
  const j = dir < 0 ? i - 1 : i + 1;
  if (j < 0 || j >= list.length) return [...list]; // already at an end
  const next = [...list];
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}

function isCondition(x: unknown): x is Condition {
  if (!x || typeof x !== "object") return false;
  const kind = (x as Record<string, unknown>).kind;
  return typeof kind === "string" && (KNOWN_KINDS as readonly string[]).includes(kind);
}

function isRule(x: unknown): x is ColorRule {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  return typeof o.id === "string" && isCondition(o.when);
}

/** Serialize a rule set for the settings store. */
export function serializeRules(list: ColorRule[]): string {
  return JSON.stringify(list);
}

/** Parse a persisted rule set, dropping any malformed entries (a bad rule never breaks startup). */
export function parseRules(json: string | null | undefined): ColorRule[] {
  if (!json) return [];
  try {
    const raw = JSON.parse(json);
    return Array.isArray(raw) ? raw.filter(isRule) : [];
  } catch {
    return [];
  }
}
