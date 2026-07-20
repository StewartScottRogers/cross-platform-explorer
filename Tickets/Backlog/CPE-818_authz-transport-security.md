---
id: CPE-818
title: AuthZ (path-scope + capability) + transport security (TLS/mTLS)
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
Child of CPE-810. Full-stack authorization + channel crypto (decision: full security stack now).
**AuthZ**: a `path-scope` allowlist provider and a `capability`-grant provider behind the CPE-816
trait, composed `all-must-pass`, deciding whether a `Principal` may run *this op on this path* — the
file-explorer-specific risk surface. **Transport security**: TLS + mTLS channel providers. Prereq: CPE-816.

## Acceptance Criteria
- [ ] `path-scope` + `capability` AuthZ providers; `all-must-pass` composition enforced.
- [ ] Path-scope denies traversal outside the granted roots (tested with escape attempts).
- [ ] TLS + mTLS transport-security providers; local mode stays null/passthrough.
- [ ] Deny paths audited; unit-tested; clippy clean both modes.

## Work Log
