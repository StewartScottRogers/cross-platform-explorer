/**
 * Navigation Mode's mini ":" command-line bridge (CPE-1554, epic CPE-1487 "Keyboard Navigation
 * Mode"). CPE-1487's brief calls for a `:`-style command line inside Navigation Mode that resolves
 * typed text against the app's **existing** Command Palette verbs — this module deliberately does
 * NOT fork a second command registry or matcher. It is a thin wrapper around `filterCommands`/
 * `isEnabled`, both imported read-only from `./commandPalette`.
 *
 * INERT: nothing calls this yet. CPE-1556 mounts `NavCommandLine.svelte` (which uses this module)
 * and feeds it the real `paletteCommands` array assembled in `App.svelte`, wiring it to Navigation
 * Mode's `startCommand` intent (CPE-1552's `navMode.ts`). Landing this ships with zero behavior
 * change — it's only reachable from its own tests today.
 */

import { filterCommands, isEnabled, type Command } from "./commandPalette";

/**
 * Filter+rank `commands` for a mini command-line `query`, reusing `commandPalette.ts`'s own
 * `filterCommands` (for matching/ranking) and `isEnabled` (for availability) — no separate scoring
 * logic lives here. A leading `:` — the command line's own prompt prefix, present if the caller
 * passes the raw input including it — is stripped before matching, so callers may pass either the
 * bare query or the full `:foo` text the user typed. An empty/whitespace-only query (after
 * stripping the `:`) returns every enabled command, unfiltered, in the same order `filterCommands`
 * would give a blank query (declaration order) — so opening the command line with no input shows
 * every available verb. Disabled commands (`isEnabled(c) === false`) are excluded unconditionally,
 * including from that blank-query "show everything" case and from an otherwise exact label match.
 */
export function filterNavCommands(commands: Command[], query: string): Command[] {
  const stripped = query.startsWith(":") ? query.slice(1) : query;
  return filterCommands(commands, stripped)
    .map((scored) => scored.command)
    .filter(isEnabled);
}
