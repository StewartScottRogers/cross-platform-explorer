---
id: CPE-1583
title: "hrdrClaudeNative.cmd: put the containing folder in the new tab's caption"
type: Task
status: Done
priority: Low
component: Tooling
tags: [ready]
created: 2026-08-10
closed: 2026-08-10
---

## Why
User request (2026-08-10). When hrdr opens the Claude tab, the caption was just "Claude" / "Claude 2" — no
indication of which folder/checkout it's running in. Adding the containing folder makes tabs opened on
different checkouts easy to tell apart.

## Change
`hrdrClaudeNative.cmd`: derive the folder name from `REPO_DIR` (the tab's `--cwd`) via
`for %%I in ("%REPO_DIR%") do set "REPO_NAME=%%~nxI"` and fold it into `AGENT_LABEL` →
`Claude - <folder>` (and `Claude N - <folder>` for subsequent tabs). The label is what `herdr tab create
--label` uses as the caption.

## Verify
Batch syntax valid (`%%~nxI` = last path component = folder name); no test surface. Foreman-applied per direct
user request.
