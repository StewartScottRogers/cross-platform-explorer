---
id: CPE-1625
title: "Two agents running against the same folder: only the first is ever marked visited, so the second is invisible to Radar/Cost/History for the whole run"
type: Bug
status: Backlog
priority: Medium
component: Frontend
epic: CPE-1486
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Reviewer of CPE-1606 (PR #815) — a real gap in the fix that landed, not a
hypothetical. Filed rather than folded in, so the merged fix stayed bounded.

## The gap
`markVisited` (`src/lib/agentSessions.ts` ~L563-565) resolves the visited session with
`sessions.find((s) => normalizePath(s.cwd) === normalizePath(target))` — it takes the **first** match.

If two agent sessions run against the **same** `cwd` — entirely plausible in this app, which is built
around fleets and parallel agents — only the first-found session is ever added to `visitedSessionIds`,
even after the user opens that folder. The second session is then never armed, and stays permanently
invisible to Radar, Cost, and History for the rest of the run.

Before CPE-1606 (which armed every running session unconditionally) both co-located sessions were
watched. So this is a narrow regression introduced by an otherwise-correct fix, and it cuts directly
against `AGENT-WATCH.md`'s own tiebreaker: *nothing the agent does should be invisible*.

## Fix
Mark **every** session whose `cwd` matches the visited path, not just the first — the visited set is keyed
by session id, so it should collect all matches. Check the same first-match assumption elsewhere in the
module while you are in there.

## Acceptance criteria
- Two running sessions sharing one `cwd`: visiting that folder arms **both**; a test covers it.
- That test fails against the current code (negative control).
- Nested / deepest-match behaviour for genuinely different folders is unchanged.

**Conflict surface:** `src/lib/agentSessions.ts`, `src/lib/agentSessions.test.ts`.

## Work Log

**2026-08-11** — Fixed and verified.

- **Change:** `markVisited` in `src/lib/agentSessions.ts` used `sessions.find(...)` to resolve the
  session id at the visited path — first match only. Replaced with `sessions.filter(...).map(...)` to
  collect every session id whose `cwd` normalizes to the visited target, so co-located sessions are all
  added to the (id-keyed) visited set in one pass. `changed`-detection and the next-set builder were
  updated from a single `hitId` to the `hitIds` array; the "same instance when unchanged" identity
  contract is preserved.
- **First-match audit (as the ticket asked):** grepped `agentSessions.ts` for `.find(`/`.filter(` — the
  `.find()` at the old L71 was the only first-match lookup in the module. `watchTargetFor`'s
  deepest-match loop picks a *path string* by length, not a session id, so ties there don't drop a
  session — no other instance of the bug found in this file.
- **Test added:** `src/lib/agentSessions.test.ts` — "CPE-1625: arms BOTH sessions when two agents share
  the exact same cwd" (two sessions `s1`/`s2` both at `/work/api`, visiting `/work/api/routes`).
- **Negative control:** ran `npx vitest run src/lib/agentSessions.test.ts` against the pre-fix code —
  the new test FAILED as expected: `expected Set{ 's1' } to deeply equal Set{ 's1', 's2' }` (20 passed /
  1 failed). After the fix, same command: 21/21 passed, including all existing deepest-match/sibling/
  retention/eviction/no-op-identity tests unchanged.
- **Full verification:** `npm run check` → 0 errors, 0 warnings. `npx vitest run` (full suite) → 273
  files / 3326 tests, all passed (baseline ~272/~3319; the delta is the one new test).
- **Scope note:** left CPE-1626 (decoupling metrics flush from watcher teardown) untouched — separate
  ticket, separate concern, same file.
- Branch `cpe-1625-colocated-sessions`, PR opened against `main`.
