---
id: CPE-047
title: Executables (exe/msi/dll) show the generic icon despite having type names
type: Defect
status: Open
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-11
closed:
---

## Summary

In `src/lib/filetypes.ts`, `exe`, `msi`, and `dll` appear in `TYPE_NAME_BY_EXT` (so the Type column
reads "Application", "Windows Installer Package", "Application extension") but are absent from
`CATEGORY_BY_EXT`. `categoryOf()` therefore returns `"unknown"` for them and they render with the
generic fallback icon — a visible mismatch between the named type and the icon.

## Environment

- OS: All (Windows most visibly, given .exe/.msi/.dll)
- App version: 0.5.1
- Node / Rust version: n/a

## Steps to Reproduce

1. Open a folder containing a `.exe`, `.msi`, or `.dll`.
2. Note the Type column says "Application" etc. but the icon is the generic unknown-file icon.

## Expected Behavior

Executables/installers get a distinct icon matching their named type.

## Actual Behavior

Generic "unknown" icon.

## Acceptance Criteria

- [ ] Add an `"executable"` (or similar) `FileCategory` and map `exe`/`msi`/`dll` to it
- [ ] `Icon.svelte` renders a glyph for the new category
- [ ] `categoryOf` unit test covers exe/msi/dll
- [ ] A consistency test asserts every extension in `TYPE_NAME_BY_EXT` also has a `CATEGORY_BY_EXT` entry (prevents regressions)

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

2026-07-11 — Filed during Nightshift loop 1 as a discovery while fixing [[CPE-046]]. Deferred (not worked in the demo) because it needs an Icon.svelte change, i.e. GUI verification, which is paused while the user is present.

## Notes

Pairs well with [[CPE-048]] (extension-table completeness). The consistency test in the acceptance
criteria would have caught this gap automatically.
