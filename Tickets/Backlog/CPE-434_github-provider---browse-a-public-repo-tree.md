---
id: CPE-434
title: "GitHub provider - browse a public repo tree"
type: Feature
status: Open
priority: High
component: Multiple
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-429
---

## Summary
First real capability (CPE-429): browse ANY public GitHub repo file tree read-only via the GitHub API
(host-brokered, CPE-433). Proves the provider-agnostic browse path end to end.

## Acceptance Criteria
- [ ] Enter owner/repo -> list the tree (default branch), navigate folders, view a file (read-only).
- [ ] Unauthenticated for public repos; authenticated for private once creds land (CPE-439).
- [ ] Graceful errors (not found, rate limited, offline); in-flight cancel on navigation change.
- [ ] Response parsing pure + unit-tested.
