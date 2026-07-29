---
id: CPE-869
title: Document sidecar health, Repair & self-healing in the in-app docs
type: docs
component: Frontend
priority: low
status: Done
tags: ready
epic: CPE-862
created: 2026-07-21
closed: 2026-07-21
---

## Summary
The sidecar-reliability work (CPE-863 Repair/status, CPE-867 auto-restore, CPE-868 launch retry) shipped
without an in-app docs page. Add one so users know about Settings → Platform (status pills, Repair,
capabilities) and the automatic self-healing.

## Acceptance Criteria
- [x] `src/docs/13-sidecar-health.md` documents the Platform panel, the Repair button, and self-healing
      (auto-restore + launch retry); auto-included in the docs library (glob).
- [x] `npm run check` + docs guard tests green.

## Work Log
- 2026-07-21 (autonomous) — Added the doc page (category "Agent Deck"). Keeps the docs library current per
  the self-maintaining-docs rule.
