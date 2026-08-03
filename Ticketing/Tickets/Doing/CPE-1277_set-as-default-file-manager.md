---
id: CPE-1277
title: "Set-as-default-file-manager: register CPE as a default-apps candidate + toggle (honest, reversible)"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-712
---

## Summary
Epic CPE-712's context-menu shell integration is already shipped (`shell_menu::install_shell_integration` +
Settings toggle CPE-1023: "open in CPE" on Directory/Directory\Background/Drive). This adds the remaining
"default file manager" piece — HONESTLY scoped: modern Windows does NOT allow a program to silently force
itself as the default; the app can only REGISTER the necessary associations/ProgIDs + a capabilities entry so
it appears as a choice, and the USER confirms in Settings → Default apps. Build exactly that, reversibly.

## Build (honest + reversible; Windows primary, degrade cleanly elsewhere)
- Extend `shell_menu` (pure plan + an install/uninstall applier, mirror the existing pattern): register CPE under
  `HKCU\Software\RegisteredApplications` + a `Capabilities` key (ApplicationName/Description + FileAssociations /
  UrlAssociations as appropriate) and the ProgID(s) needed so CPE appears as an option for folders / relevant file
  types in Windows "Default apps". Do NOT attempt to programmatically force the default (Windows blocks it / it's
  a UX dark pattern) — register + then DIRECT the user to confirm.
- Commands: `set_default_file_manager()` (install the registrations, then open the Windows Default-apps settings page
  — e.g. `ms-settings:defaultapps` — so the user can confirm), `unset_default_file_manager()` (clean, complete
  removal of everything it added), `default_file_manager_status()` (registered? — best-effort detection).
- Settings UI: in the existing shell-integration Settings area, add a "Set as default file manager" action with
  HONEST copy: "Registers Cross-Platform Explorer with Windows; you'll confirm it in Settings → Default apps."
  Reversible via an "unregister" action. macOS/Linux: degrade to the platform-appropriate association (LSHandlers /
  xdg-mime default) OR clearly no-op-with-note if out of scope this slice.
- Reuse the existing registry/plan mechanism in shell_menu; no new dependency; async + spawn_blocking; capability entries if needed.

## Acceptance criteria
- cargo build/test/clippy clean (all modes); no new dep; CPE-1271 guard + bindings drift green.
- Registrations are COMPLETE + REVERSIBLE (unset removes everything set); status detection works.
- HONEST UX: never claims to have force-set the default; directs the user to Windows Default apps.
- Unit tests for the plan (what keys/values get written) + the uninstall completeness (mirror shell_menu's existing tests).

## Notes
Attended verify: run set-default → confirm CPE appears + is selectable in Windows Settings → Default apps → then
unset and confirm clean removal. Part of the CPE-712 epic (context menu already done).
