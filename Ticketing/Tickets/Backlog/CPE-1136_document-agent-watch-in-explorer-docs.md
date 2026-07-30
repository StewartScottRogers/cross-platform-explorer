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
- [x] `03-explorer.md` has an `Agent Watch` subsection covering: how the drawer opens (watched session matching
      the current folder), and each of the five tabs (Live, Replay, Cost, Radar, History) at a user level, with
      the Replay scrubber (scrub → folder reconstructs; play/transport) described concretely.
- [x] The off-means-off promise is stated (no session → no Agent Watch, zero cost).
- [x] Every described behaviour is verifiable against the actual components (no invented UI).
- [x] `npm run check` passes and the `src/lib/sectionDocs.test.ts` guard still passes (no registry change
      needed; if the test runner is vitest, run the docs guard test).
- [x] No new `Section`; no changes outside `src/docs/03-explorer.md`.

## Notes
- Serves the docs mandate `[[maintain-in-app-docs-library]]` (CPE-579/595).
- Directly documents the replay scrubber human-verified this session (its render is pinned by CPE-1135; this
  gives users the written explanation).

## Work Log

- Added a `## Agent Watch` subsection to `src/docs/03-explorer.md` (end of the page, after "Shell
  integration"), with sub-headings for each drawer tab: Live, Replay, Cost, Radar, History. Also states
  how the drawer opens (agent strip + Log button, appearing only when the current folder matches a
  running agent session's project folder) and the off-means-off promise (no watched session → no strip,
  no drawer, no watcher, no cost).
- Verified every described behaviour against the real components before writing:
  - `src/lib/components/AgentTimeline.svelte` — the drawer's tab strip (`tab: "live" | "replay" | "cost" |
    "radar" | "history"`, lines ~414-458) and each tab's actual markup/logic: Live's `ConsultedFiles` +
    diff-peek rows (lines ~460-499), Replay's transport/speed/slider + reconstructed listing + "Show in
    file pane" toggle (lines ~500-640), Cost's per-session card fields (lines ~641-683), Radar's overlap
    list + "Competing renames" section (lines ~684-742), History's totals/ratios/by-model/by-agent/bar
    chart (lines ~743-868).
  - `src/App.svelte` (lines ~774-908, ~3647-3655) and `src/lib/agentSessions.ts`'s `watchTargetFor` — how
    `activeWatchCwd` (deepest running-agent project folder containing the current path) drives the strip
    and gates the drawer; `showTimeline` reset to `false` whenever `activeWatchCwd` clears.
  - `src/lib/components/ExplorerPane.svelte` (lines ~306-324, ~437-450) — the `.agent-log-btn` ("Log"
    button, i18n key `agent.log`) that toggles `showTimeline`, and the strip's dot/name/change-chip markup
    (i18n key `agent.watch` → "Agent Watch — {name}").
  - `src/lib/i18n.ts` — confirmed the strip/button copy ("Agent Watch — {name}", "Log", "watching for
    changes…") used verbatim in the doc.
  - `AGENT-WATCH.md` — cross-checked the off-means-off boundary and per-tab framing (advisory cost
    figures, "activity overlap" not "conflict" wording, pull-only History load) against the shipped
    feature list.
  - `src/docs/16-checkpoints.md` (and skimmed a couple other `src/docs/*.md` pages) for voice/format —
    short H2/H3 structure, bold for UI labels, concrete "click X to do Y" phrasing.
- No new `Section` added; `src/lib/sectionDocs.ts` untouched.
- Verification: `npm run check` → 0 errors, 0 warnings. `npx vitest run src/lib/sectionDocs.test.ts` → 2
  tests passed.
- Only files touched: `src/docs/03-explorer.md` (content) and this ticket file (checkboxes + Work Log).
