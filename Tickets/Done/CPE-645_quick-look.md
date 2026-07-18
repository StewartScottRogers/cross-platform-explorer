---
id: CPE-645
title: Spacebar quick-look overlay for images
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-615
---

## Summary
Child of CPE-615. Press **Space** on a selected image → a full-screen quick-look overlay showing it
large; **←/→** step through the folder's images, **Esc/Space** close. App owns the keyboard while
open (no dual-listener race); the overlay renders via `convertFileSrc` (streams the file, no decode).

## Acceptance Criteria
- [x] Space opens quick-look on a single selected image (no-op otherwise / while typing).
- [x] ←/→ cycle the folder's images; Esc/Space close; mouse nav + close buttons + a name/counter bar.
- [x] `QuickLook.svelte` is a dumb renderer; App handles keys + index. `npm run check` clean; suite green.

## Notes
Gallery mode (bigger-tile view) remains a follow-up child of CPE-615.

## Work Log
2026-07-18 (nightshift) — Built the quick-look overlay on top of the thumbnail/preview work.
