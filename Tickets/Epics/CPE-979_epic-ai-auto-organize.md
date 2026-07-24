---
id: CPE-979
title: "EPIC: AI auto-organize & declutter"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-24
closed:
---

## Goal
Point the app at a messy folder — the classic `Downloads` graveyard — and get a **proposed tidy plan**: group
by kind/date/project into sensible subfolders, flag obvious junk (installers already run, duplicates, zero-
byte files), suggest better names — all as a **preview you approve or tweak** before anything moves, with
one-click undo. Rules-driven first (deterministic, explainable); AI-assisted classification as an opt-in
layer on top.

## Why
"Clean up my Downloads" is the chore everyone postpones. An explorer that proposes a safe, reviewable
reorganization — and can undo it — is genuinely useful and distinctly *smart*. It reuses everything already
built: `duplicates` (exact-dupe detection), `folder_stats`/`disk_usage` (what's here), `folder_template`
(target scaffolds), `restore_plan`/[[CPE-732]] (undo), and the [[CPE-977]] plan/preview/confirm loop. Rules
first keeps it honest and testable; the AI layer only *suggests*, never auto-moves.

## Rough scope (areas, not child tickets)
- A **pure organization planner**: `plan_organize(listing, ruleset) -> MovePlan` — given a folder's entries
  + a declarative ruleset (by extension/kind/date/size/age), produce the proposed subfolder moves + renames.
  Deterministic, filesystem-free, cargo-testable; the headless core.
- **Junk/clutter heuristics**: stale installers, zero-byte, exact duplicates (reuse `duplicates`), broken
  shortcuts — surfaced as suggestions, never auto-deleted.
- An **AI classification layer** (opt-in, feature-gated seam): infer a file's *project/topic* from name +
  content to group beyond extension — augmenting the rules, always as a suggestion.
- A **preview + apply + undo** flow: show the before/after tree, let the user toggle individual moves, apply
  via existing primitives inside a checkpoint (one-click revert).

## Open questions (resolve at activation — big-design)
- The default ruleset + how much is user-editable vs. built-in presets ("by type", "by date", "by project").
- Where the AI classifier runs (local vs external, feature gate) and how its suggestions are marked/overridable.
- Confidence + safety: never move across volumes silently; caps; always-preview; how to treat files open/in-use.
- Overlap/reuse with [[CPE-740]] folder templates (target scaffolds) and [[CPE-711]] selection.

## Definition of Done
- Running "organize" on a folder yields a reviewable move/rename plan (rules-based; AI-assisted opt-in) with
  no changes until the user approves.
- Applying is undoable via a checkpoint; junk/duplicate suggestions are surfaced, never auto-actioned.
- With the feature off, no classifier loads and the plain explorer is unaffected.

## Notes
- Build the **pure `plan_organize` rules engine first** (headless, cargo-tested), reusing `duplicates`/
  `folder_stats`; layer the AI classifier + preview UI after. Shares the plan/confirm/undo machinery with
  [[CPE-977]] and [[CPE-732]]. See [[go-with-recommendation]].
