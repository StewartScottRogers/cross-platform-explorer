// Pure watched-folder rule model + planner (CPE-793, epic CPE-734). A rule pairs a trigger `Condition`
// (reused from CPE-774) with an ordered action pipeline; the planner resolves the actions for a landed file
// (rename templates expanded via CPE-781). No DOM/IO — unit-tested — so the executor (CPE-794) and the
// dry-run editor (CPE-795) are thin. First enabled matching rule wins, so one rule handles a file.

import type { DirEntry } from "./types";
import { matchesCondition, isValidCondition, type Condition } from "./colorRules";
import { expandTemplate } from "./cmdTemplate";

/** One step in a rule's action pipeline. */
export type Action =
  | { kind: "move"; dest: string }
  | { kind: "copy"; dest: string }
  | { kind: "tag"; tag: string }
  | { kind: "rename"; template: string };

export interface WatchRule {
  id: string;
  name: string;
  when: Condition;
  actions: Action[];
  enabled?: boolean;
}

/** An action with any template resolved against the entry (e.g. rename → concrete new name). */
export interface PlannedAction {
  action: Action;
  /** The resolved value for actions that carry a template (rename); otherwise the literal dest/tag. */
  resolved: string;
}

export interface Plan {
  rule: WatchRule;
  actions: PlannedAction[];
}

function newId(): string {
  return `wr_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
}

function resolve(action: Action, entry: DirEntry): string {
  switch (action.kind) {
    case "rename":
      return expandTemplate(action.template, entry);
    case "move":
    case "copy":
      return action.dest;
    case "tag":
      return action.tag;
  }
}

/** The plan for `entry`: the first enabled rule whose condition matches, with its actions resolved; or
    `null` when nothing matches. This is the dry-run engine. Pure. */
export function planForEntry(entry: DirEntry, rules: WatchRule[], now: number): Plan | null {
  for (const rule of rules) {
    if (rule.enabled === false) continue;
    if (matchesCondition(entry, rule.when, now)) {
      return { rule, actions: rule.actions.map((action) => ({ action, resolved: resolve(action, entry) })) };
    }
  }
  return null;
}

// ── list store (mirrors smartFolders / colorRules) ──────────────────────────────────────────────
export function addRule(list: WatchRule[], name: string, when: Condition, actions: Action[]): WatchRule[] {
  return [...list, { id: newId(), name, when, actions, enabled: true }];
}
export function removeRule(list: WatchRule[], id: string): WatchRule[] {
  return list.filter((r) => r.id !== id);
}
export function renameRule(list: WatchRule[], id: string, name: string): WatchRule[] {
  return list.map((r) => (r.id === id ? { ...r, name } : r));
}
export function setRuleEnabled(list: WatchRule[], id: string, enabled: boolean): WatchRule[] {
  return list.map((r) => (r.id === id ? { ...r, enabled } : r));
}
export function updateRule(list: WatchRule[], id: string, patch: Partial<Omit<WatchRule, "id">>): WatchRule[] {
  return list.map((r) => (r.id === id ? { ...r, ...patch } : r));
}
/** Move a rule one step earlier (`dir < 0`) / later (`dir > 0`) — precedence order (first match wins).
    Clamped at the ends; unknown id / `dir === 0` → a no-op copy. */
export function moveRule(list: WatchRule[], id: string, dir: number): WatchRule[] {
  const i = list.findIndex((r) => r.id === id);
  if (i === -1 || dir === 0) return [...list];
  const j = dir < 0 ? i - 1 : i + 1;
  if (j < 0 || j >= list.length) return [...list];
  const next = [...list];
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}

const isRule = (x: unknown): x is WatchRule => {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  // Validate `when` structurally (not just truthy): a known-kind condition with missing fields would
  // otherwise survive parse and throw later in the planner's `matchesCondition`.
  return (
    typeof o.id === "string" &&
    typeof o.name === "string" &&
    isValidCondition(o.when) &&
    Array.isArray(o.actions)
  );
};

/** Parse persisted rules. Tolerant: bad JSON / wrong shape → `[]`, invalid entries dropped. */
export function parseRules(json: string | null): WatchRule[] {
  if (!json) return [];
  try {
    const raw = JSON.parse(json);
    return Array.isArray(raw) ? raw.filter(isRule) : [];
  } catch {
    return [];
  }
}
export function serializeRules(list: WatchRule[]): string {
  return JSON.stringify(list);
}
