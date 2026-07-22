---
id: CPE-001
title: Enable managing this repo from both the CLI (RunClaude.cmd) and the desktop
type: Task
status: Done
priority: Medium
component: Multiple
estimate: 30m
created: 2026-07-10
closed: 2026-07-10
---

## Summary

The user will use the new `RunClaude.cmd` launcher to manage this repository from the Claude Code
CLI, while also continuing to manage it from the Claude desktop (Cowork) app. Ensure both surfaces
work against the same repo and are documented so either can be used interchangeably.

## Acceptance Criteria

- [x] `RunClaude.cmd` is present in the repo root and launches a Claude Code session scoped to the repo
- [x] The Claude Code CLI is installed and on PATH so the launcher works
- [x] CLI slash commands (`ticketing-*`, `skills-organise`, `menu-render`) are installed in `.claude/commands/`
- [x] Desktop management assets remain intact (RELEASING.md, scripts/release.ps1, STATUS.html, scheduled tasks)
- [x] The ticket system is committed to git so tickets are shared across both surfaces
- [x] CLAUDE.md documents both surfaces and how they interoperate

## Resolution

Both surfaces now manage the same repo:

- **CLI:** Added `RunClaude.cmd` (path-independent `%~dp0`, launches `claude --dangerously-skip-permissions`).
  Verified Claude Code v2.1.202 is installed on the host. The `.claude/commands/` slash commands
  (ticketing suite, skills-organise, menu-render) are available in any CLI session started here.
- **Desktop:** Existing release runbook (RELEASING.md), release helper (scripts/release.ps1), status
  dashboard (STATUS.html), and the `cpe-daily-status` / `cpe-weekly-deps` scheduled tasks are unchanged
  and continue to work.
- **Interop:** The ticket system (`Tickets/` + `.claude/commands/`) is committed to git, so tickets
  filed from the CLI appear on the desktop and vice-versa. Added a "Managing this project — two surfaces"
  section to CLAUDE.md documenting the split (CLI = coding + tickets, desktop = release + monitoring)
  and confirming nothing is locked to a single surface except the desktop-only scheduled tasks.

Files touched: `RunClaude.cmd` (new), `CLAUDE.md` (management + ticket sections), plus the ticket
system scaffold under `Tickets/` and `.claude/commands/`.

## Work Log

2026-07-10 — Picked up. Estimate: 30m. Plan: add launcher, verify CLI, document both surfaces, wire ticket system.
2026-07-10 — Added RunClaude.cmd to repo root (CRLF line endings for Windows batch).
2026-07-10 — Verified `claude` CLI installed on host: v2.1.202 at %APPDATA%\npm\claude.ps1. Launcher will work.
2026-07-10 — Confirmed .claude/commands/ holds menu-render + ticketing suite + skills-organise.
2026-07-10 — Appended "Managing this project — two surfaces" and "Ticket System" sections to CLAUDE.md.
2026-07-10 — All acceptance criteria met. Closing as Done and placing in Done/ (no subdivision needed yet).

## Notes

First ticket in the system — also serves as a worked example of the CPE-NNN lifecycle. Done/ has no
subdirectories yet, so the closed ticket lives directly in Done/ (see Tickets/Done/wiki.md for the
depth-subdivision rules that kick in at 50 files).
