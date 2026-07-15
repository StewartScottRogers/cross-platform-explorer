---
id: CPE-459
title: "Browse GitLab repos too (second forge provider)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
CPE-434's browse was GitHub-specific. Extend `forge_browse` to GitLab (its repository-tree API has a
different shape/endpoint), proving the "any forge" claim with a second provider. Pure parser + path
builder are unit-tested; the RepoBrowser already exposes a provider dropdown.

## Acceptance Criteria
- [x] `forge_browse` builds the correct API path per provider (GitHub Contents vs GitLab tree).
- [x] A GitLab tree-response parser normalizes to `RepoEntry` (folders-first, total on garbage).
- [x] Unit tests for the path builder + the GitLab parser.

## Resolution
`forge_egress::browse_path(provider, repo, sub)` builds the right path per provider — GitHub Contents (`/repos/{repo}/contents/{sub}`, also gitea/forgejo/codeberg which are API-compatible) vs GitLab tree (`/projects/{owner%2Frepo}/repository/tree?per_page=100&path=…`). `parse_gitlab_tree` normalizes GitLab's `{name,type:tree|blob,path}` (folders-first, total); `parse_browse(provider, json)` dispatches to the right parser. `forge_browse` uses both. The RepoBrowser provider dropdown already lists GitLab, so browsing a GitLab repo now works end-to-end. 3 new unit tests (path builder + GitLab parser + dispatch); clippy clean both feature modes. Live GitLab fetch is GUI-verified; bitbucket/others still fall back to the GitHub shape (follow-up).
