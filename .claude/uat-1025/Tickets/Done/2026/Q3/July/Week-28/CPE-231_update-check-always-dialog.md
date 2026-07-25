---
id: CPE-231
title: Manual "Check for Updates" should always show a dialog (checking / up-to-date / available / error)
type: Feature
status: Done
priority: High
component: Frontend
estimate: 1h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

When the user picks Application → Check for Updates and they are already on the
latest version, the app only flashes a transient status-bar note ("You're up to
date.") for 5s. Users read that as "nothing happened / it's broken", because they
expect a dialog. Meanwhile the upgrade dialog (current → new + Install) only
appears when a newer release exists, which never happens when checking from the
newest build.

Make a **manual** check always open the update dialog with the right state:
- **checking** — "Checking for updates…" with an indeterminate bar.
- **available** — new version, current version, What's New, Install & Restart /
  Later (existing behaviour).
- **uptodate** — "You're on the latest version — X." + Close.
- **error** — friendly message + Close / Try Again.

The silent **startup** check is unchanged: quiet when up to date, prompt only
when an update is found (no dialog spam on every launch).

## Acceptance Criteria

- [ ] Manual Check for Updates always shows a dialog; never just a status note.
- [ ] Up-to-date state names the current version.
- [ ] Available state still shows current + new version and installs on demand
      with progress, then relaunches.
- [ ] Error state (check or install failure) shows a message and a Try Again.
- [ ] Startup check stays silent unless an update is available.
- [ ] `npm run check` passes; `npm run build` compiles.

## Resolution

Reworked `UpdateDialog.svelte` into a 5-state modal (checking / available /
uptodate / downloading / error) with per-state title, body, and actions. In
`App.svelte`, `checkForUpdates(manual)` now opens the dialog immediately in
"checking" for a manual check, then resolves to "available" (current + new
version + notes + Install & Restart / Later), "uptodate" (names the current
version + Close), or "error" (message + Try Again). Added `retryUpdate()` (retry
the install if an update is pending, else re-check). The silent startup check is
unchanged — it stays quiet and only opens the dialog in "available".

Removed the old transient "You're up to date." / error status-bar notices for the
manual path, which were the source of the "nothing happened" confusion. Render
guard changed from `showUpdate && pendingUpdate` to `showUpdate` so the
no-update states can render.

Verified: `npm run check` → 0/0; `npm run build` compiles. The available-state
path will be exercised live by the 0.9.1 release (a running 0.9.0 checking will
find 0.9.1 and show current→new + Install).

## Work Log

2026-07-12 — User reported Check for Updates "not working" (no dialog). Diagnosed: running build was the latest, so check() found nothing and only flashed a 5s status note.
2026-07-12 — Made manual check always show the dialog (checking/available/uptodate/error); added retry; kept startup check silent. check + build clean. Closed.

## Notes

Builds directly on CPE-230 (UpdateDialog + updater flow). No backend/capability
change. This release (0.9.1) also serves to demonstrate the upgrade dialog: a
0.9.0 install checking will now find 0.9.1 and show current→new + Install.
