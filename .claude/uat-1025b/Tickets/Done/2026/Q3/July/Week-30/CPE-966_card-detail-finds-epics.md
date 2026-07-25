---
id: CPE-966
title: Card detail / directives fail for epics + archived tickets (find_ticket_file misses Epics/, Done/**)
type: bug
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-23
closed: 2026-07-23
epic: CPE-503
---

## Summary
Clicking an **epic** card opens the detail popup with "Couldn't find CPE-NNN under Tickets/" (reported for
CPE-616). Root cause: `find_ticket_file` (backing `board_card_detail`/`board_directive`/`board_note`/
`board_review`) searches only the 5 top-level workflow `COLUMNS` (Backlog/Doing/Blocked/Deferred/Done) —
**not** `Tickets/Epics/`, `Tickets/Sprints/`, or the dated `Done/**` archive subfolders. So epics and
archived tickets return "not found"; only open tickets in a column resolve — exactly the "for some" symptom.

## Acceptance Criteria
- [x] `find_ticket_file` searches **all** of `Tickets/` recursively (reuse `find_ticket_file_recursive`),
      so epics (`Epics/`), sprints, and archived Done tickets (`Done/**`) resolve.
- [x] A unit test: a temp `Tickets/Epics/CPE-616_*.md` + a `Done/2026/../CPE-1_*.md` are both found.
- [x] `cargo test` + clippy clean; card detail + Send-directive work on epics (GUI-verify).