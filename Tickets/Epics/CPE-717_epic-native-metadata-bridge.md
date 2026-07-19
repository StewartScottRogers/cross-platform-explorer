---
id: CPE-717
title: "EPIC: Native metadata bridge — Finder tags, NTFS streams, xattrs"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Read and write OS-native file metadata — macOS Finder tags & comments, NTFS alternate data streams, Linux
extended attributes — and reconcile them with CPE's internal tag store so labels survive outside the app.

## Why
CPE's tags live in its own store today; a file tagged in CPE looks untagged in Finder/Explorer and vice
versa. Bridging to native metadata makes labels portable and interoperable with the rest of the OS.

## Rough scope (areas, not child tickets)
- Per-OS read/write of native metadata (Finder tags/comments, NTFS ADS, Linux xattrs).
- A reconciliation layer mapping native tags <-> CPE's internal tags (two-way sync, conflict policy).
- Surfacing native comments/attributes in Properties + as columns.
- Opt-in so users who don't want native writes keep the internal-only store.

## Open questions (resolve at activation)
- Sync direction/authority and conflict resolution between native and internal tags.
- Filesystem support gaps (FAT/exFAT lack xattrs/ADS) and graceful degradation.
- Performance of reading native metadata across large listings.

## Definition of Done
- Tags set in CPE appear in the OS's native metadata and vice-versa, per the chosen sync policy.
- Native comments/attributes are visible in Properties and available as columns.
- With the bridge off, tagging behaves exactly as today (internal store only).
