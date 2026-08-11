---
id: CPE-1606
title: "Agent Watch keeps every running agent session's filesystem watcher armed even when you never open its folder — violates AGENT-WATCH.md's \"off means off\" boundary"
type: Bug
status: Backlog
priority: Medium
component: Frontend
epic: CPE-1486
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Found writing the CPE-1604 docs page for Agent Watch (epic CPE-1569) while verifying the existing
`03-explorer.md` claim — "Leave the folder, or let the agent's session end, and the strip disappears
along with everything it drives — no watched session means no Agent Watch, no background watcher, and no
cost" — against the real code. That claim is false for the *watcher*, only true for the *strip*.

`AGENT-WATCH.md`'s Boundaries section states the one constraint the mode may never spend: *"With Agent
Watch disabled, the plain explorer must still be fast, small, and predictable. Watchers idle, no
background polling, no startup penalty."* `CLAUDE.md`'s own Precedence section calls this "the single hard
constraint" of the whole mode.

## The gap
`src/lib/agentSessions.ts` (`watchTargetFor`, ~line 37) computes `activeWatchCwd` — the folder the
**strip and file-list badges** key off — as "the deepest running-agent project that contains the folder
you're currently navigated into, or none." That part behaves as documented: leave the folder, the strip
disappears.

But the actual filesystem-watch arming doesn't use `activeWatchCwd` at all. `watchTargets()`
(`agentSessions.ts` ~line 54) returns **every currently-running session**, unconditionally:

```ts
export function watchTargets(sessions: AgentSession[]): AgentSession[] {
  return sessions; // ALL of them — not filtered to activeWatchCwd
}
```

and `src/App.svelte`'s `reconcileAgentWatch` (~line 1431-1436) arms a real filesystem watcher against
every session in that set:

```ts
async function reconcileAgentWatch(sessions: AgentSession[]) {
  ...
  const desired = new Map<string, string>();
  for (const s of watchTargets(sessions)) if (s.cwd) desired.set(s.sessionId, s.cwd);
  // starts a watch for each entry in `desired`, independent of activeWatchCwd
```

The in-code comment above it is explicit that this is deliberate, for CPE-1099's cross-session Radar/Cost/
History features: *"Agent Watch now watches every running session concurrently ... cwd-overlap is the
radar's frontend fold, not a watch-selection concern."*

Net effect: the moment you launch **any** coding agent from the AI Console, a `notify` filesystem watcher
is armed on its project folder and stays armed — accumulating timeline/cost/activity data — for as long as
that session lives, **even if you never once navigate the explorer into that folder**. Leaving a watched
folder only hides the strip; it does not stop the watcher, contrary to both the shipped `03-explorer.md`
prose and `AGENT-WATCH.md`'s stated boundary.

## Reproduction
1. Launch a coding agent from the AI Console pointed at some project folder `P`.
2. In the explorer, navigate anywhere **other than** `P` (or a folder never related to `P` at all).
3. Have the agent create/edit a file in `P`.
4. Open **Session history…** (Command Palette) — the session's events are recorded, proving the watcher
   ran the whole time even though the strip was never shown and you never looked at `P`.

## Is this a bug or an intentional trade-off?
Almost certainly an intentional, reasoned trade-off (the code comment cites CPE-1099 by name and explains
the reasoning) — filing this as a **doc-accuracy bug**, not a "revert the behavior" ask. Two acceptable
resolutions, either is fine:
1. **Fix the docs** (already done in CPE-1604's new `explorer-agent-watch.md`) to state plainly that only
   the strip/annotations are folder-scoped, while the watcher itself runs per-session for the session's
   whole lifetime — and update `AGENT-WATCH.md`'s Boundaries section to reflect the CPE-1099 change instead
   of the pre-CPE-1099 single-session model it still describes.
2. **Or**, if the "off means off" boundary is meant to be load-bearing (e.g. for machines with many
   long-lived agent sessions where this could add up), scope the watch set back down and make Radar/Cost/
   History explicitly "only sessions you've visited this run" instead of "every running session."

## Acceptance criteria
- `AGENT-WATCH.md`'s Boundaries section accurately describes the current (CPE-1099-era) watch-everything
  behavior, OR the behavior itself changes to match the original boundary — pick one and make the two
  agree.
- No further doc claims "leaving the folder stops the watcher" unless that's actually true again.

## Notes
Conflict surface: `AGENT-WATCH.md`, `src/lib/agentSessions.ts`, `src/App.svelte` (`reconcileAgentWatch`).
`src/docs/explorer-agent-watch.md` (CPE-1604) already documents the shipped behavior honestly — this
ticket is about closing the gap between the design doc and reality, not blocking CPE-1604. Model: sonnet
(mostly a docs/behavior decision, not fiddly code).
