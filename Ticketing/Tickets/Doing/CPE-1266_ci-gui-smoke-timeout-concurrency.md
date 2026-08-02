---
id: CPE-1266
title: "Fix CI stall: gui-smoke jobs hang with no timeout + no concurrency groups clog the queue"
type: bug
component: ci
priority: high
status: Doing
tags: ready
created: 2026-08-02
---

## Problem (root cause of the multi-hour CI stall)
GitHub Actions jobs were queued for HOURS with zero starting. Root cause: the `gui-smoke` workflow's two jobs
(`gui-smoke` windows-latest, `gui-smoke-linux` ubuntu-latest) are `continue-on-error: true` but have NO
`timeout-minutes`. The known WebView2/WebKitGTK WebDriver flakiness (CPE-1048) makes the driver HANG (session
never created), so a hung job runs to GitHub's 6-hour max, **holding the account's concurrency slots the whole
time**. ~15 such runs accumulated over the session and starved every other queued run (CI, release) → total stall.
Compounded by NO `concurrency` groups anywhere, so each of ~15 rapid pushes spawned full CI+gui-smoke+pages runs
that all piled up instead of superseding.

## Fix
- Add `timeout-minutes` (~20) to both gui-smoke jobs so a hung WebDriver dies in minutes, never holding a slot for hours.
- Add `concurrency: { group: <workflow>-<ref>, cancel-in-progress: true }` to the push/PR workflows (gui-smoke.yml,
  ci.yml, and pages if applicable) so a newer push to a ref cancels the ref's superseded in-flight run.
- Immediate remediation already done: cancelled the ~15 hung in_progress + ~45 stale queued runs to free concurrency.

## Acceptance criteria
- gui-smoke jobs cannot hang beyond ~20 min; queue no longer clogs on gui-smoke flakiness.
- Superseded pushes auto-cancel via concurrency groups.
- YAML valid; CI picks up runs again (verified by a fresh push going in_progress).
