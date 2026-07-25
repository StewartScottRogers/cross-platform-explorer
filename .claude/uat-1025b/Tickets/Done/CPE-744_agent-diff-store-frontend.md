---
id: CPE-744
title: Agent Watch — frontend per-path diff store (before/after ingestion)
type: feature
component: Frontend
priority: high
status: Open
tags: needs-prereq
created: 2026-07-19
epic: CPE-727
estimate: 2-3h
---

## Summary
Child of CPE-727. Ingest the host before/after records (CPE-743) into a bounded, per-path/per-event **diff
store** alongside `agentActivity.ts`, so touched rows and timeline entries can look up "what changed" for a
file. Reuses `diff.ts` (`inlineDiff(oldText, newText)` for hunks; no unified-diff text needed).

## Scope
- Listen on the CPE-743 channel; fold `{path, before, after}` into a store keyed by path (keep the latest;
  optionally a short per-path history for the timeline).
- Bounded retention (count + total chars) with eviction, mirroring the activity map's discipline.
- A pure selector returning the diff hunks for a path (computed via `diff.ts`), headless-testable.
- Clear on stop-watching (reuse the `clearActivity` lifecycle); empty/idle when not watching.

## Acceptance
- [ ] Before/after records fold into a per-path store; a selector yields hunks via `diff.ts`.
- [ ] Store is bounded/evicts and clears on stop-watching; zero cost when not watching.
- [ ] Headless tests cover ingest, latest-wins, eviction, clear, and hunk derivation.

## Notes
Prereq: CPE-743 (emits the before/after records). Frontend-only.
