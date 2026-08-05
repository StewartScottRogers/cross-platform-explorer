---
title: "What clean BACKEND slice can be built autonomously (cargo-testable, no user resource) now that GUI is tapped?"
date: 2026-08-05
tags: [frontier, backend, cpe-server, cargo-testable, 3d-model, stl, obj, cpe-118, format-readers, vein]
status: current
---

## Question
GUI-with-existing-backend is tapped ([[clean-gui-vein-tapped-after-declutter-2026-08-05]]). What backend work can
the crew build + verify headlessly (cargo test + clippy, no model key / network / Mac / creds / hardware / heavy dep)?

## Finding
The pure-DETECTOR vein is also tapped — every In-Progress algorithm epic (CPE-1002 file-safety, CPE-1000
file-type, CPE-716 drive-bay) has its detector children Done; only attended-UI remainders. The remaining clean
backend work is **unbuilt FORMAT READERS** in the Blocked queue — and exactly ONE is clean today:

**BUILD: 3D-model geometry reader — `crates/server/src/model_3d.rs` (STL + OBJ), epic CPE-118, filed CPE-1333.**
- `read_model_info(bytes) -> ModelInfo { format, triangle_count, vertex_count, bounding_box, ascii }`. Binary STL
  (80B header + u32 count + 50B/tri), ASCII STL (solid/facet/vertex lines), OBJ (v/f lines). glTF/GLB trivial
  follow-up (JSON → reuses serde_json).
- **ZERO new deps** (hand-rollable) — clears lean-core. Fully cargo-testable with INLINE `&[u8]` fixtures (~134B
  binary-STL, ASCII OBJ cube) — no external files, matches `binary_preview.rs`/`image_column.rs` convention.
- Real value: CPE-118 is Blocked ONLY on the attended WebGL viewer; its ticket says the format "falls back to the
  metadata pane" — this reader IS that fallback (analog of shipped `media_column.rs`/`doc_column.rs`). Also feeds
  CPE-1000's `file_type.rs` detector (currently has NO 3D `FileType` arms).
- Risk LOW. One gotcha: binary STL can embed "solid" in its header, so don't disambiguate on the prefix —
  validate `80 + 4 + 50*n == len` (itself a good test).

## Rejected (evidence per candidate — do NOT re-research)
- **CPE-097 HEIC** — needs native `libheif`; pure-Rust HEIC not production-ready; display-verified. HEAVY/native.
- **CPE-219 DICOM** — needs `dicom-rs` stack; validated against clinical sample data. HEAVY dep + attended.
- **CPE-111 RAR** — non-free UnRAR (licensing) or incomplete pure-Rust RAR5. LICENSING-gated.
- **CPE-102 camera-RAW embedded-preview** — code is viable (CR2/NEF/ARW are TIFF; kamadak-exif+image already deps,
  no new dep to walk IFDs for the embedded JPEG), BUT cargo-testing needs a REAL committed CR2/NEF/ARW fixture
  (offsets are camera-specific, can't synthesize inline) → fails the "small inline fixture" bar. Viable only if a
  licensed sample is committed. Second-best; not clean today.
- AI epics (976/977/979/980 embedder/model/tesseract), remote/network (616/810), drive eject/network (716),
  the CPE-118 viewer itself — all USER-GATED.

## How to apply
Build CPE-1333 (STL+OBJ + file_type signatures + thin command + bindings regen). After it, the remaining format
readers are dep-gated/licensing-gated/attended or need a committed binary fixture — take those WITH the user (or a
committed sample). glTF/GLB is a clean follow-up increment to CPE-1333 (JSON, serde_json already a dep).
