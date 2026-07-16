---
id: CPE-499
title: "Threat-model extension: push/write ops + Generic-Git arbitrary-host egress"
type: Task
status: Done
priority: Medium
component: Docs
tags: [needs-prereq]
estimate: 1-2h
created: 2026-07-16
epic: CPE-488
closed: 2026-07-16
---

## Summary
Extend the forge threat model ([[CPE-440]]) to the two things v2 adds: **write/push** operations and
**Generic-Git arbitrary-host** egress. Cover per-connection host admission (no SSRF), token handling
on push, and untrusted-content-on-disk from any remote.

## Acceptance Criteria
- [x] Threat-model doc updated for push/write + generic-host egress.
- [x] Host admission demonstrated SSRF-safe (explicit consent, no wildcard) — cross-check with CPE-498.
- [x] Token never logged / leaked on push; untrusted-clone consent still applies to any remote.
- [x] Any guard tests updated.

## Resolution
Extended `docs/security/forge-threat-model.md` with a new **§J — v2 delta** covering the two surfaces
v1 had deferred, now shipped:
- **§J.1 push execution (CPE-495):** STRIDE rows showing `forge_sync` has **no force arm**, pushes only
  to the repo's own upstream with **no injected token** (auth via git's own machinery, nothing logged),
  and runs plan→confirm→apply via the Sync dialog. Flipped the §D rows from 🟡/⛔ to ✅.
- **§J.2 Generic-Git host admission (CPE-498):** rows showing a user-supplied host reaches git **only**
  via the consent-based, **no-wildcard**, **fail-closed** allow-list (`forge_admit_host` +
  `repos::parse_remote`), the same hardened clone builder + token-scrub apply, and only https/ssh/scp
  transports parse — cross-referenced to the CPE-498 unit tests that guard each claim.
- **§J.3 residual risks (honest gaps):** (1) a token-clone persists the credential in `.git/config`
  (on-disk in the user profile, **never logged**) — contradicts the aspirational §G "never in
  `.git/config`"; recorded as an accepted risk + a credential-helper hardening follow-up. (2) private-
  range / self-hosted hosts are permitted **by design** under host-naming consent (not IP-blocked —
  that would break legitimate LAN forges), consistent with §F user-asserted trust. (3) DNS anti-
  rebinding isn't applied to the git-clone child (shell-out design, §F).
- Added a **v2 sign-off record row** (2026-07-16) to §I.

No new guard tests were needed: the SSRF-safe admission (no-wildcard, no-silent-admission), transport
allow-list, and token-scrub are already unit-tested by CPE-498 (`generic.rs` + `build_generic_clone`
tests) and the force-safety by `plan_sync` tests — §J cites them. Docs-only change.

## Work Log
2026-07-16 — Picked up (prereq CPE-498 now Done). Estimate: 1-2h.
2026-07-16 — Extended the forge threat model: added §J (v2 delta — push execution + Generic-Git host admission), flipped §D push rows to ✅, added a v2 sign-off row. Documented residual risks honestly (token in .git/config; private-range hosts permitted-by-design under consent). Guard tests already exist in CPE-498/495 — cited, none added. All ACs met.

## Notes
**needs-prereq:** [[CPE-498]] (documents/guards the generic-host + push surface it introduces).
