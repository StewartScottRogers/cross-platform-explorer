---
id: CPE-483
title: "Clear orphaned session-daemons on startup/install so they can't lock the sidecar binary"
type: Defect
status: Open
priority: Medium
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-15
epic: CPE-261
---

## Summary
Leftover `ai-console.exe --session-daemon` processes (from the CPE-309 daemon builds — they outlive
the app by design) caused two real failures: (1) they held `sidecars\ai-console.exe` **file-locked**,
so the NSIS installer silently skipped updating the sidecar, leaving a **new host running a stale
sidecar** (the black-terminal saga, see [[CPE-309]]); (2) a surviving daemon kept serving old,
output-less sessions. The app must never let an orphaned daemon linger.

## Acceptance Criteria
- [ ] On host startup, detect + terminate any orphaned `ai-console --session-daemon` process that this
      host does not own, and delete a stale daemon port file. Scope it tightly (only this app's sidecar
      binary path) so it never touches unrelated processes.
- [ ] The `/run` install flow and `/remove` uninstall flow kill **all** `cross-platform-explorer` +
      `ai-console` processes (including `--session-daemon`) **before** running the installer, so the
      sidecar binary is never locked during an update. (Docs updated in this ticket.)
- [ ] If/when the daemon path is re-enabled (CPE-309), the host owns + reaps it deterministically on
      exit; a graceful shutdown leaves no orphan.
- [ ] A note in RELEASING/install docs: "a stale sidecar shows as the host version in the registry —
      verify `sidecars\ai-console.exe` timestamp matches the host exe after install."

## Notes
Discovered while root-causing the AI Console black terminal (CPE-309, 2026-07-15). The immediate
lesson (kill-all-before-install) has been folded into the `/run` + `/remove` command docs; the
host-startup cleanup is the remaining code slice here.
