---
id: CPE-1606
title: "Agent Watch keeps every running agent session's filesystem watcher armed even when you never open its folder — violates AGENT-WATCH.md's \"off means off\" boundary"
type: Bug
status: Doing
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

## Work Log — 2026-08-11

Picked resolution **#2**: scoped the watch set back down instead of just documenting the
watch-everything behavior, since "off means off" is explicitly called the mode's one hard constraint
(`CLAUDE.md` Precedence section) and the literal repro (launch an agent, never open its folder) is
exactly the case that constraint exists to cover.

**Code changes:**
- `src/lib/agentSessions.ts` — added `markVisited(sessions, current, visited)`, a pure function that
  grows a "visited this run" `Set<sessionId>`: a session's id is added the first time `current` falls
  inside its project (via the existing `watchTargetFor`), and pruned once the session stops appearing in
  `sessions` (i.e. actually ends). Returns the same `Set` instance when nothing changed, so callers can
  skip redundant reconcile work. `watchTargets(sessions, visited)` now filters to `visited.has(sessionId)`
  instead of returning every running session unconditionally (the CPE-1099 behavior this replaces).
- `src/App.svelte` — added a `visitedSessionIds` component variable and a `reconcileVisitedAndWatch`
  function that calls `markVisited` then feeds the result into `reconcileAgentWatch`, which now takes the
  visited set as a second parameter and passes it through to `watchTargets`. Replaced the old
  `$: reconcileAgentWatch($agentSessions);` reactive statement with
  `$: reconcileVisitedAndWatch($agentSessions, currentPath);` (a plain function call, not a
  self-referential `$: visited = f(..., visited)` assignment, to keep the read-then-write unambiguous
  under Svelte's reactivity model). `activeWatchCwd` (the strip/badge computation) is untouched — it
  still uses `watchTargetFor` directly and behaves exactly as before.

**Debounce/retention reasoning (per the sprint brief's "don't thrash on sibling navigation" ask):**
Once a project is visited, its session id stays in `visitedSessionIds` — and its watcher stays armed —
for the rest of that session's life, even after the explorer navigates away, including to a sibling
agent's project. This is a deliberate choice, not leftover CPE-1099 residue, for two reasons:
1. **No watcher thrash.** Naively disarming the instant you leave the folder would tear down and re-arm
   a `notify` watcher (an async Tauri round-trip each way) on every single navigation when a user hops
   back and forth between two sibling agent projects (e.g. `/work/api` and `/work/web`, both running
   agents) — exactly the scenario the brief called out.
2. **Metrics integrity.** `reconcileAgentWatch`'s "stop the removed" loop calls `flushSession` before
   `stopAgentWatch` (CPE-1113) — i.e. removing a session from the armed set finalizes its metrics row as
   if the session had ended. Disarming on a mere navigate-away (while the session is still running) would
   prematurely flush that session's Cost/History data, and a later re-visit would start a *second*,
   fragmented row for the same live session instead of resuming the first. Retaining the watch avoids
   this entirely — a session's metrics are flushed exactly once, when it actually ends.
A session that is **never** visited, however, never enters `visitedSessionIds`, so it's never armed at
all, no matter how long it runs — that's what fixes the ticket's literal repro (steps 1–4 never navigate
the explorer into `P`).

**Docs reconciled:**
- `AGENT-WATCH.md` — Boundaries section rewritten to describe the CPE-1606 behavior accurately: off means
  off now genuinely holds for a project you never open; visited-and-retained is called out explicitly as
  a deliberate choice with the reasoning above, replacing the stale pre-CPE-1099 "watchers idle" framing
  that CPE-1099 had silently invalidated. Also updated the "Filesystem watcher (CPE-398)" bullet under
  "What's built" to cross-reference the new gating.
- `src/docs/explorer-agent-watch.md` — the "Limits/notes" bullet (added by CPE-1604 to describe the
  CPE-1099 watch-everything behavior honestly) rewritten to describe the post-fix behavior: "off means
  off" now holds for a project you never open; the nuance is that a *visited* project stays watched for
  the session's life (with the why), not that every running session is watched regardless of whether
  you ever opened it.
- `src/docs/03-explorer.md` — checked; it already delegates to `explorer-agent-watch.md` without
  restating the false claim (the claim the ticket quotes from its "Why" section had already been removed
  by CPE-1604, replaced by a cross-reference). No change needed there.

**Tests added** (`src/lib/agentSessions.test.ts`, vitest): rewrote the CPE-1099-era `watchTargets`
describe block and added a new `markVisited` describe block — 12 new/rewritten test cases covering: no
sessions ⇒ empty; running-but-unvisited sessions stay unarmed (the literal repro); only the visited
session is armed while a sibling stays untouched; a solo unrelated navigation leaves the visited set
empty; visiting one sibling doesn't visit another; a visited session is retained after navigating away;
visiting two siblings in turn accumulates both without evicting either; a session that actually ends is
pruned from the visited set (real "leaving disarms"); and `markVisited` returns the same `Set` instance
(reference equality) when nothing changed.

**Verification (run from the worktree, all synchronous, all observed passing):**
- `npm run check` → `svelte-check found 0 errors and 0 warnings`.
- `npx vitest run` (full suite, not just the new file) → `Test Files 272 passed (272)` /
  `Tests 3319 passed (3319)`.
- `npx vitest run src/lib/agentSessions.test.ts` → `Test Files 1 passed (1)` / `Tests 20 passed (20)`
  (was 12 before this change).
- No Rust files touched, so `cargo build`/`cargo clippy` were not run (not applicable per the ticket's
  own conflict surface — frontend-only).

**Assumptions:** interpreted the sprint brief's "leaving disarms" test requirement as "a session that
actually ends is disarmed" (tested explicitly) rather than "navigating away from a still-running visited
session disarms it," per the explicit "debounce/retain sensibly ... don't thrash" instruction in the same
brief and the metrics-fragmentation risk above — logged here per instruction. No new dependencies added.
