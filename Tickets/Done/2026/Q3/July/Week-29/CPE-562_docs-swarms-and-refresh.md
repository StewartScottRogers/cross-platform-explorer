---
id: CPE-562
title: "Documents library — add a Swarms page + refresh AI Console / Agent Board / Overview"
type: Task
status: Done
priority: Medium
component: Docs
tags: [ready]
epic: CPE-534
estimate: 45m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
User request (2026-07-17): keep the **Swarms** feature documented in the in-app Documents library, and
update all the Documents. The library (`src/docs/*.md`, CPE-536) had no Swarms page, and several pages had
gone stale against recent work (Agent Board root/filter/view-prefs; the AI Console "Run swarm" button).

## Acceptance Criteria
- [x] New `src/docs/09-swarms.md` documents the Swarms feature (what it is, how to run one, how agents
      coordinate over the shared mailbox/memory, guardrails), flagged as a preview.
- [x] `04-ai-console.md` gains a "Run swarm" section pointing to the Swarms page.
- [x] `06-agent-board.md` refreshed for CPE-551/554/555/556/560: choosing/auto-detecting the project root,
      the card filter (+ Esc to clear / no-match hint), and remembered view preferences.
- [x] `01-overview.md` lists Swarms in the Agent Workspace.
- [x] Docs still index/build (`docs.test.ts`); `npm run check` clean.

## Resolution
Added `09-swarms.md` (order 9 — auto-indexed by the `import.meta.glob` loader, no registration needed) and
updated `04-ai-console.md`, `06-agent-board.md`, `01-overview.md` to match the shipped behaviour. Kept the
Swarms page honest that it's a preview (coordination pieces tested; end-to-end is experimental).
`docs.test.ts` 5 passed (DOCS now 9, still Overview-first + non-decreasing order); `npm run check` 0/0.

## Notes
Fulfils the standing "maintain the in-app Documents library" practice ([[CPE-534]]). Revisit the Swarms
page's "preview" caveat once the live swarm passes real-agent GUI QA ([[CPE-541]]).
