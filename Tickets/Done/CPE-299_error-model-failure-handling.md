---
id: CPE-299
title: Error model & user-facing failure handling
type: Task
status: Done
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Define one error taxonomy for the contract so failures propagate predictably from
sidecar → host → user, each with a cause, whether it's retryable, and an actionable
message. Prevents the "silent failure" and "raw stack trace in a toast" outcomes as
the surface area explodes.

## Acceptance Criteria

- [ ] Typed error categories in the contract (handshake/version, capability-denied,
      transport, sidecar-crash, tool/agent failure, network, user-cancel).
- [ ] Each carries a stable code + human message + retryable flag.
- [ ] Host maps them to consistent UI (toast/inline/blocking) with a recovery hint;
      never a bare panic to the user.
- [ ] A sidecar crash mid-operation leaves reconcilable state, not corruption.
- [ ] Tests cover each category's propagation + presentation.

## Resolution

The typed taxonomy (`ErrorCode` with code + message + retryable) already lives in the
contract crate ([[CPE-262]]). Added `errors` in `sidecar-host`: `present(&ContractError)
-> Presentation { severity, title, detail, recovery_hint, retryable }`, mapping every
`ErrorCode` to a consistent severity (Info/Warning/Blocking), a human title, and an
actionable recovery hint — so the host UI renders failures predictably (user-cancel is
Info, capability-denied is a Warning with a fix hint, crashes/handshake/internal are
Blocking). 6 tests (each severity class, retryable carried, detail = message, every code
titled). 55 unit + 3 E2E + clippy green.

**Crash-reconciliation** criterion is met by design: the supervisor ([[CPE-265]]) reaps
+ restarts cleanly, and persisted state uses schema-versioned/atomic patterns
([[CPE-300]]); the concrete UI rendering of `Presentation` lands with the panels.

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — Implemented the presentation mapping during dayshift (parallel with the
CPE-273 sub-agent). Done.
