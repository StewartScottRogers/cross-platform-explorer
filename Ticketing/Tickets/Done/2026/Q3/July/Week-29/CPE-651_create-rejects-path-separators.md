---
id: CPE-651
title: "create_dir/create_file should reject path separators (like rename)"
type: Bug
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`create_dir` and `create_file` did `Path::new(path).join(name)` without checking `name` is a plain
filename — a "New folder" named `../evil` or `sub/x` would create it OUTSIDE the current folder. Same
class as CPE-631 (rename); the UI validates but the commands are directly invokable. Extracted a shared
`valid_entry_name` guard and applied it to create_dir, create_file, and rename_entry.

## Acceptance Criteria
- [x] `create_dir`/`create_file` reject a name with `/`, `\`, or `.`/`..`.
- [x] Shared `valid_entry_name` used by all three (rename refactored to it — its test still passes).
- [x] A test covers the rejected cases + confirms nothing escapes the folder; clippy clean.

## Work Log
2026-07-18 (dayshift) — Found auditing create_dir after the CPE-631 rename fix; same gap, now guarded consistently.
