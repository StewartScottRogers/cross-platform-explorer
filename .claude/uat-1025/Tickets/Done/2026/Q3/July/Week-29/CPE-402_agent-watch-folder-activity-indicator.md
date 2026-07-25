---
id: CPE-402
title: Agent Watch — folder rows show when the agent is changing files inside them
type: feature
priority: medium
estimate: S
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [agent-watch, ui]
epic: AGENT-WATCH.md
depends-on: CPE-399
---

## Problem / value
File-row annotations (CPE-399) only light up files in the folder on screen. When the agent works
in a subfolder (e.g. you're at the repo root while it edits `src/lib/*`), nothing tells you WHERE
the activity is. A folder row that indicates "the agent is changing files in here" lets you follow
it down the tree — the file list becomes a live heat map (AGENT-WATCH.md: visibility, sooner).

## Scope
- A directory row gets an "activity inside" indicator when any recent watched activity path is a
  descendant of that folder. Distinct from the per-file kind badges; fades with the activity TTL.
- Works in details + icons views; empty activity ⇒ no indicators (off means off).

## Acceptance
- [x] A folder containing recent agent activity shows an indicator; unrelated folders don't
- [x] Pure descendant-check helper, unit-tested (direct + nested child, sibling-prefix, self)
- [x] Component test: dir row flagged when a descendant is active
