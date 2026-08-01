---
id: CPE-1235
title: "parentDir returns empty for a POSIX file at the filesystem root (live-refresh edge)"
type: Bug
priority: Low
component: frontend
tags: [ready]
created: 2026-08-01
closed:
---

## Problem
Flagged by the CPE-1230 re-check. `parentDir` (`src/lib/contentSearch.ts`) returns `""` for a POSIX
file directly at the filesystem root (`/foo.txt` → `lastIndexOf("/")` is 0, fails the `cut > 0` check).
`watchPathsForScope` filters empty strings, so a tag smart folder scoped to such a file gets no watched
directory and won't live-refresh. Pre-existing `parentDir` quirk; Windows drive roots (`C:\foo.txt` →
`"C:"`) are unaffected. Genuinely rare (a tagged file at the volume root).

## Acceptance criteria
- `parentDir("/foo.txt")` returns `"/"` (the root), so root-level files get a watchable parent.
- No regression to existing parentDir callers (content search, etc.) — add/keep tests.
