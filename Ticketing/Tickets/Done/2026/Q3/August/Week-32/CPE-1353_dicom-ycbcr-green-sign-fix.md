---
id: CPE-1353
title: "DICOM color: fix YCbCr→RGB green-term sign bug (color Doppler renders non-primary hues wrong)"
type: Bug
status: Done
priority: Low
component: cpe-server
tags: [ready]
epic: CPE-219
created: 2026-08-05
closed: 2026-08-05
---

## Problem

`crates/server/src/dicom.rs::convert_ybr_full_to_rgb_u8` (added in CPE-1352) was ported verbatim from
`dicom-pixeldata-0.10.0`'s `convert_colorspace_u8`, which has a **sign bug** in the green term vs pydicom /
DICOM PS3.3 C.7.6.3.1.2 (BT.601 full-range). The Cb coefficient in `G` is **positive** (`+0.344136·Cb'`)
but must be **negative**:

```
correct:  G = Y − 0.344136·(Cb−128) − 0.714136·(Cr−128)
current:  G = Y + 0.344136·(Cb−128) − 0.714136·(Cr−128)   ← wrong sign on the Cb term
```

Effect: YBR_FULL/YBR_FULL_422 color DICOM (standard for **color Doppler ultrasound**) renders non-primary
hues with a wrong green channel — e.g. pure green `[Y=150,Cb=44,Cr=21]` decodes to `[0,198,1]` instead of
`[0,255,1]`. Pure primaries don't expose it (G clamps to 0). This predates CPE-1352 (the old
`to_dynamic_image` path called the same buggy upstream fn), so it's not a regression — but we now own our
copy of the function, so we can and should make it correct.

## Fix

In `convert_ybr_full_to_rgb_u8`, flip the Cb-term sign in the `g` computation:

```rust
// was: let g = (y + (0.114 * 1.772 / 0.587) * cb + (-0.299 * 1.402 / 0.587) * cr) + 0.5;
   let g = (y - (0.114 * 1.772 / 0.587) * cb + (-0.299 * 1.402 / 0.587) * cr) + 0.5;
```

(`0.114*1.772/0.587 = 0.344136`; `-0.299*1.402/0.587 = -0.714136` — the Cr term is already correct.)
Update the doc comment to note we intentionally diverge from dicom-pixeldata to match pydicom / the DICOM
standard.

## Acceptance criteria

- A YBR_FULL **green** fixture (`[Y=150,Cb=44,Cr=21]`) decodes to `~[0,255,1]` (±1); **blue**
  (`[Y=29,Cb=255,Cr=107]`) → `~[0,0,255]`; pure **red** (`[76,85,255]`) still → `~[255,0,0]`. Add these as
  regression tests (the existing red test stays green).
- `cargo test --features dicom-thumb dicom` + clippy both modes green. No other behavior change.

## Notes

Small, isolated, headless. Makes our DICOM color MORE correct than upstream dicom-pixeldata. Surfaced by the
CPE-1352 (#647) re-review.

## Work Log
- 2026-08-05 (sprint): PR #648 merged. Flipped the Cb-term sign in convert_ybr_full_to_rgb_u8 green channel (+->-) to match DICOM PS3.3 C.7.6.3.1.2 / pydicom; green+blue regression test added. Foreman-applied; independent verifier APPROVE (round-tripped own hues orange/cyan within +/-1, proved test non-hollow). Now more correct than upstream dicom-pixeldata (which has the sign bug).
