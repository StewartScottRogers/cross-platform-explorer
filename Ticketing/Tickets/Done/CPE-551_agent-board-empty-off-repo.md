---
id: CPE-551
title: "Agent Board shows nothing unless the current folder has a Tickets/ subfolder"
type: Bug
status: Open
priority: High
component: Frontend
tags: [ready]
estimate: 1-2h
created: 2026-07-16
closed:
---

## Summary
User QA (2026-07-16, running v0.32.0-sidecar): "Agent Boards are not working." Root cause found in
`App.svelte`: `<BoardView root={currentPath} …>` — the board scans `<currentPath>/Tickets/…`. So the board
is only populated when the **currently-browsed folder** happens to be a project with a `Tickets/`
subfolder. Browsing anywhere else (the normal case in the installed app) makes the board look broken —
empty, with no explanation.

## Expected Behavior
Opening the Agent Board either (a) shows the tickets of a sensible project root, or (b) clearly explains
there is no `Tickets/` folder here and offers to point at one — never a silent empty panel that reads as
"broken."

## Acceptance Criteria
- [ ] When the current folder has no readable `Tickets/`, the board shows a clear empty-state message
      (not a blank/broken panel) explaining it needs a project with a `Tickets/` folder.
- [ ] The user can point the board at a project folder (native folder picker), and it remembers the last
      chosen root across opens.
- [ ] When a `Tickets/` folder IS present (current or chosen root), cards load as before.
- [ ] `npm run check` clean; a component test covers the empty-state + chosen-root path.

## Notes
Assumption (nightshift, user asleep): the board is a project tool, so pointing it at a chosen project
root + a remembered last-root is the right model, with the current folder as the default when it has
`Tickets/`. Revisit if the user wanted it always pinned to a specific repo.
