---
id: CPE-791
title: Integrity baseline store + on-demand verify
type: feature
status: Open
priority: low
component: Multiple
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-737
estimate: 3-4h
---

## Summary
Persist a chosen folder's checksum baseline (reusing the sha256 backend) and re-scan on demand, diffing via
CPE-790 to flag unexpected changes / missing files. Opt-in; no background scanning unless configured.

## Acceptance Criteria
- [ ] Baseline a folder (recursive sha256 + size + mtime) and persist it; re-verify produces the CPE-790 report.
- [ ] Opt-in; nothing scans unless the user baselines/verifies; large trees stay responsive (streamed).
- [ ] check + suite green.

## Notes
Prereq: CPE-790. Reuse the checksum backend; a scheduled verifier is a later follow-up.
