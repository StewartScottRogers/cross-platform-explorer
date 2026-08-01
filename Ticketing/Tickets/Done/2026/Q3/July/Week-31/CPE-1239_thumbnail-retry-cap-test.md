---
id: CPE-1239
title: "Test: thumbnail client retry-cap exhaustion resolves null (OOM regression pin) + spawn_blocking-panic cancel-leak"
type: Task
priority: Low
component: frontend
tags: [ready]
created: 2026-08-01
epic: CPE-718
closed:
---

## Context
The CPE-1237 gauntlet (Reviewer + UAT) confirmed the bounded-retry cap (`MAX_REQUEUE_ATTEMPTS=4` in
`thumbnailClient.ts`) fixes the real OOM (a "completed" batch that resolves nothing re-queueing forever)
— but only *by inspection* + the full-suite-completes-clean signal. There's no DIRECT test that drives a
request PAST the cap and asserts it resolves `null` (icon fallback) without further backend calls.

## Acceptance criteria
- A vitest case forces a request to exceed `MAX_REQUEUE_ATTEMPTS` (e.g. a `thumbnails_stream` mock that
  always "completes" without ever yielding that key) and asserts: the request resolves to `null` after
  exactly the cap, does NOT spin, and issues no further `thumbnails_stream` invokes past the cap.
- (Minor, optional) `src-tauri/src/lib.rs`: ensure the `thumb_stream_cancels()` registry entry is removed
  even if the `spawn_blocking` batch panics (e.g. remove-on-scope-guard), closing the negligible leak the
  reviewer noted.

## Notes
Pins the exact OOM regression CPE-1237 fixed. Low priority (mechanism verified by inspection + suite).
