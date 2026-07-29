---
id: CPE-1106
title: "Batch media: optional image-overlay Watermark op"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-723
---

## Summary
Fills the LAST Definition-of-Done gap in epic CPE-723 (Batch media operations): the DoD names a watermark op.
Add an **optional `Watermark { image, position, opacity }`** op that alpha-composites a chosen overlay image
(logo/stamp) onto each batch image. **User decision (2026-07-26): "optional configuration that has no
watermark if not set."** Built as an **image overlay** (not text) deliberately — image compositing is
DEPENDENCY-FREE via the existing `image` crate, honouring the repo's lean-core / no-new-deps guardrail; text
watermarking would require a font-rasteriser dependency (a follow-up if wanted). Empty/unset overlay ⇒ no-op
(no watermark), matching the user's "none if not set".

## Context (verified)
- `crates/server/src/batch_media.rs` — `enum MediaOp { Resize, Convert, Rotate, Flip, Rename, StripMetadata,
  Compress }` (Compress added in CPE-1103). Add a `Watermark` variant.
- `crates/server/src/batch_transform.rs` — `apply_ops(input, ops)` decodes once, folds ops on the
  `DynamicImage`, re-encodes. Watermark = load the overlay image + alpha-blend it onto the working image at a
  corner with an opacity — all via `image`'s `imageops::overlay` / manual pixel blend (NO new dep).
- `src/lib/components/BatchMediaDialog.svelte` + `src/lib/batchMedia.ts` — op dropdown + `mediaOpLabel`. The
  overlay-image path needs a native file picker (the repo already uses `@tauri-apps/plugin-dialog` `open`,
  imported in `App.svelte`; see [[path-inputs-need-a-picker]]).

## Design (buildable)
1. **`MediaOp::Watermark { image: String, position: Corner, opacity: u8 }`** — plain derives, serde tag
   `watermark`, snake_case. `Corner` = enum `{ TopLeft, TopRight, BottomLeft, BottomRight, Center }` (serde
   snake_case), default `BottomRight`. `opacity` 0-100. **`image` empty ⇒ the op is a no-op** (skip, no error)
   — this is the "optional, none if unset" behaviour. `validate()`: opacity ≤ 100; if `image` non-empty it
   need not exist at validate time (checked at apply). `plan()` summary: `"watermark {basename} {position}
   {opacity}%"` (or nothing if image empty).
2. **`apply_ops` — Watermark step:** if `image` non-empty, load it (bounded/decode-bomb guarded like the main
   decode), compute the paste origin from `Corner` + the overlay/base dims (clamp so it stays in-bounds),
   alpha-blend at `opacity/100` (per-pixel blend, or scale the overlay's alpha then `imageops::overlay`).
   Saturating arithmetic; never panic; overlay larger than base → clamp/scale-to-fit or anchor at the corner
   and clip (pick one, document). A missing/undecodable overlay file → skip-with-reason for that file (honest,
   not fatal), consistent with the batch's skip-on-error model.
3. **Frontend** — `mediaOpLabel` case `watermark` → `Watermark {basename} {corner} {opacity}%` (or `Watermark
   (none)` if unset). Dialog "Watermark" op: a **Browse** button (native image picker via the plugin-dialog
   `open`, image extensions filter) to set `image`, a `Corner` dropdown, an opacity 0-100 field. Reflowing
   pill; theme vars; visible-border dialog conventions unchanged.
4. **Bindings** — regenerate so `MediaOp` includes `{op:"watermark", image, position, opacity}` + the `Corner`
   type; drift-guard passes.

## ⚠ Notes / guardrails
- **No new deps** — image compositing only (that's why image-overlay, not text). Saturating/validated opacity;
  no panic on any input. Non-destructive default path unchanged.
- Optional by construction: empty `image` ⇒ no-op ⇒ "no watermark if not set" (the user's requirement).
- Order note: Watermark applies to the current pixels, so Resize/Rotate before Watermark vs after changes the
  result — document (ops apply left-to-right, already the pipeline's contract).

## Acceptance Criteria
- [ ] `MediaOp::Watermark{image,position,opacity}` plans + validates (opacity ≤100; empty image ⇒ no-op) and,
      with a real overlay, composites it at the chosen corner/opacity onto a decoded image; a missing overlay
      is a per-file skip-with-reason, not a panic/fatal; `cargo test -p cpe-server` green (tests: empty-image
      no-op; a real overlay changes pixels at the target corner; opacity 0 ≈ unchanged; oversized overlay
      clamped; missing file skipped).
- [ ] Dialog offers a Watermark op with an image Browse picker + corner + opacity; `mediaOpLabel` renders it;
      bindings regenerated, drift-guard passes, `npm run check` clean; BatchMediaDialog test extended if the
      op-builder path is exercised.
- [ ] clippy clean (default + `--features index`); no new deps; no panic on any input; existing batch-media
      tests green.

## Work Log
2026-07-26 (workshift) — Filed as the final CPE-723 DoD gap. User chose "optional watermark, none if unset";
built as dep-free image overlay (text deferred — needs a font dep vs the lean-core guardrail). Closes CPE-723
once merged (with compress CPE-1103 + the shipped resize/convert/rotate/flip/rename).
