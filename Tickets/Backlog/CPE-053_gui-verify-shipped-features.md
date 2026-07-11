---
id: CPE-053
title: Verify shipped features in the live GUI (deferred from supervised session)
type: Test
status: Open
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-11
closed:
---

## Summary

CPE-046, 047, 050, 051, and 052 were implemented and merged during a **supervised** session on
2026-07-11 while the user was present at the machine. Per the Nightshift yield rule, the GUI
verification step (install + drive the running app) was deferred for all of them. Their logic is
fully unit-tested and the app type-checks and builds, but the actual on-screen behaviour has not been
observed. This ticket queues that verification for the next **unattended** Nightshift, where the app
can be installed and driven without competing with the user for the desktop.

## Status update (2026-07-11)

**Most of this is now covered by automated headless tests** added in [[CPE-054]] (testing-library
drives the real components in jsdom). What remains here is only the *literal visual appearance* that a
DOM test can't judge — a quick human smoke-look, not behavioral verification.

### Now AUTOMATED (behavior verified in jsdom — no longer needs a human)

- [x] **CPE-050 (new-folder auto-number):** App test asserts `create_dir` is called with "New folder (2)".
- [x] **CPE-051 (name validation):** App test drives an inline rename to `bad/name.md`; notice shown, `rename_entry` not called.
- [x] **CPE-052 (wildcard search):** App test types `*.md`; only matching names remain.
- [x] **CPE-047 (executable icon):** FileList test asserts the `.exe` row renders the executable glyph (distinct stroke) and Type "Application".

### Residual — human visual smoke-check only (low priority)

- [ ] The executable glyph actually *looks* like a sensible app icon at 16px and 40px (aesthetic).
- [ ] **CPE-046 (UNC breadcrumbs):** if a real network share is available, confirm `\\server\share`
      breadcrumbs render and navigate. (Logic is unit-tested; rendering shares the drive-letter path.)
- [ ] Overall visual sanity pass once a screenshot/UI-automation capability or a human is available.

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

2026-07-11 — Filed to capture GUI-verification debt from the supervised session (user chose to proceed while present). Everything listed is merged to `main`, pushed, and unit-test green at commit 72cc118; only live-app observation is outstanding.
2026-07-11 — CPE-054 added automated headless behavioral coverage for CPE-047/050/051/052, discharging the behavioral half of this ticket. Narrowed remaining scope to the literal visual/aesthetic smoke-check. Kept Open at Low priority.

## Notes

This is the one real debt the supervised session created. Run it first thing in the next unattended
Nightshift, before starting new feature loops. Related: [[CPE-046]] [[CPE-047]] [[CPE-050]]
[[CPE-051]] [[CPE-052]].
