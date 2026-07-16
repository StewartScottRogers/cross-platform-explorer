---
id: CPE-499
title: "Threat-model extension: push/write ops + Generic-Git arbitrary-host egress"
type: Task
status: Open
priority: Medium
component: Docs
tags: [needs-prereq]
estimate: 1-2h
created: 2026-07-16
epic: CPE-488
---

## Summary
Extend the forge threat model ([[CPE-440]]) to the two things v2 adds: **write/push** operations and
**Generic-Git arbitrary-host** egress. Cover per-connection host admission (no SSRF), token handling
on push, and untrusted-content-on-disk from any remote.

## Acceptance Criteria
- [ ] Threat-model doc updated for push/write + generic-host egress.
- [ ] Host admission demonstrated SSRF-safe (explicit consent, no wildcard) — cross-check with CPE-498.
- [ ] Token never logged / leaked on push; untrusted-clone consent still applies to any remote.
- [ ] Any guard tests updated.

## Notes
**needs-prereq:** [[CPE-498]] (documents/guards the generic-host + push surface it introduces).
