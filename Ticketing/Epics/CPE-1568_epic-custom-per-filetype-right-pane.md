---
id: CPE-1568
title: "EPIC: Custom per-file-type right pane — view + sensible actions tailored to each file type"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
created: 2026-08-10
closed:
---

> **Filed 2026-08-10 (user request).** Umbrella epic — decomposed just-in-time from the file-type-coverage
> research spike (dispatched 2026-08-10). Dormant brief until activated.

## Why (user's words)
"Present [each file type] to the right pane" = **view it, and offer whatever actions make sense** — and the
right pane is **custom to each file type**. The user wants the right pane to become a type-aware surface: a
JSON file gets a JSON viewer + its sensible actions; an image gets an image viewer + its actions; a font gets
a glyph/specimen view; etc. — driven by file extension / detected type.

## Not a green field — this EXTENDS a large existing pipeline
The app already has a rich right-pane/preview system — the spike inventories it and finds the GAPS:
- Frontend registry + dispatch: `src/lib/preview/provider.ts`, `PreviewPane.svelte`, `src/lib/preview/*`
  (csv, markdown, highlight, outline, loaders).
- Typed preview components: `CertPreview`, `EmailPreview`, `IcalPreview`, `JwtPreview`, `VcardPreview`,
  `FloatPreview`, `ImageCompareView`, plus PDF/DICOM/HEIC/RAW image paths.
- Backend: `crates/server/src/file_type.rs` (extension/magic detection) + `*_preview.rs`
  (binary/data/email/ical/image/jwt/vcard).
- Adjacent: media player pane (CPE-720), universal thumbnail pipeline (CPE-718), code-intelligence preview
  (CPE-724), structured previews (CPE-1433), archive preview.

## Goal
A **type-aware right pane** that, for the selected file, renders the best available custom viewer AND surfaces
the actions that make sense for that type — with a clear, registry-driven mapping from file type → (viewer,
actions), and a graceful fallback for unknown types. Every new viewer follows STREAMING.md (large payloads),
MENUS.md (actions), and ships docs per CPE-579.

## Decomposition (pending the research spike — the spike produces the concrete slice list)
The spike delivers: (1) a catalog of common file types by extension/family with what a "custom view + sensible
actions" means for each; (2) a coverage matrix vs the existing providers above; (3) the prioritized GAP list;
(4) a proposed registry/architecture for type→(viewer,actions) so new types plug in cleanly; (5) headless-first
slices. Slices are filed as `CPE-NNN` child tickets once the spike returns and the epic is activated.

## Constraints
PURPOSE.md fast/small/predictable: viewers stream/cap; no heavy deps in the core (heavy format engines go via
the sidecar pattern, cf. Binary Studio CPE-1561). Reuse the existing provider registry — do NOT fork a parallel
one. Actions per type must be discoverable (context menu / action bar) and follow MENUS.md.
