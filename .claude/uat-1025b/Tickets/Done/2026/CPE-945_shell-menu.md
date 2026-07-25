---
id: CPE-945
title: Shell context-menu applicability model
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-712
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
First headless slice of the shell-citizen epic (CPE-712). `cpe_server::shell_menu`:
- `MenuVerb { id, label, command, applies }` + `AppliesTo { Files, Folders, Any, Extensions(list) }` +
  `SelItem { path, is_dir }`.
- `verbs_for(verbs, selection) -> Vec<&MenuVerb>` — return the registered verbs to show for the current
  selection, in registration order; a verb shows only when it applies to **every** selected item (uniform
  files/folders, or every file matching an extension list, case-insensitive). No OS registration here.

## Acceptance Criteria
- [x] Any/Files/Folders/Extensions applicability over a whole selection; empty selection → no verbs.
- [x] Mixed selections hide file/folder-only verbs; ext match is case-insensitive. 4 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Activated CPE-712 with the verb-applicability core. The per-OS shell registration
  (Windows registry / macOS Services / Linux .desktop) + the default file-manager handshake are remaining.
