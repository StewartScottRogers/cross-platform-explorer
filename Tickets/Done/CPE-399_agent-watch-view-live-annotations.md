---
id: CPE-399
title: Agent Watch — live activity annotations on the main file list
type: feature
priority: high
estimate: L
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [big-design, agent-watch, ui]
epic: AGENT-WATCH.md
depends-on: CPE-398
---

## Goal
The payoff: with a session selected, the main file list for its Project Folder becomes the
Agent Watch view — rows annotate live as the agent works, so the user can follow and
intervene in real time (AGENT-WATCH.md tiebreaker: make activity more visible, sooner).

## Scope
- Consume CPE-398 events: badge/highlight rows on create / modify / move / delete, with a
  brief recency fade; optional "most-recent-activity" ordering.
- A compact activity strip/log of the latest actions.
- Intervene affordance is out of scope (observe-only, per the doc).

## Off means off
All of this activates only for a watched session; the normal file list is untouched otherwise.

## Acceptance
- [x] File rows reflect live agent mutations with clear, decaying annotations
- [x] Activity strip lists recent actions
- [x] Zero visual/behavioural change to the list when not watching
