---
id: CPE-770
title: Pure hex-dump formatting + magic-byte signature detection
type: feature
status: In Progress
priority: low
component: Frontend
tags: ready
created: 2026-07-19
closed:
epic: CPE-719
estimate: 1-2h
---

## Summary
Foundation for the hex/binary inspector (epic CPE-719). A pure, dependency-free module
(`src/lib/hexdump.ts`) that turns a byte range into the canonical hex-dump rows and identifies common
file formats by their magic bytes — no DOM, no IO, fully unit-tested, so the later HexView component
(CPE-773) is a thin render over verified logic.

## Scope
- `hexRows(bytes: Uint8Array, baseOffset = 0, bytesPerRow = 16): HexRow[]` where each row has the absolute
  `offset` (hex-formatted, zero-padded), the `hex` cells (two-digit, uppercase, space-grouped; padded to
  a full row so columns align on a short final row), and the `ascii` gutter (printable 0x20–0x7E as the
  glyph, everything else as `.`).
- `detectSignature(bytes: Uint8Array): { name: string; ext: string } | null` — magic-byte detection for a
  starter set: PNG, JPEG, GIF, PDF, ZIP, GZIP, ELF, Windows PE (`MZ`), WASM, class file, RIFF/WAV. Returns
  null when nothing matches.
- Helpers kept pure and total (empty input, partial final row, offsets past 0 all handled).

## Acceptance Criteria
- [x] `hexRows` produces correctly offset/padded/grouped rows incl. a short final row and a non-zero
      `baseOffset`; ASCII gutter maps printable vs non-printable correctly.
- [x] `detectSignature` identifies each format in the starter set and returns null for unknown/short input.
- [x] Pure + dependency-free; unit tests cover the above incl. edge cases (empty, 1 byte, exactly one row,
      row+1); `npm run check` + suite green.

## Notes
Foundation for CPE-773 (HexView). Sibling of CPE-771 (data-inspector decoders). No GUI — headless.

## Resolution
Added `src/lib/hexdump.ts` (pure, no DOM/IO): `hexRows(bytes, baseOffset, bytesPerRow)` → canonical
offset/hex/ascii rows (short final row padded so columns align; non-printable → `.`), and
`detectSignature(bytes)` → magic-byte match for PNG/JPEG/GIF/PDF/ZIP/GZIP/ELF/WASM/Java-class/WAV/PE (null
otherwise). 17 unit tests (`hexdump.test.ts`) cover empty/1-byte/full-row/short-row/baseOffset/bogus-per-row
and every signature + the WAV-needs-WAVE-at-8 and too-short cases. `npm run check` 0/0; suite 765 pass.
Foundation for CPE-773 (HexView). No existing code touched — headless.

