import type { AgentSession } from "./sidecar";

/**
 * Owner-coloured heat-map palette (CPE-1116, epic CPE-730): maps an actor id (a `sessionId` /
 * `"user"` / `"unknown"`) to a **theme var**, never a literal colour computed from the id (e.g.
 * HSL-from-hash) — that would bake a colour into a component's inline style that can't be reskinned
 * and isn't guaranteed legible against the surface it sits on. The fixed palette lives in
 * `src/app.css` as `--agent-1`..`--agent-6` plus dedicated `--agent-user` / `--agent-unknown` vars
 * (Opt A, decided in `.claude/research-library/entries/conflict-radar-close-plan.md`).
 */

/** Size of the cycling agent palette (`--agent-1`..`--agent-N` in `src/app.css`). Concurrent agent
 *  sessions are 1-4 in practice; more than this many reuse a colour, which is an acceptable,
 *  documented trade-off (see the ticket) rather than an unbounded, non-theme-var palette. */
export const AGENT_COLOR_COUNT = 6;

/** The CSS var for the palette's `index`-th slot (0-based in, 1-based `--agent-N` out). */
function agentVar(index: number): string {
  const slot = (((index % AGENT_COLOR_COUNT) + AGENT_COLOR_COUNT) % AGENT_COLOR_COUNT) + 1;
  return `var(--agent-${slot})`;
}

/** A short, deterministic string hash (djb2), used only as the *fallback* index when an actor's
 *  session can't be found in `sessions` (e.g. an ended session whose annotation is still fading
 *  out). Never influences colour when the actor resolves normally via the sorted session list. */
function hashIndex(id: string, mod: number): number {
  let h = 5381;
  for (let i = 0; i < id.length; i++) {
    h = (h * 33) ^ id.charCodeAt(i);
  }
  return Math.abs(h) % mod;
}

/**
 * The theme-var colour for `actor` (CPE-1116).
 *
 * - `"user"` (or empty/falsy) -> the dedicated `--agent-user` var.
 * - `"unknown"` -> the neutral, muted `--agent-unknown` var.
 * - Any other actor (a `sessionId`) -> cycles through `--agent-1..--agent-N` by a **stable** index:
 *   the actor's position in `sessions` sorted by `sessionId`. Sorting (rather than array order)
 *   means the same live session set always yields the same index no matter what order sessions
 *   arrived in, so a colour never shuffles across a re-render.
 * - If the actor isn't found in `sessions` (its session already ended, but its activity annotation
 *   is still fading out under the TTL), falls back to a deterministic id-hash so the colour stays
 *   *some* stable value rather than defaulting to unknown-grey or crashing.
 */
export function colorForActor(actor: string, sessions: AgentSession[]): string {
  if (!actor || actor === "user") return "var(--agent-user)";
  if (actor === "unknown") return "var(--agent-unknown)";

  const sortedIds = sessions
    .map((s) => s.sessionId)
    .filter((id) => !!id)
    .sort((a, b) => a.localeCompare(b));
  const i = sortedIds.indexOf(actor);
  return agentVar(i >= 0 ? i : hashIndex(actor, AGENT_COLOR_COUNT));
}
