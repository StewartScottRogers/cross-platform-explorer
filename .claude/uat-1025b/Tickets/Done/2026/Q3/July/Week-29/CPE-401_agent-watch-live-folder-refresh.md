---
id: CPE-401
title: Agent Watch — live folder refresh so created/deleted files appear as they happen
type: feature
priority: high
estimate: S
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [agent-watch, ui]
epic: AGENT-WATCH.md
depends-on: CPE-398
---

## Problem / value
CPE-399 annotates rows the agent touches, but the file list is loaded once on navigation — so a
file the agent *creates* has no row to annotate and never appears, and a *deleted* file lingers,
until a manual refresh. That breaks Agent Watch's core promise ("see it happen", AGENT-WATCH.md).

## Scope
- While watching, when a filesystem activity batch adds/removes/renames a direct child of the
  folder currently on screen, debounce-refresh the listing (reusing the existing refresh(), which
  keeps selection) so new files appear (then get their "new" badge) and deleted ones vanish.
- Modified-only batches don't re-list (the row exists; the annotation suffices).
- Scoped strictly to watching — the plain explorer never auto-refreshes (predictability preserved,
  off means off).

## Acceptance
- [x] A file the agent creates in the viewed folder appears live; a deleted one disappears
- [x] Debounced/coalesced — no refresh storm under heavy churn; selection preserved
- [x] No auto-refresh when not watching
- [x] Headless test for the affects-listing decision (direct child, kind, cross-platform)
