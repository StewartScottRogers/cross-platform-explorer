---
id: CPE-934
title: More swarm demos — complex multi-agent examples + verbose console narration
type: feature
component: Sidecar
priority: medium
tags: ready
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
Expanded the swarm-demo set so users see the potential for building their own swarms:
- **More examples** — 7 demos, grouped in the dropdown as **Simple** and **Complex · multi-agent**.
- **Complex demos** — three coordinated builders on disjoint files: *Folder inventory* (files → sizes →
  linking index), *Codebase tour* (architecture/glossary/onboarding), *Test plan + checklist*.
- **Verbose** — every demo task tells its agent to **narrate to the console** it runs in (print a line per
  step), so you can watch the swarm work in the terminal.
- All demos remain safe (create files only). Updated 09-swarms.md.

## Acceptance Criteria
- [x] ≥7 demos, grouped simple/complex; complex ones staff 3 agents on disjoint files.
- [x] Every demo instructs verbose console narration. Browser-verified; harness + full suite (927) green.

## Work Log
- 2026-07-23 — Added NARRATE suffix + complex demos + optgroups; verified a complex demo loads in a browser.
