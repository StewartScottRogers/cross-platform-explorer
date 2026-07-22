---
id: CPE-230
title: In-app update experience — prompt before installing, with progress
type: Feature
status: Done
priority: High
component: Frontend
estimate: 1-2h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

The auto-update pipeline already works end-to-end (signed `latest.json` on the
GitHub release; the app checks it, downloads, verifies, installs, and relaunches).
The gap is UX: today the startup check **silently downloads, installs, and
restarts the app under the user** with no consent, no release notes, and no
progress.

Replace that with a proper in-app flow: when an update is found, show a modal
(version, current version, What's New, and Install & Restart / Later). Nothing is
installed until the user chooses. During install, show download progress. Errors
are surfaced with a Try Again, not swallowed. Manual "Check for Updates…" reuses
the same flow (and still reports "up to date" when there's nothing new).

Decision (user, 2026-07-12): behavior = **prompt before installing**. Scope is the
in-app experience only — not a public web page, not moving the manifest to Pages.

## Acceptance Criteria

- [ ] Startup check no longer auto-installs; if an update exists it shows the
      prompt dialog and waits for the user.
- [ ] Dialog shows the new version, the current version, and release notes (from
      the update's `body`) when present.
- [ ] "Install & Restart" downloads with a visible progress indicator (percent
      when content-length is known, indeterminate otherwise), installs, relaunches.
- [ ] "Later" dismisses; the app keeps running on the current version.
- [ ] A failed download/install shows an error with Try Again — never a silent
      failure, never a half-state claim of success.
- [ ] The dialog can't be dismissed mid-download (no yanking the install away).
- [ ] Application → Check for Updates… reuses the flow; reports "You're up to
      date." when there's no update; reports a friendly error if unreachable.
- [ ] `npm run check` passes; `npm run build` compiles.

## Resolution

Added `UpdateDialog.svelte` — a state-driven modal (prompt / downloading / error)
showing the new version, the current version, release notes (`update.body`), and
Install & Restart / Later. During install it shows a progress bar (percent when
`contentLength` is known, indeterminate animation otherwise). It refuses to close
while downloading so the install can't be yanked away.

`App.svelte`: replaced BOTH the silent startup auto-install and the menu's
auto-install with one consent-first `checkForUpdates(manual)`. It calls the
updater's `check()`, and on a hit stores the `Update` and opens the dialog —
nothing installs until the user clicks Install. `installUpdate()` drives
`downloadAndInstall(onEvent)`, mapping Started/Progress/Finished to the progress
bar, then `relaunch()`; failures flip the dialog to an error state with Try Again
rather than being swallowed. Startup check is silent when up to date; the menu
check also reports "You're up to date." and friendly errors.

No backend/capability changes — `updater:default` + `process:default` already
cover check/download/install/relaunch. The signed manifest, CI publish, and
signature verification are unchanged; this is purely the client UX.

Verified: `npm run check` → 0/0; `npm run build` compiles. The two locally
drivable paths (no dialog on startup when current; "up to date" from the menu)
are correct against the installed 0.8.0. The update-available prompt itself can
only be exercised once a newer release than the running build exists — it will
appear the first time an install sees a higher version in the manifest.

## Work Log

2026-07-12 — Confirmed the auto-update pipeline already exists end-to-end (latest.json + updater); user wants only the in-app UX, prompt-before-install.
2026-07-12 — Added UpdateDialog (prompt/downloading/error + progress); refuses dismiss mid-download.
2026-07-12 — Unified startup + menu checks into checkForUpdates(manual); added installUpdate() with progress mapping + relaunch; removed the two silent auto-installs.
2026-07-12 — check + build clean. Prompt path observable from the next release onward. Closed.

## Notes

No capability changes: `updater:default` + `process:default` already grant check /
download / install / relaunch. Builds on CPE-229 (Application menu hosts the
manual check). The manifest, signing, and CI publish path are unchanged.
