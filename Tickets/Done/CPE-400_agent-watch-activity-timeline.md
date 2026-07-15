---
id: CPE-400
title: Agent Watch — durable session activity timeline panel
type: feature
priority: medium
estimate: M
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [agent-watch, ui]
epic: AGENT-WATCH.md
depends-on: CPE-399
---

## Problem / value
The Agent Watch activity strip (CPE-399) is transient — entries fade after 6s and only the last
few show. There's no way to review the *full* history of what the agent did in a session. For
"follow, understand, and intervene" (AGENT-WATCH.md) a durable, scrollable log is the missing half.

## Scope
- A per-session, append-only activity timeline (bounded, newest-first): every create/modify/move/
  delete under the watched project, with kind, path, and relative time — NOT TTL-pruned like the
  row annotations.
- A toggle on the activity strip opens a docked timeline drawer listing the history; clicking an
  entry navigates the explorer to the change's containing folder.
- Clears when the watched session changes; absent entirely when not watching (off means off).

## Acceptance
- [x] Timeline records every activity event for the session, capped, newest first
- [x] Strip toggle shows/hides the drawer; entry click navigates to the file's folder
- [x] Empty/idle state; no drawer + no store growth when not watching
- [x] Headless tests for the store (append/cap/clear) + the component (render/click/empty)
