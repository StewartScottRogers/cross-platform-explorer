---
id: CPE-773
title: HexView preview provider — virtualized hex grid + data inspector + signature badge
type: feature
status: Open
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-19
closed:
epic: CPE-719
estimate: 3-4h
---

## Summary
The user-facing hex/binary inspector (epic CPE-719): a preview-pane mode showing a virtualized
offset/hex/ASCII grid over `read_file_range` (CPE-772), a data-inspector panel decoding the byte under
the cursor (CPE-771), and a magic-byte signature badge (CPE-770). Read-only v1.

## Scope
- A `HexView.svelte` preview component: virtualized rows (reuse the CPE-690 windowing approach) paging via
  `read_file_range`; offset/hex/ASCII columns from `hexRows` (CPE-770); click/hover a byte to drive the
  inspector panel (CPE-771); a signature badge from `detectSignature`.
- Register as a preview provider so binary/unknown files (and an explicit "View as hex") open it.
- Read-only; structure-template overlays deferred to a follow-up.

## Acceptance Criteria
- [ ] A large binary opens in a paged hex+ASCII view that doesn't load the whole file; scroll pages ranges.
- [ ] Selecting a byte shows its int/float/string/timestamp decodings (both endiannesses); signature badge
      shows for known formats.
- [ ] Read-only-safe; `npm run check` + suite green; GUI-verified.

## Notes
Prereq: CPE-770, CPE-771, CPE-772. Attended GUI. Structure templates = future follow-up.
