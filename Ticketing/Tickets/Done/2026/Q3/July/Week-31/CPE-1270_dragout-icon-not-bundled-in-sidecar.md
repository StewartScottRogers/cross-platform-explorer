---
id: CPE-1270
title: "Drag-out broken in shipped sidecar build: icons/icon.png dropped by --config resource override"
type: bug
component: build
priority: high
status: Done
tags: ready
created: 2026-08-02
epic: CPE-661
---

## Problem (found by user attended test of v0.57.42)
Alt+drag-out did nothing in the installed sidecar build. Root cause: CPE-672 added the drag preview icon to the
BASE `src-tauri/tauri.conf.json` `bundle.resources` as an ARRAY `["icons/icon.png"]`. The sidecar RELEASE build
applies `--config` overlays (`tauri.sidecar.{windows,unix}.conf.json`, pdfium overlay) whose `bundle.resources` are
OBJECTS. Tauri's config merge REPLACES the base array with the overlay object (type mismatch), so `icons/icon.png`
is DROPPED from the shipped sidecar bundle. Confirmed: `icons/icon.png` absent from the install dir. Then
`resolveDragIcon()` → `resolveResource("icons/icon.png")` fails → the drag plugin gets an invalid/relative icon →
`startDrag` (which passes `image: options.icon` to Rust) fails → drag-out silently no-ops. Same class as the
thumbnail pdfium-path bug: the shipped sidecar-overlay build differs from what CPE-672 tested (base config).

## Fix
- Added `"icons/icon.png": "icons/icon.png"` to the `bundle.resources` OBJECT in BOTH
  `tauri.sidecar.windows.conf.json` and `tauri.sidecar.unix.conf.json`, so the icon is bundled in the sidecar build
  (objects merge; the base array is moot for the overlay build).
- (Follow-up CPE-1269 already tracks hardening resolveDragIcon; the dragOut.ts comment claiming a missing icon "only
  degrades the preview" is wrong — the plugin fails the whole drag — noted there.)

## Acceptance criteria
- Shipped sidecar install contains `icons/icon.png`; `resolveResource` yields an absolute path; Alt+drag-out works.
- Verified by user attended test on the next build.
