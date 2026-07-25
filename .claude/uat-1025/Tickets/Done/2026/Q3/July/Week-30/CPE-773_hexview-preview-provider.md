---
id: CPE-773
title: HexView preview provider — virtualized hex grid + data inspector + signature badge
type: feature
status: Done
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-19
closed: 2026-07-20
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
- [x] A large binary opens in a paged hex+ASCII view that doesn't load the whole file; scroll pages ranges.
- [x] Selecting a byte shows its int/float/string/timestamp decodings (both endiannesses); signature badge
      shows for known formats.
- [x] Read-only-safe; `npm run check` + suite green; GUI-verified.

## Notes
Prereq: CPE-770, CPE-771, CPE-772. Attended GUI. Structure templates = future follow-up.

## Resolution
Built `src/lib/components/HexView.svelte` over the tested pure helpers + `read_file_range` (CPE-772): a
paged offset/hex/ASCII grid (`hexRows`, CPE-770 — 1 KB/page, so a large file never loads whole; prev/next
page through byte ranges), a magic-byte **signature badge** (`detectSignature`), and a **data-inspector**
panel decoding the byte under the cursor (`inspect`, CPE-771) with an **LE/BE** toggle. Registered a new
`hex` preview kind + a catch-all provider (last in the registry) so any unrecognised/binary file — which
previously fell to the metadata "Details" view — now opens in hex; folders still fall through to the
metadata fallback. Wired into `PreviewPane`. Read-only.

**GUI-verified in the running dev app (CDP):** created a 2512-byte test file (PNG magic + `01 02 03 04` +
filler, extension `.xyz`) → selecting it opened the hex view: signature **"PNG image (.png)"**; first row
`00000000 89504E470D0A1A0A0102030441414141 .PNG........AAAA` (correct offset/hex/ASCII); clicking byte 0x08
(`0x01`) → inspector **int8=1, uint16=513 (0x0201 LE), uint32=67305985 (0x04030201 LE)**; **next page** →
range `0x400–0x800 / 2512b` (paged, whole file not loaded). Test file cleaned up. `npm run check` clean;
provider/hexdump/hexinspect suites green (48; provider test updated: unrecognised type → `hex`).

Structure-template overlays remain a future follow-up (per scope).
