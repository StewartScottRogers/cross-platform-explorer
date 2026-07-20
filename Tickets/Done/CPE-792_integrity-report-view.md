---
id: CPE-792
title: Integrity report view (acknowledge / rebaseline)
type: feature
status: Done
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-737
estimate: 2-3h
---

## Summary
The alerts/report UI for epic CPE-737: show the CPE-790 report ("N files changed unexpectedly", with
corrupted/missing highlighted vs. legitimate edits), and let the user acknowledge changes or rebaseline.

## Acceptance Criteria
- [x] Report lists corrupted/missing prominently, edited/new secondarily; clear counts.
- [x] Acknowledge and rebaseline actions update the stored baseline; menus follow MENUS.md.
- [x] check + suite green; GUI-verified.

## Notes
Prereq: CPE-790, CPE-791. Attended GUI.

## Resolution
Built `src/lib/components/IntegrityDialog.svelte` over the tested integrity model + backend:
- **Baseline** a folder → `checksum_folder` → stored as a per-folder manifest (settings
  `cpe.integrityBaselines`, keyed by path, tolerant load via `parseManifest`; App owns the store).
- **Verify** → re-scan (`checksum_folder`) → `verifyManifest` (CPE-790) → grouped report. Corrupted +
  missing shown **prominently** (red, alarm-tinted counts bar); edited + new secondary; an "all intact"
  line when clean. Clear per-status counts.
- **Rebaseline (accept current)** re-scans and stores the current state as the new baseline (the
  acknowledge/rebaseline action). Baselines persist in settings.
- Opened from the command palette ("Integrity check…"); `palette.integrity` added to all 12 locales.

**GUI-verified in the running dev app (CDP):** baselined a 3-file test folder → Verify showed all intact →
then mutated on disk: `a.txt` content changed with its **mtime restored** (silent corruption), `b.txt`
content+mtime changed (normal edit), `c.txt` deleted, `d.txt` added → Verify correctly classified
**corrupted 1 / missing 1 / edited 1 / new 1 / intact 0** with the alarm styling — the flagship **bitrot
heuristic** (corruption vs. edit by mtime) proven end-to-end → Rebaseline accepted the current state (Verify
then all-intact). Test folder + the seeded baseline were cleaned up afterward. `npm run check` clean; suite
green.

Tradeoff: baselines live in `settings.json` keyed by folder path — fine for the opt-in, modest folders this
targets; a dedicated on-disk manifest store (like the audit journal) is a future option for very large trees.
