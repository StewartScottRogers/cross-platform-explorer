---
id: CPE-771
title: Pure data-inspector decoders (int/float/string/timestamp, both endiannesses)
type: feature
status: Done
priority: low
component: Frontend
tags: ready
created: 2026-07-19
closed: 2026-07-20
epic: CPE-719
estimate: 1-2h
---

## Summary
The data-inspector logic for the hex inspector (epic CPE-719): a pure module (`src/lib/hexinspect.ts`)
that, given a byte buffer and an offset, decodes the bytes as the common scalar types in both
endiannesses. No DOM/IO — fully unit-tested — so the inspector panel (CPE-773/-774) is a thin render.

## Scope
- `inspect(bytes: Uint8Array, offset: number, littleEndian: boolean): InspectRow[]` returning, where enough
  bytes remain: int8/uint8, int16/uint16, int32/uint32, int64/uint64 (BigInt), float32, float64,
  a short ASCII string preview, and Unix (32-bit seconds) + Windows FILETIME timestamps as ISO strings.
- Gracefully omit rows that would read past the end of the buffer (no throwing).
- Endianness applied consistently; use `DataView` for the numeric reads.

## Acceptance Criteria
- [x] Each type decodes correctly for known byte patterns in both LE and BE (table-driven tests).
- [x] Reads past the buffer end are omitted, not errors; offset at the last byte still yields int8/uint8.
- [x] Timestamps convert to sensible ISO strings; pure + dependency-free; `npm run check` + suite green.

## Notes
Independent of CPE-770. Consumed by the inspector panel in CPE-773. Headless.

## Resolution
Added `src/lib/hexinspect.ts` (pure): `inspect(bytes, offset, littleEndian)` → int8/uint8, int16/uint16,
int32/uint32, int64/uint64 (BigInt), float32/float64, a 16-char ASCII string preview, a Unix-32 timestamp
and a Windows FILETIME (both via `DataView`, chosen endianness). Rows needing more bytes than remain are
omitted (no throw); out-of-range/non-integer offset → []. FILETIME uses BigInt division to avoid precision
loss; timestamps clamp out-of-range to "(out of range)". 11 table-driven tests (LE/BE, int8 sign, float32=1.0,
64-bit, string-stop-at-non-printable, epoch timestamps, past-end omission, subarray byteOffset). check 0/0;
suite green. Headless; no existing code touched.

