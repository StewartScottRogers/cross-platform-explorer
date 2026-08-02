---
id: CPE-1238
title: "Video representative-frame + PDF first-page thumbnails (heavy-dep, deferred)"
type: Task
priority: Low
component: cpe-server
tags: [big-design]
created: 2026-08-01
epic: CPE-718
closed:
---

## Context
The remaining CPE-718 formats — video (representative frame) + PDF (first page) — need HEAVY native
rendering crates (ffmpeg / pdfium / mupdf) that materially grow build size and fight PURPOSE's
fast/small/predictable tiebreaker, and their output can only be truly verified by looking at a rendered
frame (GUI/real-hardware). Deferred by our choice pending a decision on the dependency-weight tradeoff.

## Acceptance criteria (when picked up)
- Decide the dependency approach (bundled native lib vs optional/feature-gated vs sidecar) with the user,
  given build-size + cross-platform + signing implications.
- Video: extract a representative frame → cached thumbnail. PDF: first-page render → cached thumbnail.
- Feature-gated so a build without them still works + incurs no cost.

## Notes
Deferred out of the CPE-718 headless slice (CPE-1236 SVG/font + CPE-1237 streaming client). Revisit with
the user for the dep-weight call.

## Work Log
- 2026-08-02 — User green-lit the dependency weight. Opus Researcher + Foreman decided the architecture
  (Library: thumbnail-native-deps-pdf-video-2026-08-02.md): PDF via pdfium-render in-process (BSD/MIT) behind
  `pdf-thumb`; video via BUNDLED ffmpeg shell-out behind `video-thumb` (LGPL/GPL stays out of the signed binary);
  never link mupdf (AGPL). Decomposed into CPE-1256 (PDF), CPE-1257 (video), CPE-1258 (ship-enablement+CI+docs).
  Closes when all three merge.

- 2026-08-02 — All 3 slices merged (CPE-1256 PDF #558, CPE-1257 video #559, CPE-1258 enablement #560). DONE.
