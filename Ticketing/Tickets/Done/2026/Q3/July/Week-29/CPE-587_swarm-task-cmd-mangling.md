---
id: CPE-587
title: "Swarm agents error (red prompt, no coordination) — cmd /c mangles a task with quotes"
type: Bug
status: Done
priority: High
component: Sidecar
tags: [ready]
epic: CPE-528
created: 2026-07-17
closed: 2026-07-17
---

## Summary
User ran a swarm; a tab appeared but showed the **task text in red** and the agent never coordinated
(no `mailbox.jsonl` / `memory/`). Root cause: on Windows the launch runs through `cmd /c <shim>`
(CPE-326, because `claude` is a `.cmd` shim), and `portable_pty` MSVC-quotes each argv (`"` → `\"`).
`cmd` doesn't understand `\"`, so it splits the `-p "<task>"` prompt at the embedded `"done"` quotes →
Claude gets a broken argv, errors, and echoes the mangled prompt in red.

## Diagnosis (confirmed empirically)
- Running the real `--mcp-config` config manually, `claude -p "<task>"` **works** end-to-end (writes
  memory, posts to mailbox) — so the mechanism is fine; only the `cmd`-quoting is broken.
- `claude` resolves to a `.cmd` shim (+ shell script), no `.exe`, so `cmd /c` is genuinely required.
- `portable_pty` 0.8.1 `append_quoted` always MSVC-quotes and offers no raw-arg mode, so argv can't be
  escaped to survive `cmd`'s re-parse.

## Fix
`swarm_plan.rs::cmd_safe_task` sanitises the task before it enters argv: straight double-quotes → `’`
(reads identically to the model) and control chars → spaces. Quote-free tasks are unchanged. Applied in
`swarm_args` for both the recipe and fallback paths.

## Acceptance Criteria
- [x] `cmd_safe_task` neutralises `"` and control chars; leaves ordinary text/punctuation intact.
- [x] `swarm_args` sanitises the task for both recipe + fallback.
- [x] Sidecar suite (**287 lib** + integration) + clippy green.

## Follow-on
[[CPE-588]] — deliver the task **verbatim** (via stdin/file, not argv) so nothing is rewritten at all;
that removes the `cmd`-quoting problem entirely and preserves the user's exact quotes.
