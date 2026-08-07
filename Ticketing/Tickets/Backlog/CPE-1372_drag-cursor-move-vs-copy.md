---
id: CPE-1372
title: "Drag cursor shows 'move' for a no-modifier cross-volume drag that actually copies"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
---

## Problem (drag-drop audit, BUG 3 — cosmetic)

With no modifier, dragging a file from drive C: onto drive D: shows the "move" cursor during hover
(`dnd.ts hoverEffect` returns "move" unconditionally without a modifier), but at drop `resolveEffect(mods,
sameVolume)` correctly performs a COPY (cross-volume, source preserved). Outcome is safe (no data loss); the
cursor just misrepresents it. Documented as intentional ("authoritative decision at drop"), so cosmetic.

## Fix direction

If cheap, make `hoverEffect` volume-aware (same best-effort same-volume check as drop) so the hover cursor
matches the actual op; otherwise accept as a known cosmetic limitation and document it.
