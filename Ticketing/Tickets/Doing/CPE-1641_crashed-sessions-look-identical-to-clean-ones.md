---
id: CPE-1641
title: "A crashed agent session looks identical to a clean one in History — endedCleanly is recorded and never shown, and its duration is silently inflated"
type: Bug
status: Doing
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

## Work Log (2026-08-11)

**Ticket claims verified against the code first, as instructed:**
- Confirmed `endedCleanly` (`src/lib/bindings.gen.ts`, `crates/server/src/metrics_journal.rs`,
  `src/lib/agentSessionMetrics.ts`) is written and persisted, and was read by **no** `.svelte` file —
  `AgentTimeline.svelte`'s History tab only ever called `rollup()` (`agentMetricsRollup.ts`), whose
  `MetricsTotals`/`ModelRollupRow`/`AgentRollupRow` carry no such field. Claim held.
- Confirmed History had no per-session row at all — only aggregates. Claim held.
- Confirmed `flushAllSessionsForcibly` (`agentSessionMetrics.ts`) stamped a forced row's `endedAt` at
  flush time (`now`, i.e. whenever `closeAllConsoles` ran), not at the session's real end. Claim held —
  this is the inflated-duration bug.

**Fix, both halves:**
1. **Surfaced the distinction (per-session list, not just an aggregate qualifier — the ticket flagged
   this as "may be the more valuable feature anyway").** Added `sessionRows`/`isSessionEndedCleanly`/
   `uncleanSessionCount` (pure helpers, `agentMetricsRollup.ts`) and a new **Sessions** table in the
   History tab (`AgentTimeline.svelte`): newest-first, one row per persisted session, each carrying a
   `Clean`/`Ended unexpectedly` status pill. The unclean pill is distinguished by an icon (`Icon
   name="info"`) + label + a stronger border — deliberately NOT colour/hue alone (CPE-1600 precedent) —
   and its Duration cell is prefixed `~` (a best-effort-estimate marker). When the list has any unclean
   row, a caveat note above the aggregate totals says so, rather than excluding those rows from the
   totals (the ticket allowed either). All new tokens are semantic (`--border`, `--border-strong`,
   `--surface-alt`, `--text`) with no new hard-coded hex — verified against the `src/app.css.test.ts`
   hex-literal ratchet guard, which caught (and I fixed) an initial `var(--warn, #b5872b)`-style
   fallback that would have grown it.
2. **Fixed the duration.** Added `lastActivityAt` to `SessionAccumulator` (stamped by `foldSessionStarted`
   and by every folded `fs-diff` in `foldDiffsForMetrics`/`ingestDiffsForMetrics`, which now take an
   optional `now`). `flushAllSessionsForcibly` now derives a forced row's effective end from
   `acc.lastActivityAt ?? acc.startedAt` — never from flush time — falling back to a zero-duration lower
   bound (`startedAt`) when no activity was ever observed. This can only under-, never over-, count the
   real duration. Throughput ratios (`tokensPerMinute`) already omit themselves division-safely when the
   resulting wall-clock is 0, so no separate change was needed there.

**Not changed:** `SessionMetricsRecord`'s wire shape / `metrics_journal.rs` — the fix is purely in how the
frontend accumulator computes the effective end before building the record, so no `specta::Type` struct
changed and `bindings.gen.ts` did not need regenerating.

**Tests added** (in `agentSessionMetrics.test.ts`, `agentMetricsRollup.test.ts`,
`AgentTimeline.test.ts`) — expectations derived from the spec (ticket + `AGENT-WATCH.md`), not from the
implementation:
- `lastActivityAt` stamped by `started` and by each diff fold, advancing forward.
- **Regression test** (`flushAllSessionsForcibly`): started at t=1000, last real diff at t=3000, Agent
  Deck closed 2 hours later — asserts `rec.endedAt === 3000` and `rec.wallClockMs === 2000`. Manually
  reverted the `flushAllSessionsForcibly` fix and re-ran: this test (and the "falls back to startedAt"
  one) failed loudly (`expected 7203000 to be 3000`, `expected 999999 to be 1000`), confirming they
  actually catch the bug; restored the fix and reran — 126/126 pass in that file's suite.
  - Fallback test: a session with cost-only activity (no diff ever folded) flushes with
    `endedAt === startedAt` and `wallClockMs === 0` — an honest lower bound, not a flush-time guess.
- `isSessionEndedCleanly`/`sessionRows`/`uncleanSessionCount`: sorting, counting, and the "absent field
  reads as clean" default (never misreads a pre-CPE-1626 row as crashed).
- Component tests: a crashed and a clean session render two distinct status pills with distinct labels
  (`AgentTimeline.test.ts`); the unclean-aggregate caveat note appears only when a crashed row is present
  and never for an all-clean or pre-CPE-1626 (field-absent) history; a crashed row's duration cell reads
  `~3s` vs a clean row's plain `3s`.

**Docs (CPE-579):** updated `src/docs/explorer-agent-watch.md`'s History section (describes the new
Sessions list + pill + `~` marker + caveat note) and its "Closing the whole Agent Deck…" limits note
(removed the now-stale "marker is recorded but not yet shown anywhere" callout). Also updated
`AGENT-WATCH.md`'s CPE-1626 retrospective section, which described the old flush-time-stamped `endedAt`
behaviour this ticket replaces. No new `Section` was added, so `sectionDocs.ts` needed no change.

**Verification:**
- `npm run check` — 0 errors, 0 warnings.
- `npx vitest run` — 287 files / 3646 tests pass (full suite, including the `app.css.test.ts` hex-literal
  ratchet, `sectionDocs.test.ts`, and the new/updated tests above).
- No Rust files touched — `cargo test`/`cargo clippy` not applicable to this change.
