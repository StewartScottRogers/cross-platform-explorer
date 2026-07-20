---
id: CPE-784
title: Pure POSIX permission model (mode ↔ rwx ↔ octal)
type: feature
status: In Progress
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed:
epic: CPE-710
estimate: 1-2h
---

## Summary
Foundation for the attributes/permissions editor (epic CPE-710). A pure module (`src/lib/permissions.ts`)
that converts a POSIX mode between symbolic (`rwxr-xr-x`) and octal (`755`) forms and exposes a per-class
read/write/execute breakdown + bit toggles — so the editor (CPE-786) is a thin render + a backend chmod.

## Scope
- `modeToSymbolic(mode)` → `"rwxr-xr-x"` (owner/group/other rwx over the low 9 bits).
- `modeToOctal(mode)` → `"755"` (3-digit).
- `octalToMode(str)` / `symbolicToMode(str)` → the numeric mode, or `null` on malformed input.
- `describePermissions(mode)` → `{ owner, group, other }` each `{ read, write, execute }`.
- `setPermission(mode, who, perm, value)` → new mode with one bit toggled.
- Pure + total; round-trips (`octalToMode(modeToOctal(m)) === m & 0o777`, same for symbolic).

## Acceptance Criteria
- [x] symbolic/octal formatting + parsing are correct and round-trip; malformed input → null.
- [x] `describePermissions` and `setPermission` operate on the right owner/group/other bits.
- [x] Pure + dependency-free; unit tests cover formatting/parsing/round-trip/toggle/edge cases; check + suite green.

## Notes
POSIX low 9 bits (special bits out of scope for v1). Foundation for CPE-786. Headless.

## Resolution
Added `src/lib/permissions.ts` (pure): `modeToSymbolic`/`modeToOctal` format the low-9-bit POSIX mode;
`octalToMode`/`symbolicToMode` parse (null on malformed); `describePermissions` breaks it into
owner/group/other rwx; `setPermission` toggles one bit. Round-trips verified. 5 tests. check 0/0. Headless;
no existing code touched. Foundation for CPE-786.

