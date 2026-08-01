---
id: CPE-1241
title: "QA: gui-smoke pin + Visual Critic screenshot for the ShredConfirmDialog (CPE-1240)"
type: Task
priority: Medium
component: gui-smoke
tags: [ready]
created: 2026-08-01
epic: CPE-738
closed:
---

## Context
CPE-1240 (#539) shipped the secure-delete confirm dialog (ShredConfirmDialog) — a PERMANENT, no-undo
destructive surface. Reviewer + UAT verified its honesty copy + safeguard + conventions at code level;
it needs a gui-smoke screenshot for the Visual Critic (the dialog's rendered clarity/tone matters for a
destructive op).

## Repro to drive
Right-click a file (not folder/archive/Home) → "Securely delete…" → the ShredConfirmDialog opens with
the permanence warning + platform caveat + scheme picker + "Shred permanently" danger button. Do NOT
click Shred (would permanently destroy the seeded fixture) — snap the dialog, then Cancel/Escape.

## Acceptance criteria
- New `gui-smoke/specs/shred-dialog.smoke.ts` (mirror near-duplicates/saved-search specs) drives the real
  built app to OPEN the ShredConfirmDialog on a seeded throwaway file, asserts the permanence + caveat
  copy + danger button render, `snap("shred-dialog")`s it, then dismisses via Cancel (NEVER confirms —
  no real shred).
- Spec passes green + captures `shred-dialog.png`. Visual Critic judges: clearly reads as dangerous/
  permanent, honest tone, visible border, danger button distinct, nothing clipped, on-theme.

## Notes
QA-Architect burndown for the new destructive surface; mirrors CPE-1221/1233. CRITICAL: the spec must
never actually confirm the shred.
