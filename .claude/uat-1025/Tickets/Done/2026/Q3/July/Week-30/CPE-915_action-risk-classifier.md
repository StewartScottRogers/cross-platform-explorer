---
id: CPE-915
title: Agent action risk classifier (gate high-impact commands)
type: feature
component: Sidecar
priority: medium
tags: ready
epic: CPE-729
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
First headless slice of "intervene & approve" (CPE-729). `ai-console::guardrail::assess_command(cmd)`
scores a shell command an agent is about to run — `RiskLevel::{Low, Medium, High}` + reasons — so the
explorer can gate high-impact actions with an approve/reject prompt. Complements
`scope::dangerous_flags` (which flags dangerous *launch* config): this flags dangerous *runtime* commands.

- **High**: recursive/force deletes, force-push / hard reset / git clean, pipe-to-shell (RCE), `sudo`,
  `chmod 777`, `mkfs`/`dd`, fork bomb, shutdown/reboot, publish.
- **Medium**: network fetch, dependency/package installs, normal commit/push/merge/rebase, moves.
- **Low**: everything else (reads). `needs_approval(level, policy)` with `ApprovalPolicy::{Off, HighOnly
  (default), MediumAndUp}`.

Heuristic (case-insensitive substring, not a shell parser) — errs toward flagging, since a false prompt is
cheap and a missed destructive command isn't.

## Acceptance Criteria
- [x] High for destructive/RCE/privilege commands; Medium for fetch/install/mutating-git; Low for reads.
- [x] Highest level wins when patterns of several levels are present; reasons surfaced + deduped.
- [x] `needs_approval` respects the policy threshold. 5 unit tests; clippy `-D warnings` clean.

## Work Log
- 2026-07-22 — Activated CPE-729 with the runtime action classifier. The approve/reject/edit-scope prompt
  UI + wiring it into the agent's pre-exec hook are the remaining children.
