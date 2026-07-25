---
id: CPE-712
title: "EPIC: Shell citizen — OS context-menu & default file manager"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Register CPE into each OS's native shell so "Open in Cross-Platform Explorer" appears in Explorer / Finder /
Nautilus right-click menus, and let users set CPE as the system default file manager — with clean uninstall.

## Why
A file manager that other apps and the desktop can hand off to is a true system citizen. This is table
stakes for daily-driver use and makes CPE reachable from everywhere the OS offers a "reveal in..." action.

## Rough scope (areas, not child tickets)
- Windows: shell verbs + registry entries; optional default-handler registration.
- macOS: Services / `LSHandlers` integration.
- Linux: `.desktop` entry + `xdg-mime` default association.
- An in-app "Set as default / add to context menu" toggle with a clean, complete uninstall path.

## Open questions (resolve at activation)
- Privilege requirements per OS (registry/HKLM, `xdg` scope) and how to elevate cleanly.
- Reversibility guarantees — never leave stale shell entries behind.
- Which entries: folder background, on-folder, on-file, drive — and their verbs.

## Definition of Done
- "Open in CPE" appears in the native context menu on each OS after opt-in.
- Users can set CPE as the default file manager where the OS allows it.
- Disabling the integration removes every registered entry with no residue.

## Decisions (2026-07-24, PM take-on — user off-shift, calls made + logged)
- **Privilege:** register under **user scope** everywhere — Windows `HKCU\Software\Classes` (no UAC),
  Linux `~/.local/share/applications` (no root), macOS per-user Services. Avoids elevation entirely; a
  system-wide/default-handler variant can be a later opt-in.
- **Reversibility:** each per-OS plan is emitted as **data** (install set + remove set) with a unit-tested
  invariant that every installed key/file is in the remove set — so "no residue" is provable, not hoped.
- **Entries to register (first wave):** on-folder, folder-background, and on-drive "Open in CPE" verbs
  (the file-explorer essentials). On-file and the full default-file-manager handshake are deferred to a
  later slice.
- **Sequencing:** pure per-OS *plan* models first (headless, fully testable), then the apply glue, then the
  Settings toggle — mirroring how CPE-945 landed the applicability core before any OS I/O.

## Child tickets (first wave)
- **CPE-945** — shell-menu applicability model (pure). ✅ Done.
- **CPE-1019** — Windows shell-registration plan (pure model). ✅ Done.
- **CPE-1020** — Apply/remove the Windows registration (HKCU glue). ✅ Done.
- **CPE-1021** — Linux `.desktop` + xdg-mime registration plan (pure). Parallelisable.
- **CPE-1022** — macOS Services/LSHandlers registration plan (pure). Parallelisable.
- **CPE-1023** — In-app "Shell integration" Settings toggle. ✅ Done (implementation + headless verify;
  visual GUI verify via a release build recommended as the epic's closing check).

## Work Log
2026-07-23 (dayshift) — **Activated.** First slice: **CPE-945** — `shell_menu::verbs_for`: the pure
applicability core deciding which registered context-menu verbs to show for a selection. Remaining: the
per-OS shell registration glue and the default-file-manager handshake.
2026-07-24 (PM take-on) — Decomposed the first real wave (CPE-1019–1023) per the Decisions above; user is
off-shift so the product calls were made and logged rather than asked. Building CPE-1019 next.
2026-07-25 — **CPE-1019 + CPE-1020 done.** The Windows path is now functional end-to-end at the backend:
the pure plan (1019) + the HKCU apply/remove glue (1020), exposed as `install_shell_integration` /
`uninstall_shell_integration` / `shell_integration_installed` commands. UAT-verified against the real
registry (install → verbs present via `reg query`; uninstall → no residue). Remaining: CPE-1021/1022
(Linux/macOS plans), CPE-1023 (Settings toggle + GUI verify).
