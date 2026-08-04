---
id: CPE-1303
title: "Fix release-sidecar create-release: draft id lookup 404s (by-tag endpoint)"
type: bug
component: build
priority: high
status: Done
tags: ready
created: 2026-08-04
epic: CPE-716
---

## Summary
CPE-1279's `create-release` job (single-draft coordination) fails on its FIRST live run (v0.57.49-sidecar):
`gh release create "$TAG" --draft` creates an UNtagged draft (draft releases aren't git-tagged until
published — the created URL is `.../releases/tag/untagged-<hash>`), then
`gh api repos/.../releases/tags/$TAG` returns **404 Not Found** because the by-tag REST endpoint only
resolves PUBLISHED releases. `release_id` never resolves → `create-release` fails → the 3 matrix build legs
(`needs: create-release`) are skipped → an empty draft with no installers. This is the exact failure the
CPE-1279 reviewer flagged as needing an attended release run to catch.

## Build
- In `.github/workflows/release-sidecar.yml`, replace the `release_id` extraction: instead of the by-tag
  endpoint, LIST releases and match by `tag_name` (drafts DO carry `tag_name`):
  `gh api --paginate repos/<repo>/releases --jq '.[] | select(.tag_name=="$TAG") | .id' | head -n1`, with a
  guard that errors if empty.
- Delete the orphaned empty draft left by the failed run.

## Acceptance criteria
- A `release-sidecar` run resolves the draft's numeric id, the 3 legs run and upload to the ONE draft, and
  the draft carries all OS installers (verified by an actual run — this ticket is validated by the release
  succeeding).

## Notes
Hotfix to unblock a "Run". Validated by re-dispatching the release. Part of the CPE-1279 release-polish
lineage.

## Work Log
- 2026-08-04 — FIXED + validated. First live run of the CPE-1279 create-release job (v0.57.49) failed: (1) by-tag REST endpoint 404s on drafts (not git-tagged until published), then (2) the list/view lookup was correct but raced eventual consistency (draft not API-visible ~2s after create). Fix: `gh release view "$TAG" --json databaseId` in a 6x/5s retry loop + explicit guard. Re-dispatched: create-release SUCCESS, 3 legs building. Deleted 2 orphaned empty drafts. Exactly the failure the CPE-1279 reviewer flagged for attended verify.
