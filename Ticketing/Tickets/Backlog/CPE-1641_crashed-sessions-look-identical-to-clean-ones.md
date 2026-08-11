---
id: CPE-1641
title: "A crashed agent session looks identical to a clean one in History — endedCleanly is recorded and never shown, and its duration is silently inflated"
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
Found by the independent UAT tester of CPE-1626 (PR #830). That PR fixed a real data-loss bug — a crashed or
reaped agent session used to lose its record entirely — by force-flushing on Agent Deck close and tagging the
row `endedCleanly: false`. The data is now safe. **The distinction never reaches the user.**

This is the crew's recurring failure shape, one layer along: the information is captured correctly and then
silently dropped before anyone sees it. Precedents — CPE-1591 (archive scanner reported "no risk detected"
after reading zero entries), CPE-1615 (a corrupt binary rendered as a healthy empty module), CPE-1600 (a
failed checkpoint had to be made structurally distinct from a real one). Same rule: **"we don't know" must
never look like "it's fine."**

## The gap
The UAT traced every consumer. `endedCleanly` appears in `bindings.gen.ts` (wire type),
`agentSessionMetrics.ts` (writer), `metrics_journal.rs` (backend struct + tests) — and in **no `.svelte`
file's logic or markup at all**.

- The only UI reading the persisted journal is `AgentTimeline.svelte`'s **History** tab, whose sole data path
  is `agentMetricsRollup.ts`'s `rollup()`. That folds into `MetricsTotals` / `ModelRollupRow` /
  `AgentRollupRow`, none of which carry the field. The tester proved it numerically: two records identical
  except for `endedCleanly` roll up to **byte-identical** totals, per-model, per-agent, averages and ratios.
- There is no per-session row anywhere in History today — only aggregates — so even a badge has nowhere to
  attach yet.
- The live **Cost** tab is structurally exempt too: it renders `SessionMetrics` (the in-memory accumulator),
  which has no such field.

## The compounding problem — inflated durations
On a forced flush, `endedAt` is stamped at **flush time** (when the user clicks "Close all consoles"), not at
the real crash time. A session that died at 2:00pm and whose deck is closed at 4:00pm persists a
`wallClockMs` two hours too long — and everything derived from it (History's "Total time", the
tokens-per-minute throughput ratio) is silently overstated, with nothing on screen to say the number is
unreliable. A user comparing agent efficiency would be reading fiction.

## Fix
Two halves, both needed:
1. **Surface the distinction.** Make an unclean end visible where a user would look. Options worth weighing:
   a per-session listing in History (which does not exist yet and may be the more valuable feature anyway),
   or a clear qualifier on the aggregates when they include unclean rows. Follow CPE-1600's precedent — the
   distinction must survive colour-blindness and a hurried glance, so carry it with an icon/label, not hue.
2. **Stop overstating duration.** Either persist the last-observed-activity timestamp as the effective end
   for a forced flush, or record the duration as explicitly unknown/lower-bounded rather than computing it
   from a flush-time stamp. **Do not** silently substitute a guess that looks precise — an honest
   "unknown" beats a confident wrong number, which is the whole point of this ticket.
   Then decide what the throughput ratio should do when duration is unknown: omit it rather than divide by a
   fabricated number.

## Acceptance criteria
- A crashed/reaped session is visibly distinguishable from a clean one in the app, not just in the journal;
  a test asserts the UI actually renders the distinction.
- Aggregates that include unclean rows either exclude the unreliable duration or say so.
- No forced-flush row reports a duration derived from flush time as if it were measured.
- Existing history (pre-CPE-1626 rows, which default to `endedCleanly: true`) still reads correctly — the
  UAT confirmed that default is factually right for those rows, since the old code only ever flushed on an
  end-shaped event. Don't regress that.

**Conflict surface:** `src/lib/components/AgentTimeline.svelte`, `src/lib/agentMetricsRollup.ts`,
`src/lib/agentSessionMetrics.ts`, and possibly `crates/server/src/metrics_journal.rs` if the persisted shape
changes. Land CPE-1626 (PR #830) first.
