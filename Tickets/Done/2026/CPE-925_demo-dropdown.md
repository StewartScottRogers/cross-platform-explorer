---
id: CPE-925
title: Agent Deck — a dropdown of multiple demo swarms (3–4 types)
type: feature
component: Sidecar
priority: medium
tags: ready
created: 2026-07-23
status: Done
---

## Summary
Follow-up to CPE-924. Replace the single "Try a demo" button with a **dropdown of ready-made demo
swarms** (4 types), so users can pick a demo that interests them and load it in one click. Each option
pre-fills the swarm task field with safe, file-creating-only tasks and a how-to note; Start runs the real
swarm path.

Demos: **Hello swarm** (greeting + notes), **Explain this folder** (folder report), **Docs starter**
(README/CONTRIBUTING skeletons), **Cleanup plan (safe)** (writes a proposal, deletes nothing).

## Acceptance Criteria
- [x] A select with ≥4 demo types next to Run swarm; a "Load demo" action fills the tasks for the chosen one.
- [x] Each demo is safe (creates files only) and shows its own how-to note.
- [x] Covered by the launcher jsdom harness.

## Work Log
- 2026-07-23 — Filed + started.

- 2026-07-23 — Replaced the single demo button with a 4-option dropdown (Hello swarm / Explain this folder / Docs starter / Cleanup plan-safe) + Load demo; each pre-fills safe file-creating tasks + its own how-to note. Updated 09-swarms.md. Launcher jsdom harness: 64 tests pass (2 for the dropdown).
