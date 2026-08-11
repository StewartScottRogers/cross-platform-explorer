---
id: CPE-1626
title: "Tearing down a watcher flushes the session's metrics as if it ended — so a premature teardown silently drops the rest of that session's history"
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
Found by the independent Reviewer of CPE-1606 (PR #815) while verifying that PR's justification for
retaining watchers after you navigate away. The justification held, but the underlying mechanism is worse
than the PR's own comment claims — and it is the thing standing between us and a true "off means off".

## The coupling
`reconcileAgentWatch`'s "stop the removed" loop (`src/App.svelte`) runs:

    await flushSession(id); await stopAgentWatch(id); armedWatches.delete(id);

Any session dropped from `desired` therefore has its metrics flushed **as if the session had ended**
(CPE-1113).

The PR's comment says a premature flush would produce two *fragmented* rows. It would not.
`flushSession` (`src/lib/agentSessionMetrics.ts`) is guarded by `flushedSessionIds` (marked at L393,
*before* the await at L398) and is a hard no-op once an id has been flushed — it is only un-marked when a
genuine `started` announcement for that id arrives (L175). So the real failure mode is:

> one **premature, incomplete** row is persisted, and **all activity for the rest of that still-running
> session is silently and permanently dropped** from the journal, because the true end-of-session flush
> becomes a no-op.

Silent data loss in the activity record, rather than a visibly split row.

## Why it matters
This coupling is the only reason CPE-1606 had to retain watchers for the lifetime of a visited session
instead of disarming when you navigate away. Decouple it and the mode can honour `AGENT-WATCH.md`'s
boundary literally — leave the folder, the watcher stops — at no cost to the metrics record.

## Fix
Introduce an explicit **pause vs end** distinction in the metrics model: teardown-for-navigation pauses
(no flush, resumable), and only a genuine session end flushes. Then revisit CPE-1606's retention and
disarm on navigate-away if the numbers stay intact.

Also **correct the inaccurate "fragmented second row" characterisation** in the doc comment and in
`AGENT-WATCH.md`, which currently describes a failure mode that does not happen.

## Acceptance criteria
- A paused-then-resumed session produces ONE complete history/cost row covering its whole life; a test
  covers it and fails against the current code.
- With the decoupling in place, watchers disarm on navigate-away and `AGENT-WATCH.md`'s boundary is
  literally true again — or the ticket explains, with measurements, why retention is still preferred.
- The corrected failure-mode description replaces the current inaccurate one.

**Conflict surface:** `src/lib/agentSessionMetrics.ts`, `src/App.svelte`, `src/lib/agentSessions.ts`,
`AGENT-WATCH.md`, `src/docs/explorer-agent-watch.md`. Overlaps CPE-1625 — sequence them.
