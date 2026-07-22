---
id: CPE-614
title: "EPIC: Tags, labels & smart folders"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed: 2026-07-18
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

## Decisions (2026-07-18, activated in dayshift — best-guess, user away)
- **Persistence:** an app-side JSON store (`tags.json` in the app config dir, like settings) mapping
  path → { tags: [..], label: "" }. The filesystem is never touched. **Path-keyed** for v1 — a move or
  rename *outside* the app orphans the entry; a re-link tool is a future follow-up (logged, not v1).
- **Colour labels:** a small fixed palette (none/red/orange/yellow/green/blue/purple/grey); one label
  per entry; tints the row.
- **Tags UI:** assign/remove via context menu + a tag editor popover; chips on tagged rows (reflowing
  tick-tack convention). A "Tags" sidebar section lists every tag with a count; clicking filters the
  current view to that tag.
- **Smart folders:** v1 = filter the current view by a single tag (reuses the filter pipeline). A full
  saved-query language is deferred to a follow-up child once tags exist.
- **OS-native tags:** no — app-local (Windows has none).

## Child tickets (created just-in-time as worked)
1. CPE-635 — Backend tag store: persisted JSON + pure core (get/set/all/rename-path) + commands, cargo-tested.
2. CPE-636 — Frontend tag store/service consuming the commands (pure + tested).
3. CPE-637 — Assign/remove tags + colour label via context menu + a tag editor popover.
4. CPE-638 — Tag chips on entries + row label tint.
5. CPE-639 — "Tags" sidebar section (all tags + counts) → filter the view by a tag.
6. CPE-640 — Import/export the tag store.
7. CPE-641 — Docs + i18n for the tags feature.

## Resolution (closed 2026-07-18)
Tags shipped across 15 children (store/service, assign menu, chips, sidebar filter, import/export, bulk +
batch tagging, filter helpers/indicator, tags-follow-move/rename), and the final DoD gate — "at least one
smart-folder query type works end-to-end and updates as files change" — landed as CPE-667 (a saved,
self-updating tag query surfaced as a virtual folder). DoD met: tag/label + chips + view/sidebar filter;
one smart-folder query type live and reactive; tag store persists/exports with a moved/renamed story;
zero weight when nothing tagged and no smart folder open; core logic headlessly tested. No carve-outs.
