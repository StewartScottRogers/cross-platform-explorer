---
id: CPE-614
title: "EPIC: Tags, labels & smart folders"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal

Add an **organizational metadata layer** on top of the filesystem: user-defined **tags** and **colour
labels** on files/folders, a tag sidebar to filter by them, and **smart folders** — saved virtual
views defined by a query (tag / name-glob / type / size / date) that update live. Lets users organise
across directory boundaries without moving anything.

## Why

The explorer is excellent at *where* files are but has nothing for *what they are about*. Finder-style
colour labels and tags are among the most-requested organisational features, and smart folders ("all
my invoices", "large videos over 1 GB", "everything tagged #urgent") turn the app from a browser into a
lightweight organiser — a clear differentiator that stays additive (off = plain explorer).

## Rough scope (areas, not child tickets)

- A tag/label store: metadata keyed to files, persisted app-side (the filesystem isn't touched).
- Assign/remove tags + a colour label via context menu, properties, and the command palette; a small
  coloured chip on tagged rows (reuse the reflowing tick-tack convention).
- A "Tags" section in the sidebar; clicking a tag filters/collects matching files (a virtual listing).
- Smart folders: a saved query (tag AND/OR name-glob/type/size/date range) shown as a pinned sidebar
  entry that lists live matches; reuses the existing find-by-name / filter primitives.
- Import/export of the tag store so it's portable and backup-friendly.

## Open questions (resolve at activation)

- **Persistence & fragility (the hard one):** tags are path-keyed, so a move/rename outside the app
  orphans them. Track by a stable id? Reconcile on access? Accept path-keying with a re-link tool?
- Where do smart-folder queries physically live in the UI (sidebar entries vs a saved-search view)?
- Do tags/labels sync with OS-native tags (macOS Finder tags, Windows has none) or stay app-local?
- Scope of the query language for smart folders (keep it to the existing filter primitives for v1).

## Definition of Done

- A user can tag/label files, see the chips, and filter the current view (and the sidebar) by tag.
- At least one smart-folder query type works end-to-end and updates as files change.
- The tag store survives restart, is exportable, and has a documented story for moved/renamed files.
- Zero weight on the plain explorer when nothing is tagged and no smart folder is open.
- Core query/matching logic is headlessly tested.
