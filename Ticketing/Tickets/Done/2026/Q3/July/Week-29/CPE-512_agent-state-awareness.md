---
id: CPE-512
title: "Agent semantic-state awareness (idle/working/blocked/done) on Agent Grid tiles + tabs"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [ready]
estimate: 2-3h
created: 2026-07-16
epic: CPE-501
closed: 2026-07-16
---

## Summary
From the Herdr spike ([[CPE-511]]): Herdr's headline feature is that it *understands what each pane is
doing* — surfacing live **idle / working / blocked / done** per agent from terminal-output heuristics.
Our Agent Grid ([[CPE-501]]) shows the panes but not their state. Add a per-session **semantic state
indicator** on the grid tile header + tab, computed from recent PTY output + recency — so at a glance
you can see which agents are **working**, which are **blocked waiting for you**, and which are **done**.

Filed autonomously during the dayshift on the CPE-511 recommendation; scoped to the launcher (in-window)
indicator. Relaying state out to the explorer's left-pane Agents leaves / Agent Watch is a noted
follow-up (needs a host event channel), not in this ticket.

## Acceptance Criteria
- [x] A pure, unit-tested classifier maps recent terminal output → `blocked | working | done | idle`
      (blocked = an input prompt awaiting the user; done = a completion signal at an idle prompt).
- [x] Each grid tile header **and** its tab shows a state dot/label with a distinct, theme-safe colour
      (blocked outranks working outranks done outranks idle).
- [x] State updates live as output arrives and decays to idle when a session goes quiet (recency).
- [x] `blocked` is visually the most prominent (it needs the user) — but uses theme-safe styling, not a
      hard-coded alarm red on menu text (menu rules don't apply to a status dot, but stay theme-safe).
- [x] Tests for the classifier patterns + the recency combination.

## Resolution
Added Herdr-style agent-state awareness to the AI Console, in `launcher.html`.

- **Pure classifier** `agentStateFromText(text)` → `blocked | done | working | idle` by priority: an
  input prompt awaiting the user (`(y/n)`, `[Y/n]`, "continue?/approve/press enter", or a line ending
  `?`) is **blocked**; a completion marker at the end (done/completed/finished/✓) is **done**; a
  spinner / trailing `…` / activity verb (thinking/running/building/…) is **working**; else **idle**.
- **`sessionState(s, now)`** layers **recency**: output within 1.5 s reads as *working* — unless the
  text shows the agent is actually *blocked* (a prompt outranks recency). Both are unit-tested.
- **Wiring:** `noteOutput` keeps a 2 KB rolling tail + last-output time per session (fed from
  `routeWrite`, so it works regardless of the CPE-508 throttle); `renderState` paints a **state dot**
  on the tab **and** the grid pane header (with a `working/blocked/…` word in the header); an 800 ms
  `tickStates` sweep decays *working* → *done/idle* when a session goes quiet.
- **Blocked is most prominent** (it needs you): the tab gets an amber inset accent and the dot pulses —
  theme-safe fixed hues, not menu text (menu colour rules untouched).

Explorer-side surfacing (left-pane Agents leaves / Agent Watch) is left as a follow-up — it needs a
host event channel to relay state out of the sidecar. Tests: classifier priority across
blocked/done/working/idle; the recency layering (fresh write → working, quiet → text class, prompt
wins); dots present on tab + pane header + blocked tab marking. 51 launcher + 526 frontend tests pass;
`npm run check` clean.

## Work Log
2026-07-16 — Filed + picked up (dayshift) on the [[CPE-511]] Herdr-spike recommendation. Estimate: 2-3h.
2026-07-16 — Built the pure classifier + recency-aware sessionState, wired noteOutput/renderState/tick sweep, added the state dot to tab + pane header with a prominent blocked treatment. 3 new jsdom tests.
2026-07-16 — Verified: 51 launcher + 526 frontend tests pass; `npm run check` clean. **Assumption logged (dayshift):** filed this ticket autonomously from the spike's top recommendation; scoped to the in-window indicator (explorer relay deferred). Heuristic patterns are best-effort and tunable. All ACs met.

## Notes
From [[CPE-511]]. Extends the Agent Grid (CPE-506/507). Explorer-side surfacing is a follow-up.
