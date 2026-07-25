---
id: CPE-929
title: Agent Deck area "?" (swarm/grid) should open the regular Documents dialog
type: bug
component: Frontend
priority: medium
tags: ready
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
The Agent Deck's area **"?" buttons** (next to *Run swarm* and *Grid*) are titled "Documents: …" but were
opening the launcher's **own inline help panel**, not the app's regular Documents dialog. Fixed: those
buttons now ask the host to open the **regular Documents dialog at the correct section**:
- swarm "?" → **Swarms** (`09-swarms`)
- grid "?" → **Agent Grid** (`05-agent-grid`)

The launcher emits a Tauri `open-docs` event (`{ slug }`); the main app listens, opens `DocsView` at that
slug, and focuses the main window. Falls back to the inline panel if Tauri events aren't reachable, so help
is never dead. The top-bar "?" stays an inline quick-guide by design.

## Acceptance Criteria
- [x] Swarm "?" opens the regular Documents dialog at the Swarms section (not the inline panel).
- [x] Grid "?" opens it at the Agent Grid section.
- [x] Fallback to inline panel when Tauri is unavailable. Launcher harness + full suite green (924 tests).

## Work Log
- 2026-07-23 — Launcher openDocsSection() emits open-docs; App listens → openDocsSlug + window focus.
