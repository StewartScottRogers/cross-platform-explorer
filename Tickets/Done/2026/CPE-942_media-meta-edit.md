---
id: CPE-942
title: Editable media-metadata model (EXIF/IPTC/ID3 set/clear policy)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-725
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
First headless slice of the media metadata studio (CPE-725). `cpe_server::media_meta_edit`:
- `MetaField { group, key, value, editable }` (EXIF/IPTC/ID3) + `MetaEdit { Set, Clear }`.
- `apply_edits(fields, edits) -> EditResult { fields, applied, rejected }` — the pure edit policy: Set
  updates an existing editable field or adds a new one; Clear removes an editable field; edits against a
  **read-only** field (camera intrinsics like dimensions) or malformed edits are skipped and recorded in
  `rejected` while the rest still apply. Case-insensitive group/key match. `validate_edit` guards shape.

The codec layer reads real EXIF/IPTC/ID3 in and writes the result back; this owns the edit *policy*.

## Acceptance Criteria
- [x] Set updates/adds editable fields; Clear removes them; read-only fields refused (recorded, not changed).
- [x] Deterministic; case-insensitive match; malformed edits rejected. 6 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Activated CPE-725 with the edit-apply policy core. The per-format EXIF/IPTC/ID3
  read/write codecs and the studio editor UI are the remaining children.
