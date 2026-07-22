---
id: CPE-891
title: Fix stale ContentSearch/Duplicates dialog tests broken by the streaming migration (main red)
type: bug
component: Frontend
priority: high
tags: ready
epic: CPE-662
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
The streaming-liveness migration (CPE-662: #185 ContentSearch, #188 Duplicates) switched both dialogs
from a blocking `invoke("search_file_contents" / "find_duplicates")` returning one `Vec` to a
`rawInvoke("*_stream")` call that streams batches through a `Channel` imported from
`@tauri-apps/api/core`. Their unit tests were **not** updated: they still mocked the old command names
and, critically, mocked `@tauri-apps/api/core` with only an `invoke` export — no `Channel`. So the
components' `new Channel<…>()` threw *"No 'Channel' export is defined on the '@tauri-apps/api/core'
mock"*, failing 7 tests and turning the **Frontend** CI job **red on main** (it stayed red through
#191/#192).

Fix: update both tests to the streaming contract — provide a `Channel` class in the module mock, drive
the results through the channel's `onmessage`, and assert the `*_stream` command names (matching the
`onMatch`/`onGroup` channel arg with `expect.objectContaining`).

## Acceptance Criteria
- [x] `@tauri-apps/api/core` mock in both dialog tests exports a `Channel` class.
- [x] ContentSearchDialog test drives matches through the `onMatch` Channel and asserts
      `search_file_contents_stream`.
- [x] DuplicatesDialog test drives groups through the `onGroup` Channel and asserts
      `find_duplicates_stream`.
- [x] Full frontend suite green (`npm run check` = 0 errors; `vitest run` all pass).

## Work Log
- Root cause: components import `Channel` from core (per the streaming standard) but the tests' module
  mock omitted it; the tests also still asserted the pre-stream command names.
- Fixed both `.test.ts` files; `npm run check` clean, 911 frontend tests pass locally.
