---
id: CPE-1486
title: "EPIC: Browsable in-app Trash bin — list / restore / empty"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (workshift PM, from the superfile reference pass — see [[superfile-pm-reference]]).**
> **Dormant brief — not decomposed until activated** via `/ticketing-epic activate CPE-1486`.

## Why
superfile exposes the trash as a browsable location. CPE already **routes deletes to the OS trash** (the
`trash` crate is a dependency and deletes are recoverable), but there is no in-app way to **browse, restore,
or empty** trash — users have to drop out to Explorer/Finder/Nautilus. Surfacing it in-app is a consistency
play that keeps the whole delete→recover loop inside CPE.

## Goal
A navigable **Trash** location (sidebar entry) that lists trashed items with their original path + deletion
time, and supports **restore** (back to origin) and **empty** (permanent), cross-platform.

## Rough slices (decompose just-in-time when activated)
- Backend command layer over the `trash` crate: `list_trash` (items + original path + timestamp), `restore`,
  `empty_trash` — bounded/streamed listing per STREAMING.md; skip-on-error like `list_dir`.
- A **Trash** sidebar entry + a listing view (reuse the file-list surface where possible) with Restore / Empty
  actions (menus per MENUS.md; destructive text stays `var(--text)`, not red).
- Cross-platform behavior (Windows Recycle Bin / macOS Trash / Linux XDG trash) — verify what the `trash`
  crate exposes for *listing* (its API may be delete-only; if listing isn't supported cross-platform, scope
  a per-OS reader or descope the platforms it can't do and note it).

## Notes
Lower value than CPE-1484/1485 since most OSes already expose trash well — hence Low priority. Confirm the
`trash` crate can enumerate/restore (not just delete) before committing scope; if not, this may need a small
per-OS trash reader. Ship docs per CPE-579.
