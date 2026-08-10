---
id: CPE-1580
title: "hrdrClaudeNative.cmd: default to the latest Claude model (claude-opus-5)"
type: Task
status: Done
priority: Low
component: Tooling
tags: [ready]
created: 2026-08-10
closed: 2026-08-10
---

## Why
User request (2026-08-10). The bootstrap defaulted `CLAUDE_MODEL` to `claude-opus-4-8`; the latest and most
capable model is now Opus 5 (`claude-opus-5`).

## Change
`hrdrClaudeNative.cmd`: default `CLAUDE_MODEL` → `claude-opus-5` (+ update the "CUSTOMISING THE MODEL"
comment). `CLAUDE_MODEL` env override still honored, so a user can pin any model
(`setx CLAUDE_MODEL "claude-sonnet-5"`).

## Verify
Batch syntax unchanged (string value only); no test surface. Foreman-applied per direct user request.
