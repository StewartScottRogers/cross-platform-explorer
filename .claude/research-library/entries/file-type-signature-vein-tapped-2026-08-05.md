---
title: "Is there clean headless file-type/format-signature work left after CPE-1341–1344?"
date: 2026-08-05
tags: [frontier, headless, file-type, magic-bytes, cpe-1000, cpe-1341, cpe-1342, cpe-1343, cpe-1344, ole2, tar, ftyp, tapped, gold-plating]
status: current
---

## Question
After the 2026-08-05 sprint shipped four file-type tickets, is there more *clean, headless,
cargo-testable, no-user-resource, no-heavy-dep* work in the file-type/format-signature vein
(`crates/server/src/file_type.rs` + its consumers), or is it tapped?

## Finding (evidence-based scan + 4 shipped tickets, 2026-08-05)
The signature vein is now **essentially TAPPED** — after these four it's down to gold-plating.
An independent frontier-scan Researcher (read the panic-safety batteries, glob.rs, type_class.rs,
type_mismatch_scan.rs, and every consumer of `file_type::{detect_type,mismatch}`) found exactly
**two** real items, both now shipped, plus a third it honestly flagged as make-work.

**Shipped this shift (all merged, gauntlet + CI green, 0 escaped defects):**
- **CPE-1341** (#637) — `detect_type` now reads the ISO-BMFF `ftyp` **major brand** (offset 8) so real
  `.mov`/`.heic`/`.avif`/`.3gp` are detected as their own type instead of being **false-flagged as MP4
  mismatches** (a live bug: every ftyp file mapped to `Mp4`, exts mp4/m4a/m4v only). Unknown brands still
  fall back to `Mp4`. New variants: `Mov`, `Heic`, `Avif`, `ThreeGpp`.
- **CPE-1342** (#637) — 11 new magic signatures: `Tar` (`ustar`@257), `Psd`, `Cab`, `Icns`, `Ar` (+.deb),
  `Aiff` (FORM+AIFF/AIFC@8), `Midi`, `Flv`, `Cur`, `Lz4`, `Lzip`.
- **CPE-1343** (#638) — **bug introduced by CPE-1342**: `type_mismatch_scan.rs` capped its header read at
  **64 bytes**, but TAR's magic is at **offset 257**, so the File-Health tree-sweep could never flag a
  disguised `.tar` (the per-row column path reads 1 MiB and *did*). Raised `HEADER_CAP` to **512**.
  → **Lesson: when you add an offset-based signature to `file_type.rs`, check `type_mismatch_scan.rs`
  HEADER_CAP (and `column_cells.rs`) can actually read that deep.** TAR@257 is the deepest offset in the
  detector; 512 covers it with margin.
- **CPE-1344** (#639) — added the **OLE2/CFBF** container (`D0 CF 11 E0 A1 B1 1A E1`@0) → `FileType::Ole2`,
  exts `doc/xls/ppt/msi/msg/vsd` (legacy Office + `.msi` malware-disguise vector) — were all sniffing as
  `None`, invisible to mismatch detection.

## What's left = gold-plating (do NOT manufacture as filler)
The scan's honest call, which I endorse: the only remaining signature additions are **DDS** (`DDS `@0)
and **EOT** (completes the TTF/OTF/WOFF/WOFF2 font quartet) — genuinely common but low-value "one-more-format"
padding. The checkpoint history shows this vein mined ticket after ticket (fonts → 1341/1342 → 1343/1344);
this is where it stops paying. **Skip DDS/EOT unless a real use-case appears.**

## Ruled out (don't re-check)
`parser_panic_safety.rs` + `binary_data_preview_panic_safety.rs` cover every parser entrypoint (incl.
write_mp4, read_model_info, PLY). `glob.rs`, `type_class.rs` correct/tested. Only `type_mismatch_scan.rs`
had the undersized cap (fixed). Backlog empty; Blocked = genuinely gated (HEIC/DICOM/RAR/RAW/3D-viewer/signing).

## Bottom line
The clean headless format vein is tapped. Next real work needs the **user** (a resource/decision) or a
**gated/heavy-dep** epic — same conclusion as [[headless-frontier-tapped-2026-07-29]], now reconfirmed one
layer deeper. Read this before dispatching a fresh file-type/format-signature hunt; if nothing new landed,
skip it and don't build DDS/EOT filler.
