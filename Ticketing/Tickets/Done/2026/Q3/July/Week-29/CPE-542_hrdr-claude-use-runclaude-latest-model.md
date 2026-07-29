---
id: CPE-542
title: "hrdrClaudeNative.cmd — use RunClaude.cmd + the latest model"
type: Task
status: Done
priority: Low
component: Docs
tags: [ready]
estimate: 15m
created: 2026-07-16
closed: 2026-07-16
---

## Summary
`hrdrClaudeNative.cmd` (launch Claude Code inside the herdr multiplexer) pointed at a non-existent
external launcher (`..\AgenticCliOptions\CodingAgents\Claude\Claude--run.cmd`, carried over from another
setup) and defaulted to an old model. Point it at this repo's **`RunClaude.cmd`** and default to the
**latest** model.

## Acceptance Criteria
- [x] `hrdrClaudeNative.cmd`'s in-pane command is `%~dp0RunClaude.cmd` (which exists), not the external
      native launcher; `HP_CWD` is the repo root.
- [x] Default model is the latest — `claude-opus-4-8` (override example updated to `claude-sonnet-5`).
- [x] `RunClaude.cmd` honours `CLAUDE_MODEL` (default `claude-opus-4-8`) and passes `--model`, so both
      the standalone and herdr paths use the latest model.

## Resolution
- `RunClaude.cmd`: added `if not defined CLAUDE_MODEL set "CLAUDE_MODEL=claude-opus-4-8"` + `claude
  --model %CLAUDE_MODEL% …`.
- `hrdrClaudeNative.cmd`: `CLAUDE_CMD=%~dp0RunClaude.cmd`, default `CLAUDE_MODEL=claude-opus-4-8`,
  `HP_CWD=%~dp0`, refreshed the header comments.

## Notes
The script still `call`s `%~dp0_hrdr-launch.cmd` (the herdr pane wrapper) at the end — that helper isn't
in this repo, so it must be present in the herdr environment for the script to launch. Out of scope for
this fix (RunClaude + model), but flagged.
