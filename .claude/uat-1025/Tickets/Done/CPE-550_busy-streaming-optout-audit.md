---
id: CPE-550
title: "Busy cursor — streaming/self-progress opt-out audit + convention docs"
type: Task
status: Open
priority: Low
component: Frontend
tags: [needs-prereq]
epic: CPE-547
estimate: 1h
created: 2026-07-16
closed:
---

## Summary
Wave 3 (closeout) of [[CPE-547]]. The busy cursor must not **double-signal**: operations that already
show their own progress (streaming agent sessions, updater downloads, sidecar streaming reads) should
NOT also flip the global wait cursor. This ticket audits those call sites, confirms each uses
`rawInvoke` (the opt-out from [[CPE-548]]) rather than the tracked wrapper, adds them to the
[[CPE-549]] guard-test allowlist with a one-line justification each, and documents the convention so
future streaming features opt out correctly.

## Prereq
`needs-prereq`: [[CPE-548]] (wrapper/`rawInvoke`) and ideally [[CPE-549]] (allowlist mechanism).

## Acceptance Criteria
- [ ] Every streaming / self-progress `invoke` call site is identified (agent sessions, updater, sidecar
      streaming, Agent Watch reads) and confirmed to use `rawInvoke` (not the tracked wrapper).
- [ ] Each opt-out is in the guard-test allowlist with a one-line reason.
- [ ] A short "busy cursor & opt-out" convention note is added to the in-repo design docs (where the
      other UI conventions live) so new features know when to use `invoke` vs `rawInvoke`.
- [ ] `npm run check` clean; guard test passes with the documented allowlist.

## Notes
Closes the epic's "escape hatch" scope item. Low priority / small — it's the tidy-up that makes the
coverage honest (no flicker, no double progress). After this lands, [[CPE-547]] can close.
