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
