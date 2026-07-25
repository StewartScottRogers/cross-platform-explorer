---
id: CPE-1023
title: In-app "Shell integration" settings toggle
type: feature
component: Frontend
priority: medium
tags: needs-prereq
epic: CPE-712
created: 2026-07-24
status: Backlog
---

## Summary
CPE-712 slice: the user-facing control. A toggle in **Settings** (per the avoid-modal-permission-popups
rule — no launch-time consent modal) that adds/removes the "Open in CPE" context-menu integration by
calling `install_shell_integration` / `uninstall_shell_integration` (CPE-1020). Reflects current state
(query whether the entries exist), shows a plain-language description, and reports success/failure inline.
Uninstall path is the reversibility guarantee made reachable from the UI.

Prereq: **CPE-1020** (the apply/remove commands). Cross-platform-safe: on OSes whose apply glue isn't landed
yet, the toggle is shown disabled with a "coming to <OS>" note rather than hidden.

## Acceptance Criteria
- [ ] A Settings toggle installs/removes the shell integration via the backend commands; state is read back
      and reflected on open.
- [ ] Failure surfaces inline (no crash, no modal-permission popup); copy is plain-language.
- [ ] Docs updated per CPE-579 (settings section → doc slug) if a new settings section is added.

## Work Log
- 2026-07-24 (PM take-on) — Filed as the UI cap on the Windows path (CPE-1019 → 1020 → 1023). Placed in
  Settings, not a launch modal, per [[avoid-modal-permission-popups]].
