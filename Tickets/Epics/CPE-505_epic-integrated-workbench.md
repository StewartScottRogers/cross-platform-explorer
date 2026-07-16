---
id: CPE-505
title: "EPIC: Integrated workbench — in-pane diff/editor + embedded browser"
type: Task
status: Proposed
priority: Medium
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-16
---

## Summary
Part of the **Agent Workspace** program (sibling to the AI Console [[CPE-261]]; from spike [[CPE-500]]).
Close the review loop in one window — an **in-pane diff/editor** and an **embedded browser** beside the
agent grid (the explorer is already the file tree). A brief only until activated.

## Goal
Read an agent's diff, edit alongside it, and open localhost to see the result — without leaving the
window (BridgeSpace's integrated editor + browser).

## Rough scope (NOT decomposed)
- A **diff view** of agent changes (git-backed).
- An **editor** beside the grid (reuse/extend the preview-pane text editor CPE-066).
- An **embedded browser** pane (open localhost / a URL) under the app's strict CSP.

## Open questions (resolve at activation)
- Embedded browser under the strict CSP — a dedicated webview window vs. an iframe; security review.
- Reuse the preview pane's editor, or a richer code editor?
- Diff source + granularity (working tree vs a session's changes).

## Notes
From [[CPE-500]]; the explorer file tree + preview/edit are the starting point.
