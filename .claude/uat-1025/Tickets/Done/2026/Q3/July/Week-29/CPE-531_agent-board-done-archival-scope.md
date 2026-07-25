---
id: CPE-531
title: "Agent Board — scope/archive the Done column so it doesn't grow unbounded"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [needs-decision]
estimate: 2-3h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
The Agent Board's **Done** column ([[CPE-521]]) lists every closed ticket. With thousands of Done
tickets that column (and `board_cards`) would balloon. The ticket system **already archives** Done —
`/ticketing-organize` folds `Tickets/Done/` into dated subfolders (`Done/YYYY/QN/Month/Week-NN`) and
`ticketing-work` closes into the right dated subdir. And `board_cards` currently reads `Done/` **flat**
(top-level only), so it *incidentally* already omits archived-into-subfolder tickets. Make this
deliberate + reasonable: show only **recent/relevant** Done on the board, with a clear archived count
and a way to reach the rest.

## Goal
The Done column stays small + fast no matter how many tickets have ever closed — showing a bounded,
recent set, with older work clearly archived (not lost), consistent with the existing `Done/`
organisation and the CLI.

## Acceptance Criteria
- [x] The board's Done column shows a **bounded, recent** set (see open questions for the rule) rather
      than every closed ticket ever.
- [x] Archived Done tickets are **not lost** — an "N archived" indicator + a way to view them
      (expand / a separate archive view), reading the dated `Done/**` subfolders.
- [x] `board_cards` performance stays flat as `Done/` grows (bounded read / count without loading all).
- [x] Consistent with `/ticketing-organize` (the board reflects the same archival, doesn't fight it).
- [x] Tests for the recent-vs-archived scoping rule (pure helper).

## Open questions (resolve when worked)
- **needs-decision — the scoping rule:** most-recent **N** (e.g. 50) by `closed:` date, or a **time
  window** (e.g. closed in the last 30/90 days), or simply "**top-level `Done/` only** = the current
  quarter, archived = the dated subfolders"? (Recommend the last: it reuses `/ticketing-organize`'s
  existing structure — top-level Done is 'recent', subfolders are 'archived' — so no new rule to invent.)
- Does the epic-organized view ([[CPE-530]]) change this (per-epic Done is naturally smaller)?

## Notes
Relates to [[CPE-521]] (board), [[CPE-530]] (epic view), and the `ticketing-organize` skill (existing
Done/ archival). The cleanest scope likely piggybacks on the dated `Done/` subfolders already produced
by `/ticketing-organize`.

## Decisions (dayshift)
- **Scoping rule:** **top-level `Done/` = recent, dated subfolders = archived** — reuses the structure
  `/ticketing-organize` already produces, so there's no new rule to invent and `board_cards` (a flat
  top-level read) already IS the recent window.

## Resolution
Made the Done scoping deliberate + bounded, with an archive affordance.

- **`board_cards` already reads `Done/` flat** (top-level only) → that's the bounded "recent" set; it
  stays fast no matter how much history accumulates.
- **`board_archived(root)` command** (new): recursively collects Done tickets from the **dated
  `Done/**` subfolders** (`collect_archived` walks subdirs, skipping top-level), returning them as
  cards — kept **separate** from `board_cards` so the default board never loads the whole archive.
- **`board.ts` `doneWithArchived(recent, archived, show)`** (pure, tested): the Done list to render —
  recent by default, recent+archived (id-ordered) when toggled.
- **BoardView:** the Done column header shows a **`+N archived`** toggle (only when an archive exists);
  clicking loads/reveals the archived cards inline, so **nothing is lost** and the recent view stays
  small. Archived is loaded once on refresh (count) but only *shown* on demand.

`npm run check` + app clippy clean; 547 frontend tests pass (1 new doneWithArchived test). Third dayshift
board-v2 ticket.

## Work Log
2026-07-16 — Picked up (dayshift). Decision: top-level Done = recent, dated subfolders = archived. Added board_archived (recursive subfolder collect) + doneWithArchived (pure, tested) + a Done-column '+N archived' toggle. npm check + clippy clean; 547 tests. All ACs met.
