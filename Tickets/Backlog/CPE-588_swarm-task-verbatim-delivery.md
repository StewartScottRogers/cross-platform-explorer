---
id: CPE-588
title: "Deliver swarm task verbatim (stdin/file), not via cmd-parsed argv"
type: Task
status: Open
priority: Low
component: Sidecar
tags: [ready]
epic: CPE-528
estimate: 2-3h
created: 2026-07-17
---

## Summary
Follow-on to [[CPE-587]]. CPE-587 sanitises the swarm task (`"` → `’`) so it survives the Windows
`cmd /c` re-parse, but that rewrites the user's text. Deliver the task **verbatim** instead so no
rewriting is needed and exact quotes are preserved.

## Options (pick during work)
- **stdin**: launch `claude -p` with no task arg and write the task to the session's input after launch
  (Claude reads the prompt from stdin — confirmed). Needs an EOF signal in the ConPTY (Ctrl-Z) or an
  `--input-format` mode.
- **file + redirect (Windows)**: write the task to `<mission>/task.txt` and append an unquoted `<`
  redirect (portable_pty passes a bare `<` through un-quoted, and `cmd` does the redirect); keep the
  task in argv on Unix (no `cmd`). Task content never touches the command line.

Once landed, drop `cmd_safe_task` (or keep it as defence-in-depth) and preserve the task exactly.

## Acceptance Criteria
- [ ] A swarm task containing `"`, `%`, `&`, etc. reaches the agent byte-for-byte.
- [ ] Works on Windows (cmd shim) and Unix; tested.
