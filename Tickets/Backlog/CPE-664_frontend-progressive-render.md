---
id: CPE-664
title: Frontend progressive middle-pane render from the stream
type: feature
component: Frontend
priority: high
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-662
estimate: 2-3h
---

## Summary
Child of CPE-662. Replace the blocking `entries = await invoke("list_dir")` at `App.svelte:743` with a
`Channel` subscription: reset `entries = []`, append each incoming batch (the reactive `visible` pipeline
re-sorts automatically), and clear loading when the stream ends. Add a **generation token** so navigating
away mid-load supersedes the in-flight stream (stale batches dropped, no bleed into the new folder), and a
lightweight loading affordance replacing the busy-cursor-until-done model. Prereq: CPE-663.

## Acceptance Criteria
- [ ] `loadPath` streams via `list_dir_stream`, painting the first batch immediately.
- [ ] Sort/filter/tag pipeline still produces the same final ordering as the one-shot path.
- [ ] A folder change mid-load supersedes the previous stream (generation guard); no stale rows appear.
- [ ] Small folders are no slower than before (spot-measured); error + empty-folder + HOME paths intact.
- [ ] `npm run check` clean; targeted jsdom/unit coverage for the supersede guard where feasible.

## Work Log
