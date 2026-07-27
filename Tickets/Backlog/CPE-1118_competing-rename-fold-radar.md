---
id: CPE-1118
title: "Conflict radar: competing-rename detection fold + Radar surfacing"
type: feature
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-07-26
epic: CPE-730
---

## Summary
CPE-730 DoD item — detect and surface **competing renames** across sessions (deferred by CPE-914). A pure
frontend fold over the actor-tagged, from→to-paired rename stream (from CPE-1117), surfaced in the Radar tab.
FRONTEND only. **Blocked on CPE-1117** (no `from`/`to` data until it lands). Design + ground truth:
`.claude/research-library/entries/conflict-radar-close-plan.md` (Ticket C). Mirror sidecar `conflict_rename.rs`.

## Design (buildable)
- new `src/lib/agentRenameConflicts.ts` — `foldRenameConflicts(entries)` mirroring `conflict_rename.rs`: group
  renamed entries carrying `{from, to, actor}`; **Divergence** = same `from`, ≥2 distinct actors, ≥2 distinct
  `to` (key on `from`); **Collision** = same `to`, ≥2 distinct actors, ≥2 distinct `from` (key on `to`); ignore
  same-agent and `from==to`; optionally gate by the existing `OVERLAP_WINDOW_MS` const.
- `src/lib/agentActivity.ts` — `TimelineEntry` gains `from`/`to` (default absent).
- `src/lib/components/AgentTimeline.svelte` (Radar tab) — a second section "Competing renames" with a
  diverge/collide badge + `friendlyActor` pills, matching the existing `.rd-*` markup + tick-tack reflow; hedged
  wording consistent with `agentConflicts.ts`; empty → the existing "no overlap" empty state.

## ⚠ Guardrails
- Frontend only; no backend. No new deps. Theme vars only; pills reflow. Off-means-off (fold needs ≥2 distinct
  actors — zero cost single-session). **Sequences AFTER CPE-1116** (both edit `agentActivity.ts` type region) and
  after CPE-1114 merges (both edit `AgentTimeline.svelte`).

## Acceptance Criteria
- [ ] Divergence (same from, diff to) and collision (diff from, same to) across distinct actors render in the
      Radar tab with a kind badge + actor pills; same-agent renames and `from==to` no-ops never flagged.
- [ ] Empty → existing "no overlap" empty state; off-means-off; no new deps; reflows per tick-tack convention.
- [ ] `npm run check` clean; `npm test` green.

## Tests
- `agentRenameConflicts.test.ts` porting `conflict_rename.rs`'s suite: disjoint→none, same-from/diff-to→
  divergence, diff-from/same-to→collision, same-agent→none, no-op→none, both-reported, sorted output.

## Work Log
2026-07-26 (workshift) — Filed from the CPE-730 close plan. **Blocked on CPE-1117** (rename from→to capture) and
sequenced after CPE-1116/CPE-1114 (shared files). Once this + CPE-1116 + CPE-1117 land, CPE-730's DoD is met →
CLOSE the epic.
