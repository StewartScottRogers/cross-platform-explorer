---
id: CPE-715
title: "EPIC: Link forge — symlink, junction & hardlink management"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
First-class creation, inspection, and safe-editing of symbolic links, Windows junctions, and hardlinks,
with clear visual badges for linked entries and loop-safe traversal.

## Why
Links are a real part of a filesystem a serious explorer must handle. Today they're at best detected;
creating, resolving, and repairing them is a concrete power-user gap.

## Rough scope (areas, not child tickets)
- Per-OS link creation/resolution commands, building on existing `entry_is_symlink` detection.
- A "New Link..." flow (symlink / junction / hardlink as appropriate to the OS).
- Visual badges for linked entries + a target/"resolves to" indicator.
- Broken-link detection and target repair; reparse-point awareness on Windows; loop-safe traversal.

## Open questions (resolve at activation)
- Privilege needs (Windows symlink creation) and elevation UX.
- How aggressively to resolve/badge links in large listings (perf).
- Behaviour when navigating into a link vs. showing its target.

## Definition of Done
- Users can create symlinks/junctions/hardlinks and see linked entries badged with their target.
- Broken links are flagged and can be repaired; traversal never loops.
- No measurable listing cost when a folder has no links.
