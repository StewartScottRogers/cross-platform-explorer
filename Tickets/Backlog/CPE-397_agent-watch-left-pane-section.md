---
id: CPE-397
title: Agent Watch — left-pane "Agents" section + navigate-to-project
type: feature
priority: high
estimate: M
status: Open
created: 2026-07-14
tags: [big-design, agent-watch, ui]
epic: AGENT-WATCH.md
depends-on: CPE-396
---

## Goal
Surface active agent sessions (CPE-396) as a section in the main window's left pane
(sibling of Favorites in Sidebar.svelte). Clicking a session navigates the explorer to its
Project Folder — the two-way tie between the console's Project Folder and the explorer.

## Scope
- New collapsible "Agents" / "Active sessions" section in Sidebar.svelte, populated from
  the session store. Each node: agent name + Project Folder, status dot.
- Click → navigate the explorer to that folder (reuse existing navigation) and mark it the
  watched session.
- Empty when nothing is running (section hidden), so the plain sidebar is unchanged.

## Acceptance
- [ ] Running agent appears in the left pane
- [ ] Clicking navigates the explorer to its Project Folder
- [ ] Section absent when no sessions; no layout/startup change when off
