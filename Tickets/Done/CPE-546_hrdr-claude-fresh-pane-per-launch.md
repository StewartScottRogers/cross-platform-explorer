---
id: CPE-546
title: "hrdrClaudeNative.cmd — start a fresh Claude pane each launch (unique-name suffix)"
type: Defect
status: In Progress
priority: High
component: Docs
tags: [ready]
estimate: 30m
created: 2026-07-16
closed: 2026-07-16
---

## Summary
Running `hrdrClaudeNative.cmd` when a "Claude" pane already exists does nothing useful: it
prints *"A Claude pane already exists - focusing it"* and exits (the CPE-544 behavior), so the
user never gets a new, working Claude pane. Because a live idle "Claude" pane is normally
present, every launch hits this branch — the launcher effectively refuses to start Claude.

This reverses the CPE-544 decision in favor of the behavior of the source engine this file was
migrated from (`_hrdr-launch.cmd`), which auto-suffixes the agent name so each launch starts a
fresh tracked pane.

## Steps to Reproduce
1. Have a "Claude" pane already tracked in herdr (the common case).
2. Run `hrdrClaudeNative.cmd`.

## Expected Behavior
A new, tracked Claude pane starts and is focused — `Claude`, then `Claude 2`, `Claude 3`, … so
repeat launches always give a working Claude session.

## Actual Behavior
Prints "A Claude pane already exists - focusing it" and exits 0 without starting anything.

## Acceptance Criteria
- [x] herdr agent names are unique per session, so the script probes `agent get "<name>"`
      (exit 0 = taken) and increments a numeric suffix until a free name is found.
- [x] It then `agent start`s that free name (`Claude`, `Claude 2`, …) with the existing
      `--cwd/--env CLAUDE_MODEL/--focus -- cmd /c RunClaude.cmd` invocation.
- [x] No more "focus existing and exit" short-circuit; every launch yields a fresh pane.
- [x] Bootstrap + attach behavior from CPE-545 preserved (herdr path fallback, server
      auto-start, attach vs. "switch to the new window").

## Resolution
Replaced the focus-existing guard with the source engine's free-name loop and switched to
`setlocal EnableDelayedExpansion` so the `!AGENT_NAME!`/`!N!` suffixing works:
```
set "AGENT_NAME=Claude"
set "N=1"
:namecheck
"%HERDR%" agent get "!AGENT_NAME!" >nul 2>nul
if not errorlevel 1 (
    set /a N+=1
    set "AGENT_NAME=Claude !N!"
    goto :namecheck
)
"%HERDR%" agent start "!AGENT_NAME!" --cwd "%REPO_DIR%" --env CLAUDE_MODEL=%CLAUDE_MODEL% --focus -- cmd /c "%CLAUDE_CMD%"
```
`agent get` exits 0 when the name is taken and 1 when free, so the loop lands on the first
unused `Claude N`.

## Verification
herdr 0.7.4-preview: `agent get Claude` → exit 0 (taken), `agent get "Claude 2"` → exit 1
(free) ⇒ the loop selects "Claude 2". Confirmed `agent start "<name>" --cwd … --env … --focus
-- cmd /c …` is accepted and spawns a new pane.

## Work Log
- 2026-07-16 — User reported the launcher only ever says "already exists" and never launches
  Claude. Root cause: the CPE-544 focus-existing branch. Restored the source `_hrdr-launch.cmd`
  unique-name suffixing so each run starts a fresh Claude pane. Verified name-probe exit codes
  and the start invocation against live herdr. Closed.

## Notes
Supersedes the CPE-544 focus-existing behavior. A single fixed name was the wrong model for a
"launch me a Claude" action — the source engine's suffixing is what the user actually wanted.
