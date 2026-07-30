---
id: CPE-1136
title: "Docs: document the Agent Watch drawer (live activity + replay scrubber + cost/radar/history) in the Explorer page"
type: chore
component: Docs
priority: medium
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-728
---

## Summary
The **Agent Watch** feature set is shipped (epic CPE-728 Done, plus CPE-731/1097/1098/1099/1100/1114) but has
**almost no in-app documentation**. A grep of `src/docs/*.md` finds only one passing mention of agent
"sessions" (`03-explorer.md:65`); the drawer itself and its five tabs — **Live**, **Replay**, **Cost**,
**Radar**, **History** — are undocumented. This violates the standing "self-maintaining docs library" mandate
(CPE-579): a shipped user-facing capability should be documented.

Agent Watch is a **drawer within the Explorer**, not a top-level nav Section (the `Section` union in
`src/lib/sectionDocs.ts` has no Agent-Watch entry, and the section-docs guard passes). So its documentation
belongs as a **new subsection in `src/docs/03-explorer.md`** — NOT a new `Section`/registry entry (adding a
Section would be scope creep and require a nav change). Keep the `sectionDocs` guard green (no registry edits
needed).

## What to document (accurate to the actual UI/components — READ them, don't guess)
Study these to describe the feature truthfully:
- `src/lib/components/AgentTimeline.svelte` — the drawer + its tab strip (`tab: "live" | "replay" | "cost" |
  "radar" | "history"`). Tab labels render as Live / Replay / Cost / Radar / History (≈ lines 422-450).
- `src/App.svelte` + `src/lib/agentSessions.ts` (`watchTargetFor`) + `src/lib/components/ExplorerPane.svelte`
  (`.agent-log-btn`) — HOW the drawer opens: it appears for a **watched agent session whose folder matches the
  current folder**, opened via the agent-watch button on the folder view.
- `src/lib/replayFold.ts` / `src/lib/agentReplay.ts` — the **Replay scrubber**: scrub a slider across the
  session's recorded activity and the folder listing reconstructs at that moment (files appearing / changing /
  disappearing), plus play/transport controls. This is the feature just human-verified (burndown row CPE-1094).
- Cost tab (CPE-1098/1114 — live token+USD + a History rollup), Radar tab (CPE-1100 — multi-actor conflict
  overlap). Describe what each shows at a user level.
- Note the **off-means-off** guarantee: with no watched session, Agent Watch is absent and costs nothing (this
  is a core design promise — see `AGENT-WATCH.md` / `PURPOSE.md` precedence).

## Design
- Add a well-structured `## Agent Watch` subsection (with sub-headings per tab) to `src/docs/03-explorer.md`,
  matching the existing docs' voice/format (see the other `src/docs/*.md` pages + `16-checkpoints.md` as a tone
  reference).
- Do **not** add a new `Section` or touch `src/lib/sectionDocs.ts` — Agent Watch is documented under the
  existing `explorer` section. The section-docs guard (`src/lib/sectionDocs.test.ts`) must stay green.
- Accuracy over length: describe what a user actually sees and does. No invented buttons/behaviours.

## Acceptance Criteria
- [ ] `03-explorer.md` has an `Agent Watch` subsection covering: how the drawer opens (watched session matching
      the current folder), and each of the five tabs (Live, Replay, Cost, Radar, History) at a user level, with
      the Replay scrubber (scrub → folder reconstructs; play/transport) described concretely.
- [ ] The off-means-off promise is stated (no session → no Agent Watch, zero cost).
- [ ] Every described behaviour is verifiable against the actual components (no invented UI).
- [ ] `npm run check` passes and the `src/lib/sectionDocs.test.ts` guard still passes (no registry change
      needed; if the test runner is vitest, run the docs guard test).
- [ ] No new `Section`; no changes outside `src/docs/03-explorer.md`.

## Notes
- Serves the docs mandate `[[maintain-in-app-docs-library]]` (CPE-579/595).
- Directly documents the replay scrubber human-verified this session (its render is pinned by CPE-1135; this
  gives users the written explanation).
