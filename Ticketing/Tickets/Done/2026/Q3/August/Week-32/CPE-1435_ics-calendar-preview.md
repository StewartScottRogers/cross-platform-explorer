---
id: CPE-1435
title: ".ics iCalendar structured preview (events: what/when/where/who)"
type: Feature
status: Backlog
priority: Medium
component: Full-stack
tags: [ready]
epic: CPE-1433
created: 2026-08-07
---
## Scope
Structured preview for `.ics` (RFC 5545 iCalendar) files. Same template as CPE-1434 (crypto-viewer shape).

**Backend** — new `crates/server/src/ical_preview.rs`: `ical_preview(bytes: &[u8]) -> IcalPreview` (specta::Type),
ZERO new deps (hand-rolled). Handle RFC 5545 **line unfolding** (a leading space/tab continues the previous
line), split components (VEVENT / VTODO / VJOURNAL), and decode the useful properties per event: SUMMARY,
DTSTART, DTEND (humanized), LOCATION, DESCRIPTION, ORGANIZER, ATTENDEE (list), STATUS, RRULE (as a readable
recurrence summary if feasible, else raw). Return a list of events. Property-parameter handling (`;TZID=`,
`;VALUE=DATE`) at least tolerated. Malformed → graceful partial / `Err`, never panic.

**Command + frontend + tests + docs** — exactly mirror CPE-1434: thin `ical_preview` command (spawn_blocking,
size-guarded, registered + bindings regen), a `calendar` provider kind for `.ics` before text, a loader,
`IcalPreview.svelte` (event card(s): summary heading, when/where, attendee pill row that reflows, recurrence
note). cargo tests (a hand-built VEVENT with folded lines + multiple attendees + an all-day DATE value; a
malformed calendar → graceful). Provider-selection + jsdom render specs. Extend
`src/docs/30-structured-previews.md` (same slug, no new sectionDocs entry needed — but keep the guard green).
Add a small sample `.ics` under `samples/`.

## Acceptance
- Opening a `.ics` shows event card(s) with what/when/where/who; malformed degrades to text/hex (no panic).
- Zero new deps; backend pure + cargo-tested; provider + render specs pass; bindings regen; check + cargo test +
  vitest green.

## Notes
Second child of epic CPE-1433. Build AFTER CPE-1434 merges — shares provider.ts / PreviewPane import block /
`generate_handler!` list with 1434/1436, so serialize to avoid merge conflicts. Rebase onto 1434's wiring.
