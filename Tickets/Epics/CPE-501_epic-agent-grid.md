---
id: CPE-501
title: "EPIC: Agent Grid — tiled split-pane grid of agent terminals"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-16
---

## Summary
Part of the **Agent Workspace** program (a sibling/evolution of the AI Console [[CPE-261]], proposed by
spike [[CPE-500]] after researching BridgeSpace). Evolve the AI Console's single-pane, tabbed sessions
into a **tiled, split-pane grid** so several agent terminals are visible and interactable at once. A
brief only — not decomposed until activated.

## Goal
See N running agents side by side (BridgeSpace shows up to 16), split any direction, without losing the
session model (daemon reattach CPE-309, session chips CPE-490) the AI Console already has.

## Rough scope (NOT decomposed)
- Split-pane layout engine (horizontal/vertical splits, resize, focus).
- Grid ↔ tabs toggle; per-pane the existing terminal + status.
- Carry the session-identity chip (CPE-490) + reattach (CPE-309) into grid slots.
- Keyboard navigation between panes; layout persistence.

## Open questions (resolve at activation)
- Max panes / performance ceiling? Virtualise off-screen terminals?
- Grid-only, or grid + tabs coexisting? Per-workspace layout persistence?
- Narrow-window / responsive behaviour.

## Notes
Successor/sibling to [[CPE-261]]; from [[CPE-500]]. Highest-value + closest-to-existing of the five —
recommended to build first.
