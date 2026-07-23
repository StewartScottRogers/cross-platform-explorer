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

## Work Log
2026-07-23 (dayshift) — **Activated.** First slice: **CPE-945** — `shell_menu::verbs_for`: the pure
applicability core deciding which registered context-menu verbs to show for a selection. Remaining: the
per-OS shell registration glue and the default-file-manager handshake.
