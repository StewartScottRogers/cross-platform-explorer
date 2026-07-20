---
id: CPE-794
title: Folder watcher + action executor (watched-folder rules)
type: feature
status: Open
priority: medium
component: Multiple
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-734
estimate: 4h+
---

## Summary
Backend for epic CPE-734: a `notify`-based watcher on user-chosen folders that, when a file lands, runs the
CPE-793 plan through the existing move/copy/tag/rename primitives, logging each action. Opt-in; reversible via undo.

## Acceptance Criteria
- [ ] A watched folder fires on new/changed files; the plan executes via existing primitives; actions logged.
- [ ] Opt-in (nothing watches unless configured); loop/oscillation guarded; actions reversible where possible.
- [ ] cargo/CI green.

## Notes
Prereq: CPE-793. Runs while the app is open (v1). Reuse the FS watcher (Agent Watch) + move/tag commands.
