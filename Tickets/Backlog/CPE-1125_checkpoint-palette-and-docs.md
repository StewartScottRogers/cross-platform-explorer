---
id: CPE-1125
title: "Checkpoint & rollback: palette action + in-app docs"
type: feature
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-07-26
epic: CPE-732
---

## Summary
CPE-732 first wave (PM slice E). A command-palette action to create/list/revert checkpoints via the CPE-1123
commands, plus a self-maintaining in-app docs page. FRONTEND only. **Blocked on CPE-1123** (needs the commands +
regenerated bindings to invoke).

## What to build
- A palette action `tool.checkpoint` (create a checkpoint; list + preview + revert) wired to the CPE-1123
  `checkpoint_*` commands via the generated bindings (import `invoke` from `src/lib/invoke.ts`; prefer the
  generated `commands.checkpoint*`).
- **Self-maintaining docs (CPE-579):** add a `src/docs/*.md` page for the checkpoint feature AND its
  `section → doc slug` entry in `src/lib/sectionDocs.ts` (the guard test `sectionDocs.test.ts` fails CI otherwise).

## ⚠ Guardrails
- Frontend only; no backend. No new deps. Theme vars only (app light-only). Menus/palette follow MENUS.md; any
  pills reflow. Off-means-off. **Sequence after CPE-1123 merges** (needs the commands/bindings).

## Acceptance Criteria
- [ ] A palette action creates/lists/previews/reverts checkpoints via the `checkpoint_*` commands; a docs page
      exists and is registered in `sectionDocs.ts` (guard test green). `npm run check` clean; `npm test` green; no
      new deps.

## Work Log
2026-07-26 (workshift) — CPE-732 first wave (PM slice E). Blocked on CPE-1123 (command layer + bindings). The
richer VISUAL surface (restore panel + timeline markers) is the deferred GUI cap CPE-1126.
