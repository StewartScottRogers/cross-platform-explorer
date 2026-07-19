---
id: CPE-733
title: "EPIC: Session audit log & export"
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
