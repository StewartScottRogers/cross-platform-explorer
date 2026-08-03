---
id: CPE-1269
title: "Drag-out: harden icon resolution fallback + add resolveDragIcon unit coverage"
type: chore
component: frontend
priority: low
status: Backlog
tags: ready
created: 2026-08-02
epic: CPE-661
---

## Summary
Two non-blocking items from the CPE-672 review (PR #567):
1. If `resolveResource` fails (Tauri present but resource resolution errors), `resolveDragIcon()` returns the RELATIVE
   `DEFAULT_DRAG_ICON` uncached, and FileList caches that relative string into `dragOutIcon` → later drags pass a
   relative icon verbatim, violating the plugin's "absolute path" invariant. Harden: only cache/use an absolute result;
   otherwise let the plugin use its own default or retry.
2. `src/lib/dragOut.test.ts` was not updated for the new `resolveResource`/`resolveDragIcon` logic — it doesn't mock
   `@tauri-apps/api/path`, so the success path has no direct unit coverage. Add a mock + tests for resolve-success,
   resolve-failure fallback, and cache-only-on-success.

## Acceptance criteria
- resolveDragIcon never yields a non-absolute path that reaches the plugin as `icon`.
- dragOut.test.ts covers resolve success + failure + caching. npm run check + vitest green.

## Notes
Low priority; pre-existing weakness from the CPE-1264 fallback design. Not user-visible unless resource resolution fails.
