---
id: CPE-1280
title: "Live removable-drive detection: sidebar auto-updates on USB plug/unplug"
type: bug
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-716
---

## Summary
The sidebar Drives section is loaded once at startup (and again after an eject), so plugging in or
unplugging a USB drive does NOT update the left pane live — a new drive doesn't appear and a removed
one lingers until the app is relaunched (or a drive is ejected). Detect the connected-drive set
changing and refresh the sidebar automatically.

## Build
- A small always-on frontend watcher (`src/lib/driveWatch.ts`) that polls `list_drives` on a short
  interval and fires a callback ONLY when the drive SET actually changes (plug → appears, unplug →
  drops out). `list_drives` is cheap (probes drive-letter roots), and the callback only runs on a real
  transition, so idle cost is negligible — honours the fast/small/predictable tiebreaker. Also poke a
  check on window focus for instant feedback after alt-tabbing back.
- Diff is pure + injectable (`drivesSignature`/`drivesChanged`), mirroring `driveScheduler.ts`, with
  vitest coverage.
- App.svelte: extract the eject refresh into a shared `applyDriveList(d)` that reassigns `drives`,
  prunes usage/eject state for vanished drives, and (re)probes the current set; wire the watcher to it
  in onMount and tear it down in onDestroy.
- No backend change: reuses the existing `list_drives` command (cross-platform). POSIX reports a single
  `/` root that never changes, so the watcher is a no-op there (correct for this command — mount-level
  enumeration is a separate, larger CPE-616/CPE-716 concern).

## Acceptance criteria
- Plugging in a USB drive makes it appear in the sidebar Drives section within a few seconds, with its
  usage bar + eject affordance; unplugging removes it — no relaunch needed.
- Idle app does no meaningful extra work (poll only re-renders on an actual drive-set change).
- vitest covers the diff helpers; `npm run check` clean; existing tests green.

## Notes
Attended verify: with the app open, plug in a USB stick → it appears; safely eject / physically remove
it → it disappears. Part of epic CPE-716 (drive bay); complements CPE-1278 (eject) and CPE-797
(backup-on-connect scheduler, which stays opt-in and separate).

## Work Log
- 2026-08-03 — Live removable-drive detection. New `src/lib/driveWatch.ts`: always-on poller over
  `list_drives` (4s) that fires ONLY on a real drive-set change (pure `drivesSignature`/`drivesChanged`,
  order- + case-insensitive), plus `pokeDriveWatch` on window focus for instant feedback. App.svelte:
  extracted the eject refresh into a shared `applyDriveList(d)` (reassigns `drives`, prunes usage/eject
  state for vanished drives, re-probes the current set — no bar flicker for survivors); wired
  start/stop/poke in onMount/onDestroy. No backend change (reuses `list_drives`, cross-platform; POSIX's
  single `/` root makes it a no-op there). Docs: explorer page notes drives update live. 6 new vitest;
  full suite 1934 green; `npm run check` clean. Attended verify pending: plug/unplug a USB with the app
  open. Epic CPE-716.
