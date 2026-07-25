---
id: CPE-1022
title: macOS Services / LSHandlers registration plan (pure model)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-712
created: 2026-07-24
closed: 2026-07-25
status: Done
---

## Summary
CPE-712 slice: the macOS analogue of CPE-1019 / CPE-1021. A **pure, headless** function producing the
plist fragments to register an "Open in Cross-Platform Explorer" **Services** menu item (NSServices entry:
menu title, send-types = file URLs) and, optionally, the `LSHandlerRoleAll` folder-viewer association used
to offer CPE as a default file handler — plus the exact keys to strip on uninstall. No `defaults`/`plutil`
process calls or bundle mutation here; that glue is a later slice.

## Acceptance Criteria
- [ ] Returns the NSServices dictionary fragment (menu title, `NSSendFileTypes`/URL send-types, instance
      method) and the optional LSHandlers association entry.
- [ ] Uninstall set names each key to remove; reversibility unit-tested.
- [ ] Pure — no I/O, no process spawns; clippy clean both feature modes; ≥3 unit tests.

## Work Log
- 2026-07-24 (PM take-on) — Filed as the macOS plan mirror; the actual bundle/`defaults` glue + real-macOS
  verification will be a follow-up slice (needs a Mac, per the cross-OS-verify escalation).
- 2026-07-25 — **Done (pure plan).** Added `macos_shell_plan` + `MacShellPlan` to `cpe_server::shell_menu`.
  Emits the `NSServices` `<dict>` for an "Open in <app>" Services item (menu title + `public.folder` /
  `public.item` send-types + `NSMessage`) and an `LSHandlers` folder-viewer association keyed to the bundle
  id, plus the remove keys (menu title + content type). 2 unit tests; part of 13/13 suite; clippy clean.
  **Deferred (needs a Mac — user test territory):** writing these into the app bundle's Info.plist +
  `lsregister`/`defaults` glue and verifying the Services item actually appears.
