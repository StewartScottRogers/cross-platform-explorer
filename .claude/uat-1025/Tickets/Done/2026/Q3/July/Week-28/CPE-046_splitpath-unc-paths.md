---
id: CPE-046
title: Breadcrumbs break for Windows UNC paths (\\server\share)
type: Defect
status: Done
priority: Medium
component: Frontend
estimate: 30m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

`splitPath()` in `src/lib/format.ts` builds the clickable breadcrumb segments for the address bar.
It detects Windows paths with `/^[a-zA-Z]:/` (a drive letter). UNC paths of the form
`\\server\share\folder` do not start with a drive letter, so they fail that test and fall through to
the POSIX branch. The POSIX branch splits on `/` only, so a backslash-delimited UNC path is not split
at all — it produces a single nonsensical breadcrumb whose path is prefixed with `/`. Navigating a
network share therefore shows a broken address bar.

## Environment

- OS: Windows 11 (UNC / network shares)
- App version: 0.5.1
- Node / Rust version: n/a (pure frontend logic)
- Other context: Found during Nightshift code review of `src/lib`.

## Steps to Reproduce

1. Navigate to a UNC path such as `\\server\share\folder`.
2. Observe the breadcrumb / address bar.

## Expected Behavior

Breadcrumbs are `\\server\share` (the share root) → `folder`, each navigable, matching how the drive
root and subsequent segments work for `C:\...` paths.

## Actual Behavior

A single breadcrumb containing the whole raw string, with a bogus leading `/` in its target path;
segments are not individually navigable.

## Acceptance Criteria

- [ ] `splitPath("\\\\server\\share\\folder")` returns navigable segments rooted at `\\server\share`
- [ ] Forward-slash UNC input (`//server/share/folder`) is handled equivalently
- [ ] The share root segment's `path` is `\\server\share` (double backslash preserved)
- [ ] Existing drive-letter and POSIX behavior is unchanged
- [ ] Unit tests cover UNC in `src/lib/format.test.ts`; full suite stays green

## Resolution

Added a UNC branch to `splitPath()` ahead of the drive-letter check: paths beginning with two
slashes (either style) are normalised to backslashes, the first two components become the
`\\server\share` root segment, and remaining components accumulate as navigable segments. Drive-letter
and POSIX branches are untouched. Added three unit tests (UNC, forward-slash UNC, bare share root).
`npm run check` clean; full suite 94 passed (was 91). Committed on branch `cpe-046-splitpath-unc`.

## Work Log

2026-07-11 — Filed during Nightshift loop 1 after code review of `src/lib/format.ts` found UNC paths fall into the POSIX branch. Confirmed no existing UNC coverage in `format.test.ts`.
2026-07-11 — Implemented UNC branch in `splitPath`; added tests. `npm run check` = 0 errors; `npm test` = 94 passed. Committed on branch `cpe-046-splitpath-unc`. GUI verify (install + drive breadcrumb on a share) DEFERRED — user present, GUI paused per Nightshift rules.
2026-07-11 — Discovery during review: `exe`/`msi`/`dll` have friendly type names but no icon category (render as generic "unknown"). Out of scope here (needs an Icon change); filing follow-up ticket CPE-047.

## Notes

Related: [[CPE-037]] added address-bar editing which relies on these segments.
