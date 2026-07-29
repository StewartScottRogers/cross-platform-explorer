---
id: CPE-1010
title: Promote the workshift from a memory to a /workshift skill
type: chore
component: Tooling
priority: medium
tags: ready
status: Done
created: 2026-07-24
---

## Summary
The autonomous **workshift** procedure had accreted into a large always-loaded *memory*. It's really an
invokable operational procedure (trigger word + roles + rules + sub-steps), so it belongs as a **skill** —
discoverable in the skill list, loaded on-demand (frees context budget), committed + shared across the CLI
and desktop, next to the other `/ticketing-*` / `/run` / `/remove` skills. User asked "build workshift.md".

## What was done
- **New `.claude/commands/workshift.md`** — the `/workshift` skill: trigger phrases (start/stop/restart),
  the crew (Foreman / Workers / Researchers / Reviewer), the **≥2-independent-checks** QA pipeline, the
  three-way escalation policy, model right-sizing, the reporting format (ASCII banners + local timestamps +
  the `FOREMAN` block), GUI = build→deploy→run, machine-sharing (don't auto-yield), honesty + guardrails.
- Cross-cutting habits that apply *outside* the workshift too stay as **memories** and are referenced from
  the skill: `use-ascii-art-when-addressing-user`, `gui-verify-needs-build-deploy-run`,
  `workshift-summarize-with-context`, `loop-behavior-needs-timestamps`.
- The `workshift-mode` memory is slimmed to a short pointer ("the procedure is the `/workshift` skill —
  invoke it on 'start the workshift'"), keeping trigger-recognition in always-loaded memory while the full
  procedure lives on-demand in the skill.

## Acceptance Criteria
- [x] `.claude/commands/workshift.md` exists and captures the full workshift procedure.
- [x] Skill format matches the repo's other command files (plain markdown, filename = command name).
- [x] Cross-cutting preferences remain memories, referenced from the skill; `workshift-mode` memory slimmed
      to a pointer.

## Notes
- Filed + built during the workshift itself (user: "build workshift.md"). Docs/tooling only — no app code.
