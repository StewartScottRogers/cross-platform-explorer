---
id: CPE-852
title: Agent Board sidecar — Kanban over Tickets/ (read + move cards)
type: feature
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-21
epic: CPE-850
estimate: 4h+
---

## Summary
Second child of CPE-850. Make the sidecar's served UI a real, interactive Agent Board: read the project's
`Tickets/` folders (via the granted `context` root from the host) into columns/cards, render the Kanban,
and move a card between columns (moves the ticket file + updates its `status:` frontmatter). The
card/column/frontmatter logic is small — reimplement in the sidecar or lift into a tiny contract-free
shared crate (the sidecar must not depend on `cpe-server`/the app).

## Acceptance Criteria
- [ ] The sidecar reads `Tickets/{Backlog,Doing,Blocked,Deferred,Done,…}` under the context root into
      typed cards; the served UI renders the columns.
- [ ] Dragging/moving a card to another column moves the file and rewrites `status:` (same effect as the
      in-app board / `ticket_board`).
- [ ] No dependency on `cpe-server` or the app (ADR 0001 / CI guard); pure logic cargo-tested.

## Notes
Prereq: **CPE-851**.

## Work Log
