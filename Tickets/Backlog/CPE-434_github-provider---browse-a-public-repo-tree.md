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

## Work Log
2026-07-15 - Nightshift. Landed the parser core: sidecar/repos/src/browse.rs - RemoteEntry (name/path/is_dir/size, provider-agnostic) + pure parse_github_contents() (GitHub Contents API -> entries, folders-first, handles a directory array or a single-file object, malformed->empty). 3 unit tests, clippy clean. REMAINING for end-to-end browse: the host-brokered egress call (CPE-433) that fetches the JSON, and the left-pane UI (CPE-435). Ticket stays open for that wiring.
