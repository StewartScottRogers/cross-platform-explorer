---
id: CPE-1010
title: Promote the sprint from a memory to a /sprint skill
type: chore
component: Tooling
priority: medium
tags: ready
status: Done
created: 2026-07-24
---

## Summary
The autonomous **sprint** procedure had accreted into a large always-loaded *memory*. It's really an
invokable operational procedure (trigger word + roles + rules + sub-steps), so it belongs as a **skill** —
discoverable in the skill list, loaded on-demand (frees context budget), committed + shared across the CLI
and desktop, next to the other `/ticketing-*` / `/run` / `/remove` skills. User asked "build sprint.md".

## What was done
- **New `.claude/commands/sprint.md`** — the `/sprint` skill: trigger phrases (start/stop/restart),
  the crew (Foreman / Workers / Researchers / Reviewer), the **≥2-independent-checks** QA pipeline, the
  three-way escalation policy, model right-sizing, the reporting format (ASCII banners + local timestamps +
  the `FOREMAN` block), GUI = build→deploy→run, machine-sharing (don't auto-yield), honesty + guardrails.
- Cross-cutting habits that apply *outside* the sprint too stay as **memories** and are referenced from
  the skill: `use-ascii-art-when-addressing-user`, `gui-verify-needs-build-deploy-run`,
  `sprint-summarize-with-context`, `loop-behavior-needs-timestamps`.
- The `sprint-mode` memory is slimmed to a short pointer ("the procedure is the `/sprint` skill —
  invoke it on 'start the sprint'"), keeping trigger-recognition in always-loaded memory while the full
  procedure lives on-demand in the skill.

## Acceptance Criteria
- [x] `.claude/commands/sprint.md` exists and captures the full sprint procedure.
- [x] Skill format matches the repo's other command files (plain markdown, filename = command name).
- [x] Cross-cutting preferences remain memories, referenced from the skill; `sprint-mode` memory slimmed
      to a pointer.

## Notes
- Filed + built during the sprint itself (user: "build sprint.md"). Docs/tooling only — no app code.
