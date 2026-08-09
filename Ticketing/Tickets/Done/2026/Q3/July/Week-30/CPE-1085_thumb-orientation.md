---
id: CPE-1085
title: "Thumbnail EXIF orientation — cpe_server::thumb_orient (orient_for_display)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-718
estimate: 1-2h
---

## Summary
Child of CPE-718 (Universal thumbnail pipeline). Today thumbnails ignore EXIF orientation, so phone photos
thumbnail sideways. Provide a reusable orientation-correction module for the thumbnail path. **Pure** in
`crates/server`, `cargo test` on the 3-OS matrix — no GUI, no user resource, **no new deps** (`image` +
`kamadak-exif` already vendored). New file, independent — dispatch FIRST.

## Design (buildable)
New module `crates/server/src/thumb_orient.rs`, registered `pub mod thumb_orient;` in
`crates/server/src/lib.rs` **immediately after `pub mod thumbnail;`**. The EXIF-orientation logic already
exists as PRIVATE fns in `crates/server/src/batch_transform.rs` (`read_exif_orientation` ~line 52,
`normalize_orientation` ~line 63, the standard 8-value table) — read them and lift the proven logic here as
PUBLIC fns (batch_transform's are private, so a documented local copy is fine — same pattern as CPE-1079's
rename-marker copy; do NOT edit batch_transform.rs).

```rust
use image::DynamicImage;
pub fn read_exif_orientation(bytes: &[u8]) -> Option<u32>;          // kamadak-exif, mirror batch_transform
pub fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage;  // the 8-value table; 1/unknown = no-op
pub fn orient_for_display(img: DynamicImage, bytes: &[u8]) -> DynamicImage;     // read from bytes + apply; no-op if absent
```

## ⚠ Notes
- No `f64`, no serialized types → plain derives, no serde/specta. No `std::path`, no `#[cfg]`.
- The 8-value table maps EXIF orientation → rotate/flip compose (values 2/4=flip, 3=180, 5/7=rotate+flip,
  6=rotate90, 8=rotate270; 1/unknown=no-op) — copy batch_transform's `normalize_orientation` exactly.

## Tests (`#[cfg(test)] mod tests`)
Copy the proven `jpeg_with_exif_orientation(w, h, orient)` fixture builder from batch_transform.rs's tests.
- orientation=6 on a 10×4 **wide** JPEG → `orient_for_display` result is **portrait** (`height() > width()`).
- orientation=1 → dims unchanged; no-EXIF bytes → unchanged.
- every value 1..=8 AND a bogus 99 → never panics.
- **Assert dimensions/aspect, NEVER exact byte lengths** (3-OS encoder variance).

## Acceptance Criteria
- [ ] `orient_for_display` rotates a wide orientation=6 image to portrait; orientation=1/absent → unchanged.
- [ ] All EXIF values 1..=8 + a bogus value handled without panic.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean default AND
      `--features index`; no new deps.

## Work Log
2026-07-26 (sprint) — Filed by the Product Manager as the CPE-718 orientation slice (the reusable home the
thumbnail path needs). Independent new file; distinct lib.rs anchor. CPE-1086 depends on this module's
`orient_for_display`.

2026-07-26 (sprint, overnight Worker) — Built `crates/server/src/thumb_orient.rs`: `read_exif_orientation`,
`apply_orientation`, `orient_for_display`, all public. Logic is a documented local copy of
`batch_transform::read_exif_orientation` / `normalize_orientation` (both private there) — same
copy-not-edit pattern as CPE-1079; `batch_transform.rs` untouched. Registered `pub mod thumb_orient;`
in `lib.rs` immediately after `pub mod thumbnail;`. Tests copy `jpeg_with_exif_orientation` fixture
builder from batch_transform's tests; cover orientation=6 wide→portrait, orientation=1 unchanged,
no-EXIF unchanged, all values 1..=8 plus a bogus 99 without panic, and a direct
`read_exif_orientation` read-back. `cargo test -p cpe-server`: 989 passed, 0 failed (5 new). Clippy
`--all-targets -- -D warnings` clean and `--features index` clean. No new dependencies (Cargo.toml
untouched). No blockers, no open assumptions beyond what the ticket already specified. Branch
`cpe-1085-thumb-orient`, PR opened against `main`; ticket left in `Doing` pending merge (per the
CPE-1048-style pattern of moving to `Done` only once the PR lands).
