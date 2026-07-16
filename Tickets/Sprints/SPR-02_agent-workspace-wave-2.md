---
id: SPR-02
title: "Sprint 2 — Agent Workspace, wave 2 (Swarm coordinator)"
status: Active
start: 2026-07-16
end: 2026-07-30
created: 2026-07-16
closed:
---

## Goal
Wave 2 of the **Swarm** epic [[CPE-502]]: build the **coordinator** that ties the wave-1 substrates
([[CPE-514]] locks · [[CPE-515]] team · [[CPE-516]] mailbox) into a working orchestration — one mission
→ role-assigned tasks → sequenced by file ownership → coordinated via the mailbox → gated before done.

## Tickets
Wave-2 orchestration of [[CPE-502]] (each carries `sprint: SPR-02`):
- [x] CPE-517 — Coordinator dispatch loop (mission → tasks → collect) *(big-design; keystone)*
- [x] CPE-518 — Quality gates before a task is "done"
- [ ] CPE-519 — Cost/token budgets + retry + coordinator authority

Suggested order: CPE-517 first (the keystone the other two extend), then CPE-518 / CPE-519.
