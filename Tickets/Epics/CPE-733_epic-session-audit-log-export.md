---
id: CPE-733
title: "EPIC: Session audit log & export"
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
A durable, exportable, per-session record of everything the agent did to the filesystem — ordered,
timestamped, session-attributed, including reads and (with diff-peek) content — that survives app restart
and exports to JSON / Markdown / CSV for compliance, review, or sharing.

## Why
The timeline is transient and capped (~300 events) today. Making it a permanent, browsable, exportable
artifact turns Agent Watch into a system of record for what agents did to a codebase.

## Rough scope (areas, not child tickets)
- On-disk append-only event journaling per session.
- A session-history browser (past sessions, not just the live one).
- Export to JSON / Markdown / CSV with filtering.
- Foundation shared with replay ([[CPE-728]]) — the persisted ordered event log.

## Open questions (resolve at activation)
- Journal format/location and retention/rotation policy.
- Overlap/sharing with the replay event log and diff snapshots.
- Privacy/redaction for exported content (secrets in diffs).

## Definition of Done
- Every session's filesystem activity is journaled durably and survives restart.
- Past sessions can be browsed and exported to JSON/Markdown/CSV with filtering.
- With Agent Watch off, no journaling occurs.

## Work Log
2026-07-20 (autonomous) — Activated. Open questions resolved: journal = append-only **JSON-lines** per
session on disk (backend child); export foundation is **pure** (events → JSON/CSV/Markdown + filtering);
shares the event model with replay (CPE-728); redaction of exported content deferred to the export UI.
Pure export/filter lands first.

## Child tickets
1. **CPE-799** — Pure audit export + filter (`src/lib/auditExport.ts`): `AuditEvent` model,
   `filterEvents(events, opts)`, and `toJson`/`toCsv`/`toMarkdown` with correct escaping. Unit-tested.
   **Foundation, headless.**
2. **CPE-800** — On-disk append-only per-session journal (survives restart) + read-back. **Backend.**
   *(prereq: 799 model)*
3. **CPE-801** — Session-history browser + filtered export UI. **GUI.** *(prereq: 799, 800)*
