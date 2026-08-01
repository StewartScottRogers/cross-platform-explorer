---
id: CPE-1227
title: "Test gap: add a do_move_into/move_entries schedule-migration regression test (CPE-1225 follow-up)"
type: Task
priority: Low
component: cpe-server
tags: [ready]
estimate: 20m
created: 2026-08-01
closed:
---

## Context
CPE-1225 wired snapshot-schedule migration (`reschedule`) into `rename_entry_impl`, `do_move_into`
(both branches), and `move_exact_impl`. Regression tests cover `rename_entry_impl` + `move_exact_impl`,
but NOT the `do_move_into`/`move_entries_impl` (drag/drop / paste-move) route — a coverage asymmetry
the CPE-1225 reviewer flagged. The wiring there was confirmed correct by read and `reschedule` is
unit-tested, so risk is low; this just closes the gap.

## Acceptance criteria
- A regression test drives a scheduled folder through `move_entries_impl`/`do_move_into` and asserts
  its schedule catalog entry migrates to the moved path (mirroring CPE-1222's
  `move_entries_impl_migrates_tags_for_a_file_and_a_moved_directorys_subtree`).
