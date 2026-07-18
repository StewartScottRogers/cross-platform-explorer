// Command Palette core (CPE-602): a searchable registry of the app's actions. Pure + DOM-free so the
// matching/ranking is unit-tested; the Svelte overlay just renders `filterCommands(...)` and calls
// `run()`. Commands are declared where their handlers live (App.svelte) — nothing is duplicated here.

export interface Command {
  /** Stable id (for keys). */
  id: string;
  /** What the user reads and searches. */
  label: string;
  /** Optional grouping header, e.g. "Navigate", "View". */
  group?: string;
  /** Extra search terms not shown in the label (synonyms). */
  keywords?: string;
  /** Display-only shortcut hint, e.g. "Ctrl+T". */
  shortcut?: string;
  /** Perform the action. */
  run: () => void;
  /** When present and false, the command shows greyed and can't be run. */
  enabled?: () => boolean;
}

/** A command paired with its match score (higher = better), for rendering + ranking. */
export interface ScoredCommand {
  command: Command;
  score: number;
}

/**
 * Score how well `query` matches `text` (both compared case-insensitively). Higher is better; `0` means
 * no match. Ranking, best to worst: exact equality > prefix > word-boundary start > plain substring >
 * in-order subsequence (fuzzy). An empty query matches everything with a neutral score.
 */
export function scoreMatch(text: string, query: string): number {
  const t = text.toLowerCase();
  const q = query.trim().toLowerCase();
  if (!q) return 1;
  if (t === q) return 1000;
  if (t.startsWith(q)) return 800;
  // Word-boundary start (after a space, /, \, -, _, .).
  const at = t.indexOf(q);
  if (at > 0 && /[\s/\\\-_.]/.test(t[at - 1])) return 600;
  if (at >= 0) return 400 - at; // earlier substring beats later
  // Subsequence: every query char appears in order.
  let qi = 0;
  for (let i = 0; i < t.length && qi < q.length; i++) {
    if (t[i] === q[qi]) qi++;
  }
  return qi === q.length ? 100 : 0;
}

/**
 * Filter + rank commands for `query` against their label and keywords. Ties keep the original
 * (declaration) order — a stable sort — so a blank query returns the commands as given.
 */
export function filterCommands(commands: Command[], query: string): ScoredCommand[] {
  const q = query.trim();
  return commands
    .map((command, i) => {
      const score = Math.max(scoreMatch(command.label, q), command.keywords ? scoreMatch(command.keywords, q) - 50 : 0);
      return { command, score, i };
    })
    .filter((s) => s.score > 0)
    .sort((a, b) => b.score - a.score || a.i - b.i)
    .map(({ command, score }) => ({ command, score }));
}

/** Whether a command is runnable right now (no `enabled` predicate ⇒ always). */
export function isEnabled(c: Command): boolean {
  return c.enabled ? c.enabled() : true;
}
