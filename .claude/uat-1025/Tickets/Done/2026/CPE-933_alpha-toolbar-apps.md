---
id: CPE-933
title: Keep the sidecar app buttons alphabetical within their toolbar section
type: feature
component: Frontend
priority: low
tags: ready
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
The out-of-process app buttons in their toolbar section (CPE-857) were in hard-coded order (Agent Board,
Repositories, Agent Deck). Make them **always alphabetical** within the section. A reactive `appOrder`
sorts the apps by their (localised) labels and drives each button's CSS `order`, so the section stays
sorted regardless of markup order — result: **Agent Board, Agent Deck, Repositories**.

Uses CSS `order` (the group is `inline-flex`) so no button's markup/behaviour changes — the Agent Deck
button keeps its running-agents badge, dynamic title, and close-all context menu.

## Acceptance Criteria
- [x] App buttons render alphabetically by label within the section.
- [x] Adding an app (label + `order`) keeps it sorted. `npm run check` + full suite (926) green.

## Work Log
- 2026-07-23 — Computed `appOrder` from labels; applied `style="order:…"` to each app button.
