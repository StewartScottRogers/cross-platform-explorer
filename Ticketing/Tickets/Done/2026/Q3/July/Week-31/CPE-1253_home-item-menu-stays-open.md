---
id: CPE-1253
title: "Bug: Home Folders-tab item context menu doesn't stay open (closes a tick after right-click)"
type: Bug
priority: Low
component: Frontend
tags: [ready]
estimate: 1h
created: 2026-08-02
closed:
---

## Context
Surfaced (not caused) by the CPE-1249 gui-smoke gate. `home-item-menu.smoke.ts` test 2 fails: after
right-clicking a Home "Folders" tab item, the context menu (`.ctx`) is NOT still present a tick later
("expected false to equal true"). Confirmed PRE-EXISTING — fails identically on `main` baseline, unrelated
to the vault work. The menu opens then immediately closes.

## What to do
Investigate why the Home Folders-tab item context menu dismisses immediately (likely a click/blur/outside-
click handler firing on the same event that opened it). Fix so it stays open like other context menus
(see docs/design/MENUS.md). Re-enable/confirm the gui-smoke assertion.

## Done 2026-08-02 (workshift) — merged #556 @ a382f6ea
Test-only fix: NOT an app bug — the gui-smoke row was below the 700px fold so the right-click missed the webview. App menu-stays-open logic (ContextMenu OPEN_GUARD_MS + HomeView stopPropagation) verified already-correct. Spec now scrollIntoView before measuring (sibling-spec pattern). Reviewer APPROVE; gui-smoke home-item-menu 2/2.
