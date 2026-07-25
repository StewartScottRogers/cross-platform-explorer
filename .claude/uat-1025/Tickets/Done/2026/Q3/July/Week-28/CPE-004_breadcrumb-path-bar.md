---
id: CPE-004
title: Add a breadcrumb path bar with click-to-navigate
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-10
closed: 2026-07-11
---

## Summary

The toolbar showed the raw current path as static text. Turn it into a breadcrumb of clickable
segments so the user can jump to any ancestor directory in one click.

## Acceptance Criteria

- [x] The current path renders as clickable segments split on the path separator
- [x] Clicking a segment navigates to that ancestor directory
- [x] The active/last segment is styled distinctly and is not a link
- [x] Works with both Windows (`\`) and POSIX (`/`) separators
- [x] Long paths truncate gracefully without breaking the toolbar layout

## Resolution

Added `splitPath(fullPath): PathSegment[]` to `src/lib/format.ts` — a pure function returning
cumulative `{ name, path }` segments, so each crumb carries the absolute path it navigates to.

Cross-platform handling: Windows paths are detected by a leading drive letter (`/^[a-zA-Z]:/`),
forward slashes are normalised to backslashes (Windows accepts both), and the drive becomes the root
crumb (`C:` -> `C:\`). POSIX paths get a `/` root crumb. Put in `lib/` rather than the component
specifically so it could be unit-tested — added 7 tests covering POSIX, Windows, bare roots
(`/` and `C:\`), forward-slash normalisation, and a round-trip assertion that the last segment's
path equals the input.

UI: the toolbar `<span class="path">` became a `<nav class="breadcrumbs">` of `<button class="crumb">`
elements separated by `›`, with the final segment rendered as a non-interactive
`<span class="crumb current">` carrying `aria-current="page"`. Overflow is handled by
`overflow-x: auto` with the scrollbar hidden, so long paths scroll horizontally instead of wrapping
or breaking the toolbar.

Verified: `npm run check` -> 0 errors, 0 warnings; `npm test` -> 17 passed.

Files changed: `src/lib/format.ts`, `src/lib/format.test.ts`, `src/App.svelte`, `src/app.css`.

## Work Log

2026-07-11 — Picked up alongside CPE-005 (both touch the same component). Estimate: 1-2h.
2026-07-11 — Added splitPath() to lib/format.ts as a pure, testable function rather than inlining the logic in the component.
2026-07-11 — Added 7 unit tests: POSIX, Windows, bare roots, forward-slash normalisation on Windows, round-trip.
2026-07-11 — Replaced the static path span with a nav of crumb buttons; last crumb is non-interactive with aria-current.
2026-07-11 — Long-path handling via horizontal scroll with hidden scrollbar. svelte-check 0/0; vitest 17/17. Closing as Done.

## Notes

`splitPath` decides Windows-vs-POSIX from the path string itself rather than from a platform flag,
so it stays a pure function and is testable for both platforms from any host.
