---
id: CPE-404
title: AI Console toolbar button shows a running-agent count
type: feature
priority: low
estimate: XS
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [ui, agent-watch]
---

## Problem / value
Once the AI Console window is closed, nothing in the main window signals that agents are still
running. A small live count on the AI Console toolbar button keeps that visible at a glance.

## Scope
- A pill badge on the "AI Console" button showing the number of active sessions (from the
  already-live agentSessions store, CPE-396); tooltip pluralized; absent when none run.

## Acceptance
- [x] Badge shows the running-agent count; hidden when zero
- [x] Driven by the reactive session store (tested in agentSessions.test.ts); svelte-check clean
