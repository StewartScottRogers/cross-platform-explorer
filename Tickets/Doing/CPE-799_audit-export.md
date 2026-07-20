---
id: CPE-799
title: Pure audit-log export (JSON / CSV / Markdown) + filter
type: feature
status: In Progress
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed:
epic: CPE-733
estimate: 1-2h
---

## Summary
Foundation for the session audit log (epic CPE-733). A pure module (`src/lib/auditExport.ts`) that filters
a list of audit events and renders them to JSON / CSV / Markdown with correct escaping — so the history
browser + export UI (CPE-801) is a thin render.

## Scope
- `AuditEvent { ts, session, kind, path, detail? }`.
- `filterEvents(events, { kinds?, since?, until?, pathIncludes? })` → filtered list.
- `toJson(events)` (pretty), `toCsv(events)` (RFC-ish: quote fields with comma/quote/newline, double
  embedded quotes; header row), `toMarkdown(events)` (pipe-escaped table).
- Pure + total (empty list, special chars, missing detail).

## Acceptance Criteria
- [x] `filterEvents` honours kinds/time-range/path filters (and combinations).
- [x] CSV escapes comma/quote/newline correctly; Markdown escapes pipes; JSON round-trips.
- [x] Pure + dependency-free; unit tests cover filtering + all three formats incl. escaping; check + suite green.

## Notes
Shares the event model with replay (CPE-728). Foundation for CPE-800/801. Headless.

## Resolution
Added `src/lib/auditExport.ts` (pure): `AuditEvent` model, `filterEvents(events, {kinds?, since?, until?,
pathIncludes?})`, and `toJson`/`toCsv`/`toMarkdown` with correct escaping (CSV RFC-4180 quoting, Markdown
pipe/newline escaping). 5 tests incl. escaping + empty-list. check 0/0. Headless. Foundation for CPE-800/801.

