---
id: CPE-816
title: Security core — 3-plane traits, composable chain, default-deny
type: feature
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-20
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810. The pluggable security foundation at the contract boundary. Three plane traits —
**Transport security**, **Authentication**, **Authorization** — plus an **ordered interceptor chain**
with a per-plane *combine policy* (`all-must-pass` / `any-passes` / `first-match`) so multiple
providers run at once (the "run multiple security solutions" requirement). Config-driven registry:
adding a provider = implement the trait + register, no core changes. A **null/passthrough** provider
for local mode keeps the plain explorer fast (security bills only remotely). Reuse `audit_journal.rs`
for the audit hook on every decision. Prereq: CPE-811 (envelope carries principal/session).

Two invariants, both tested:
- **Default-deny at the boundary** — structurally impossible to leave a command unsecured.
- **Local = null/passthrough** — trusted in-process principal, no transport crypto.

## Acceptance Criteria
- [ ] `TransportSec` / `AuthN` / `AuthZ` traits + a provider registry + config-driven activation/order.
- [ ] Interceptor chain supports all-must-pass / any-passes / first-match combine policies (unit-tested).
- [ ] Null/passthrough local provider; a default-deny test proves an unconfigured boundary denies.
- [ ] Every decision hooks the audit journal; headless-testable; clippy clean both modes.

## Work Log
