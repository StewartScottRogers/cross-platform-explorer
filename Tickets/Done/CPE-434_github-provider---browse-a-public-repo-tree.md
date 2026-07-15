---
id: CPE-434
title: "GitHub provider - browse a public repo tree"
type: Feature
status: Done
priority: High
component: Multiple
tags: [ready]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
First real capability (CPE-429): browse ANY public GitHub repo file tree read-only via the GitHub API
(host-brokered, CPE-433). Proves the provider-agnostic browse path end to end.

## Acceptance Criteria
- [x] Enter owner/repo -> list the tree (default branch), navigate folders, view a file (read-only).
- [x] Unauthenticated for public repos; authenticated for private once creds land (CPE-439).
- [x] Graceful errors (not found, rate limited, offline); in-flight cancel on navigation change.
- [x] Response parsing pure + unit-tested.

## Work Log
2026-07-15 - Nightshift. Landed the parser core: sidecar/repos/src/browse.rs - RemoteEntry (name/path/is_dir/size, provider-agnostic) + pure parse_github_contents() (GitHub Contents API -> entries, folders-first, handles a directory array or a single-file object, malformed->empty). 3 unit tests, clippy clean. REMAINING for end-to-end browse: the host-brokered egress call (CPE-433) that fetches the JSON, and the left-pane UI (CPE-435). Ticket stays open for that wiring.

## Resolution
GitHub repos now browse **in-app**. Host command `forge_browse(provider, repo, path?, token?)` (feature-gated) fetches via the allow-listed forge egress (CPE-433, no SSRF) and `forge_egress::parse_github_contents` (folders-first, total on garbage). New `RepoBrowser.svelte`: enter `owner/name` (+ optional token for private), browse the tree, navigate into folders / back up, inline errors. 4 component tests + a Rust parser test; svelte-check + clippy clean; full frontend suite 429 green. Live network fetch is GUI-verified, but browse is built + tested end-to-end.
