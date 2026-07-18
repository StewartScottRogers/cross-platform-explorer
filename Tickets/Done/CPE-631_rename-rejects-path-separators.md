---
id: CPE-631
title: "rename_entry should reject path separators / traversal in the new name"
type: Bug
component: Backend
priority: low
status: Done
tags: ready
estimate: 15m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`rename_entry` did `parent.join(new_name)` without checking that `new_name` is a plain filename, so a
name containing `/`, `\`, or `..` would relocate the file out of its folder rather than rename it. The
rename UI validates, but the command is directly invokable, so it should guard too (defense in depth).

## Acceptance Criteria
- [x] `rename_entry` rejects a `new_name` containing `/`, `\`, or equal to `.`/`..`.
- [x] Normal renames still work; a test covers the rejected cases and confirms nothing escapes the folder.
- [x] cargo test + clippy clean.

## Resolution
Added a guard after the empty-name check. Unit test `rename_refuses_a_path_separator_or_traversal`.

## Work Log
2026-07-18 (dayshift) — Found auditing rename/move backends after the archive zip-slip fixes; a rename
is name-only and shouldn't be a move.
