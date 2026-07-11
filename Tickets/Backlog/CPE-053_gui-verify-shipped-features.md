---
id: CPE-053
title: Verify shipped features in the live GUI (deferred from supervised session)
type: Test
status: Open
priority: Medium
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

## Acceptance Criteria — drive the running app and confirm each

- [ ] **CPE-046 (UNC breadcrumbs):** navigate to a `\\server\share\folder` path; breadcrumbs read
      `\\server\share` → `folder`, each segment clickable and navigating correctly.
- [ ] **CPE-047 (executable icon):** a folder containing `.exe` / `.msi` / `.dll` shows the new
      app-window glyph (not the generic unknown icon); Type column still reads "Application" etc.
- [ ] **CPE-050 (new-folder auto-number):** press New folder twice in the same directory; second is
      "New folder (2)"; a third is "New folder (3)"; gaps fill lowest-first.
- [ ] **CPE-051 (name validation):** rename an item to `a/b` or `CON`; a friendly message appears and
      the rename is blocked (no raw OS error).
- [ ] **CPE-052 (wildcard search):** type `*.md` in the search box; only markdown files remain;
      `report?.md`-style patterns behave as specified; plain text still substring-matches.
- [ ] Any discrepancy found becomes its own Bug ticket and is fixed.

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

2026-07-11 — Filed to capture GUI-verification debt from the supervised session (user chose to proceed while present). Everything listed is merged to `main`, pushed, and unit-test green at commit 72cc118; only live-app observation is outstanding.

## Notes

This is the one real debt the supervised session created. Run it first thing in the next unattended
Nightshift, before starting new feature loops. Related: [[CPE-046]] [[CPE-047]] [[CPE-050]]
[[CPE-051]] [[CPE-052]].
